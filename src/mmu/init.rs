use crate::{
    cpu::mmu::{
        data_sync_barrier, instruction_barrier, invalidate_tlb, read_sctlr_el1,
        wait_for_invalidate, write_mair_el1, write_sctlr_el1, write_tcr_el1, write_ttbr0_el1,
    },
    mmu::Table,
};

mod bits {
    pub const T0SZ: u64 = 0;
    pub const IRGN0: u64 = 8;
    pub const ORGN0: u64 = 10;
    pub const SH0: u64 = 12;
    pub const TG0: u64 = 14;
    pub const EPD1: u64 = 23;
    pub const TG1: u64 = 30;
    pub const IPS: u64 = 32;
}

const NORMAL_MEMORY: u64 = 0xFF;
const DEVICE_MEMORY: u64 = 0x00;

const NORMAL_SLOT: u64 = 0;
const DEVICE_SLOT: u64 = 1;

const MAIR_EL1: u64 = (NORMAL_MEMORY << (NORMAL_SLOT * 8)) | (DEVICE_MEMORY << (DEVICE_SLOT * 8));

const VA_BITS: u64 = 39;
const T0SZ: u64 = 64 - VA_BITS;

const WRITE_BACK: u64 = 0b01;
const INNER_SHAREABLE: u64 = 0b11;
const GRANULE_4KIB_TTBR0: u64 = 0b00;
const GRANULE_4KIB_TTBR1: u64 = 0b10;
const PHYSICAL_40_BITS: u64 = 0b010;
const WALK_DISABLED: u64 = 1;

const TCR_EL1: u64 = (T0SZ << bits::T0SZ)
    | (WRITE_BACK << bits::IRGN0)
    | (WRITE_BACK << bits::ORGN0)
    | (INNER_SHAREABLE << bits::SH0)
    | (GRANULE_4KIB_TTBR0 << bits::TG0)
    | (WALK_DISABLED << bits::EPD1)
    | (GRANULE_4KIB_TTBR1 << bits::TG1)
    | (PHYSICAL_40_BITS << bits::IPS);

const SCTLR_M: u64 = 1 << 0;
const SCTLR_C: u64 = 1 << 2;
const SCTLR_I: u64 = 1 << 12;
const SCTLR_EL1: u64 = SCTLR_M | SCTLR_C | SCTLR_I;

pub fn init(table: &mut Table) {
    let t_base = table.base();

    data_sync_barrier();

    invalidate_tlb();

    wait_for_invalidate();

    // set mair, tcr, ttbr0
    write_mair_el1(MAIR_EL1);
    write_tcr_el1(TCR_EL1);
    write_ttbr0_el1(t_base as u64);

    instruction_barrier();

    // enable mmu
    let sctlr_el1 = read_sctlr_el1();
    write_sctlr_el1(sctlr_el1 | SCTLR_EL1);

    instruction_barrier();
}
