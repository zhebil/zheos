mod reg {
    pub const DR: usize = 0x00;
    pub const FR: usize = 0x18;
    pub const IBRD: usize = 0x24;
    pub const FBRD: usize = 0x28;
    pub const LCR_H: usize = 0x2C;
    pub const CR: usize = 0x30;
    pub const IMSC: usize = 0x38;
    pub const ICR: usize = 0x44;
}

mod fr {
    pub const BUSY: u32 = crate::bit_mask(3);
    pub const TXFF: u32 = crate::bit_mask(5);
}

mod lcr_h {
    pub const FEN: u32 = crate::bit_mask(4);
    pub const WLEN_8BIT: u32 = 0b11 << 5;
}

mod cr {
    pub const DISABLE: u32 = 0x000;
    pub const ENABLE: u32 = 0x301; // UARTEN | TXE | RXE
}

mod icr {
    pub const ALL_MASK: u32 = 0x7FF;
}

pub struct UARTDriver {
    addr: usize,
}

impl UARTDriver {
    const BASE: usize = 0x0900_0000;

    const UARTCLK: u32 = 24_000_000; // 24MHz
    const BAUD: u32 = 115_200;
    const SCALED_DIVISOR: u32 = (4 * Self::UARTCLK + Self::BAUD / 2) / Self::BAUD;

    pub const fn new() -> Self {
        Self { addr: Self::BASE }
    }

    pub fn init(&self) {
        // Disable UART
        self.write_register(reg::CR, cr::DISABLE);

        // Wait for busy flag
        while self.read_register(reg::FR) & fr::BUSY != 0 {}

        // Flush FIFO
        self.write_register(reg::LCR_H, 0);

        // Set baud divisors
        self.write_register(reg::IBRD, Self::SCALED_DIVISOR / 64);
        self.write_register(reg::FBRD, Self::SCALED_DIVISOR % 64);

        // Write the line control register. This is also what latches the divisors above.
        self.write_register(reg::LCR_H, lcr_h::WLEN_8BIT | lcr_h::FEN);

        // Mask every interrupt, clear anything already pending
        self.write_register(reg::IMSC, 0);
        self.write_register(reg::ICR, icr::ALL_MASK);

        // Enable
        self.write_register(reg::CR, cr::ENABLE);
    }

    fn read_register(&self, offset: usize) -> u32 {
        let addr = (self.addr + offset) as *const u32;
        unsafe { core::ptr::read_volatile(addr) }
    }

    fn write_register(&self, offset: usize, data: u32) {
        let addr = (self.addr + offset) as *mut u32;
        unsafe { core::ptr::write_volatile(addr, data) }
    }

    #[allow(dead_code)]
    fn read_data(&self) -> u32 {
        self.read_register(reg::DR)
    }

    fn write_data(&self, c: u8) {
        self.write_register(reg::DR, c as u32)
    }

    pub fn putc(&self, c: u8) {
        // Wait until FIFO is not full
        while self.read_register(reg::FR) & fr::TXFF != 0 {}
        self.write_data(c)
    }
}
