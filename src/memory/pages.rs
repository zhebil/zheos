use core::ptr::NonNull;

use crate::memory::{
    map::MemoryMap,
    pfn::{PAGE_SIZE, Pfn},
    region::{PageRange, Region},
};

pub struct Pages {
    region: Region,
    base: Pfn,
    len: usize,
}

const ENTRY_SIZE: usize = 8;

const MAX_ARENA_PAGES: usize = (1 << 19) - 1;

// 1 extra bit reserved to distinguish between buddy and slab entries
pub enum Entry {
    Buddy {
        free: bool, // 1 bit
        order: u8,  // 4 bits
    },
    Slab {
        class: u8,         // 4 bits
        free_head: u16,    // 10 bits
        in_use: u16,       // 10 bits
        next_partial: u32, // 19 bits
        prev_partial: u32, // 19 bits
    },
}

impl Pages {
    pub fn new(map: &mut MemoryMap) -> Option<Pages> {
        let arena = map.arena();
        let len = arena.size / PAGE_SIZE;

        if len == 0 || len > MAX_ARENA_PAGES {
            return None;
        }

        let needed = Self::required_size(arena);
        let run = map.unreserved().find(|run| run.pages() >= needed)?;

        let region = Region {
            base: run.start.to_addr(),
            size: needed * PAGE_SIZE,
        };

        let start = NonNull::new(region.base as *mut u8)?;

        // SAFETY: the run came from the map's unreserved list, so nothing else
        // owns these pages, and the reserve below keeps it that way.
        unsafe { start.write_bytes(0, region.size) };

        map.reserve(region).ok()?;

        Some(Pages {
            region,
            base: Pfn::from_addr_down(arena.base),
            len,
        })
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn base(&self) -> Pfn {
        self.base
    }

    pub fn covers(&self) -> PageRange {
        PageRange {
            start: self.base,
            end: self.base.offset(self.len),
        }
    }

    pub fn read(&self, pfn: Pfn) -> Entry {
        Entry::from_u64(unsafe { self.entry(pfn).read() })
    }

    pub fn write(&mut self, pfn: Pfn, entry: Entry) {
        unsafe { self.entry(pfn).write(entry.to_u64()) }
    }

    pub fn required_size(arena: Region) -> usize {
        (arena.size / PAGE_SIZE * ENTRY_SIZE).div_ceil(PAGE_SIZE)
    }

    fn entry(&self, pfn: Pfn) -> *mut u64 {
        // SAFETY: in range for any pfn inside `covers`, which is every pfn that
        // reaches here - the free lists only ever hold arena pages, and `free`
        // tests a buddy against `covers` before looking it up.
        unsafe { (self.region.base as *mut u64).add(pfn.index_from(self.base)) }
    }
}

impl Entry {
    fn to_u64(&self) -> u64 {
        match self {
            Entry::Buddy { free, order } => {
                (false as u64) | ((*free as u64) << 1) | ((*order as u64 & 0xF) << 2)
            }
            Entry::Slab {
                class,
                free_head,
                in_use,
                next_partial,
                prev_partial,
            } => {
                (true as u64)
                    | ((*class as u64 & 0xF) << 1)
                    | ((*free_head as u64 & 0x3FF) << 5)
                    | ((*in_use as u64 & 0x3FF) << 15)
                    | ((*next_partial as u64 & 0x7FFFF) << 25)
                    | ((*prev_partial as u64 & 0x7FFFF) << 44)
            }
        }
    }

    fn from_u64(val: u64) -> Self {
        let is_slab = (val & 1) != 0;

        if is_slab {
            Self::Slab {
                class: ((val >> 1) & 0xF) as u8,
                free_head: ((val >> 5) & 0x3FF) as u16,
                in_use: ((val >> 15) & 0x3FF) as u16,
                next_partial: ((val >> 25) & 0x7FFFF) as u32,
                prev_partial: ((val >> 44) & 0x7FFFF) as u32,
            }
        } else {
            Self::Buddy {
                free: (val & 2) != 0,
                order: ((val >> 2) & 0xF) as u8,
            }
        }
    }
}
