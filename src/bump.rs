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

#[cfg(test)]
mod tests {
    use super::*;

    const RAM: Region = Region {
        base: 0x4000_0000,
        size: 128 << 20,
    };

    const PAGE: usize = 4096;

    fn page() -> Layout {
        Layout::from_size_align(PAGE, PAGE).unwrap()
    }

    fn at(base: usize, size: usize) -> Region {
        Region { base, size }
    }

    fn alloc(bump: &mut Bump, layout: Layout) -> usize {
        bump.alloc(layout)
            .expect("expected the allocation to fit")
            .as_ptr() as usize
    }

    #[test]
    fn hands_out_the_start_of_memory_first() {
        let mut bump = Bump::new(RAM);

        assert_eq!(alloc(&mut bump, page()), RAM.base);
    }

    #[test]
    fn every_block_is_aligned_and_inside_memory() {
        let mut bump = Bump::new(RAM);

        for align in [1, 8, 64, PAGE, 1 << 21] {
            let layout = Layout::from_size_align(1, align).unwrap();
            let addr = alloc(&mut bump, layout);

            assert_eq!(addr % align, 0, "align {align}");
            assert!(addr >= RAM.base && addr < RAM.end(), "align {align}");
        }
    }

    #[test]
    fn blocks_never_overlap_each_other() {
        let mut bump = Bump::new(RAM);
        let mut handed_out = [Region::EMPTY; 8];

        for i in 0..handed_out.len() {
            let base = alloc(&mut bump, page());
            let block = at(base, PAGE);

            for earlier in &handed_out[..i] {
                assert!(!block.is_overlapping(earlier), "{block} hit {earlier}");
            }

            handed_out[i] = block;
        }
    }

    #[test]
    fn jumps_over_a_reservation_sitting_at_the_start() {
        let mut bump = Bump::new(RAM);
        bump.reserve(at(RAM.base, PAGE)).ok();

        assert_eq!(alloc(&mut bump, page()), RAM.base + PAGE);
    }

    #[test]
    fn realigns_after_a_jump_lands_mid_page() {
        let mut bump = Bump::new(RAM);
        bump.reserve(at(RAM.base, 100)).ok();

        assert_eq!(alloc(&mut bump, page()), RAM.base + PAGE);
    }

    #[test]
    fn a_reservation_that_only_touches_the_block_does_not_move_it() {
        let mut bump = Bump::new(RAM);
        bump.reserve(at(RAM.base + PAGE, PAGE)).ok();

        assert_eq!(alloc(&mut bump, page()), RAM.base);
    }

    #[test]
    fn a_byte_lands_exactly_where_the_reservation_ends() {
        let mut bump = Bump::new(RAM);
        bump.reserve(at(RAM.base, 100)).ok();

        let byte = Layout::from_size_align(1, 1).unwrap();

        assert_eq!(alloc(&mut bump, byte), RAM.base + 100);
    }

    #[test]
    fn no_block_ever_lands_in_a_reservation() {
        let mut bump = Bump::new(RAM);
        let reserved = [
            at(RAM.base, PAGE),
            at(RAM.base + 3 * PAGE, 2 * PAGE),
            at(RAM.base + 9 * PAGE, PAGE),
        ];
        for r in reserved {
            bump.reserve(r).ok();
        }

        for _ in 0..8 {
            let block = at(alloc(&mut bump, page()), PAGE);

            for r in &reserved {
                assert!(!block.is_overlapping(r), "{block} hit {r}");
            }
        }
    }

    #[test]
    fn survives_the_largest_run_of_jumps_it_can_face() {
        let mut bump = Bump::new(RAM);
        for i in 0..RESERVED_LEN {
            bump.reserve(at(RAM.base + i * PAGE, PAGE)).ok();
        }

        assert_eq!(alloc(&mut bump, page()), RAM.base + RESERVED_LEN * PAGE);
    }

    #[test]
    fn refuses_a_block_that_would_run_off_the_end() {
        let mut bump = Bump::new(at(RAM.base, PAGE));

        assert!(
            bump.alloc(Layout::from_size_align(2 * PAGE, PAGE).unwrap())
                .is_none()
        );
    }

    #[test]
    fn reserve_reports_full_instead_of_dropping_a_region() {
        let mut bump = Bump::new(RAM);
        for i in 0..RESERVED_LEN {
            assert!(
                bump.reserve(at(RAM.base + i * PAGE, PAGE)).is_ok(),
                "at {i}"
            );
        }

        assert!(bump.reserve(at(RAM.base, PAGE)).is_err());
    }
}
