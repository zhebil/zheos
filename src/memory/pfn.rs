pub const PAGE_SIZE: usize = 4096;

/// Page Frame Number
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pfn(usize);

impl Pfn {
    pub const ZERO: Pfn = Pfn(0);

    pub fn to_addr(self) -> usize {
        self.0 * PAGE_SIZE
    }

    pub fn from_addr_down(addr: usize) -> Self {
        Self(addr / PAGE_SIZE)
    }

    pub fn from_addr_up(addr: usize) -> Self {
        Self(addr.div_ceil(PAGE_SIZE))
    }

    pub fn offset(self, offset: usize) -> Self {
        Self(self.0 + offset)
    }

    pub fn pages_until(self, end: Pfn) -> usize {
        end.0 - self.0
    }

    pub fn alignment_order(self) -> usize {
        self.0.trailing_zeros() as usize
    }

    pub fn index_from(self, base: Pfn) -> usize {
        self.0 - base.0
    }

    /// Get the buddy PFN for a given order.
    pub fn buddy(self, order: usize) -> Self {
        Self(self.0 ^ (1 << order))
    }
}
