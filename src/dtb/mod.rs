use crate::region::Region;

use cursor::Cursor;
use header::Header;
use structure::Nodes;

pub use structure::Node;

mod cursor;
mod header;
mod structure;

const ROOT_CHILD_DEPTH: u32 = 1;

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

    /// A root child's `reg` is decoded with the root's cell counts, so every
    /// lookup needs these first.
    pub fn root_cells(&self) -> Option<(usize, usize)> {
        Some(self.nodes()?.next()?.cells())
    }

    pub fn find_compatible(&self, compatible: &[u8]) -> Option<Node<'a>> {
        self.find(|node| node.is_compatible(compatible))
    }

    pub fn find_memory(&self) -> Option<Node<'a>> {
        self.find(|node| node.is_memory())
    }

    pub fn region(&self) -> Region {
        Region {
            base: self.blob.as_ptr() as usize,
            size: self.blob.len(),
        }
    }

    /// Root children only. Every device on `virt` is one, and anything deeper
    /// would need a stack of its ancestors' cell counts to read `reg` at all.
    fn find(&self, predicate: impl Fn(&Node<'a>) -> bool) -> Option<Node<'a>> {
        self.nodes()?
            .find(|node| node.depth() == ROOT_CHILD_DEPTH && predicate(node))
    }

    fn nodes(&self) -> Option<Nodes<'a>> {
        let start = self.header.off_dt_struct() as usize;
        let end = start + self.header.size_dt_struct() as usize;
        let struct_blob = self.blob.get(start..end)?;
        Some(Nodes::new(struct_blob, self.strings))
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
