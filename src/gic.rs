use crate::uart;
use core::{arch::asm, fmt::Write};

mod distributor {
    use crate::mem;

    // Base address of GIC Distributor
    const BASE: u64 = 0x800_0000;

    // Offset of GIC Distributor registers
    const CTLR: u64 = 0x0;

    // Register containing information about the GIC Distributor
    const TYPER: u64 = 0x4;

    // Mask for the number of interrupt lines
    const IT_LINES_NUMBER_MASK: u32 = 0x1F;

    // Number of interrupt lines per block
    const LINES_PER_BLOCK: u32 = 32;

    #[derive(Debug)]
    pub enum InitError {
        InvalidTyper,
    }

    pub fn init() -> Result<(), InitError> {
        let typer = mem::read_32(BASE + TYPER);

        let it_lines_number = typer & IT_LINES_NUMBER_MASK;

        // Number of interrupt lines
        let n = LINES_PER_BLOCK * (it_lines_number + 1);

        // Validate that the number of interrupt lines is within the valid range
        if n == 0 || n > 1020 {
            return Err(InitError::InvalidTyper);
        }

        // Enable distributor
        mem::write_32(BASE + CTLR, 1);
        Ok(())
    }
}

mod cpu {
    use crate::mem;

    // Base address of GIC CPU Interface
    const BASE: u64 = 0x801_0000;

    // Offset of GIC CPU Interface control register
    const CTLR: u64 = 0x0;

    // Offset of GIC CPU Interface priority mask register
    const PMR: u64 = 0x4;

    // Offset of GIC CPU Interface interrupt acknowledge register
    const IAR: u64 = 0x0C;

    // Mask for the interrupt number
    const IRQ_MASK: u32 = 0x3FF;

    const EOIR: u64 = 0x10;

    pub const SPURIOUS_INTERRUPT_ID: u32 = 1023;

    pub fn init() {
        // Enable CPU interface
        mem::write_32(BASE + CTLR, 1);
        // Allow any priority interrupts
        mem::write_32(BASE + PMR, 0xFF);
    }

    pub fn acknowledge_interrupt() -> u32 {
        // Reading IAR returns the highest priority active interrupt and locks it
        let irq = mem::read_32(BASE + IAR);

        // Mask to get the interrupt number (last 10 bits)
        irq & IRQ_MASK
    }

    pub fn end_of_interrupt(irq: u32) {
        // Writing to EOIR releases the interrupt and allows higher-priority interrupts to be signaled.
        mem::write_32(BASE + EOIR, irq);
    }
}

pub fn init() -> Result<(), ()> {
    distributor::init().map_err(|_| ())?;

    cpu::init();

    unsafe {
        asm!("msr daifclr, #3", options(preserves_flags, nostack));
    }
    Ok(())
}

#[unsafe(no_mangle)]
pub extern "C" fn handle_interrupt() {
    let mut uart = uart::UARTDriver::new();
    let iid = cpu::acknowledge_interrupt();
    if iid == cpu::SPURIOUS_INTERRUPT_ID {
        let _ = writeln!(uart, "Spurious interrupt");
    } else {
        let _ = writeln!(uart, "Interrupt ID: {}", iid);
        // End only if not spurious
        cpu::end_of_interrupt(iid);
    }
}
