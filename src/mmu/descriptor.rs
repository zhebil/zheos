use crate::mmu::level::Level;

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

#[derive(Clone, Copy)]
pub struct Descriptor {
    pub kind: Kind,
    pub attr_idx: AttrIndex,
    pub ns: bool,
    pub ap: AccessPermissions,
    pub sh: SH,
    pub af: bool,
    pub ng: bool,
    /// The physical address itself, not a page number. A level 1 block needs its
    /// low 30 bits zero, a level 2 block its low 21, a level 3 page its low 12.
    pub address: usize,
    pub contig: bool,
    pub pxn: bool,
    pub uxn: bool,
}

impl Descriptor {
    pub const ZERO: Descriptor = Descriptor::from_u64(0);
    pub const DEVICE_BLOCK: Descriptor = Descriptor {
        kind: Kind::Block,
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
    pub const NORMAL_BLOCK: Descriptor = Descriptor {
        kind: Kind::Block,
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

    pub const fn from_u64(value: u64) -> Self {
        Descriptor {
            kind: Kind::from_u64(value),
            attr_idx: AttrIndex::from_u64(value),
            ns: value & (1 << bits::NS) != 0,
            ap: AccessPermissions::from_u64(value),
            sh: SH::from_u64(value),
            af: value & (1 << bits::AF) != 0,
            ng: value & (1 << bits::NG) != 0,
            address: (value & bits::ADDRESS) as usize,
            contig: value & (1 << bits::CONTIGUOUS) != 0,
            pxn: value & (1 << bits::PXN) != 0,
            uxn: value & (1 << bits::UXN) != 0,
        }
    }

    pub const fn to_u64(self) -> u64 {
        self.kind.to_u64()
            | self.attr_idx.to_u64() << bits::ATTR_INDEX
            | (self.ns as u64) << bits::NS
            | self.ap.to_u64() << bits::AP
            | self.sh.to_u64() << bits::SH
            | (self.af as u64) << bits::AF
            | (self.ng as u64) << bits::NG
            | (self.address as u64 & bits::ADDRESS)
            | (self.contig as u64) << bits::CONTIGUOUS
            | (self.pxn as u64) << bits::PXN
            | (self.uxn as u64) << bits::UXN
    }
}

#[derive(Clone, Copy)]
pub enum Kind {
    /// Bits `00` and `10`. Not an error - it is how a slot says nothing is mapped.
    Invalid,
    /// A leaf above level 3
    Block,
    /// At level 1 or 2 - the address of the table below.
    /// At level 3 - 4KiB page address
    Table,
}

impl Kind {
    /// What bits\[1:0] of a leaf have to say at `level`. `0b01` is a block at
    /// levels 1 and 2 and *invalid* at level 3, where a leaf is a page and uses
    /// `0b11` - the same encoding that means "table" further up.
    pub const fn from_level(level: Level) -> Self {
        match level {
            Level::Level1 | Level::Level2 => Self::Block,
            Level::Level3 => Self::Table,
        }
    }

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
