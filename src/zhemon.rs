use crate::{console, mem, print, println, uart};
use console::{Line, read_line, write_new_line};
use parser::{LineParseError, ParsedCommand, Parser};

mod cursor;
mod parser;

const PROMPT: u8 = b'\\';

pub struct Zhemon {
    last_address: u64,
    next_address: u64,
}

impl Zhemon {
    pub fn new() -> Self {
        Self {
            last_address: 0,
            next_address: 0,
        }
    }

    pub fn start(&mut self) {
        let mut line_buff = [0u8; 128];
        loop {
            self.put_prompt();

            let line_res = read_line(&mut line_buff);

            let Line::Ready(line) = line_res else {
                continue;
            };

            if line.device_error {
                println!("Device Error");
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
        uart().putc(PROMPT);
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
        let byte = mem::read_byte(address as usize);
        println!("{:016x}: {:02x}", address, byte);
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
                write_new_line();
            }

            // Write address
            if carry == 0 || is_start {
                print!("{:016x}:", address);
            }

            // Align start bytes
            if is_start && carry != 0 {
                for _ in 1..=carry {
                    print!("   ");
                }
            }

            // Write bytes
            let byte = mem::read_byte(address as usize);
            print!(" {:02x}", byte);
        }
        write_new_line();
    }

    fn handle_store_continuing(&mut self, byte: u8) {
        mem::write_byte(self.next_address as usize, byte);
        self.next_address += 1;
    }

    fn handle_run(&mut self) {
        if self.last_address % 4 != 0 {
            println!("Error: Address is not aligned");
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
            uart().putc(b' ');
        }

        uart().putc(b'^');
        write_new_line();

        println!("Error: {}", description);
    }
}
