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

pub struct Entry {
    pub free: bool,
    pub order: u8, // 4 bits
}

impl Metadata {
    pub fn new(arena: Region, storage: Pfn) -> Option<Metadata> {
        let pages = arena.size / PAGE_SIZE;

        if pages == 0 {
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

    pub fn read(&self, pfn: Pfn) -> Entry {
        Entry::from_byte(unsafe { self.byte(pfn).read() })
    }

    pub fn write(&mut self, pfn: Pfn, entry: Entry) {
        unsafe { self.byte(pfn).write(entry.to_byte()) }
    }

    pub fn required_size(arena: Region) -> usize {
        (arena.size / PAGE_SIZE).div_ceil(PAGE_SIZE)
    }

    fn byte(&self, pfn: Pfn) -> *mut u8 {
        // SAFETY: in range for any pfn inside `covers`, which is every pfn that
        // reaches here - the free lists only ever hold arena pages, and `free`
        // tests a buddy against `covers` before looking it up.
        unsafe { (self.region.base as *mut u8).add(pfn.index_from(self.base)) }
    }
}

impl Entry {
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
