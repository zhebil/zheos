use core::{alloc::Layout, ptr::NonNull};

use crate::{bump::Bump, println, region::Region};

const SLOTS: usize = 512;
const SLOT_MASK: usize = SLOTS - 1;

/// Size and alignment in one type. A table is exactly one 4 KiB page, and every
/// descriptor pointing at one assumes the low twelve bits of its address are zero,
/// so the alignment is a correctness requirement rather than a preference.
#[repr(C, align(4096))]
struct Page([u64; SLOTS]);

/// Where each field sits in a descriptor. Every shift is the low end of that
/// field's range, and no two fields may overlap - which is the whole invariant.
mod bits {
    pub const ATTR_INDEX: u64 = 2;
    pub const NS: u64 = 5;
    pub const AP: u64 = 6;
    pub const SH: u64 = 8;
    pub const AF: u64 = 10;
    pub const NG: u64 = 11;
    pub const CONTIGUOUS: u64 = 52;
    pub const PXN: u64 = 53;
    pub const UXN: u64 = 54;

    /// Bits 47:12. An address ORs straight in - its own bit 12 is already at
    /// position 12 - so there is no shift, only a mask.
    pub const ADDRESS: u64 = 0x0000_FFFF_FFFF_F000;
}

pub struct Table {
    slots: NonNull<u64>,
}

#[derive(Debug, Clone, Copy)]
enum Level {
    Level1 = 1,
    #[allow(dead_code)]
    Level2 = 2,
    #[allow(dead_code)]
    Level3 = 3,
}

impl Level {
    fn next(&self) -> Option<Self> {
        match self {
            Level::Level1 => Some(Level::Level2),
            Level::Level2 => Some(Level::Level3),
            Level::Level3 => None,
        }
    }

    /// Two spaces per level down, so a child table reads as nested under its parent.
    const fn indent(&self) -> &'static str {
        match self {
            Level::Level1 => "",
            Level::Level2 => "  ",
            Level::Level3 => "    ",
        }
    }
}

#[derive(Clone, Copy)]
pub enum Memory {
    DeviceBlock,
    NormalBlock,
}

const L1_BLOCK_SIZE: u64 = 0x4000_0000; // 1 GiB
const L2_BLOCK_SIZE: u64 = 0x200000; // 2 MiB
const L3_BLOCK_SIZE: u64 = 0x1000; // 4 KiB

impl Table {
    /// `Bump` deliberately does not zero, and an untouched slot has to read as
    /// "nothing here", so zeroing is part of construction and not the caller's job.
    pub fn new(bump: &mut Bump) -> Option<Table> {
        let slots = bump.alloc(Layout::new::<Page>())?.cast();
        let mut table = Table { slots };

        for slot in 0..SLOTS {
            table.set(slot, TableValue::ZERO);
        }

        Some(table)
    }

    /// The address the next level up stores, and eventually `TTBR0_EL1`.
    pub fn base(&self) -> usize {
        self.slots.as_ptr() as usize
    }

    pub fn set(&mut self, slot: usize, value: TableValue) {
        // Volatile because once the MMU is on, the hardware table walker reads
        // this memory and the compiler has no idea that it does.
        unsafe {
            self.slots
                .add(slot & SLOT_MASK)
                .write_volatile(value.to_u64())
        }
    }

    pub fn get(&self, slot: usize) -> TableValue {
        TableValue::from_u64(unsafe { self.slots.add(slot & SLOT_MASK).read_volatile() })
    }

    pub fn translate(&self, va: usize) -> Option<usize> {
        self.translate_recursive(va, Level::Level1)
    }

    /// Translate an address to a page table entry
    fn translate_recursive(&self, va: usize, current_level: Level) -> Option<usize> {
        let slot = Self::extract_slot_bits(va, current_level);

        let value = self.get(slot);

        match value.kind {
            TableValueKind::Invalid => None,
            TableValueKind::Block => {
                // The offset is whatever this level does not translate: 30 bits at
                // level 1, 21 at level 2. Using the wrong level here is invisible on
                // an identity map and wrong on every other one.
                let offset = va & Self::offset_mask(current_level);
                Some(value.address as usize | offset)
            }
            TableValueKind::Table => {
                let next_level = current_level.next()?;
                let t = Table::from_base(value.address as usize)?;
                t.translate_recursive(va, next_level)
            }
        }
    }

    pub fn identity_map(
        &mut self,
        bump: &mut Bump,
        region: Region,
        kind: Memory,
    ) -> Result<(), ()> {
        let mut addr = region.base as u64;
        // TODO: find better place for them

        while addr < region.end() as u64 {
            let remaining = region.end() as u64 - addr;
            let l1_slot = Self::extract_slot_bits(addr as usize, Level::Level1);

            if Self::is_aligned(addr, Level::Level1) && remaining >= L1_BLOCK_SIZE {
                self.set(l1_slot, Self::block(addr, kind));
                addr += L1_BLOCK_SIZE;
            } else if Self::is_aligned(addr, Level::Level2) && remaining >= L2_BLOCK_SIZE {
                let mut l2_table = self.child_table(l1_slot, bump).ok_or(())?;
                let l2_slot = Self::extract_slot_bits(addr as usize, Level::Level2);
                l2_table.set(l2_slot, Self::block(addr, kind));

                addr += L2_BLOCK_SIZE;
            } else {
                // Anything finer than 2 MiB needs level 3, which does not exist yet.
                // Falling through and writing a block anyway would either produce a
                // descriptor with address bits below 21 set - which `block` does not
                // mask off - or map past the end of the region.
                return Err(());
            }
        }

        Ok(())
    }

    /// Every non-empty slot, as a range of addresses and what is there. Runs of
    /// consecutive blocks with identical attributes collapse into one line, so the
    /// 64 blocks covering RAM read as one row rather than filling the screen.
    pub fn print_map(&self) {
        println!("va range                      what   memory  perms   -> pa");
        self.print_level(Level::Level1, 0);
    }

    fn print_level(&self, level: Level, va_base: usize) {
        // How much address space one slot at this level covers: 1 GiB, 2 MiB, 4 KiB.
        let step = Self::offset_mask(level) + 1;
        let indent = level.indent();

        let mut slot = 0;
        while slot < SLOTS {
            let value = self.get(slot);
            let base = va_base + slot * step;

            match value.kind {
                TableValueKind::Invalid => slot += 1,
                TableValueKind::Block => {
                    let mut last = slot;
                    while last + 1 < SLOTS
                        && value
                            .continues_into(self.get(last + 1), ((last + 1 - slot) * step) as u64)
                    {
                        last += 1;
                    }

                    println!(
                        "{}L{}[{:3}..{:3}] {:#012x}..{:#012x} block  {:6}  {:6}  -> {:#012x}",
                        indent,
                        level as u8,
                        slot,
                        last,
                        base,
                        base + (last + 1 - slot) * step,
                        value.attr_idx.name(),
                        value.perms(),
                        value.address,
                    );

                    slot = last + 1;
                }
                TableValueKind::Table => {
                    println!(
                        "{}L{}[{:3}     ] {:#012x}..{:#012x} table                  -> {:#012x}",
                        indent,
                        level as u8,
                        slot,
                        base,
                        base + step,
                        value.address,
                    );

                    if let (Some(next), Some(child)) =
                        (level.next(), Table::from_base(value.address as usize))
                    {
                        child.print_level(next, base);
                    }

                    slot += 1;
                }
            }
        }
    }

    pub fn child_table(&mut self, slot: usize, bump: &mut Bump) -> Option<Table> {
        let desc = self.get(slot);
        match desc.kind {
            TableValueKind::Table => Table::from_base(desc.address as usize),
            TableValueKind::Invalid => {
                let new_table = Table::new(bump)?;
                self.set(
                    slot,
                    TableValue {
                        kind: TableValueKind::Table,
                        address: new_table.base() as u64,
                        ..TableValue::ZERO
                    },
                );
                Some(new_table)
            }
            TableValueKind::Block => None,
        }
    }

    /// Rebuild a handle from an address read back out of a descriptor. `None` only
    /// for address zero, which is never a table - so this costs a compare, not a
    /// panic path.
    pub fn from_base(base: usize) -> Option<Table> {
        Some(Table {
            slots: NonNull::new(base as *mut u64)?,
        })
    }

    fn is_aligned(addr: u64, level: Level) -> bool {
        let size = match level {
            Level::Level1 => L1_BLOCK_SIZE,
            Level::Level2 => L2_BLOCK_SIZE,
            Level::Level3 => L3_BLOCK_SIZE,
        };

        addr & (size - 1) == 0
    }

    pub fn block(addr: u64, kind: Memory) -> TableValue {
        let base = match kind {
            Memory::DeviceBlock => DEVICE_BLOCK,
            Memory::NormalBlock => NORMAL_BLOCK,
        };

        TableValue {
            address: addr,
            ..base
        }
    }

    const fn level_offset(level: Level) -> u8 {
        12 + 9 * (3 - level as u8)
    }

    const fn extract_slot_bits(va: usize, level: Level) -> usize {
        (va >> Self::level_offset(level)) & 0x1FF
    }

    const fn offset_mask(level: Level) -> usize {
        (1 << Self::level_offset(level)) - 1
    }
}

#[derive(Clone, Copy)]
pub enum TableValueKind {
    /// Bits `00` and `10`. Not an error - it is how a slot says nothing is mapped.
    Invalid,
    /// A leaf above level 3: 1 GiB at level 1, 2 MiB at level 2.
    Block,
    /// At level 1 or 2, the address of the table below. At level 3 the same
    /// encoding means a 4 KiB page instead.
    Table,
}

impl TableValueKind {
    const fn from_u64(value: u64) -> Self {
        match value & 0b11 {
            0b01 => Self::Block,
            0b11 => Self::Table,
            _ => Self::Invalid,
        }
    }

    const fn to_u64(self) -> u64 {
        match self {
            Self::Invalid => 0b00,
            Self::Block => 0b01,
            Self::Table => 0b11,
        }
    }
}

/// Which of the eight memory types in MAIR_EL1 applies.
#[derive(Clone, Copy)]
pub enum AttrIndex {
    Normal,
    Device,
    Other(u8),
}

impl AttrIndex {
    const fn from_u64(value: u64) -> Self {
        match (value >> bits::ATTR_INDEX) & 0b111 {
            0 => Self::Normal,
            1 => Self::Device,
            other => Self::Other(other as u8),
        }
    }

    const fn to_u64(self) -> u64 {
        match self {
            Self::Normal => 0,
            Self::Device => 1,
            Self::Other(other) => other as u64 & 0b111,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Device => "device",
            Self::Other(_) => "other",
        }
    }
}

/// AP\[2:1]. Note that execute permission is not in here - that is PXN and UXN.
#[derive(Clone, Copy)]
pub enum AccessPermissions {
    KernelReadWrite = 0b00,
    AllReadWrite = 0b01,
    KernelReadOnly = 0b10,
    AllReadOnly = 0b11,
}

impl AccessPermissions {
    const fn from_u64(value: u64) -> Self {
        // Two bits, four variants: the final arm is `0b11`, not a fallback.
        match (value >> bits::AP) & 0b11 {
            0b00 => Self::KernelReadWrite,
            0b01 => Self::AllReadWrite,
            0b10 => Self::KernelReadOnly,
            _ => Self::AllReadOnly,
        }
    }

    const fn to_u64(self) -> u64 {
        self as u64
    }
}

/// Ignored for Device memory, which is never cached in the first place.
#[derive(Clone, Copy)]
pub enum SH {
    NonShareable = 0b00,
    Reserved = 0b01,
    OuterShareable = 0b10,
    InnerShareable = 0b11,
}

impl SH {
    const fn from_u64(value: u64) -> Self {
        match (value >> bits::SH) & 0b11 {
            0b00 => Self::NonShareable,
            0b01 => Self::Reserved,
            0b10 => Self::OuterShareable,
            _ => Self::InnerShareable,
        }
    }

    const fn to_u64(self) -> u64 {
        self as u64
    }
}

#[derive(Clone, Copy)]
pub struct TableValue {
    pub kind: TableValueKind,
    pub attr_idx: AttrIndex,
    pub ns: bool,
    pub ap: AccessPermissions,
    pub sh: SH,
    pub af: bool,
    pub ng: bool,
    /// The physical address itself, not a page number. A level 1 block needs its
    /// low 30 bits zero, a level 2 block its low 21, a level 3 page its low 12.
    pub address: u64,
    pub contig: bool,
    pub pxn: bool,
    pub uxn: bool,
}

impl TableValue {
    pub const ZERO: TableValue = TableValue::from_u64(0);

    pub const fn from_u64(value: u64) -> Self {
        TableValue {
            kind: TableValueKind::from_u64(value),
            attr_idx: AttrIndex::from_u64(value),
            ns: value & (1 << bits::NS) != 0,
            ap: AccessPermissions::from_u64(value),
            sh: SH::from_u64(value),
            af: value & (1 << bits::AF) != 0,
            ng: value & (1 << bits::NG) != 0,
            address: value & bits::ADDRESS,
            contig: value & (1 << bits::CONTIGUOUS) != 0,
            pxn: value & (1 << bits::PXN) != 0,
            uxn: value & (1 << bits::UXN) != 0,
        }
    }

    /// What the kernel may do here. `!AF` is shouted rather than abbreviated because
    /// a clear access flag faults on every single access, and it is the easiest bit
    /// in a descriptor to forget.
    const fn perms(self) -> &'static str {
        if !self.af {
            return "!AF";
        }

        match (self.ap, self.pxn) {
            (AccessPermissions::KernelReadWrite, false) => "rwx",
            (AccessPermissions::KernelReadWrite, true) => "rw",
            (AccessPermissions::AllReadWrite, false) => "rwx/u",
            (AccessPermissions::AllReadWrite, true) => "rw/u",
            (AccessPermissions::KernelReadOnly, false) => "rx",
            (AccessPermissions::KernelReadOnly, true) => "r",
            (AccessPermissions::AllReadOnly, false) => "rx/u",
            (AccessPermissions::AllReadOnly, true) => "r/u",
        }
    }

    /// True when `next` is the same kind of block with the same attributes, sitting
    /// `delta` further along in physical memory - which is what makes two rows of the
    /// map safe to print as one.
    const fn continues_into(self, next: TableValue, delta: u64) -> bool {
        matches!(next.kind, TableValueKind::Block)
            && next.to_u64() & !bits::ADDRESS == self.to_u64() & !bits::ADDRESS
            && next.address == self.address + delta
    }

    pub const fn to_u64(self) -> u64 {
        self.kind.to_u64()
            | self.attr_idx.to_u64() << bits::ATTR_INDEX
            | (self.ns as u64) << bits::NS
            | self.ap.to_u64() << bits::AP
            | self.sh.to_u64() << bits::SH
            | (self.af as u64) << bits::AF
            | (self.ng as u64) << bits::NG
            | (self.address & bits::ADDRESS)
            | (self.contig as u64) << bits::CONTIGUOUS
            | (self.pxn as u64) << bits::PXN
            | (self.uxn as u64) << bits::UXN
    }
}

/// Slot 0 of the level 1 table: 0x0000_0000 .. 0x4000_0000, every device on the
/// machine
pub const DEVICE_BLOCK: TableValue = TableValue {
    kind: TableValueKind::Block,
    attr_idx: AttrIndex::Device,
    ns: false,
    ap: AccessPermissions::KernelReadWrite,
    sh: SH::NonShareable,
    af: true,
    ng: false,
    address: 0x0000_0000,
    contig: false,
    pxn: true,
    uxn: true,
};

/// Slot 1: 0x4000_0000 .. 0x8000_0000, all of RAM. PXN is clear because the
/// kernel's own code is in here.
pub const NORMAL_BLOCK: TableValue = TableValue {
    kind: TableValueKind::Block,
    attr_idx: AttrIndex::Normal,
    ns: false,
    ap: AccessPermissions::KernelReadWrite,
    sh: SH::InnerShareable,
    af: true,
    ng: false,
    address: 0x4000_0000,
    contig: false,
    pxn: false,
    uxn: true,
};

// Checked when the constants are evaluated, so a wrong shift is a build failure
// rather than a number that has to be noticed on the console.
const _: () = assert!(DEVICE_BLOCK.to_u64() == 0x0060_0000_0000_0405);
const _: () = assert!(NORMAL_BLOCK.to_u64() == 0x0040_0000_4000_0701);

// And that decoding inverts encoding, which is the bug a single direction hides.
const _: () =
    assert!(TableValue::from_u64(DEVICE_BLOCK.to_u64()).to_u64() == DEVICE_BLOCK.to_u64());
const _: () =
    assert!(TableValue::from_u64(NORMAL_BLOCK.to_u64()).to_u64() == NORMAL_BLOCK.to_u64());
