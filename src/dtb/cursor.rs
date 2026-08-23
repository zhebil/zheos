#[derive(Clone, Copy)]
pub struct Cursor<'a> {
    blob: &'a [u8],
    offset: usize,
}

const U32_SIZE: usize = 4;

impl<'a> Cursor<'a> {
    pub fn new(blob: &'a [u8], offset: usize) -> Self {
        Cursor { blob, offset }
    }

    pub fn read_u32(&mut self) -> Option<u32> {
        let bytes = self.blob.get(self.offset..self.offset + U32_SIZE)?;
        self.offset += U32_SIZE;
        Some(u32::from_be_bytes(bytes.try_into().unwrap()))
    }

    pub fn bytes(&mut self, size: usize) -> Option<&'a [u8]> {
        let bytes = self.blob.get(self.offset..self.offset + size)?;
        self.offset += size;
        Some(bytes)
    }

    pub fn cstring(&mut self) -> Option<&'a [u8]> {
        let bytes = self.blob.get(self.offset..)?;
        let len = bytes.iter().position(|&byte| byte == 0)?;
        self.offset += len + 1;
        bytes.get(..len)
    }

    pub fn done(&self) -> bool {
        self.offset + U32_SIZE > self.blob.len()
    }

    pub fn align_u32(&mut self) {
        self.offset = (self.offset + 3) & !3;
    }
}
