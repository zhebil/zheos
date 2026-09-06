use crate::memory::{
    gaps::gaps,
    pfn::PAGE_SIZE,
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

    pub fn reserved(&self) -> impl Iterator<Item = Region> + Clone {
        self.reserved.iter().take(self.len).copied()
    }

    pub fn unreserved(&self) -> impl Iterator<Item = PageRange> {
        gaps(self.arena, self.reserved())
    }
}

pub const fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

pub const fn align_down(addr: usize, align: usize) -> usize {
    addr & !(align - 1)
}
