use core::fmt::Display;

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

    pub fn is_overlapping(&self, other: &Region) -> bool {
        self.base < other.end() && other.base < self.end()
    }
}

impl Display for Region {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:#012x}: {:x} bytes", self.base, self.size)
    }
}
