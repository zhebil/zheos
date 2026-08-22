use crate::{bit::compose_u32, mem::read_32, print, println};

fn be_read_from_arr(arr: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(arr[offset..offset + 4].try_into().unwrap())
}

const MAGIC_OFFSET: usize = 0x00;
const TOTAL_SIZE_OFFSET: usize = 0x04;
const OFF_DT_STRUCT_OFFSET: usize = 0x08;
const OFF_DT_STRINGS_OFFSET: usize = 0x0C;
const OFF_MEM_RSVMAP_OFFSET: usize = 0x10;
const VERSION_OFFSET: usize = 0x14;
const LAST_COMP_VERSION_OFFSET: usize = 0x18;
const BOOT_CPUID_OFFSET: usize = 0x1C;
const SIZE_DT_STRINGS_OFFSET: usize = 0x20;
const SIZE_DT_STRUCT_OFFSET: usize = 0x24;

const TOKEN_BEGIN_NODE: u32 = 0x01;
const TOKEN_END_NODE: u32 = 0x02;
const TOKEN_PROP: u32 = 0x03;
const TOKEN_NOOP: u32 = 0x04;
const TOKEN_END: u32 = 0x09;

pub struct Dtb<'a> {
    blob: &'a [u8],
}

impl<'a> Dtb<'a> {
    pub unsafe fn from_ptr(base: usize) -> Dtb<'a> {
        let total_size = u32::from_be(read_32(base + TOTAL_SIZE_OFFSET));
        let blob = unsafe { core::slice::from_raw_parts(base as *const u8, total_size as usize) };
        Dtb { blob }
    }

    fn u32_at(&self, offset: usize) -> u32 {
        be_read_from_arr(self.blob, offset)
    }

    pub fn magic(&self) -> u32 {
        self.u32_at(MAGIC_OFFSET)
    }

    pub fn total_size(&self) -> u32 {
        self.u32_at(TOTAL_SIZE_OFFSET)
    }

    pub fn off_dt_struct(&self) -> u32 {
        self.u32_at(OFF_DT_STRUCT_OFFSET)
    }

    pub fn off_dt_strings(&self) -> u32 {
        self.u32_at(OFF_DT_STRINGS_OFFSET)
    }

    pub fn off_mem_rsvmap(&self) -> u32 {
        self.u32_at(OFF_MEM_RSVMAP_OFFSET)
    }

    pub fn version(&self) -> u32 {
        self.u32_at(VERSION_OFFSET)
    }

    pub fn last_comp_version(&self) -> u32 {
        self.u32_at(LAST_COMP_VERSION_OFFSET)
    }

    pub fn boot_cpuid(&self) -> u32 {
        self.u32_at(BOOT_CPUID_OFFSET)
    }

    pub fn size_dt_strings(&self) -> u32 {
        self.u32_at(SIZE_DT_STRINGS_OFFSET)
    }

    pub fn size_dt_struct(&self) -> u32 {
        self.u32_at(SIZE_DT_STRUCT_OFFSET)
    }

    // Temporary
    pub fn print_header(&self) {
        println!("Magic: {:#010x}", self.magic());
        println!("Total size: {:#010x}", self.total_size());
        println!("Off dt struct: {:#010x}", self.off_dt_struct());
        println!("Off dt strings: {:#010x}", self.off_dt_strings());
        println!("Off mem rsvmap: {:#010x}", self.off_mem_rsvmap());
        println!("Version: {}", self.version());
        println!("Last comp version: {}", self.last_comp_version());
        println!("Boot cpuid: {}", self.boot_cpuid());
        println!("Size dt strings: {:#010x}", self.size_dt_strings());
        println!("Size dt struct: {:#010x}", self.size_dt_struct());
    }

    pub fn reservations(&self) -> Reservations<'_> {
        let start = self.off_mem_rsvmap() as usize;
        let end = self.total_size() as usize;
        Reservations {
            addr: start,
            end,
            blob: self.blob,
        }
    }

    fn strings(&self) -> &'a [u8] {
        let start = self.off_dt_strings() as usize;
        &self.blob[start..start + self.size_dt_strings() as usize]
    }

    fn print_indent(depth: u32) {
        print!("{:<width$}", " ", width = depth as usize * 2);
    }

    pub fn walk_struct(&self) {
        let start = self.off_dt_struct() as usize;
        let end = start + self.size_dt_struct() as usize;
        let nodes = Nodes::new(self.blob, self.strings(), start, end);

        nodes.for_each(|node| {
            if node.name == "" {
                println!("/");
            } else {
                Self::print_indent(node.depth);
                println!("{}", node.name);
            }

            node.properties.for_each(|(name, value)| {
                Self::print_indent(node.depth + 1);
                print!("{}: ", name);
                for i in 0..value.len() {
                    print!("{:02x}", value[i]);
                }
                println!();
            });
        });
    }
}

pub struct Reservations<'a> {
    addr: usize,
    end: usize,
    blob: &'a [u8],
}

const RESERVATION_ITEM_SIZE: usize = 16;

impl<'a> Iterator for Reservations<'a> {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<(usize, usize)> {
        if self.addr + RESERVATION_ITEM_SIZE > self.end {
            return None;
        }

        let addr_high = be_read_from_arr(self.blob, self.addr);
        let addr_low = be_read_from_arr(self.blob, self.addr + 4);
        let size_high = be_read_from_arr(self.blob, self.addr + 8);
        let size_low = be_read_from_arr(self.blob, self.addr + 12);

        let addr = compose_u32(addr_high, addr_low);
        let size = compose_u32(size_high, size_low);

        if addr == 0 && size == 0 {
            return None;
        }

        self.addr += RESERVATION_ITEM_SIZE;

        Some((addr as usize, size as usize))
    }
}

pub struct Node<'a> {
    name: &'a str,
    depth: u32,
    properties: Properties<'a>,
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

            let token = be_read_from_arr(self.blob, self.cursor);
            match token {
                TOKEN_BEGIN_NODE => {
                    self.cursor += 4;
                    let name_len = find_len(self.blob, self.cursor)?;
                    let name = get_name(self.blob, self.cursor, name_len);
                    self.cursor += round_up_to_4(name_len + 1);
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
                    let len = be_read_from_arr(self.blob, self.cursor);
                    self.cursor += 4;
                    let _name_offset = be_read_from_arr(self.blob, self.cursor);
                    self.cursor += 4;

                    self.cursor += round_up_to_4(len as usize);
                }
                TOKEN_NOOP => {
                    self.cursor += 4;
                }
                TOKEN_END => {
                    self.cursor += 4;
                    continue;
                }
                _ => {
                    println!("Unknown token: {:#x}", token);
                    return None;
                }
            };
        }
    }
}

struct Properties<'a> {
    cursor: usize,
    end: usize,
    blob: &'a [u8],
    strings: &'a [u8],
}

impl<'a> Iterator for Properties<'a> {
    type Item = (&'a str, &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.cursor + 4 > self.end {
                return None;
            }

            let token = be_read_from_arr(self.blob, self.cursor);
            match token {
                TOKEN_PROP => {
                    self.cursor += 4;
                    let len = be_read_from_arr(self.blob, self.cursor);
                    self.cursor += 4;
                    let name_offset = be_read_from_arr(self.blob, self.cursor);

                    let name_len = find_len(self.strings, name_offset as usize)?;
                    let name = get_name(self.strings, name_offset as usize, name_len);

                    self.cursor += 4;

                    let value = self.blob.get(self.cursor..self.cursor + len as usize)?;
                    self.cursor += round_up_to_4(len as usize);

                    return Some((name, value));
                }
                _ => {
                    return None;
                }
            };
        }
    }
}

fn find_len(blob: &[u8], addr: usize) -> Option<usize> {
    blob[addr..].iter().position(|&b| b == 0)
}

fn round_up_to_4(n: usize) -> usize {
    (n + 3) & !3
}

fn get_name<'a>(blob: &'a [u8], addr: usize, len: usize) -> &'a str {
    str::from_utf8(&blob[addr..addr + len]).unwrap_or("<invalid utf8>")
}
