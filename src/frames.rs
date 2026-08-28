use core::{alloc::Layout, fmt::Display, ptr::NonNull};

use crate::region::Region;

pub const PAGE_SIZE: usize = 4096;
pub const MAX_ORDER: usize = 10;

pub struct Frames {
    base: Pfn,
    pages: usize,
    metadata: Region,
    free_lists: [Option<Pfn>; MAX_ORDER + 1],
    free_pages: usize,
}

impl Frames {
    pub fn new(arena: Region, reserved: &[Region]) -> Option<Frames> {
        let base = align_up(arena.base, PAGE_SIZE);
        let end = arena.end() & !(PAGE_SIZE - 1);
        let pages = end.checked_sub(base)? / PAGE_SIZE;

        if pages == 0 {
            return None;
        }

        let arena = Region {
            base,
            size: end - base,
        };

        let mut reservations = Reservations {
            arena,
            regions: reserved,
            metadata: None,
        };

        let layout = Layout::from_size_align(pages.next_multiple_of(PAGE_SIZE), PAGE_SIZE).ok()?;
        let metadata = reservations.reserve_metadata(layout)?;
        let metadata_ptr = NonNull::new(metadata.base as *mut u8)?;

        // SAFETY: reserve_metadata returned a range inside the arena that overlaps
        // no reservation, and nothing else holds a pointer into it.
        unsafe { metadata_ptr.write_bytes(0, metadata.size) };

        let mut frames = Frames {
            base: Pfn(base / PAGE_SIZE),
            pages,
            metadata,
            free_lists: [None; MAX_ORDER + 1],
            free_pages: 0,
        };

        frames.seed(&reservations);

        Some(frames)
    }

    pub fn free_blocks(&self, order: usize) -> usize {
        let mut count = 0;
        let mut current = self.free_lists.get(order).copied().flatten();

        while let Some(pfn) = current {
            count += 1;
            current = unsafe { pfn.read_next() };
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
            let buddy = Frames::buddy(pfn, order);

            if !self.arena().contains(buddy) {
                break;
            }

            let buddy_metadata = self.read_entry(buddy);

            if !buddy_metadata.free || buddy_metadata.order != order as u8 {
                break;
            }

            if self.unlink(buddy, order).is_none() {
                break;
            };

            pfn = pfn.min(buddy);
            order += 1;
        }

        self.push(pfn, order);
    }

    fn buddy(pfn: Pfn, order: usize) -> Pfn {
        let offset = 1 << order;
        let buddy_offset = pfn.0 ^ offset;
        Pfn(buddy_offset)
    }

    fn seed(&mut self, reservations: &Reservations) {
        for run in reservations.free_runs() {
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

        unsafe { pfn.write_next(current) };

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

        self.unlink(pfn, order)?;
        Some(pfn)
    }

    fn unlink(&mut self, pfn: Pfn, order: usize) -> Option<()> {
        let head = self.free_lists.get_mut(order)?;
        let mut current = (*head)?;

        if current == pfn {
            *head = unsafe { pfn.read_next() };
            self.free_pages -= 1 << order;
            return Some(());
        }

        loop {
            let next = unsafe { current.read_next() }?;

            if next == pfn {
                let after = unsafe { pfn.read_next() };
                unsafe { current.write_next(after) };

                self.free_pages -= 1 << order;
                return Some(());
            }

            current = next;
        }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pfn(usize);

impl Pfn {
    const EMPTY_PATTERN: usize = usize::MAX;

    pub fn to_addr(self) -> usize {
        self.0 * PAGE_SIZE
    }

    fn from_addr_down(addr: usize) -> Self {
        Self(addr / PAGE_SIZE)
    }

    fn from_addr_up(addr: usize) -> Self {
        Self(addr.div_ceil(PAGE_SIZE))
    }

    fn offset(self, offset: usize) -> Self {
        Self(self.0 + offset)
    }

    fn pages_until(self, end: Pfn) -> usize {
        end.0 - self.0
    }

    fn alignment_order(self) -> usize {
        self.0.trailing_zeros() as usize
    }

    fn index_from(self, base: Pfn) -> usize {
        self.0 - base.0
    }

    unsafe fn read_next(self) -> Option<Pfn> {
        let raw = unsafe { (self.to_addr() as *const usize).read() };

        (raw != Self::EMPTY_PATTERN).then_some(Pfn(raw))
    }

    unsafe fn write_next(self, next: Option<Pfn>) {
        let raw = next.map_or(Self::EMPTY_PATTERN, |next| next.0);

        unsafe { (self.to_addr() as *mut usize).write(raw) }
    }
}

const fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
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

pub struct PageRange {
    // including
    start: Pfn,
    // excluding
    end: Pfn,
}

impl PageRange {
    fn new(region: Region, arena: Region) -> Option<Self> {
        let base = region.base.max(arena.base);
        let end = region.end().min(arena.end());
        if base < end {
            Some(Self {
                start: Pfn::from_addr_down(base),
                end: Pfn::from_addr_up(end),
            })
        } else {
            None
        }
    }

    fn contains(&self, pfn: Pfn) -> bool {
        self.start <= pfn && pfn < self.end
    }

    fn overlaps(&self, other: &PageRange) -> bool {
        self.start < other.end && other.start < self.end
    }
}

struct Reservations<'a> {
    arena: Region,
    regions: &'a [Region],
    metadata: Option<Region>,
}

impl Reservations<'_> {
    fn reserve_metadata(&mut self, layout: Layout) -> Option<Region> {
        let mut count = 0;
        let mut next = self.arena.base;

        loop {
            // There could not be more jumps than reserved regions
            if count > self.regions.len() {
                return None;
            }

            count += 1;
            let region = Region {
                base: align_up(next, layout.align()),
                size: layout.size(),
            };

            if region.end() > self.arena.end() {
                return None;
            }

            let candidate = PageRange::new(region, self.arena)?;

            if let Some(taken) = self.overlapping(&candidate) {
                next = taken.end.to_addr();
                continue;
            }

            self.metadata = Some(region);
            return Some(region);
        }
    }

    fn overlapping(&self, range: &PageRange) -> Option<PageRange> {
        self.ranges().find(|other| other.overlaps(range))
    }

    fn ranges(&self) -> impl Iterator<Item = PageRange> {
        let arena = self.arena;

        self.regions
            .iter()
            .copied()
            .chain(self.metadata)
            .filter_map(move |region| PageRange::new(region, arena))
    }

    fn containing(&self, pfn: Pfn) -> Option<Pfn> {
        self.ranges()
            .find(|range| range.contains(pfn))
            .map(|range| range.end)
    }

    fn next_base_above(&self, pfn: Pfn) -> Option<Pfn> {
        self.ranges()
            .filter(|range| range.start > pfn)
            .map(|range| range.start)
            .min()
    }

    fn free_runs(&self) -> impl Iterator<Item = PageRange> {
        let mut cursor = Pfn::from_addr_up(self.arena.base);
        let end = Pfn::from_addr_down(self.arena.end());

        core::iter::from_fn(move || {
            while let Some(stop) = self.containing(cursor) {
                cursor = stop;
            }

            if cursor >= end {
                return None;
            }

            let next_reserved = self.next_base_above(cursor).unwrap_or(end).min(end);

            let run = PageRange {
                start: cursor,
                end: next_reserved,
            };

            cursor = run.end;

            Some(run)
        })
    }
}
