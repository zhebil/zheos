use crate::{print, println, uart};
use core::{
    arch::{asm, global_asm},
    fmt::Write,
    sync::atomic::{AtomicBool, Ordering},
};

global_asm!(include_str!("vectors.s"));

unsafe extern "C" {
    static vector_table: u8;
}

pub fn install_vectors() {
    unsafe {
        let base = &raw const vector_table as u64;
        asm!(
            "msr vbar_el1, {base}",
            "isb",
            base = in(reg) base,
            options(nomem, nostack, preserves_flags),
        );
    }
}

mod esr {
    pub const UNKNOWN: u64 = 0x00;
    pub const DATA_ABORT_CURRENT_EL: u64 = 0x25;
    pub const INSTR_ABORT_CURRENT_EL: u64 = 0x21;
    pub const BRK: u64 = 0x3C;
}

pub enum ESRClass {
    Unknown,
    DataAbortCurrentEL,
    InstrAbortCurrentEL,
    Brk,
    Other(u8),
}

impl ESRClass {
    /// FAR_EL1 holds stale data for anything that was not a memory access.
    const fn has_fault_address(&self) -> bool {
        matches!(self, Self::DataAbortCurrentEL | Self::InstrAbortCurrentEL)
    }
}

impl From<u64> for ESRClass {
    fn from(val: u64) -> Self {
        match val {
            esr::UNKNOWN => Self::Unknown,
            esr::DATA_ABORT_CURRENT_EL => Self::DataAbortCurrentEL,
            esr::INSTR_ABORT_CURRENT_EL => Self::InstrAbortCurrentEL,
            esr::BRK => Self::Brk,
            _ => Self::Other(val as u8),
        }
    }
}

struct ESR {
    class: ESRClass,
    syndrome: u64,
}

impl ESR {
    fn new(val: u64) -> Self {
        Self {
            class: ESRClass::from((val >> 26) & 0x3F),
            syndrome: val & 0x1FFFFFF,
        }
    }
}

static ALREADY_FAULTED: AtomicBool = AtomicBool::new(false);

#[unsafe(no_mangle)]
pub extern "C" fn handle_exception(
    esr_el1: u64, /* Exception Syndrome Register */
    far_el1: u64, /* Fault Address Register */
    elr_el1: u64, /* Exception Link Register */
    kind: u64,    /* 0 if synchronous, 1 otherwise */
    frame: *const [u64; 31],
) -> ! {
    let af = ALREADY_FAULTED.load(Ordering::Relaxed);

    // The second fault already overwrote ESR/FAR/ELR, so there is nothing left
    // worth reporting - only the fact that it happened.
    if af {
        for byte in b"\r\n!! DOUBLE FAULT\r\n" {
            uart().putc(*byte);
        }
    } else {
        ALREADY_FAULTED.store(true, Ordering::Relaxed);

        println!(
            "Kind: {}",
            if kind == 1 { "unexpected slot" } else { "sync" }
        );

        let esr = ESR::new(esr_el1);
        println!("Exception Syndrome Register: {:#018x}", esr_el1);
        print!("Exception Syndrome Class: ");
        match &esr.class {
            ESRClass::Unknown => {
                println!("Unknown reason (undefined instruction)");
            }
            ESRClass::DataAbortCurrentEL => {
                println!("Data Abort Current EL");
            }
            ESRClass::InstrAbortCurrentEL => {
                println!("Instruction Abort Current EL");
            }
            ESRClass::Brk => {
                println!("Breakpoint");
            }
            ESRClass::Other(val) => {
                println!("Other {:#04x}", val);
            }
        }
        println!("Exception Syndrome Syndrome: {:#018x}", esr.syndrome);

        if esr.class.has_fault_address() {
            println!("Fault Address Register: {:#018x}", far_el1);
        }

        println!("Exception Link Register: {:#018x}", elr_el1);

        for (i, value) in unsafe { &*frame }.iter().enumerate() {
            println!("x{}: {:#018x}", i, value);
        }
        uart().flush();
    }

    loop {
        unsafe { asm!("wfi") } // wait for interrupts
    }
}
