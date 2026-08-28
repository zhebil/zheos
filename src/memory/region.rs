use core::fmt::Display;

use crate::memory::pfn::Pfn;

/// One `(address, size)` pair. A range of physical memory, whoever it came from.
#[derive(Clone, Copy, Debug)]
pub struct Region {
    pub base: usize,
    pub size: usize,
}

impl Region {
    pub const EMPTY: Region = Region { base: 0, size: 0 };

    pub fn end(&self) -> usize {
        self.base + self.size
    }
}

impl Display for Region {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:#012x}: {:x} bytes", self.base, self.size)
    }
}

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

    pub fn pages(&self) -> usize {
        self.start.pages_until(self.end)
    }

    pub fn contains(&self, pfn: Pfn) -> bool {
        self.start <= pfn && pfn < self.end
    }
}
