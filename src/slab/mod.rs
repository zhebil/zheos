mod cache;

use core::alloc::Layout;

pub use cache::Cache;

use crate::memory::{
    pages::{Entry, Pages, Slot},
    pfn::{PAGE_SIZE, Pfn},
};

const CLASSES_COUNT: usize = 9; // 8, 16, 32, 64, 128, 256, 512, 1024, 2048

const CLASSES: [usize; CLASSES_COUNT] = [8, 16, 32, 64, 128, 256, 512, 1024, 2048];

pub fn class_of(layout: Layout) -> Option<usize> {
    let size = layout.size();
    let align = layout.align();

    for (i, &c) in CLASSES.iter().enumerate() {
        if c < size {
            continue;
        }

        if !c.is_multiple_of(align) {
            continue;
        }
        return Some(i);
    }

    None
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Slab {
    pfn: Pfn,
}

impl Slab {
    pub fn at(pages: &Pages, pfn: Pfn) -> Option<Slab> {
        matches!(pages.read(pfn), Entry::Slab { .. }).then_some(Slab { pfn })
    }

    pub fn from_addr(pages: &Pages, addr: usize) -> Option<Slab> {
        Slab::at(pages, Pfn::from_addr_down(addr))
    }

    pub fn init(pages: &mut Pages, pfn: Pfn, class_idx: usize) -> Option<Slab> {
        let size = *CLASSES.get(class_idx)?;

        let slab = Slab { pfn };
        slab.link_slots(size);

        pages.write(
            pfn,
            Entry::Slab {
                class: class_idx as u8,
                // First slot is free
                free_head: Slot::new(0),
                in_use: 0,
                next_partial: None,
                prev_partial: None,
            },
        );

        Some(slab)
    }

    fn link_slots(&self, size: usize) {
        let base = self.pfn.to_addr();
        let number_of_slots = PAGE_SIZE / size;

        for i in 0..number_of_slots {
            let current_base = base + i * size;
            let next = if i < number_of_slots - 1 {
                Slot::new(i + 1)
            } else {
                None
            };

            unsafe { *(current_base as *mut u16) = Slot::to_raw(next) };
        }
    }

    pub fn alloc(&self, pages: &mut Pages) -> Option<usize> {
        let Entry::Slab {
            class,
            free_head,
            in_use,
            next_partial,
            prev_partial,
        } = pages.read(self.pfn)
        else {
            return None;
        };

        // No free space
        let head = free_head?;
        let size = *CLASSES.get(class as usize)?;

        if head.index() >= PAGE_SIZE / size {
            return None;
        }

        let address = self.pfn.to_addr() + head.index() * size;
        let next_free_head = Slot::from_raw(unsafe { *(address as *mut u16) });

        pages.write(
            self.pfn,
            Entry::Slab {
                class,
                free_head: next_free_head,
                in_use: in_use + 1,
                next_partial,
                prev_partial,
            },
        );

        Some(address)
    }

    pub fn free(&self, pages: &mut Pages, address: usize) -> Option<()> {
        let Entry::Slab {
            class,
            free_head,
            in_use,
            next_partial,
            prev_partial,
        } = pages.read(self.pfn)
        else {
            return None;
        };

        let size = *CLASSES.get(class as usize)?;
        let base = self.pfn.to_addr();

        // Make sure address is inside the slab page
        if address < base {
            return None;
        }

        let offset = address - base;

        if offset >= PAGE_SIZE || !offset.is_multiple_of(size) {
            return None;
        }

        let idx = Slot::new(offset / size)?;

        // Make sure the slot is not already freed or empty
        if in_use == 0 || Some(idx) == free_head {
            return None;
        }

        // Make the freed slot point to the previous free slot
        unsafe { (*(address as *mut u16)) = Slot::to_raw(free_head) };

        pages.write(
            self.pfn,
            Entry::Slab {
                class,
                free_head: Some(idx),
                in_use: in_use - 1,
                next_partial,
                prev_partial,
            },
        );

        Some(())
    }

    fn class(&self, pages: &Pages) -> Option<usize> {
        match pages.read(self.pfn) {
            Entry::Slab { class, .. } => Some(class as usize),
            Entry::Buddy { .. } => None,
        }
    }

    fn links(&self, pages: &Pages) -> (Option<Slab>, Option<Slab>) {
        let Entry::Slab {
            next_partial,
            prev_partial,
            ..
        } = pages.read(self.pfn)
        else {
            return (None, None);
        };

        (
            next_partial
                .map(|index| pages.pfn_of(index))
                .and_then(|pfn| Slab::at(pages, pfn)),
            prev_partial
                .map(|index| pages.pfn_of(index))
                .and_then(|pfn| Slab::at(pages, pfn)),
        )
    }

    fn set_next(&self, pages: &mut Pages, value: Option<Slab>) {
        let Entry::Slab {
            class,
            free_head,
            in_use,
            prev_partial,
            ..
        } = pages.read(self.pfn)
        else {
            return;
        };

        let next_partial = value.map(|slab| pages.index_of(slab.pfn));

        pages.write(
            self.pfn,
            Entry::Slab {
                class,
                free_head,
                in_use,
                next_partial,
                prev_partial,
            },
        );
    }

    fn set_prev(&self, pages: &mut Pages, value: Option<Slab>) {
        let Entry::Slab {
            class,
            free_head,
            in_use,
            next_partial,
            prev_partial: _,
        } = pages.read(self.pfn)
        else {
            return;
        };

        let prev_partial = value.map(|slab| pages.index_of(slab.pfn));

        pages.write(
            self.pfn,
            Entry::Slab {
                class,
                free_head,
                in_use,
                next_partial,
                prev_partial,
            },
        );
    }

    fn is_full(&self, pages: &Pages) -> bool {
        let Entry::Slab { free_head, .. } = pages.read(self.pfn) else {
            return false;
        };

        free_head.is_none()
    }

    fn is_empty(&self, pages: &Pages) -> bool {
        let Entry::Slab { in_use, .. } = pages.read(self.pfn) else {
            return false;
        };

        in_use == 0
    }
}
