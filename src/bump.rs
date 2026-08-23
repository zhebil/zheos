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

const RESERVED_LEN: usize = 8;

const WORD: usize = 16;
const PAGE: usize = 4096;

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

/// The paths one boot-time allocation never touches: several allocations in a
/// row, a page-aligned one, a write that has to survive, and a reservation the
/// pointer has to step over.
pub fn self_check(bump: &mut Bump) -> Result<(), &'static str> {
    check_allocations(bump)?;
    check_skip()
}

fn check_allocations(bump: &mut Bump) -> Result<(), &'static str> {
    let small = Layout::from_size_align(WORD, WORD).map_err(|_| "word layout")?;

    let first = alloc_at(bump, small)?;
    let second = alloc_at(bump, small)?;
    let third = alloc_at(bump, small)?;

    require(
        first < second && second < third,
        "allocations do not ascend",
    )?;
    require(
        second - first >= WORD && third - second >= WORD,
        "allocations overlap",
    )?;

    // What TABLES will ask for, and the alignment nothing has exercised yet.
    let page = Layout::from_size_align(PAGE, PAGE).map_err(|_| "page layout")?;
    let table = bump.alloc(page).ok_or("no room for a page")?;

    require(table.addr().get() & (PAGE - 1) == 0, "page is not aligned")?;

    // Volatile so the round trip is real memory traffic rather than a value the
    // compiler kept in a register. Until this line every check was arithmetic.
    unsafe { table.write_volatile(0xAA) };
    require(
        unsafe { table.read_volatile() } == 0xAA,
        "the write did not stick",
    )
}

/// A made-up arena with a hole in the middle, so the jump runs at every boot
/// rather than only once 112 MiB have been handed out. Nothing here is ever
/// dereferenced; the addresses only have to be arithmetic.
fn check_skip() -> Result<(), &'static str> {
    const BASE: usize = 0x1000;
    const HOLE: usize = 0x3000;

    let mut bump = Bump::new(Region {
        base: BASE,
        size: 4 * PAGE,
    });

    bump.reserve(Region {
        base: HOLE,
        size: PAGE,
    })
    .map_err(|_| "synthetic reservation")?;

    let page = Layout::from_size_align(PAGE, PAGE).map_err(|_| "page layout")?;
    let two_pages = Layout::from_size_align(2 * PAGE, PAGE).map_err(|_| "two-page layout")?;

    require(alloc_at(&mut bump, page)? == BASE, "first page moved")?;

    // Two pages fit neither before the hole nor after it, so this jumps and
    // then fails - the case that must leave the pointer where it found it.
    let before = bump.remaining();
    require(
        bump.alloc(two_pages).is_none(),
        "two pages fit where they cannot",
    )?;
    require(
        bump.remaining() == before,
        "a failed allocation consumed memory",
    )?;

    require(
        alloc_at(&mut bump, page)? == BASE + PAGE,
        "second page moved",
    )?;
    require(
        alloc_at(&mut bump, page)? == HOLE + PAGE,
        "the hole was not stepped over",
    )?;

    require(bump.alloc(page).is_none(), "allocated past the end")
}

fn alloc_at(bump: &mut Bump, layout: Layout) -> Result<usize, &'static str> {
    Ok(bump.alloc(layout).ok_or("allocation failed")?.addr().get())
}

fn require(held: bool, complaint: &'static str) -> Result<(), &'static str> {
    if held { Ok(()) } else { Err(complaint) }
}
