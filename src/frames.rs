use core::{alloc::Layout, ptr::NonNull};

use crate::region::Region;

const PAGE_SIZE: usize = 4096;
const MAX_ORDER: usize = 10;

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

        // frames.seed(&reservations);

        Some(frames)
    }

    pub fn metadata(&self) -> Region {
        self.metadata
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pfn(usize);

impl Pfn {
    fn to_addr(self) -> usize {
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

struct PageRange {
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
}
