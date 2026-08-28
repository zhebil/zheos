use core::alloc::Layout;

use crate::{frames::pfn::Pfn, region::Region};

pub struct PageRange {
    // including
    pub start: Pfn,
    // excluding
    pub end: Pfn,
}

impl PageRange {
    pub fn new(region: Region, arena: Region) -> Option<Self> {
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

    pub fn contains(&self, pfn: Pfn) -> bool {
        self.start <= pfn && pfn < self.end
    }

    fn overlaps(&self, other: &PageRange) -> bool {
        self.start < other.end && other.start < self.end
    }
}

pub struct Reservations<'a> {
    arena: Region,
    regions: &'a [Region],
    metadata: Option<Region>,
}

impl<'a> Reservations<'a> {
    pub fn new(arena: Region, regions: &'a [Region]) -> Self {
        Self {
            arena,
            regions,
            metadata: None,
        }
    }

    pub fn reserve_metadata(&mut self, layout: Layout) -> Option<Region> {
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

    pub fn free_runs(&self) -> impl Iterator<Item = PageRange> {
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

pub const fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}
