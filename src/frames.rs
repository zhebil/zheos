use core::{alloc::Layout, ptr::NonNull};

use crate::region::Region;

const PAGE_SIZE: usize = 4096;
const MAX_ORDER: usize = 10;

pub struct Frames {
    arena: Region,
    list: [Pfn; MAX_ORDER + 1],
}

impl Frames {
    pub fn new(arena: Region, reserved: &[Region]) -> Option<Frames> {
        let metadata_size = (arena.size / PAGE_SIZE).next_multiple_of(PAGE_SIZE);
        let layout = Layout::from_size_align(metadata_size, PAGE_SIZE).ok()?;

        let metadata_ptr = Self::alloc_itself(arena, layout, reserved)?;
        let metadata_region = Region {
            base: metadata_ptr.as_ptr() as usize,
            size: layout.size(),
        };

        for addr in (metadata_region.base..metadata_region.end()).step_by(8) {
            unsafe {
                *(addr as *mut u64) = 0;
            }
        }

        // 8 bits per metadata entry
        let metadata_slice = unsafe {
            core::slice::from_raw_parts_mut(metadata_region.base as *mut u8, metadata_region.size)
        };

        // Free everything that is not reserved

        Some(Frames {
            arena,
            list: [Pfn(0); MAX_ORDER + 1],
        })
    }

    fn alloc_itself(arena: Region, layout: Layout, reserved: &[Region]) -> Option<NonNull<u8>> {
        let mut count = 0;
        let mut next = arena.base;
        loop {
            // There could not be more jumps than reserved regions
            if count > reserved.len() {
                return None;
            }

            count += 1;
            let start = align_up(next, layout.align());
            let region = Region {
                base: start,
                size: layout.size(),
            };

            if region.end() > arena.end() {
                return None;
            }

            if let Some(r) = Self::intersected_reserved_region(reserved, region) {
                next = r.end();
                continue;
            }

            return NonNull::new(start as *mut u8);
        }
    }

    fn intersected_reserved_region(reserved: &[Region], region: Region) -> Option<Region> {
        for r in reserved {
            if region.is_overlapping(r) {
                return Some(*r);
            }
        }

        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pfn(usize);

impl Pfn {
    fn to_addr(self) -> usize {
        self.0 * PAGE_SIZE
    }

    fn from_addr(addr: usize) -> Self {
        Self(addr / PAGE_SIZE)
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
