use core::{alloc::Layout, fmt::Display, ptr::NonNull};

use crate::{
    bump::Bump,
    mmu::{
        descriptor::{Descriptor, Kind},
        level::Level,
    },
    region::Region,
};

pub mod descriptor;
mod init;
pub mod level;

const SLOTS: usize = 512;
const SLOT_MASK: usize = SLOTS - 1;

/// Size and alignment in one type. A table is exactly one 4 KiB page, and every
/// descriptor pointing at one assumes the low twelve bits of its address are zero,
/// so the alignment is a correctness requirement rather than a preference.
#[repr(C, align(4096))]
struct Page([u64; SLOTS]);

pub enum MapError {
    OutOfMemory,
    Unaligned(usize),
    BlockInTheWay(usize, Level),
}

impl Display for MapError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::OutOfMemory => write!(f, "no room for another table"),
            Self::Unaligned(va) => write!(f, "{va:#012x} is finer than a 4 KiB page"),
            Self::BlockInTheWay(va, level) => {
                write!(
                    f,
                    "{va:#012x} is already inside a level {} block",
                    *level as u8
                )
            }
        }
    }
}

pub struct Table {
    slots: NonNull<u64>,
}

impl Table {
    pub fn new(bump: &mut Bump) -> Option<Table> {
        let slots = bump.alloc(Layout::new::<Page>())?.cast();
        let mut table = Table { slots };

        // Zero table since Bump does not guarantee zeroed memory.
        for slot in 0..SLOTS {
            table.set(slot, Descriptor::ZERO);
        }

        Some(table)
    }

    pub fn base(&self) -> usize {
        self.slots.as_ptr() as usize
    }

    /// Map `region` so that every virtual address in it equals its own physical
    /// address, with `template` supplying everything but the address and the kind.
    pub fn identity_map(
        &mut self,
        bump: &mut Bump,
        region: Region,
        template: Descriptor,
    ) -> Result<(), MapError> {
        self.map_range(bump, region, template, Level::Level1)
    }

    /// Walk the table the way the hardware would and report where `va` lands, or
    /// `None` if nothing is mapped there.
    pub fn translate(&self, va: usize) -> Option<usize> {
        self.translate_at(va, Level::Level1)
    }

    fn set(&mut self, slot: usize, value: Descriptor) {
        // Volatile because once the MMU is on, the hardware table walker reads
        // this memory and the compiler has no idea that it does.
        unsafe {
            self.slots
                .add(slot & SLOT_MASK)
                .write_volatile(value.to_u64())
        }
    }

    fn get(&self, slot: usize) -> Descriptor {
        Descriptor::from_u64(unsafe { self.slots.add(slot & SLOT_MASK).read_volatile() })
    }

    /// Rebuild a handle from an address read back out of a descriptor. `None` only
    /// for address zero
    fn from_base(base: usize) -> Option<Table> {
        Some(Table {
            slots: NonNull::new(base as *mut u64)?,
        })
    }

    /// The table below the slot `va` falls in, allocated and linked if it is not
    /// there yet.
    fn child_table(&mut self, bump: &mut Bump, va: usize, level: Level) -> Result<Table, MapError> {
        let slot = level.slot_of(va);
        let descriptor = self.get(slot);

        match descriptor.kind {
            // Table already was allocated. Reusing it.
            Kind::Table => Table::from_base(descriptor.address).ok_or(MapError::OutOfMemory),
            Kind::Invalid => {
                let child = Table::new(bump).ok_or(MapError::OutOfMemory)?;

                self.set(
                    slot,
                    Descriptor {
                        kind: Kind::Table,
                        address: child.base(),
                        ..Descriptor::ZERO
                    },
                );

                Ok(child)
            }
            // Splitting a block into a table means rewriting a mapping that is
            // already live, which nothing here is allowed to do.
            Kind::Block => Err(MapError::BlockInTheWay(va, level)),
        }
    }

    /// One level of the walk. Each pass covers exactly one slot.
    fn map_range(
        &mut self,
        bump: &mut Bump,
        region: Region,
        template: Descriptor,
        level: Level,
    ) -> Result<(), MapError> {
        let mut addr = region.base;

        while addr < region.end() {
            let slot_end = (addr & !level.offset_mask()) + level.size();
            let chunk_end = slot_end.min(region.end());

            if level.is_aligned(addr) && chunk_end == slot_end {
                // Chunk fills the slot exactly, so one leaf covers it.
                self.set(
                    level.slot_of(addr),
                    Descriptor {
                        kind: Kind::from_level(level),
                        address: addr,
                        ..template
                    },
                );
            } else {
                // A partial slot. Whatever falls in it needs a finer table, and
                // running out of levels is the only way an address can be too
                // fine to map at all.
                let next = level.next().ok_or(MapError::Unaligned(addr))?;

                self.child_table(bump, addr, level)?.map_range(
                    bump,
                    Region {
                        base: addr,
                        size: chunk_end - addr,
                    },
                    template,
                    next,
                )?;
            }

            addr = chunk_end;
        }

        Ok(())
    }

    fn translate_at(&self, va: usize, level: Level) -> Option<usize> {
        let descriptor = self.get(level.slot_of(va));

        let offset = va & level.offset_mask();

        match descriptor.kind {
            Kind::Invalid => None,
            Kind::Block => Some(descriptor.address | offset),
            Kind::Table => match level.next() {
                // At level 3 the same two bits mean a page, so this is a leaf and
                // the walk stops here rather than following the address down.
                None => Some(descriptor.address | offset),
                Some(next) => Table::from_base(descriptor.address)?.translate_at(va, next),
            },
        }
    }
}

pub fn enable(table: &mut Table) {
    init::init(table);
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: usize = 4096;
    const MIB: usize = 1 << 20;
    const GIB: usize = 1 << 30;

    const RAM: usize = 0x4000_0000;
    const DEVICES: usize = 0x0900_0000;

    /// Real, page-aligned memory for the tables to live in. Deliberately leaked -
    /// the process ends before it matters, and nothing here frees a table anyway.
    fn arena(pages: usize) -> Region {
        let size = pages * PAGE;
        let layout = Layout::from_size_align(size, PAGE).unwrap();
        let base = unsafe { std::alloc::alloc_zeroed(layout) };

        assert!(!base.is_null());

        Region {
            base: base as usize,
            size,
        }
    }

    fn table(pages: usize) -> (Bump, Table) {
        let mut bump = Bump::new(arena(pages));
        let table = Table::new(&mut bump).expect("no room for a root table");

        (bump, table)
    }

    fn region(base: usize, size: usize) -> Region {
        Region { base, size }
    }

    fn map(table: &mut Table, bump: &mut Bump, region: Region) {
        if let Err(error) = table.identity_map(bump, region, Descriptor::NORMAL_BLOCK) {
            panic!("{error}");
        }
    }

    #[test]
    fn every_address_in_an_identity_map_translates_to_itself() {
        let (mut bump, mut table) = table(16);

        map(&mut table, &mut bump, region(RAM, 128 * MIB));

        for va in [RAM, RAM + 1, RAM + 64 * MIB, RAM + 128 * MIB - 1] {
            assert_eq!(table.translate(va), Some(va), "{va:#x}");
        }
    }

    #[test]
    fn nothing_outside_the_map_translates() {
        let (mut bump, mut table) = table(16);

        map(&mut table, &mut bump, region(RAM, 128 * MIB));

        for va in [0, DEVICES, RAM - 1, RAM + 128 * MIB, RAM + GIB] {
            assert_eq!(table.translate(va), None, "{va:#x}");
        }
    }

    #[test]
    fn the_whole_device_range_fits_in_one_slot() {
        let (mut bump, mut table) = table(16);
        let before = bump.remaining();

        map(&mut table, &mut bump, region(0, RAM));

        assert_eq!(before - bump.remaining(), 0);
        assert_eq!(table.translate(DEVICES), Some(DEVICES));
    }

    #[test]
    fn a_region_that_fills_its_slot_needs_no_table_below_it() {
        let (mut bump, mut table) = table(16);
        let before = bump.remaining();

        map(&mut table, &mut bump, region(RAM, GIB));

        assert_eq!(before - bump.remaining(), 0);
    }

    #[test]
    fn a_region_smaller_than_a_slot_builds_the_level_below() {
        let (mut bump, mut table) = table(16);
        let before = bump.remaining();

        map(&mut table, &mut bump, region(RAM, 128 * MIB));

        assert_eq!(before - bump.remaining(), PAGE);
    }

    #[test]
    fn a_page_sized_region_walks_all_the_way_to_level_3() {
        let (mut bump, mut table) = table(16);
        let before = bump.remaining();

        map(&mut table, &mut bump, region(RAM, PAGE));

        assert_eq!(before - bump.remaining(), 2 * PAGE);
    }

    #[test]
    fn a_leaf_at_level_3_translates_even_though_it_is_spelled_like_a_table() {
        let (mut bump, mut table) = table(16);

        map(&mut table, &mut bump, region(RAM, PAGE));

        assert_eq!(table.translate(RAM), Some(RAM));
        assert_eq!(table.translate(RAM + PAGE - 1), Some(RAM + PAGE - 1));
        assert_eq!(table.translate(RAM + PAGE), None);
    }

    #[test]
    fn a_second_region_in_the_same_slot_reuses_the_table_below() {
        let (mut bump, mut table) = table(16);

        map(&mut table, &mut bump, region(RAM, 2 * MIB));
        let after_first = bump.remaining();
        map(&mut table, &mut bump, region(RAM + 2 * MIB, 2 * MIB));

        assert_eq!(after_first - bump.remaining(), 0);
        assert_eq!(table.translate(RAM), Some(RAM));
        assert_eq!(table.translate(RAM + 2 * MIB), Some(RAM + 2 * MIB));
    }

    #[test]
    fn a_region_finer_than_a_page_is_refused() {
        let (mut bump, mut table) = table(16);

        let result = table.identity_map(&mut bump, region(RAM, 100), Descriptor::NORMAL_BLOCK);

        assert!(matches!(result, Err(MapError::Unaligned(RAM))));
    }

    #[test]
    fn mapping_on_top_of_a_live_block_is_refused() {
        let (mut bump, mut table) = table(16);
        map(&mut table, &mut bump, region(RAM, GIB));

        let result = table.identity_map(&mut bump, region(RAM, PAGE), Descriptor::NORMAL_BLOCK);

        assert!(matches!(
            result,
            Err(MapError::BlockInTheWay(RAM, Level::Level1))
        ));
    }

    #[test]
    fn running_out_of_room_for_tables_is_reported() {
        let (mut bump, mut table) = table(2);

        let result = table.identity_map(&mut bump, region(RAM, PAGE), Descriptor::NORMAL_BLOCK);

        assert!(matches!(result, Err(MapError::OutOfMemory)));
    }

    #[test]
    fn an_address_above_the_39_bit_space_aliases_into_it() {
        let (mut bump, mut table) = table(16);

        map(&mut table, &mut bump, region(RAM, GIB));

        assert_eq!(table.translate(RAM + (1 << 39)), Some(RAM));
    }
}
