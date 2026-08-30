use core::{alloc::Layout, fmt::Display, ptr::NonNull};

use crate::{
    heap::Heap,
    memory::{pfn::PAGE_SIZE, region::Region},
    mmu::{
        descriptor::{Descriptor, Kind},
        level::Level,
    },
};

pub mod descriptor;
mod init;
pub mod level;

const SLOTS: usize = 512;
const SLOT_MASK: usize = SLOTS - 1;

/// A table is exactly one page, and every descriptor pointing at one assumes the
/// low twelve bits of its address are zero. That is asked for as an alignment
/// rather than assumed, so the guarantee is in the request - but only while the
/// two sizes agree.
const _: () = assert!(SLOTS * size_of::<u64>() == PAGE_SIZE);

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
    pub fn new(heap: &mut Heap) -> Option<Table> {
        let layout = Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).ok()?;
        let slots = NonNull::new(heap.alloc_layout(layout)? as *mut u64)?;
        let mut table = Table { slots };

        // Clear garbage that was left in memory.
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
        heap: &mut Heap,
        region: Region,
        template: Descriptor,
    ) -> Result<(), MapError> {
        self.map_range(heap, region, template, Level::Level1)
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
    fn child_table(&mut self, heap: &mut Heap, va: usize, level: Level) -> Result<Table, MapError> {
        let slot = level.slot_of(va);
        let descriptor = self.get(slot);

        match descriptor.kind {
            // Table already was allocated. Reusing it.
            Kind::Table => Table::from_base(descriptor.address).ok_or(MapError::OutOfMemory),
            Kind::Invalid => {
                let child = Table::new(heap).ok_or(MapError::OutOfMemory)?;

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
        heap: &mut Heap,
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

                self.child_table(heap, addr, level)?.map_range(
                    heap,
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
