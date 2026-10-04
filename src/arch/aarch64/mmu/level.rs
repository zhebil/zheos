/// Which table a walk is looking at. There is no level 0 because `T0SZ = 25`
/// gives a 39-bit address space, and 39 bits start at level 1.
#[derive(Debug, Clone, Copy)]
pub enum Level {
    Level1 = 1,
    Level2 = 2,
    Level3 = 3,
}

impl Level {
    pub const fn next(self) -> Option<Self> {
        match self {
            Level::Level1 => Some(Level::Level2),
            Level::Level2 => Some(Level::Level3),
            Level::Level3 => None,
        }
    }

    /// Where this level's slot index starts in a virtual address: 30, 21, 12.
    /// Nine bits wide at every level, because every table has 512 slots.
    const fn offset(self) -> u8 {
        12 + 9 * (3 - self as u8)
    }

    /// Which of the 512 slots an address lands in.
    pub const fn slot_of(self, va: usize) -> usize {
        (va >> self.offset()) & 0x1FF
    }

    /// The bits this level does not translate - the offset into a block.
    pub const fn offset_mask(self) -> usize {
        self.size() - 1
    }

    /// How much address space one slot covers: 1 GiB, 2 MiB, 4 KiB.
    pub const fn size(self) -> usize {
        1 << self.offset()
    }

    pub const fn is_aligned(self, addr: usize) -> bool {
        addr & self.offset_mask() == 0
    }
}
