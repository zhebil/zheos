use crate::uart;

pub struct ReadlineResult<'a> {
    pub buf: &'a mut [u8],
    pub device_error: bool,
}

#[derive(Debug, Clone, Copy)]
enum InputChar {
    Char(u8),
    Backspace,
    Newline,
    ESCAPE,
    NonPrintable,
}

impl InputChar {
    const fn from_byte(byte: u8) -> Self {
        if Self::is_newline(byte) {
            return Self::Newline;
        }

        if Self::is_backspace(byte) {
            return Self::Backspace;
        }

        if Self::is_escape(byte) {
            return Self::ESCAPE;
        }

        if Self::is_non_printable(byte) {
            return Self::NonPrintable;
        }

        Self::Char(byte)
    }

    const fn is_backspace(c: u8) -> bool {
        c == 0x08 || c == 0x7F
    }

    const fn is_newline(c: u8) -> bool {
        c == b'\r' || c == b'\n'
    }

    const fn is_escape(c: u8) -> bool {
        c == b'\x1b'
    }

    pub const fn is_non_printable(c: u8) -> bool {
        c < 0x20 || c > 0x7E
    }
}

pub fn read_line<'a>(uart: &mut uart::UARTDriver, buf: &'a mut [u8]) -> Option<ReadlineResult<'a>> {
    let mut i = 0usize;
    let mut device_error = false;

    loop {
        let c = uart.getc();

        if c.flags.framing() || c.flags.parity() || c.flags.overrun() || c.flags.brk() {
            device_error = true;
        }

        match InputChar::from_byte(c.byte) {
            InputChar::NonPrintable => continue,
            InputChar::Newline => {
                write_new_line(uart);
                break;
            }
            InputChar::Backspace => {
                if i > 0 {
                    i -= 1;
                    for b in ERASE_SEQUENCE {
                        uart.putc(*b);
                    }
                }
                continue;
            }
            InputChar::ESCAPE => {
                write_new_line(uart);
                return None;
            }
            InputChar::Char(c) => {
                if i < buf.len() {
                    buf[i] = c;
                    i += 1;
                    uart.putc(c);
                }
            }
        }
    }
    Some(ReadlineResult {
        buf: buf.get_mut(..i).unwrap_or(&mut []),
        device_error,
    })
}

pub fn write_new_line(uart: &mut uart::UARTDriver) {
    uart.putc(b'\r');
    uart.putc(b'\n');
}

const ERASE_SEQUENCE: &[u8] = b"\x08\x20\x08";
