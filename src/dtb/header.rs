const MAGIC_OFFSET: usize = 0x00;
const TOTAL_SIZE_OFFSET: usize = 0x04;
const OFF_DT_STRUCT_OFFSET: usize = 0x08;
const OFF_DT_STRINGS_OFFSET: usize = 0x0C;
const LAST_COMP_VERSION_OFFSET: usize = 0x18;
const SIZE_DT_STRINGS_OFFSET: usize = 0x20;
const SIZE_DT_STRUCT_OFFSET: usize = 0x24;
const HEADER_SIZE: usize = 0x28;

const MAGIC: u32 = 0xd00d_feed;
const SUPPORTED_VERSION: u32 = 17;

pub struct Header<'a> {
    blob: &'a [u8; HEADER_SIZE],
}

impl<'a> Header<'a> {
    pub unsafe fn from_ptr(base: usize) -> Option<Self> {
        let blob = unsafe { &*(base as *const [u8; HEADER_SIZE]) };
        if Self::u32_at(blob, MAGIC_OFFSET) != MAGIC {
            return None;
        }

        if Self::u32_at(blob, LAST_COMP_VERSION_OFFSET) > SUPPORTED_VERSION {
            return None;
        }

        Some(Header { blob })
    }

    pub fn total_size(&self) -> u32 {
        Self::u32_at(self.blob, TOTAL_SIZE_OFFSET)
    }

    pub fn off_dt_struct(&self) -> u32 {
        Self::u32_at(self.blob, OFF_DT_STRUCT_OFFSET)
    }

    pub fn off_dt_strings(&self) -> u32 {
        Self::u32_at(self.blob, OFF_DT_STRINGS_OFFSET)
    }

    pub fn size_dt_strings(&self) -> u32 {
        Self::u32_at(self.blob, SIZE_DT_STRINGS_OFFSET)
    }

    pub fn size_dt_struct(&self) -> u32 {
        Self::u32_at(self.blob, SIZE_DT_STRUCT_OFFSET)
    }

    fn u32_at(blob: &[u8; HEADER_SIZE], offset: usize) -> u32 {
        u32::from_be_bytes(blob[offset..offset + 4].try_into().unwrap())
    }
}
