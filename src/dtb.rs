use crate::mmio::read_32;

fn be_read_from_arr(arr: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(*arr.get(offset..)?.first_chunk()?))
}

const MAGIC_OFFSET: usize = 0x00;
const TOTAL_SIZE_OFFSET: usize = 0x04;
const OFF_DT_STRUCT_OFFSET: usize = 0x08;
const OFF_DT_STRINGS_OFFSET: usize = 0x0C;
const OFF_MEM_RSVMAP_OFFSET: usize = 0x10;
#[allow(dead_code)]
const VERSION_OFFSET: usize = 0x14;
const LAST_COMP_VERSION_OFFSET: usize = 0x18;
#[allow(dead_code)]
const BOOT_CPUID_OFFSET: usize = 0x1C;
const SIZE_DT_STRINGS_OFFSET: usize = 0x20;
const SIZE_DT_STRUCT_OFFSET: usize = 0x24;
const HEADER_SIZE: usize = 0x28;

const TOKEN_BEGIN_NODE: u32 = 0x01;
const TOKEN_END_NODE: u32 = 0x02;
const TOKEN_PROP: u32 = 0x03;
const TOKEN_NOOP: u32 = 0x04;
const TOKEN_END: u32 = 0x09;

const MAGIC: u32 = 0xd00d_feed;
const SUPPORTED_VERSION: u32 = 17;

// Defaults mandated by the spec for a node that declares neither.
const DEFAULT_ADDRESS_CELLS: u32 = 2;
const DEFAULT_SIZE_CELLS: u32 = 1;

pub struct Memory {
    pub addr: usize,
    pub size: usize,
}

pub struct Dtb<'a> {
    blob: &'a [u8],
    header: &'a [u8; HEADER_SIZE],
}

impl<'a> Dtb<'a> {
    pub unsafe fn from_ptr(base: usize) -> Option<Dtb<'a>> {
        if u32::from_be(read_32(base + MAGIC_OFFSET)) != MAGIC {
            return None;
        }

        let total_size = u32::from_be(read_32(base + TOTAL_SIZE_OFFSET)) as usize;
        let blob = unsafe { core::slice::from_raw_parts(base as *const u8, total_size) };
        let dtb = Dtb {
            header: blob.first_chunk()?,
            blob,
        };

        // The blob names the oldest parser that can still read it, not its own version.
        if dtb.last_comp_version() > SUPPORTED_VERSION {
            return None;
        }

        Some(dtb)
    }

    fn u32_at(&self, offset: usize) -> u32 {
        u32::from_be_bytes(self.header[offset..offset + 4].try_into().unwrap())
    }

    #[allow(dead_code)]
    pub fn magic(&self) -> u32 {
        self.u32_at(MAGIC_OFFSET)
    }

    #[allow(dead_code)]
    pub fn total_size(&self) -> u32 {
        self.u32_at(TOTAL_SIZE_OFFSET)
    }

    pub fn off_dt_struct(&self) -> u32 {
        self.u32_at(OFF_DT_STRUCT_OFFSET)
    }

    pub fn off_dt_strings(&self) -> u32 {
        self.u32_at(OFF_DT_STRINGS_OFFSET)
    }

    #[allow(dead_code)]
    pub fn off_mem_rsvmap(&self) -> u32 {
        self.u32_at(OFF_MEM_RSVMAP_OFFSET)
    }

    #[allow(dead_code)]
    pub fn version(&self) -> u32 {
        self.u32_at(VERSION_OFFSET)
    }

    pub fn last_comp_version(&self) -> u32 {
        self.u32_at(LAST_COMP_VERSION_OFFSET)
    }

    #[allow(dead_code)]
    pub fn boot_cpuid(&self) -> u32 {
        self.u32_at(BOOT_CPUID_OFFSET)
    }

    pub fn size_dt_strings(&self) -> u32 {
        self.u32_at(SIZE_DT_STRINGS_OFFSET)
    }

    pub fn size_dt_struct(&self) -> u32 {
        self.u32_at(SIZE_DT_STRUCT_OFFSET)
    }

    #[allow(dead_code)]
    pub fn reservations(&self) -> Reservations<'_> {
        let start = self.off_mem_rsvmap() as usize;
        let end = self.total_size() as usize;
        Reservations {
            addr: start,
            end,
            blob: self.blob,
        }
    }

    fn strings(&self) -> Option<&'a [u8]> {
        let start = self.off_dt_strings() as usize;
        self.blob
            .get(start..start + self.size_dt_strings() as usize)
    }

    pub fn nodes(&self) -> Option<Nodes<'a>> {
        let start = self.off_dt_struct() as usize;
        let end = start + self.size_dt_struct() as usize;
        Some(Nodes::new(self.blob, self.strings()?, start, end))
    }

    pub fn memory(&self) -> Option<Memory> {
        let mut nodes = self.nodes()?;

        // reg is an address in the parent's space, and /memory is a child of the root.
        let (address_cells, size_cells) = nodes.next()?.cells();

        let reg = nodes.find(Node::is_memory)?.property(b"reg")?;
        let addr = read_cells(reg.value, address_cells)?;
        let size = read_cells(reg.value.get(address_cells * 4..)?, size_cells)?;

        Some(Memory {
            addr: addr as usize,
            size: size as usize,
        })
    }
}

#[allow(dead_code)]
pub struct Reservations<'a> {
    addr: usize,
    end: usize,
    blob: &'a [u8],
}

#[allow(dead_code)]
const RESERVATION_ITEM_SIZE: usize = 16;

impl<'a> Iterator for Reservations<'a> {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<(usize, usize)> {
        if self.addr + RESERVATION_ITEM_SIZE > self.end {
            return None;
        }

        let entry = self.blob.get(self.addr..self.addr + RESERVATION_ITEM_SIZE)?;
        let addr = read_cells(entry, 2)?;
        let size = read_cells(entry.get(8..)?, 2)?;

        if addr == 0 && size == 0 {
            return None;
        }

        self.addr += RESERVATION_ITEM_SIZE;

        Some((addr as usize, size as usize))
    }
}

pub struct Node<'a> {
    #[allow(dead_code)]
    name: &'a [u8],
    #[allow(dead_code)]
    depth: u32,
    properties: Properties<'a>,
}

impl<'a> Node<'a> {
    fn property(&self, name: &[u8]) -> Option<Property<'a>> {
        let mut properties = self.properties;
        properties.find(|property| property.name == name)
    }

    fn is_memory(&self) -> bool {
        self.property(b"device_type")
            .is_some_and(|property| property.value == b"memory\0".as_slice())
    }

    fn cells(&self) -> (usize, usize) {
        let cells = |name: &[u8], default| {
            self.property(name)
                .and_then(|property| property.as_u32())
                .unwrap_or(default) as usize
        };

        (
            cells(b"#address-cells", DEFAULT_ADDRESS_CELLS),
            cells(b"#size-cells", DEFAULT_SIZE_CELLS),
        )
    }
}

pub struct Nodes<'a> {
    cursor: usize,
    end: usize,
    depth: u32,
    blob: &'a [u8],
    strings: &'a [u8],
}

impl<'a> Nodes<'a> {
    fn new(blob: &'a [u8], strings: &'a [u8], start: usize, end: usize) -> Self {
        Nodes {
            cursor: start,
            end,
            depth: 0,
            blob,
            strings,
        }
    }
}

impl<'a> Iterator for Nodes<'a> {
    type Item = Node<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.cursor + 4 > self.end {
                return None;
            }

            let token = be_read_from_arr(self.blob, self.cursor)?;
            match token {
                TOKEN_BEGIN_NODE => {
                    self.cursor += 4;
                    let name = name_at(self.blob, self.cursor)?;
                    self.cursor += round_up_to_4(name.len() + 1);
                    self.depth += 1;
                    return Some(Node {
                        name,
                        depth: self.depth - 1,
                        properties: Properties {
                            cursor: self.cursor,
                            end: self.end,
                            blob: self.blob,
                            strings: self.strings,
                        },
                    });
                }
                TOKEN_END_NODE => {
                    if self.depth > 0 {
                        self.depth -= 1;
                    }
                    self.cursor += 4;
                }
                TOKEN_PROP => {
                    self.cursor += 4;
                    let len = be_read_from_arr(self.blob, self.cursor)? as usize;
                    self.cursor += 8;

                    self.cursor += round_up_to_4(len);
                }
                TOKEN_NOOP => {
                    self.cursor += 4;
                }
                TOKEN_END => {
                    self.cursor += 4;
                    continue;
                }
                _ => {
                    return None;
                }
            };
        }
    }
}

#[derive(Clone, Copy)]
struct Properties<'a> {
    cursor: usize,
    end: usize,
    blob: &'a [u8],
    strings: &'a [u8],
}

impl<'a> Iterator for Properties<'a> {
    type Item = Property<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.cursor + 4 > self.end {
                return None;
            }

            let token = be_read_from_arr(self.blob, self.cursor)?;
            match token {
                TOKEN_PROP => {
                    self.cursor += 4;
                    let len = be_read_from_arr(self.blob, self.cursor)? as usize;
                    self.cursor += 4;
                    let name_offset = be_read_from_arr(self.blob, self.cursor)? as usize;
                    self.cursor += 4;

                    let name = name_at(self.strings, name_offset)?;
                    let value = self.blob.get(self.cursor..self.cursor + len)?;
                    self.cursor += round_up_to_4(len);

                    return Some(Property { name, value });
                }
                _ => {
                    return None;
                }
            };
        }
    }
}

/// The NUL-terminated name at `offset`, without its terminator.
fn name_at(blob: &[u8], offset: usize) -> Option<&[u8]> {
    let rest = blob.get(offset..)?;
    let len = rest.iter().position(|&byte| byte == 0)?;
    rest.get(..len)
}

fn round_up_to_4(n: usize) -> usize {
    (n + 3) & !3
}

struct Property<'a> {
    name: &'a [u8],
    value: &'a [u8],
}

impl<'a> Property<'a> {
    fn as_u32(&self) -> Option<u32> {
        be_read_from_arr(self.value, 0)
    }
}

/// Reads `cells` big-endian u32s as one number. Two cells is the widest a u64 holds.
fn read_cells(bytes: &[u8], cells: usize) -> Option<u64> {
    if cells == 0 || cells > 2 {
        return None;
    }

    let mut value = 0;
    for cell in bytes.get(..cells * 4)?.chunks_exact(4) {
        value = value << 32 | be_read_from_arr(cell, 0)? as u64;
    }

    Some(value)
}
