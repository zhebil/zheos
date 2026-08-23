use crate::dtb::cursor::Cursor;
use crate::dtb::{Region, Strings};

// Defaults mandated by the spec for a node that declares neither.
const DEFAULT_ADDRESS_CELLS: u32 = 2;
const DEFAULT_SIZE_CELLS: u32 = 1;

// `#interrupt-cells` on the GIC node: <kind, number, flags>.
const INTERRUPT_CELLS: usize = 3;

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

    #[inline(always)]
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

    #[inline(never)]
    pub fn property(&self, name: &[u8]) -> Option<Property<'a>> {
        let mut properties = self.properties;
        properties.find(|property| property.name == name)
    }

    pub fn is_memory(&self) -> bool {
        self.property(b"device_type")
            .is_some_and(|property| property.value == b"memory\0".as_slice())
    }

    /// `compatible` is a list of NUL-terminated strings, not one string.
    pub fn is_compatible(&self, compatible: &[u8]) -> bool {
        self.property(b"compatible").is_some_and(|property| {
            property
                .value
                .split(|&byte| byte == 0)
                .any(|entry| entry == compatible)
        })
    }

    /// One entry of `reg`, decoded with the parent's cell counts.
    pub fn region(&self, index: usize, (address, size): (usize, usize)) -> Option<Region> {
        let reg = self.property(b"reg")?;
        let entry = reg.value.get(index * (address + size) * 4..)?;

        Some(Region {
            base: read_cells(entry, address)? as usize,
            size: read_cells(entry.get(address * 4..)?, size)? as usize,
        })
    }

    /// One entry of `interrupts` as `(kind, number)`. The third cell is trigger
    /// flags, which one core behind a GICv2 has no use for.
    pub fn interrupt(&self, index: usize) -> Option<(u32, u32)> {
        let interrupts = self.property(b"interrupts")?;
        let entry = interrupts.value.get(index * INTERRUPT_CELLS * 4..)?;

        let mut cursor = Cursor::new(entry, 0);

        Some((cursor.read_u32()?, cursor.read_u32()?))
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
        Cursor::new(self.value, 0).read_u32()
    }
}

/// Reads `cells` big-endian u32s as one number. Two cells is the widest a u64 holds.
fn read_cells(bytes: &[u8], cells: usize) -> Option<u64> {
    if cells > 2 {
        return None;
    }

    let mut cursor = Cursor::new(bytes, 0);

    let mut value = 0;
    for _ in 0..cells {
        value = value << 32 | cursor.read_u32()? as u64;
    }

    Some(value)
}
