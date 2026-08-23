use crate::dtb::{Dtb, Node, Region};

/// The one address that cannot come from the device tree, because it is what
/// prints the message saying the device tree could not be read.
pub const EARLYCON_UART: usize = 0x0900_0000;

pub const DTB_BASE: usize = 0x4700_0000;

// PSCI (Power State Coordination Interface) function ID for SYSTEM_OFF.
pub const PSCI_SYSTEM_OFF: usize = 0x8400_0008;

// Each doubles as its own error message, so a failed lookup names what it wanted.
const MEMORY: &str = "/memory";
const UART: &str = "arm,pl011";
const GIC: &str = "arm,cortex-a15-gic";
const TIMER: &str = "arm,armv8-timer";

/// The timer's `interrupts` lists secure physical, non-secure physical, virtual
/// and hypervisor. CNTP_* drives the second.
const NON_SECURE_PHYSICAL: usize = 1;

// The device tree numbers each interrupt space from zero; the GIC numbers them
// all in one space, as 0-15 SGI, 16-31 PPI, 32+ SPI.
const SPI: u32 = 0;
const PPI: u32 = 1;
const PPI_BASE: u32 = 16;
const SPI_BASE: u32 = 32;

pub struct Board {
    pub uart: Device,
    pub gic: Gic,
    pub memory: Region,
    pub timer_intid: u32,
}

pub struct Gic {
    pub distributor_base: usize,
    pub cpu_base: usize,
}

pub struct Device {
    pub base: usize,
    pub intid: u32,
}

impl Board {
    pub fn discover(dtb: &Dtb) -> Result<Board, &'static str> {
        let cells = dtb.root_cells().ok_or("/")?;

        let memory = dtb
            .find_memory()
            .ok_or(MEMORY)?
            .region(0, cells)
            .ok_or(MEMORY)?;

        let node = dtb.find_compatible(UART.as_bytes()).ok_or(UART)?;
        let uart = Device {
            base: node.region(0, cells).ok_or(UART)?.base,
            intid: intid(&node, 0).ok_or(UART)?,
        };

        // Two regions, distributor first.
        let node = dtb.find_compatible(GIC.as_bytes()).ok_or(GIC)?;
        let gic = Gic {
            distributor_base: node.region(0, cells).ok_or(GIC)?.base,
            cpu_base: node.region(1, cells).ok_or(GIC)?.base,
        };

        let node = dtb.find_compatible(TIMER.as_bytes()).ok_or(TIMER)?;
        let timer_intid = intid(&node, NON_SECURE_PHYSICAL).ok_or(TIMER)?;


        Ok(Board {
            uart,
            gic,
            memory,
            timer_intid,
        })
    }
}

fn intid(node: &Node, index: usize) -> Option<u32> {
    let (kind, number) = node.interrupt(index)?;

    match kind {
        SPI => Some(number + SPI_BASE),
        PPI => Some(number + PPI_BASE),
        _ => None,
    }
}
