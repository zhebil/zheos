use crate::{console, mem, uart};
use console::read_line;
use core::fmt::Write;

const INPUT_CHAR: char = '\\';

pub struct Zhemon<'a> {
    uart: &'a mut uart::UARTDriver,
    current_address: u64,
}

impl<'a> Zhemon<'a> {
    pub fn new(uart: &'a mut uart::UARTDriver) -> Self {
        Self {
            uart,
            current_address: 0,
        }
    }

    pub fn start(&mut self) -> () {
        let mut line_buff = [0u8; 128];
        loop {
            self.put_prompt();

            let line_res = read_line(self.uart, &mut line_buff);

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
                Err(err) => {
                    self.on_parse_error(err);
                    break;
                }
            }
        }
    }

    fn handle_set_address(&mut self, address: u64) -> () {
        self.current_address = address;
    }

    fn handle_examine_one(&mut self, address: u64) -> () {
        self.current_address = address;
        let byte = mem::read_byte(address);
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
            let byte = mem::read_byte(address);
            let _ = write!(self.uart, " {:02x}", byte);
        }
        console::write_new_line(self.uart);
    }

    fn handle_store_continuing(&mut self, byte: u8) -> () {
        mem::write_byte(self.current_address, byte);
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
    cursor: Cursor<'a>,
    mode: Mode,
}

enum Mode {
    Default,
    Set { any_byte_yet: bool },
}

impl<'a> Parser<'a> {
    pub fn new(line: &'a [u8]) -> Self {
        Self {
            cursor: Cursor::new(line),
            mode: Mode::Default,
        }
    }

    pub fn validate(&mut self) -> Result<(), LineParseError> {
        loop {
            match self.parse_next() {
                Ok(ParsedCommand::Empty) => break,
                Ok(_) => continue,
                Err(err) => return Err(err),
            }
        }

        Ok(())
    }

    pub fn parse_next(&mut self) -> Result<ParsedCommand, LineParseError> {
        self.cursor.consume_spaces();

        let first_char = self.cursor.peek();
        if first_char.is_none() {
            match self.mode {
                Mode::Default => return Ok(ParsedCommand::Empty),
                Mode::Set {
                    any_byte_yet: false,
                } => return Err(LineParseError::ExpectedAByte(self.cursor.pos)),
                Mode::Set { any_byte_yet: true } => return Ok(ParsedCommand::Empty),
            }
        }

        let first_char = first_char.unwrap();
        let first_char_hex = hex::digit(first_char);

        if first_char_hex.is_none() {
            return Ok(self.parse_instruction()?);
        }

        match self.mode {
            Mode::Default => self.parse_address_command(),
            Mode::Set { .. } => self.parse_set_command(),
        }
    }

    fn parse_set_command(&mut self) -> Result<ParsedCommand, LineParseError> {
        let initial_pos = self.cursor.pos;

        let byte = self.parse_byte();

        match (byte, &self.mode) {
            (Err(_), Mode::Set { any_byte_yet: true }) => {
                self.cursor.set_pos(initial_pos);
                self.mode = Mode::Default;
                Ok(ParsedCommand::Continue)
            }
            (Err(err), _) => Err(err),
            (Ok(byte), _) => {
                self.mode = Mode::Set { any_byte_yet: true };
                Ok(ParsedCommand::StoreContinuing(byte))
            }
        }
    }

    fn parse_address_command(&mut self) -> Result<ParsedCommand, LineParseError> {
        let address = self.parse_address()?;

        self.cursor.consume_spaces();

        if let Some(next_char) = self.cursor.peek() {
            if Self::is_instruction(next_char) {
                return Ok(ParsedCommand::SetAddress(address));
            }
        }

        Ok(ParsedCommand::ExamineOne(address))
    }

    const fn is_instruction(c: u8) -> bool {
        c == b':' || c == b'R' || c == b'r' || c == b'.'
    }

    fn parse_byte(&mut self) -> Result<u8, LineParseError> {
        self.parse_hex_number(MAX_BYTE_DIGITS)
            .map_err(|e| match e {
                HexParseError::NoDigits => LineParseError::ExpectedAByte(self.cursor.pos),
                HexParseError::TooLong(p) => LineParseError::ByteTooLong(p),
            })
            .and_then(|number| Ok(number as u8))
    }

    fn parse_address(&mut self) -> Result<u64, LineParseError> {
        self.parse_hex_number(MAX_ADDRESS_DIGITS)
            .map_err(|e| match e {
                HexParseError::NoDigits => LineParseError::ExpectedAnAddress(self.cursor.pos),
                HexParseError::TooLong(p) => LineParseError::AddressTooLong(p),
            })
    }

    fn parse_instruction(&mut self) -> Result<ParsedCommand, LineParseError> {
        match self.cursor.peek() {
            Some(b'.') => {
                self.mode = Mode::Default;
                self.cursor.advance(1);
                self.cursor.consume_spaces();
                let address = self.parse_address()?;
                Ok(ParsedCommand::ExamineContinuing(address))
            }
            Some(b':') => {
                self.cursor.advance(1);
                self.mode = Mode::Set {
                    any_byte_yet: false,
                };
                Ok(ParsedCommand::Continue)
            }
            Some(b'R' | b'r') => {
                self.mode = Mode::Default;
                self.cursor.advance(1);
                Ok(ParsedCommand::Run)
            }
            _ => {
                self.cursor.advance(1);
                Err(LineParseError::UnexpectedCharacter(self.cursor.pos))
            }
        }
    }

    fn parse_hex_number(&mut self, max_digits: usize) -> Result<u64, HexParseError> {
        let mut number = 0u64;
        let mut digits = 0;

        loop {
            let c = self.cursor.peek_offset(digits);

            if c.is_none() {
                break;
            }
            let c = c.unwrap();

            let hex_digit = hex::digit(c);

            if hex_digit.is_none() {
                break;
            }

            if digits >= max_digits {
                return Err(HexParseError::TooLong(self.cursor.pos + digits));
            }

            number = hex::append(number, hex_digit.unwrap());
            digits += 1;
        }

        if digits == 0 {
            return Err(HexParseError::NoDigits);
        }

        self.cursor.advance(digits);

        Ok(number)
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_offset(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    fn consume_spaces(&mut self) {
        while let Some(b' ') = self.peek() {
            self.pos += 1;
        }
    }

    fn advance(&mut self, amount: usize) {
        self.pos += amount;
    }

    fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }
}

mod hex {
    pub const fn digit(c: u8) -> Option<u32> {
        (c as char).to_digit(16)
    }

    pub const fn append(number: u64, digit: u32) -> u64 {
        number * 16 + digit as u64
    }
}
