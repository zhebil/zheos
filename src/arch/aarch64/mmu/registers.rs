use core::arch::asm;

// System Control Register
pub fn read_sctlr_el1() -> u64 {
    let sctlr_el1: u64;
    unsafe { asm!("mrs {}, SCTLR_EL1", out(reg) sctlr_el1, options(nostack)) }
    sctlr_el1
}

pub fn write_sctlr_el1(sctlr_el1: u64) {
    unsafe { asm!("msr SCTLR_EL1, {}", in(reg) sctlr_el1, options(nostack)) }
}

// Translation Control Register
#[allow(dead_code)]
pub fn read_tcr_el1() -> u64 {
    let tcr_el1: u64;
    unsafe { asm!("mrs {}, TCR_EL1", out(reg) tcr_el1, options(nostack)) }
    tcr_el1
}

pub fn write_tcr_el1(tcr_el1: u64) {
    unsafe { asm!("msr TCR_EL1, {}", in(reg) tcr_el1, options(nostack)) }
}

// Translation Table Base Register
#[allow(dead_code)]
pub fn read_ttbr0_el1() -> u64 {
    let ttbr0_el1: u64;
    unsafe { asm!("mrs {}, TTBR0_EL1", out(reg) ttbr0_el1, options(nostack)) }
    ttbr0_el1
}

pub fn write_ttbr0_el1(ttbr0_el1: u64) {
    unsafe { asm!("msr TTBR0_EL1, {}", in(reg) ttbr0_el1, options(nostack)) }
}

// Memory Attribute Indirection Register
#[allow(dead_code)]
pub fn read_mair_el1() -> u64 {
    let mair_el1: u64;
    unsafe { asm!("mrs {}, MAIR_EL1", out(reg) mair_el1, options(nostack)) }
    mair_el1
}

pub fn write_mair_el1(mair_el1: u64) {
    unsafe { asm!("msr MAIR_EL1, {}", in(reg) mair_el1, options(nostack)) }
}

// Coprocessor Access Control Register
#[allow(dead_code)]
pub fn read_cpacr_el1() -> u64 {
    let cpacr_el1: u64;
    unsafe { asm!("mrs {}, CPACR_EL1", out(reg) cpacr_el1, options(nostack)) }
    cpacr_el1
}

pub fn data_sync_barrier() {
    unsafe { asm!("dsb ishst", options(nostack, preserves_flags)) }
}

pub fn invalidate_tlb() {
    unsafe { asm!("tlbi vmalle1", options(nostack, preserves_flags)) }
}

pub fn wait_for_invalidate() {
    unsafe { asm!("dsb ish", options(nostack, preserves_flags)) }
}

pub fn instruction_barrier() {
    unsafe { asm!("isb", options(nostack, preserves_flags)) }
}
