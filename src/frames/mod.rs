mod metadata;

use core::fmt::Display;

use crate::{
    frames::metadata::{Entry, Metadata},
    memory::{
        map::MemoryMap,
        pfn::{Links, Pfn},
    },
};

pub const MAX_ORDER: usize = 10;

pub struct Frames {
    metadata: Metadata,
    free_lists: [Option<Pfn>; MAX_ORDER + 1],
    free_pages: usize,
}

impl Frames {
    pub fn new(map: &mut MemoryMap) -> Option<Frames> {
        let arena = map.arena();

        let metadata_size = Metadata::required_size(arena);

        let run = map.unreserved().find(|run| run.pages() >= metadata_size)?;

        let metadata = Metadata::new(arena, run.start)?;

        map.reserve(metadata.region).ok()?;

        let mut frames = Frames {
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

        self.metadata.write(
            pfn,
            Entry {
                free: false,
                order: order as u8,
            },
        );

        Some(pfn)
    }

    pub fn free(&mut self, mut pfn: Pfn) {
        let entry = self.metadata.read(pfn);
        if entry.free {
            return;
        }

        let mut order = entry.order as usize;

        while order < MAX_ORDER {
            let buddy = pfn.buddy(order);

            if !self.metadata.covers().contains(buddy) {
                break;
            }

            let buddy_entry = self.metadata.read(buddy);

            if !buddy_entry.free || buddy_entry.order != order as u8 {
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

        self.metadata.write(
            pfn,
            Entry {
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
}

impl Display for Frames {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} of {} pages free",
            self.free_pages,
            self.metadata.pages()
        )
    }
}
