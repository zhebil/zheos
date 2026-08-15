use super::cursor::Cursor;

pub enum ParsedCommand {
    SetAddress(u64),
    ExamineOne(u64),
    ExamineContinuing(u64),
    StoreContinuing(u8),
    Run,
}

pub enum LineParseError {
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

enum Mode {
    Default,
    Set { any_byte_yet: bool },
}

pub struct Parser<'a> {
    cursor: Cursor<'a>,
    mode: Mode,
}

impl<'a> Parser<'a> {
    pub fn new(line: &'a [u8]) -> Self {
        Self {
            cursor: Cursor::new(line),
            mode: Mode::Default,
        }
    }

    pub fn validate(&mut self) -> Result<(), LineParseError> {
        while self.parse_next()?.is_some() {}

        Ok(())
    }

    pub fn parse_next(&mut self) -> Result<Option<ParsedCommand>, LineParseError> {
        loop {
            self.cursor.consume_spaces();

            let Some(first_char) = self.cursor.peek() else {
                return match self.mode {
                    Mode::Set {
                        any_byte_yet: false,
                    } => Err(LineParseError::ExpectedAByte(self.cursor.pos())),
                    _ => Ok(None),
                };
            };

            let command = if hex::digit(first_char).is_none() {
                self.parse_instruction()?
            } else {
                match self.mode {
                    Mode::Default => Some(self.parse_address_command()?),
                    Mode::Set { .. } => self.parse_set_command()?,
                }
            };

            if command.is_some() {
                return Ok(command);
            }
        }
    }

    fn parse_set_command(&mut self) -> Result<Option<ParsedCommand>, LineParseError> {
        let initial_pos = self.cursor.pos();

        let byte = self.parse_byte();

        match (byte, &self.mode) {
            (Err(_), Mode::Set { any_byte_yet: true }) => {
                self.cursor.set_pos(initial_pos);
                self.mode = Mode::Default;
                Ok(None)
            }
            (Err(err), _) => Err(err),
            (Ok(byte), _) => {
                self.mode = Mode::Set { any_byte_yet: true };
                Ok(Some(ParsedCommand::StoreContinuing(byte)))
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
                HexParseError::NoDigits => LineParseError::ExpectedAByte(self.cursor.pos()),
                HexParseError::TooLong(p) => LineParseError::ByteTooLong(p),
            })
            .map(|number| number as u8)
    }

    fn parse_address(&mut self) -> Result<u64, LineParseError> {
        self.parse_hex_number(MAX_ADDRESS_DIGITS)
            .map_err(|e| match e {
                HexParseError::NoDigits => LineParseError::ExpectedAnAddress(self.cursor.pos()),
                HexParseError::TooLong(p) => LineParseError::AddressTooLong(p),
            })
    }

    fn parse_instruction(&mut self) -> Result<Option<ParsedCommand>, LineParseError> {
        match self.cursor.peek() {
            Some(b'.') => {
                self.mode = Mode::Default;
                self.cursor.advance(1);
                self.cursor.consume_spaces();
                let address = self.parse_address()?;
                Ok(Some(ParsedCommand::ExamineContinuing(address)))
            }
            // A colon only changes what the following digits mean; there is nothing to carry out.
            Some(b':') => {
                self.cursor.advance(1);
                self.mode = Mode::Set {
                    any_byte_yet: false,
                };
                Ok(None)
            }
            Some(b'R' | b'r') => {
                self.mode = Mode::Default;
                self.cursor.advance(1);
                Ok(Some(ParsedCommand::Run))
            }
            _ => {
                self.cursor.advance(1);
                Err(LineParseError::UnexpectedCharacter(self.cursor.pos()))
            }
        }
    }

    #[inline(never)]
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
                return Err(HexParseError::TooLong(self.cursor.pos() + digits));
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

mod hex {
    pub const fn digit(c: u8) -> Option<u32> {
        (c as char).to_digit(16)
    }

    pub const fn append(number: u64, digit: u32) -> u64 {
        number * 16 + digit as u64
    }
}
