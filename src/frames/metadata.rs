use core::ptr::NonNull;

use crate::memory::{
    pfn::{PAGE_SIZE, Pfn},
    region::{PageRange, Region},
};

pub struct Metadata {
    pub region: Region,
    base: Pfn,
    pages: usize,
}

const METADATA_ENTRY_SIZE: usize = 8;

const MAX_ARENA_PAGES: usize = 1 << 19;

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

impl Metadata {
    pub fn new(arena: Region, storage: Pfn) -> Option<Metadata> {
        let pages = arena.size / PAGE_SIZE;

        if pages == 0 || pages > MAX_ARENA_PAGES {
            return None;
        }

        let region = Region {
            base: storage.to_addr(),
            size: Self::required_size(arena) * PAGE_SIZE,
        };

        let start = NonNull::new(region.base as *mut u8)?;

        // SAFETY: Caller ensures that it owns all pages within the metadata region.
        unsafe { start.write_bytes(0, region.size) };

        Some(Metadata {
            region,
            base: Pfn::from_addr_down(arena.base),
            pages,
        })
    }

    pub fn pages(&self) -> usize {
        self.pages
    }

    pub fn covers(&self) -> PageRange {
        PageRange {
            start: self.base,
            end: self.base.offset(self.pages),
        }
    }

    pub(super) fn read(&self, pfn: Pfn) -> Entry {
        Entry::from_u64(unsafe { self.entry(pfn).read() })
    }

    pub(super) fn write(&mut self, pfn: Pfn, entry: Entry) {
        unsafe { self.entry(pfn).write(entry.to_u64()) }
    }

    pub fn required_size(arena: Region) -> usize {
        (arena.size / PAGE_SIZE * METADATA_ENTRY_SIZE).div_ceil(PAGE_SIZE)
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
