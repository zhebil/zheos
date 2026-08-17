use core::fmt::{Display, Write};

use crate::{
    board::UART_BASE,
    input::{self, InputByte},
};

mod reg {
    pub const DR: usize = 0x00; // Data Register
    pub const FR: usize = 0x18; // Flag Register
    pub const IBRD: usize = 0x24; // Integer Baud Rate Divisor
    pub const FBRD: usize = 0x28; // Fractional Baud Rate Divisor
    pub const LCR_H: usize = 0x2C; // Line Control Register High
    pub const CR: usize = 0x30; // Control Register
    pub const IMSC: usize = 0x38; // Interrupt Mask Set/Clear Register
    pub const ICR: usize = 0x44; // Interrupt Clear Register
    pub const RSR_ECR: usize = 0x04; // Receive Status/Error Clear Register
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

mod imsc {
    pub const RX: u32 = crate::bit_mask(4);
    pub const RT: u32 = crate::bit_mask(6);
}

#[derive(Debug, Clone, Copy)]
struct UARTByte {
    pub byte: u8,
    pub flags: RxFlags,
}

impl UARTByte {
    fn into_input_byte(self) -> InputByte {
        InputByte {
            byte: self.byte,
            error: self.flags.framing()
                || self.flags.parity()
                || self.flags.brk()
                || self.flags.overrun(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RxFlags(u32);

impl RxFlags {
    const DATA_BYTE_OFFSET: u32 = 8;
    const FRAMING_ERROR: u32 = 1 << 0;
    const PARITY_ERROR: u32 = 1 << 1;
    const BREAK_ERROR: u32 = 1 << 2;
    const OVERRUN_ERROR: u32 = 1 << 3;

    pub const fn new(flags: u32) -> Self {
        Self(flags)
    }
    pub const fn framing(self) -> bool {
        self.0 & Self::FRAMING_ERROR != 0
    }

    pub const fn parity(self) -> bool {
        self.0 & Self::PARITY_ERROR != 0
    }

    pub const fn brk(self) -> bool {
        self.0 & Self::BREAK_ERROR != 0
    }

    pub const fn overrun(self) -> bool {
        self.0 & Self::OVERRUN_ERROR != 0
    }

    pub const fn from_data(data: u32) -> Self {
        Self::new(data >> Self::DATA_BYTE_OFFSET)
    }
}

pub struct UARTDriver {
    addr: usize,
}

impl UARTDriver {
    const UARTCLK: u32 = 24_000_000; // 24MHz
    const BAUD: u32 = 115_200;
    const SCALED_DIVISOR: u32 = (4 * Self::UARTCLK + Self::BAUD / 2) / Self::BAUD;

    const fn new() -> Self {
        Self { addr: UART_BASE }
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
        self.clear_interrupts();

        // Enable
        self.write_register(reg::CR, cr::ENABLE);
    }

    pub fn enable_interrupt(&self) {
        self.write_register(reg::IMSC, imsc::RX | imsc::RT);
    }

    pub fn putc(&self, c: u8) {
        // Wait until FIFO is not full
        while self.read_register(reg::FR) & fr::TXFF != 0 {}
        self.write_data(c)
    }

    pub fn flush(&self) {
        while self.read_register(reg::FR) & fr::BUSY != 0 {}
    }

    fn try_getc(&self) -> Option<UARTByte> {
        if self.has_byte() {
            let c = self.read_data();
            let flags = RxFlags::from_data(c);
            let byte = Self::data_byte_mask(c);

            Some(UARTByte { byte, flags })
        } else {
            None
        }
    }

    fn clear_interrupts(&self) {
        self.write_register(reg::ICR, icr::ALL_MASK);
    }

    fn read_register(&self, offset: usize) -> u32 {
        let addr = (self.addr + offset) as *const u32;
        unsafe { core::ptr::read_volatile(addr) }
    }

    fn write_register(&self, offset: usize, data: u32) {
        let addr = (self.addr + offset) as *mut u32;
        unsafe { core::ptr::write_volatile(addr, data) }
    }

    fn read_data(&self) -> u32 {
        self.read_register(reg::DR)
    }

    fn write_data(&self, c: u8) {
        self.write_register(reg::DR, c as u32)
    }

    fn has_byte(&self) -> bool {
        self.read_register(reg::FR) & fr::RXFE == 0
    }

    const fn data_byte_mask(c: u32) -> u8 {
        (c & 0xFF) as u8
    }

    #[allow(dead_code)]
    fn read_error_status(&self) -> RxFlags {
        RxFlags(self.read_register(reg::RSR_ECR))
    }

    #[allow(dead_code)]
    fn clear_error_status(&self) {
        self.write_register(reg::RSR_ECR, 0x00);
    }
}

impl Write for &UARTDriver {
    fn write_str(&mut self, s: &str) -> Result<(), core::fmt::Error> {
        for c in s.as_bytes() {
            if *c == b'\n' {
                self.putc(b'\r');
            }
            self.putc(*c);
        }
        Ok(())
    }
}

impl Display for RxFlags {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut any = false;
        for (set, name) in [
            (self.framing(), "FE"),
            (self.parity(), "PE"),
            (self.brk(), "BE"),
            (self.overrun(), "OE"),
        ] {
            if set {
                if any {
                    f.write_str(" ")?;
                }
                f.write_str(name)?;
                any = true;
            }
        }

        if any { Ok(()) } else { f.write_str("none") }
    }
}

static UART: UARTDriver = UARTDriver::new();

pub fn uart() -> &'static UARTDriver {
    &UART
}

pub fn handle_interrupt(_intid: u32) {
    let uart = uart();
    uart.clear_interrupts();

    // Drop on overflow
    while let Some(uart_byte) = uart.try_getc() {
        input::push_character(uart_byte.into_input_byte());
    }
}
