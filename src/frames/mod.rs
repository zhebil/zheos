mod lists;

use core::fmt::Display;

use crate::{
    frames::lists::FreeLists,
    memory::{
        map::MemoryMap,
        pages::{Entry, Pages},
        pfn::Pfn,
    },
};

pub const MAX_ORDER: usize = 10;

pub struct Frames {
    lists: FreeLists,
    total: usize,
}

impl Frames {
    pub const fn empty() -> Self {
        Self {
            lists: FreeLists::empty(),
            total: 0,
        }
    }

    pub fn new(map: &MemoryMap, pages: &mut Pages) -> Frames {
        let mut frames = Frames {
            lists: FreeLists::empty(),
            total: pages.len(),
        };

        frames.seed(map, pages);

        frames
    }

    pub fn free_blocks(&self, order: usize) -> usize {
        self.lists.blocks(order)
    }

    pub fn alloc(&mut self, pages: &mut Pages, order: usize) -> Option<Pfn> {
        let mut available_order = (order..=MAX_ORDER).find(|&o| self.lists.head(o).is_some())?;

        let pfn = self.lists.pop(available_order)?;

        while available_order > order {
            available_order -= 1;
            self.push(pages, pfn.offset(1 << available_order), available_order);
        }

        pages.write(
            pfn,
            Entry::Buddy {
                free: false,
                order: order as u8,
            },
        );

        Some(pfn)
    }

    pub fn free(&mut self, pages: &mut Pages, mut pfn: Pfn) {
        let mut order = match pages.read(pfn) {
            Entry::Buddy { free: true, .. } => {
                panic!("freeing already freed pfn. addr: {:#x}", pfn.to_addr())
            }
            Entry::Buddy { order, .. } => order as usize,
            Entry::Slab { .. } => 0,
        };

        while order < MAX_ORDER {
            let buddy = pfn.buddy(order);

            if !pages.covers().contains(buddy) {
                break;
            }

            let Entry::Buddy {
                free: true,
                order: buddy_order,
            } = pages.read(buddy)
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

        self.push(pages, pfn, order);
    }

    fn seed(&mut self, map: &MemoryMap, pages: &mut Pages) {
        for run in map.unreserved() {
            let mut block = run.start;

            while block < run.end {
                // find largest power-of-2 block that fits in this run and is aligned
                let order = MAX_ORDER
                    .min(block.alignment_order())
                    .min(block.pages_until(run.end).ilog2() as usize);

                self.push(pages, block, order);
                block = block.offset(1 << order);
            }
        }
    }

    fn push(&mut self, pages: &mut Pages, pfn: Pfn, order: usize) {
        self.lists.push(pfn, order);

        pages.write(
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
        write!(f, "{} of {} pages free", self.lists.pages(), self.total)
    }
}
