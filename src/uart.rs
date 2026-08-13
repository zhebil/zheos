mod reg {
    pub const DR: usize = 0x00; // Data Register
    pub const FR: usize = 0x18; // Flag Register
    pub const IBRD: usize = 0x24; // Integer Baud Rate Divisor
    pub const FBRD: usize = 0x28; // Fractional Baud Rate Divisor
    pub const LCR_H: usize = 0x2C; // Line Control Register High
    pub const CR: usize = 0x30; // Control Register
    pub const IMSC: usize = 0x38; // Interrupt Mask Set/Clear Register
    pub const ICR: usize = 0x44; // Interrupt Clear Register
}

mod fr {
    pub const BUSY: u32 = crate::bit_mask(3); // The wire is not quiet yet.
    pub const TXFF: u32 = crate::bit_mask(5); // Transmit FIFO is Full
    pub const RXFE: u32 = crate::bit_mask(4); // Receive FIFO is Empty
}

mod lcr_h {
    pub const FEN: u32 = crate::bit_mask(4); // FIFO Enable
    pub const WLEN_8BIT: u32 = 0b11 << 5; // Word Length 8-bit
}

mod cr {
    pub const DISABLE: u32 = 0x000; // Disable UART
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

    pub fn putc(&self, c: u8) {
        // Wait until FIFO is not full
        while self.read_register(reg::FR) & fr::TXFF != 0 {}
        self.write_data(c)
    }

    #[allow(dead_code)]
    pub fn try_getc(&self) -> Option<u8> {
        if self.has_byte() {
            let c = self.read_data();
            Some(self.data_byte_mask(c))
        } else {
            None
        }
    }

    pub fn getc(&self) -> u8 {
        // Wait until FIFO is not empty
        while !self.has_byte() {}
        let c = self.read_data();
        self.data_byte_mask(c)
    }
    pub fn flush(&self) {
        while self.read_register(reg::FR) & fr::BUSY != 0 {}
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

    fn has_byte(&self) -> bool {
        self.read_register(reg::FR) & fr::RXFE == 0
    }

    fn data_byte_mask(&self, c: u32) -> u8 {
        (c & 0xFF) as u8
    }
}
