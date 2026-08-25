use core::{alloc::Layout, ptr::NonNull};

use crate::{dtb::Dtb, region::Region};

pub fn image() -> Region {
    unsafe extern "C" {
        static __image_start: u8;
        static __stack_top: u8;
    }

    let start = &raw const __image_start as usize;
    let end = &raw const __stack_top as usize;

    Region {
        base: start,
        size: end - start,
    }
}

pub struct Full;

const IMAGE: &str = "kernel image";
const DEVICE_TREE: &str = "device tree";

const RESERVED_LEN: usize = 16;

pub struct Bump {
    next: usize,
    end: usize,
    reserved: [Region; RESERVED_LEN],
    reserved_len: usize,
}

impl Bump {
    pub fn discover(memory: Region, dtb: &Dtb) -> Result<Bump, &'static str> {
        let mut bump = Bump::new(memory);

        bump.reserve(image()).map_err(|_| IMAGE)?;
        bump.reserve(dtb.region()).map_err(|_| DEVICE_TREE)?;

        Ok(bump)
    }

    fn new(memory: Region) -> Self {
        Self {
            next: memory.base,
            end: memory.base + memory.size,
            reserved: [Region::EMPTY; RESERVED_LEN],
            reserved_len: 0,
        }
    }

    pub fn reserve(&mut self, region: Region) -> Result<(), Full> {
        if self.reserved_len >= RESERVED_LEN {
            return Err(Full);
        }
        self.reserved[self.reserved_len] = region;
        self.reserved_len += 1;
        Ok(())
    }

    /// An upper bound: reservations ahead of the pointer are not subtracted.
    pub fn remaining(&self) -> usize {
        self.end - self.next
    }

    pub fn alloc(&mut self, layout: Layout) -> Option<NonNull<u8>> {
        let mut count = 0;
        let mut next = self.next;
        loop {
            // There could not be more jumps than reserved regions
            if count > self.reserved_len {
                return None;
            }

            count += 1;
            let start = align_up(next, layout.align());
            let region = Region {
                base: start,
                size: layout.size(),
            };

            if region.end() > self.end {
                return None;
            }

            if let Some(r) = self.intersected_reserved_region(region) {
                next = r.end();
                continue;
            }

            self.next = region.end();
            return NonNull::new(start as *mut u8);
        }
    }

    fn intersected_reserved_region(&self, region: Region) -> Option<Region> {
        for r in self.reserved.iter().take(self.reserved_len) {
            if region.is_overlapping(r) {
                return Some(*r);
            }
        }

        None
    }
}

const fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}
