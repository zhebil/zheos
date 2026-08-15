use crate::{console, uart};
use console::read_line;
use core::fmt::Write;
use core::ptr::{read_volatile, write_volatile};

const INPUT_CHAR: char = '\\';
const NEW_LINE: &str = "\r\n";

pub struct Zhemon<'a> {
    uart: &'a mut uart::UARTDriver,
    current_address: u64,
    line_buff: [u8; 128],
}

impl<'a> Zhemon<'a> {
    pub fn new(uart: &'a mut uart::UARTDriver) -> Self {
        Self {
            uart,
            current_address: 0,
            line_buff: [0u8; 128],
        }
    }

    pub fn start(&mut self) -> () {
        loop {
            self.put_prompt();

            let line_res = read_line(self.uart, &mut self.line_buff);

            if line_res.is_none() {
                continue;
            }

            let line = line_res.unwrap();

            if line.device_error {
                let _ = write!(self.uart, "\r\n!Device Error\r\n");
                continue;
            }

            if line.buf == b"exit" {
                return ();
            }

            let mut parser = Parser::new();
            parser.set_line(line.buf);

            match parser.validate() {
                Ok(_) => {}
                Err(err) => {
                    self.on_parse_error(err);
                    continue;
                }
            }

            self.handle_line(&mut parser);
        }
    }

    fn put_prompt(&mut self) {
        self.uart.putc(INPUT_CHAR as u8);
    }

    fn handle_line(&mut self, parser: &mut Parser) -> () {
        loop {
            match parser.parse_next() {
                Ok(command) => match command {
                    ParsedCommand::SetAddress(address) => self.handle_set_address(address),
                    ParsedCommand::Continue => continue,
                    ParsedCommand::ExamineOne(address) => self.handle_examine_one(address),
                    ParsedCommand::ExamineContinuing(end) => self.handle_examine_continuing(end),
                    ParsedCommand::StoreContinuing(byte) => self.handle_store_continuing(byte),
                    ParsedCommand::Run => self.handle_run(),
                    ParsedCommand::Empty => break,
                },
                Err(err) => self.on_parse_error(err),
            }
        }
    }

    fn handle_set_address(&mut self, address: u64) -> () {
        self.current_address = address;
    }

    fn handle_examine_one(&mut self, address: u64) -> () {
        self.current_address = address;
        let byte = self.read_byte(address);
        let _ = write!(self.uart, "{:016x}: {:02x}\r\n", address, byte);
        self.current_address += 1;
    }

    fn handle_examine_continuing(&mut self, end: u64) -> () {
        let start = self.current_address;
        self.current_address = end + 1;

        if start > end {
            return;
        }

        for address in (start)..=end {
            let carry = address % 8;
            let is_start = address == start;

            // Start new line if needed
            if !is_start && carry == 0 {
                console::write_new_line(self.uart);
            }

            // Write address
            if carry == 0 || is_start {
                let _ = write!(self.uart, "{:016x}:", address).unwrap();
            }

            // Align start bytes
            if is_start && carry != 0 {
                let _ = write!(self.uart, "{:width$}", "", width = (carry * 3) as usize);
            }

            // Write bytes
            let byte = self.read_byte(address);
            let _ = write!(self.uart, " {:02x}", byte);
        }
        console::write_new_line(self.uart);
    }

    fn handle_store_continuing(&mut self, byte: u8) -> () {
        self.write_byte(self.current_address, byte);

        self.current_address += 1;
    }

    fn handle_run(&mut self) -> () {
        if self.current_address as usize % 4 != 0 {
            let _ = write!(self.uart, "Error: Address is not aligned");
            console::write_new_line(self.uart);
        } else {
            let f: extern "C" fn() -> () =
                unsafe { core::mem::transmute(self.current_address as *const ()) };
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
        console::write_new_line(self.uart);

        let _ = write!(self.uart, "Error: {}", description);
        console::write_new_line(self.uart);
    }

    fn read_byte(&mut self, address: u64) -> u8 {
        unsafe { read_volatile(address as *const u8) }
    }

    fn write_byte(&mut self, address: u64, byte: u8) -> () {
        unsafe { write_volatile(address as *mut u8, byte) };
    }
}

enum ParsedCommand {
    SetAddress(u64),
    ExamineOne(u64),
    ExamineContinuing(u64),
    StoreContinuing(u8),
    Run,
    Continue,
    Empty,
}

enum LineParseError {
    UnexpectedCharacter(usize),
    AddressTooLong(usize),
    ByteTooLong(usize),
    ExpectedAByte(usize),
    ExpectedAnAddress(usize),
}

enum HexParseError {
    TooLong(usize),
    NoDigits,
}

const MAX_ADDRESS_DIGITS: usize = 16;
const MAX_BYTE_DIGITS: usize = 2;

struct Parser<'a> {
    line: &'a [u8],
    pos: usize,
    is_set_operation: bool,
    set_byte_at_least_once: bool,
}

impl<'a> Parser<'a> {
    pub fn new() -> Self {
        Self {
            line: &[],
            pos: 0,
            is_set_operation: false,
            set_byte_at_least_once: false,
        }
    }

    pub fn set_line(&mut self, line: &'a [u8]) {
        self.line = line;
        self.reset_pos();
    }

    pub fn validate(&mut self) -> Result<(), LineParseError> {
        while let ParsedCommand::Empty = self.parse_next()? {}

        self.reset_pos();
        Ok(())
    }

    pub fn parse_next(&mut self) -> Result<ParsedCommand, LineParseError> {
        self.consume_spaces();

        let first_char = self.peek();
        if first_char.is_none() {
            if self.is_set_operation && !self.set_byte_at_least_once {
                return Err(LineParseError::ExpectedAByte(self.pos));
            }
            return Ok(ParsedCommand::Empty);
        }

        let first_char = first_char.unwrap();
        let first_char_hex = Self::to_hex_digit(first_char);

        if first_char_hex.is_none() {
            return Ok(self.parse_instruction()?);
        }

        if self.is_set_operation {
            self.parse_set_command()
        } else {
            self.parse_address_command()
        }
    }

    fn parse_set_command(&mut self) -> Result<ParsedCommand, LineParseError> {
        let initial_pos = self.pos;

        let byte = self.parse_byte();

        // If we have already set a byte, next operation is not necessarily a set operation
        if byte.is_err() && self.set_byte_at_least_once {
            self.pos = initial_pos;
            self.is_set_operation = false;
            return Ok(ParsedCommand::Continue);
        } else if byte.is_err() {
            return Err(byte.unwrap_err());
        }

        let byte = byte?;

        self.set_byte_at_least_once = true;

        Ok(ParsedCommand::StoreContinuing(byte))
    }

    fn parse_address_command(&mut self) -> Result<ParsedCommand, LineParseError> {
        let address = self.parse_address()?;

        if let Some(next_char) = self.peek_next() {
            if Self::is_instruction(next_char) {
                return Ok(ParsedCommand::SetAddress(address));
            }
        }

        Ok(ParsedCommand::ExamineOne(address))
    }

    fn peek(&self) -> Option<u8> {
        self.line.get(self.pos).map(|b| *b)
    }

    fn peek_offset(&self, offset: usize) -> Option<u8> {
        self.line.get(self.pos + offset).map(|b| *b)
    }

    fn peek_next(&mut self) -> Option<u8> {
        self.consume_spaces();
        self.line.get(self.pos).map(|b| *b)
    }

    const fn append_hex_digit(number: u64, digit: u32) -> u64 {
        number * 16 + digit as u64
    }

    const fn to_hex_digit(c: u8) -> Option<u32> {
        (c as char).to_digit(16)
    }

    const fn is_instruction(c: u8) -> bool {
        c == b':' || c == b'R' || c == b'r' || c == b'.'
    }

    fn consume_spaces(&mut self) {
        while self.peek() == Some(b' ') {
            self.pos += 1;
        }
    }

    fn parse_byte(&mut self) -> Result<u8, LineParseError> {
        self.parse_hex_number(MAX_BYTE_DIGITS)
            .map_err(|e| match e {
                HexParseError::NoDigits => LineParseError::ExpectedAByte(self.pos),
                HexParseError::TooLong(p) => LineParseError::ByteTooLong(p),
            })
            .and_then(|number| Ok(number as u8))
    }

    fn parse_address(&mut self) -> Result<u64, LineParseError> {
        self.parse_hex_number(MAX_ADDRESS_DIGITS)
            .map_err(|e| match e {
                HexParseError::NoDigits => LineParseError::ExpectedAnAddress(self.pos),
                HexParseError::TooLong(p) => LineParseError::AddressTooLong(p),
            })
    }

    fn parse_instruction(&mut self) -> Result<ParsedCommand, LineParseError> {
        match self.peek() {
            Some(b'.') => {
                self.is_set_operation = false;
                self.pos += 1;
                self.consume_spaces();
                let address = self.parse_address()?;
                Ok(ParsedCommand::ExamineContinuing(address))
            }
            Some(b':') => {
                self.pos += 1;
                self.is_set_operation = true;
                self.set_byte_at_least_once = false;
                Ok(ParsedCommand::Continue)
            }
            Some(b'R' | b'r') => {
                self.is_set_operation = false;
                self.pos += 1;
                Ok(ParsedCommand::Run)
            }
            _ => {
                self.pos += 1;
                Err(LineParseError::UnexpectedCharacter(self.pos))
            }
        }
    }

    fn parse_hex_number(&mut self, max_digits: usize) -> Result<u64, HexParseError> {
        let mut number = 0u64;
        let mut digits = 0;

        loop {
            let c = self.peek_offset(digits);

            if c.is_none() {
                break;
            }
            let c = c.unwrap();

            let hex_digit = Self::to_hex_digit(c);

            if hex_digit.is_none() {
                break;
            }

            if digits >= max_digits {
                return Err(HexParseError::TooLong(self.pos + digits));
            }

            number = Self::append_hex_digit(number, hex_digit.unwrap());
            digits += 1;
        }

        if digits == 0 {
            return Err(HexParseError::NoDigits);
        }

        self.pos += digits;

        Ok(number)
    }

    fn reset_pos(&mut self) {
        self.pos = 0;
        self.is_set_operation = false;
        self.set_byte_at_least_once = false;
    }
}
