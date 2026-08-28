use core::{fmt::Display, ptr::NonNull};

use crate::memory::{
    map::MemoryMap,
    pfn::{Links, PAGE_SIZE, Pfn},
    region::{PageRange, Region},
};

pub const MAX_ORDER: usize = 10;

pub struct Frames {
    base: Pfn,
    pages: usize,
    metadata: Region,
    free_lists: [Option<Pfn>; MAX_ORDER + 1],
    free_pages: usize,
}

impl Frames {
    pub fn new(map: &mut MemoryMap) -> Option<Frames> {
        let arena = map.arena();
        let pages = arena.size / PAGE_SIZE;

        if pages == 0 {
            return None;
        }

        let wanted = pages.div_ceil(PAGE_SIZE);

        let run = map.unreserved().find(|run| run.pages() >= wanted)?;
        let metadata = Region {
            base: run.start.to_addr(),
            size: wanted * PAGE_SIZE,
        };

        map.reserve(metadata).ok()?;

        let metadata_ptr = NonNull::new(metadata.base as *mut u8)?;

        // SAFETY: the run came from the map and overlaps no reservation, and
        // nothing else holds a pointer into it.
        unsafe { metadata_ptr.write_bytes(0, metadata.size) };

        let mut frames = Frames {
            base: Pfn(arena.base / PAGE_SIZE),
            pages,
            metadata,
            free_lists: [None; MAX_ORDER + 1],
            free_pages: 0,
        };

        frames.seed(map);

        Some(frames)
    }

    pub fn free_blocks(&self, order: usize) -> usize {
        let mut count = 0;
        let mut current = self.free_lists.get(order).copied().flatten();

        while let Some(pfn) = current {
            count += 1;
            current = unsafe { pfn.read_links() }.next;
        }

        count
    }

    pub fn alloc(&mut self, order: usize) -> Option<Pfn> {
        let mut available_order =
            (order..=MAX_ORDER).find(|&o| self.free_lists.get(o).copied().flatten().is_some())?;

        let pfn = self.pop(available_order)?;

        while available_order > order {
            available_order -= 1;
            self.push(pfn.offset(1 << available_order), available_order);
        }

        self.write_entry(
            pfn,
            MetadataEntry {
                free: false,
                order: order as u8,
            },
        );

        Some(pfn)
    }

    pub fn free(&mut self, mut pfn: Pfn) {
        let metadata = self.read_entry(pfn);
        if metadata.free {
            return;
        }

        let mut order = metadata.order as usize;

        while order < MAX_ORDER {
            let buddy = pfn.buddy(order);

            if !self.arena().contains(buddy) {
                break;
            }

            let buddy_metadata = self.read_entry(buddy);

            if !buddy_metadata.free || buddy_metadata.order != order as u8 {
                break;
            }

            self.unlink(buddy, order);

            pfn = pfn.min(buddy);
            order += 1;
        }

        self.push(pfn, order);
    }

    fn seed(&mut self, map: &MemoryMap) {
        for run in map.unreserved() {
            let mut block = run.start;

            while block < run.end {
                // find largest power-of-2 block that fits in this run and is aligned
                let order = MAX_ORDER
                    .min(block.alignment_order())
                    .min(block.pages_until(run.end).ilog2() as usize);

                self.push(block, order);
                block = block.offset(1 << order);
            }
        }
    }

    fn push(&mut self, pfn: Pfn, order: usize) -> Option<()> {
        let head = self.free_lists.get_mut(order)?;
        let current = *head;

        unsafe {
            pfn.write_links(Links {
                prev: None,
                next: current,
            })
        };

        if let Some(current) = current {
            let mut links = unsafe { current.read_links() };
            links.prev = Some(pfn);
            unsafe { current.write_links(links) };
        }

        *head = Some(pfn);

        self.write_entry(
            pfn,
            MetadataEntry {
                free: true,
                order: order as u8,
            },
        );

        self.free_pages += 1 << order;

        Some(())
    }

    fn pop(&mut self, order: usize) -> Option<Pfn> {
        let pfn = (*self.free_lists.get(order)?)?;

        self.unlink(pfn, order);
        Some(pfn)
    }

    fn unlink(&mut self, pfn: Pfn, order: usize) {
        let Links { prev, next } = unsafe { pfn.read_links() };

        // Move prev to next and next to prev
        if let Some(prev) = prev {
            let mut links = unsafe { prev.read_links() };
            links.next = next;
            unsafe { prev.write_links(links) };
        } else {
            let head = self.free_lists.get_mut(order).unwrap();
            *head = next;
        }

        if let Some(next) = next {
            let mut links = unsafe { next.read_links() };
            links.prev = prev;
            unsafe { next.write_links(links) };
        }

        self.free_pages -= 1 << order;
    }

    fn write_entry(&mut self, pfn: Pfn, entry: MetadataEntry) {
        unsafe {
            (self.metadata.base as *mut u8)
                .add(pfn.index_from(self.base))
                .write(entry.to_byte())
        }
    }

    fn read_entry(&self, pfn: Pfn) -> MetadataEntry {
        let byte = unsafe {
            (self.metadata.base as *const u8)
                .add(pfn.index_from(self.base))
                .read()
        };
        MetadataEntry::from_byte(byte)
    }

    fn arena(&self) -> PageRange {
        PageRange {
            start: self.base,
            end: self.base.offset(self.pages),
        }
    }
}

impl Display for Frames {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} of {} pages free", self.free_pages, self.pages)
    }
}

struct MetadataEntry {
    free: bool,
    order: u8, // 4 bits
}

impl MetadataEntry {
    fn to_byte(&self) -> u8 {
        (self.free as u8) | (self.order << 1)
    }

    fn from_byte(byte: u8) -> Self {
        Self {
            free: (byte & 1) != 0,
            order: byte >> 1,
        }
    }
}
