use crate::memory::{
    pfn::{PAGE_SIZE, Pfn},
    region::{PageRange, Region},
};

const RESERVED: usize = 16;

pub struct Full;

pub struct MemoryMap {
    arena: Region,
    reserved: [Region; RESERVED],
    len: usize,
}

impl MemoryMap {
    pub fn new(memory: Region) -> Option<MemoryMap> {
        let base = align_up(memory.base, PAGE_SIZE);
        let end = align_down(memory.end(), PAGE_SIZE);

        Some(MemoryMap {
            arena: Region {
                base,
                size: end.checked_sub(base)?,
            },
            reserved: [Region::EMPTY; RESERVED],
            len: 0,
        })
    }

    pub fn arena(&self) -> Region {
        self.arena
    }

    pub fn reserve(&mut self, region: Region) -> Result<(), Full> {
        let slot = self.reserved.get_mut(self.len).ok_or(Full)?;

        *slot = region;
        self.len += 1;

        Ok(())
    }

    pub fn reserved(&self) -> impl Iterator<Item = Region> {
        self.reserved[..self.len].iter().copied()
    }

    pub fn unreserved(&self) -> impl Iterator<Item = PageRange> {
        let mut cursor = Pfn::from_addr_up(self.arena.base);
        let end = Pfn::from_addr_down(self.arena.end());

        core::iter::from_fn(move || {
            while let Some(stop) = self.containing(cursor) {
                cursor = stop;
            }

            if cursor >= end {
                return None;
            }

            let run = PageRange {
                start: cursor,
                end: self.next_base_above(cursor).unwrap_or(end).min(end),
            };

            cursor = run.end;

            Some(run)
        })
    }

    fn ranges(&self) -> impl Iterator<Item = PageRange> {
        let arena = self.arena;

        self.reserved()
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

pub const fn align_down(addr: usize, align: usize) -> usize {
    addr & !(align - 1)
}
