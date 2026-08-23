use crate::dtb::Strings;
use crate::dtb::cursor::Cursor;

// Defaults mandated by the spec for a node that declares neither.
const DEFAULT_ADDRESS_CELLS: u32 = 2;
const DEFAULT_SIZE_CELLS: u32 = 1;

const TOKEN_BEGIN_NODE: u32 = 0x01;
const TOKEN_END_NODE: u32 = 0x02;
const TOKEN_PROP: u32 = 0x03;
const TOKEN_NOOP: u32 = 0x04;
const TOKEN_END: u32 = 0x09;

pub struct Nodes<'a> {
    cursor: Cursor<'a>,
    strings: Strings<'a>,
    depth: u32,
}

impl<'a> Nodes<'a> {
    pub fn new(blob: &'a [u8], strings: Strings<'a>) -> Self {
        Nodes {
            cursor: Cursor::new(blob, 0),
            strings,
            depth: 0,
        }
    }
}

impl<'a> Iterator for Nodes<'a> {
    type Item = Node<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.cursor.done() {
                return None;
            }

            let token = self.cursor.read_u32()?;
            match token {
                TOKEN_BEGIN_NODE => {
                    // Ignore name
                    self.cursor.cstring()?;

                    self.cursor.align_u32();

                    let depth = self.depth;
                    self.depth += 1;

                    return Some(Node {
                        depth,
                        properties: Properties {
                            cursor: self.cursor,
                            strings: self.strings,
                        },
                    });
                }
                TOKEN_PROP => {
                    let len = self.cursor.read_u32()? as usize;
                    // Skip name offset.
                    self.cursor.read_u32()?;

                    self.cursor.bytes(len)?;
                    self.cursor.align_u32();
                }
                TOKEN_END_NODE => {
                    self.depth = self.depth.saturating_sub(1);
                }
                TOKEN_NOOP | TOKEN_END => {
                    continue;
                }
                _ => {
                    return None;
                }
            };
        }
    }
}

pub struct Node<'a> {
    depth: u32,
    properties: Properties<'a>,
}

impl<'a> Node<'a> {
    pub fn depth(&self) -> u32 {
        self.depth
    }

    pub fn property(&self, name: &[u8]) -> Option<Property<'a>> {
        let mut properties = self.properties;
        properties.find(|property| property.name == name)
    }

    pub fn is_memory(&self) -> bool {
        let is_memory = self
            .property(b"device_type")
            .is_some_and(|property| property.value == b"memory\0".as_slice());

        is_memory
    }

    pub fn cells(&self) -> (usize, usize) {
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

#[derive(Clone, Copy)]
struct Properties<'a> {
    cursor: Cursor<'a>,
    strings: Strings<'a>,
}

impl<'a> Iterator for Properties<'a> {
    type Item = Property<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.cursor.done() {
                return None;
            }

            let token = self.cursor.read_u32()?;
            match token {
                TOKEN_PROP => {
                    let len = self.cursor.read_u32()? as usize;

                    let name_offset = self.cursor.read_u32()? as usize;

                    let name = self.strings.string_at(name_offset)?;
                    let value = self.cursor.bytes(len)?;

                    self.cursor.align_u32();

                    return Some(Property { name, value });
                }
                _ => {
                    return None;
                }
            };
        }
    }
}

pub struct Property<'a> {
    pub name: &'a [u8],
    pub value: &'a [u8],
}

impl<'a> Property<'a> {
    fn as_u32(&self) -> Option<u32> {
        be_read_from_arr(self.value, 0)
    }
}

/// Reads `cells` big-endian u32s as one number. Two cells is the widest a u64 holds.
pub fn read_cells(bytes: &[u8], cells: usize) -> Option<u64> {
    if cells == 0 || cells > 2 {
        return None;
    }

    let mut value = 0;
    for cell in bytes.get(..cells * 4)?.chunks_exact(4) {
        value = value << 32 | be_read_from_arr(cell, 0)? as u64;
    }

    Some(value)
}

pub fn be_read_from_arr(arr: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(*arr.get(offset..)?.first_chunk()?))
}
