use cursor::Cursor;
use header::Header;
use structure::{Nodes, read_cells};

mod cursor;
mod header;
mod structure;

const ROOT_CHILD_DEPTH: u32 = 1;

pub struct Memory {
    pub addr: usize,
    pub size: usize,
}

pub struct Dtb<'a> {
    blob: &'a [u8],
    header: Header<'a>,
    strings: Strings<'a>,
}

impl<'a> Dtb<'a> {
    pub unsafe fn from_ptr(base: usize) -> Option<Dtb<'a>> {
        let header = unsafe { Header::from_ptr(base)? };

        let total_size = header.total_size() as usize;

        let blob = unsafe { core::slice::from_raw_parts(base as *const u8, total_size) };

        let strings_start = header.off_dt_strings() as usize;
        let strings_end = strings_start + header.size_dt_strings() as usize;

        let strings_blob = blob.get(strings_start..strings_end)?;

        let strings = Strings { strings_blob };

        Some(Dtb {
            blob,
            header,
            strings,
        })
    }

    pub fn nodes(&self) -> Option<Nodes<'a>> {
        let start = self.header.off_dt_struct() as usize;
        let end = start + self.header.size_dt_struct() as usize;
        let struct_blob = self.blob.get(start..end)?;
        Some(Nodes::new(struct_blob, self.strings))
    }

    pub fn memory(&self) -> Option<Memory> {
        let mut nodes = self.nodes()?;

        // reg is an address in the parent's space, and /memory is a child of the root.
        let (address_cells, size_cells) = nodes.next()?.cells();

        let reg = nodes
            .find(|node| node.depth() == ROOT_CHILD_DEPTH && node.is_memory())?
            .property(b"reg")?;

        let addr = read_cells(reg.value, address_cells)?;
        let size = read_cells(reg.value.get(address_cells * 4..)?, size_cells)?;

        Some(Memory {
            addr: addr as usize,
            size: size as usize,
        })
    }
}

#[derive(Copy, Clone)]
pub struct Strings<'a> {
    strings_blob: &'a [u8],
}

impl<'a> Strings<'a> {
    pub fn string_at(&self, offset: usize) -> Option<&'a [u8]> {
        let mut cursor = Cursor::new(self.strings_blob, offset);
        cursor.cstring()
    }
}
