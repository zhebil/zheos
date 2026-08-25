use crate::cpu;

// Deliberately allowed "use" or higher level layers as exception to the rule.
use crate::uart::uart;
use crate::{print, println};

use core::{
    arch::{asm, global_asm},
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

/// The top four bits of a fault status code. The bottom two are the level the
/// table walk had reached, which is why these are compared after a shift.
mod dfsc {
    pub const ADDRESS_SIZE: u64 = 0b0000;
    pub const TRANSLATION: u64 = 0b0001;
    pub const ACCESS_FLAG: u64 = 0b0010;
    pub const PERMISSION: u64 = 0b0011;

    /// These carry no level, so they are matched whole.
    pub const EXTERNAL: u64 = 0x10;
    pub const ALIGNMENT: u64 = 0x21;
    pub const TLB_CONFLICT: u64 = 0x30;
}

/// Bits 5:0 of a data or instruction abort's syndrome - what the MMU objected to.
/// The four faults that carry a level report which table the walk died in, which
/// is the difference between "I never mapped it" and "I mapped it in the wrong
/// table".
enum FaultStatus {
    AddressSize(u8),
    Translation(u8),
    AccessFlag(u8),
    Permission(u8),
    External,
    Alignment,
    TlbConflict,
    Other(u8),
}

impl FaultStatus {
    const fn from_syndrome(syndrome: u64) -> Self {
        let code = syndrome & 0x3F;
        let level = (code & 0b11) as u8;

        match code >> 2 {
            dfsc::ADDRESS_SIZE => Self::AddressSize(level),
            dfsc::TRANSLATION => Self::Translation(level),
            dfsc::ACCESS_FLAG => Self::AccessFlag(level),
            dfsc::PERMISSION => Self::Permission(level),
            _ => match code {
                dfsc::EXTERNAL => Self::External,
                dfsc::ALIGNMENT => Self::Alignment,
                dfsc::TLB_CONFLICT => Self::TlbConflict,
                other => Self::Other(other as u8),
            },
        }
    }
}

struct Esr {
    class: ESRClass,
    syndrome: u64,
}

impl Esr {
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

        let esr = Esr::new(esr_el1);
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
            print!("Fault: ");
            match FaultStatus::from_syndrome(esr.syndrome) {
                FaultStatus::AddressSize(level) => {
                    println!(
                        "address size, level {} - address is outside the range TCR_EL1 was told to translate",
                        level
                    );
                }
                FaultStatus::Translation(level) => {
                    println!("translation, level {} - nothing is mapped there", level);
                }
                FaultStatus::AccessFlag(level) => {
                    println!(
                        "access flag, level {} - mapped, but bit 10 of the descriptor is clear",
                        level
                    );
                }
                FaultStatus::Permission(level) => {
                    println!(
                        "permission, level {} - mapped, but not for this kind of access",
                        level
                    );
                }
                FaultStatus::External => {
                    println!("external abort - nothing answered on the bus");
                }
                FaultStatus::Alignment => {
                    println!(
                        "alignment - unaligned access. With the MMU off every address is Device memory, which forbids it"
                    );
                }
                FaultStatus::TlbConflict => {
                    println!("TLB conflict - stale entries, a table changed without an invalidate");
                }
                FaultStatus::Other(code) => {
                    println!("unknown status {:#04x}", code);
                }
            }

            // Bit 6 is WnR, and it only means anything for a data abort - an
            // instruction abort is always a fetch.
            if matches!(esr.class, ESRClass::DataAbortCurrentEL) {
                println!(
                    "Access: {}",
                    if esr.syndrome & (1 << 6) != 0 {
                        "write"
                    } else {
                        "read"
                    }
                );
            }

            println!("Fault Address Register: {:#018x}", far_el1);
        }

        println!("Exception Link Register: {:#018x}", elr_el1);

        for (i, value) in unsafe { &*frame }.iter().enumerate() {
            println!("x{}: {:#018x}", i, value);
        }
        uart().flush();
    }

    loop {
        cpu::wait_for_interrupt();
    }
}
