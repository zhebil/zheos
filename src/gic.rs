mod distributor {
    use crate::board::GICD_BASE;
    use crate::mmio;
    // Base address of GIC Distributor
    const BASE: usize = GICD_BASE;

    // Offset of GIC Distributor registers
    const CTLR: usize = 0x0;

    // Register containing information about the GIC Distributor
    const TYPER: usize = 0x4;

    const ISENABLER: usize = 0x100;

    const ITARGETSR: usize = 0x800;

    // Mask for the number of interrupt lines
    const IT_LINES_NUMBER_MASK: u32 = 0x1F;

    // Number of interrupt lines per block
    const LINES_PER_BLOCK: u32 = 32;

    // SGIs and PPIs are always delivered to the local core; only SPIs need routing.
    const FIRST_SPI: u32 = 32;

    const TARGET_CPU0: u8 = 0x01;

    pub fn init() {
        let typer = mmio::read_32(BASE + TYPER);

        let it_lines_number = typer & IT_LINES_NUMBER_MASK;

        // Number of interrupt lines
        let n = LINES_PER_BLOCK * (it_lines_number + 1);

        assert!(n >= 32, "GICD_TYPER read {typer:#x} - wrong base?");

        // Enable distributor
        mmio::write_32(BASE + CTLR, 1);
    }

    pub fn enable(intid: u32) {
        let block = intid as usize / 32;
        let bit = intid % 32;
        let enable_addr = BASE + ISENABLER + block * 4;

        mmio::write_32(enable_addr, 1 << bit);

        // One byte per interrupt here, unlike the bit-per-interrupt ISENABLER above.
        if intid >= FIRST_SPI {
            mmio::write_byte(BASE + ITARGETSR + intid as usize, TARGET_CPU0);
        }
    }
}

mod cpu {
    use crate::board::GICC_BASE;
    use crate::mmio;

    // Base address of GIC CPU Interface
    const BASE: usize = GICC_BASE;

    // Offset of GIC CPU Interface control register
    const CTLR: usize = 0x0;

    // Offset of GIC CPU Interface priority mask register
    const PMR: usize = 0x4;

    // Offset of GIC CPU Interface interrupt acknowledge register
    const IAR: usize = 0x0C;

    // Mask for the interrupt number
    pub const IRQ_MASK: u32 = 0x3FF;

    const EOIR: usize = 0x10;

    pub const SPURIOUS_INTERRUPT_ID: u32 = 1023;

    pub fn init() {
        // Enable CPU interface
        mmio::write_32(BASE + CTLR, 1);
        // Allow any priority interrupts
        mmio::write_32(BASE + PMR, 0xFF);
    }

    pub fn acknowledge_interrupt() -> u32 {
        // Reading IAR returns the highest priority active interrupt and locks it
        mmio::read_32(BASE + IAR)
    }

    pub fn end_of_interrupt(irq: u32) {
        // Writing to EOIR releases the interrupt and allows higher-priority interrupts to be signaled.
        mmio::write_32(BASE + EOIR, irq);
    }
}

pub fn init() {
    distributor::init();

    cpu::init();
}

#[must_use]
pub struct Interrupt {
    val: u32,
}

impl Interrupt {
    pub fn intid(&self) -> u32 {
        self.val & cpu::IRQ_MASK
    }

    pub fn end(self) {
        cpu::end_of_interrupt(self.val);
    }

    fn is_spurious(&self) -> bool {
        self.intid() == cpu::SPURIOUS_INTERRUPT_ID
    }

    fn new(val: u32) -> Self {
        Self { val }
    }
}

pub fn enable(intid: u32) {
    distributor::enable(intid);
}

pub fn acknowledge() -> Option<Interrupt> {
    let iar = cpu::acknowledge_interrupt();

    let interrupt = Interrupt::new(iar);

    if interrupt.is_spurious() {
        None
    } else {
        Some(interrupt)
    }
}
