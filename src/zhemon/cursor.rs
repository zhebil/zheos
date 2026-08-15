pub struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    pub fn peek_offset(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    pub fn consume_spaces(&mut self) {
        while let Some(b' ') = self.peek() {
            self.pos += 1;
        }
    }

    pub fn advance(&mut self, amount: usize) {
        self.pos += amount;
    }

    pub fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }
}
