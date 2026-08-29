mod lists;
mod metadata;

use core::fmt::Display;

pub use metadata::Entry;

use crate::{
    frames::{lists::FreeLists, metadata::Metadata},
    memory::{map::MemoryMap, pfn::Pfn},
};

pub const MAX_ORDER: usize = 10;

pub struct Frames {
    metadata: Metadata,
    lists: FreeLists,
}

impl Frames {
    pub fn new(map: &mut MemoryMap) -> Option<Frames> {
        let arena = map.arena();

        let metadata_size = Metadata::required_size(arena);

        let run = map.unreserved().find(|run| run.pages() >= metadata_size)?;

        let metadata = Metadata::new(arena, run.start)?;

        map.reserve(metadata.region).ok()?;

        let mut frames = Frames {
            metadata,
            lists: FreeLists::empty(),
        };

        frames.seed(map);

        Some(frames)
    }

    pub fn page(&self, pfn: Pfn) -> Entry {
        self.metadata.read(pfn)
    }

    pub fn set_page(&mut self, pfn: Pfn, entry: Entry) {
        self.metadata.write(pfn, entry);
    }

    pub fn free_blocks(&self, order: usize) -> usize {
        self.lists.blocks(order)
    }

    pub fn alloc(&mut self, order: usize) -> Option<Pfn> {
        let mut available_order = (order..=MAX_ORDER).find(|&o| self.lists.head(o).is_some())?;

        let pfn = self.lists.pop(available_order)?;

        while available_order > order {
            available_order -= 1;
            self.push(pfn.offset(1 << available_order), available_order);
        }

        self.metadata.write(
            pfn,
            Entry::Buddy {
                free: false,
                order: order as u8,
            },
        );

        Some(pfn)
    }

    pub fn free(&mut self, mut pfn: Pfn) {
        let mut order = match self.metadata.read(pfn) {
            Entry::Buddy { free: true, .. } => return,
            Entry::Buddy { order, .. } => order as usize,
            Entry::Slab { .. } => 0,
        };

        while order < MAX_ORDER {
            let buddy = pfn.buddy(order);

            if !self.metadata.covers().contains(buddy) {
                break;
            }

            let Entry::Buddy {
                free: true,
                order: buddy_order,
            } = self.metadata.read(buddy)
            else {
                break;
            };

            if buddy_order != order as u8 {
                break;
            }

            self.lists.unlink(buddy, order);

            pfn = pfn.min(buddy);
            order += 1;
        }

        self.push(pfn, order);
    }

    fn seed(&mut self, map: &MemoryMap) {
        for run in map.unreserved() {
            let mut block = run.start;

            while block < run.end {
                // find largest power-of-2 block that fits in this run and is aligned
                let order = MAX_ORDER
                    .min(block.alignment_order())
                    .min(block.pages_until(run.end).ilog2() as usize);

                self.push(block, order);
                block = block.offset(1 << order);
            }
        }
    }

    fn push(&mut self, pfn: Pfn, order: usize) {
        self.lists.push(pfn, order);

        self.metadata.write(
            pfn,
            Entry::Buddy {
                free: true,
                order: order as u8,
            },
        );
    }
}

impl Display for Frames {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} of {} pages free",
            self.lists.pages(),
            self.metadata.pages()
        )
    }
}
