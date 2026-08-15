use crate::{console, mem, uart};
use console::{Line, read_line, write_new_line};
use core::fmt::Write;
use parser::{LineParseError, ParsedCommand, Parser};

mod cursor;
mod parser;

const PROMPT: u8 = b'\\';

pub struct Zhemon<'a> {
    uart: &'a mut uart::UARTDriver,
    last_address: u64,
    next_address: u64,
}

impl<'a> Zhemon<'a> {
    pub fn new(uart: &'a mut uart::UARTDriver) -> Self {
        Self {
            uart,
            last_address: 0,
            next_address: 0,
        }
    }

    pub fn start(&mut self) {
        let mut line_buff = [0u8; 128];
        loop {
            self.put_prompt();

            let line_res = read_line(self.uart, &mut line_buff);

            let Line::Ready(line) = line_res else {
                continue;
            };

            if line.device_error {
                let _ = write!(self.uart, "\r\n!Device Error\r\n");
                continue;
            }

            if line.buf == b"exit" {
                return ();
            }

            match Parser::new(line.buf).validate() {
                Ok(_) => {}
                Err(err) => {
                    self.on_parse_error(err);
                    continue;
                }
            }

            let mut parser = Parser::new(line.buf);
            self.handle_line(&mut parser);
        }
    }

    fn put_prompt(&mut self) {
        self.uart.putc(PROMPT);
    }

    fn handle_line(&mut self, parser: &mut Parser) {
        loop {
            match parser.parse_next() {
                Ok(Some(command)) => match command {
                    ParsedCommand::SetAddress(address) => self.handle_set_address(address),
                    ParsedCommand::ExamineOne(address) => self.handle_examine_one(address),
                    ParsedCommand::ExamineContinuing(end) => self.handle_examine_continuing(end),
                    ParsedCommand::StoreContinuing(byte) => self.handle_store_continuing(byte),
                    ParsedCommand::Run => self.handle_run(),
                },
                Ok(None) => break,
                Err(err) => {
                    self.on_parse_error(err);
                    break;
                }
            }
        }
    }

    fn handle_set_address(&mut self, address: u64) {
        self.last_address = address;
        self.next_address = address;
    }

    fn handle_examine_one(&mut self, address: u64) {
        self.last_address = address;
        let byte = mem::read_byte(address);
        let _ = write!(self.uart, "{:016x}: {:02x}\r\n", address, byte);
        self.next_address = address + 1;
    }

    fn handle_examine_continuing(&mut self, end: u64) {
        let start = self.next_address;
        self.last_address = end;
        self.next_address = end + 1;

        if start > end {
            return;
        }

        for address in (start)..=end {
            let carry = address % 8;
            let is_start = address == start;

            // Start new line if needed
            if !is_start && carry == 0 {
                write_new_line(self.uart);
            }

            // Write address
            if carry == 0 || is_start {
                let _ = write!(self.uart, "{:016x}:", address);
            }

            // Align start bytes
            if is_start && carry != 0 {
                for _ in 1..=carry {
                    let _ = write!(self.uart, "   ");
                }
            }

            // Write bytes
            let byte = mem::read_byte(address);
            let _ = write!(self.uart, " {:02x}", byte);
        }
        write_new_line(self.uart);
    }

    fn handle_store_continuing(&mut self, byte: u8) {
        mem::write_byte(self.next_address, byte);
        self.next_address += 1;
    }

    fn handle_run(&mut self) {
        if self.last_address % 4 != 0 {
            let _ = write!(self.uart, "Error: Address is not aligned");
            write_new_line(self.uart);
        } else {
            let f: extern "C" fn() =
                unsafe { core::mem::transmute(self.last_address as *const ()) };
            f()
        }
    }

    fn on_parse_error(&mut self, err: LineParseError) {
        let (position, description) = match err {
            LineParseError::UnexpectedCharacter(i) => (i, "Unexpected character"),
            LineParseError::AddressTooLong(i) => (i + 1, "Address too long"),
            LineParseError::ByteTooLong(i) => (i + 1, "Byte too long"),
            LineParseError::ExpectedAByte(i) => (i, "Expected a byte"),
            LineParseError::ExpectedAnAddress(i) => (i, "Expected an address"),
        };

        for _ in 0..position {
            self.uart.putc(b' ');
        }

        self.uart.putc(b'^');
        write_new_line(self.uart);

        let _ = self.uart.write_str("Error: ");
        let _ = self.uart.write_str(description);
        write_new_line(self.uart);
    }
}
