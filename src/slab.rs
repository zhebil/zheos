use core::{alloc::Layout, cmp::max};

use crate::{
    frames::Frames,
    memory::{
        pages::{Entry, Pages, Slot},
        pfn::{PAGE_SIZE, Pfn},
    },
};

pub const CLASSES_COUNT: usize = 9; // 8, 16, 32, 64, 128, 256, 512, 1024, 2048

pub const CLASSES: [usize; CLASSES_COUNT] = [8, 16, 32, 64, 128, 256, 512, 1024, 2048];

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

pub fn init(pages: &mut Pages, page_pfn: Pfn, class_idx: usize) -> Option<()> {
    let size = CLASSES.get(class_idx)?;

    let base = page_pfn.to_addr();

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

    pages.write(
        page_pfn,
        Entry::Slab {
            class: class_idx as u8,
            // First slot is free
            free_head: Slot::new(0),
            in_use: 0,
            next_partial: None,
            prev_partial: None,
        },
    );

    Some(())
}

pub fn alloc(pages: &mut Pages, pfn: Pfn) -> Option<usize> {
    match pages.read(pfn) {
        Entry::Slab {
            class,
            free_head,
            in_use,
            next_partial,
            prev_partial,
        } => {
            // No free space
            let head = free_head?;
            let size = CLASSES.get(class as usize)?;

            if head.index() >= PAGE_SIZE / size {
                return None;
            }

            let address = pfn.to_addr() + head.index() * size;
            let next_free_head = Slot::from_raw(unsafe { *(address as *mut u16) });

            pages.write(
                pfn,
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
        // It is not a slab page
        Entry::Buddy { .. } => None,
    }
}

pub fn free(pages: &mut Pages, pfn: Pfn, address: usize) -> Option<()> {
    let Entry::Slab {
        class,
        free_head,
        in_use,
        next_partial,
        prev_partial,
    } = pages.read(pfn)
    else {
        return None;
    };

    let size = CLASSES.get(class as usize)?;
    let base = pfn.to_addr();

    // Make sure address is inside the slab page
    if address < base {
        return None;
    }

    let offset = address - base;

    if offset >= PAGE_SIZE || !offset.is_multiple_of(*size) {
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
        pfn,
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

pub fn chain_len(pages: &Pages, pfn: Pfn) -> Option<usize> {
    let Entry::Slab {
        class, free_head, ..
    } = pages.read(pfn)
    else {
        return None;
    };

    let size = CLASSES.get(class as usize)?;
    let slots = PAGE_SIZE / size;
    let base = pfn.to_addr();

    let mut index = free_head;
    let mut count = 0;

    while let Some(slot) = index {
        if slot.index() >= slots || count == slots {
            return None;
        }

        index = Slot::from_raw(unsafe { *((base + slot.index() * size) as *const u16) });
        count += 1;
    }

    Some(count)
}

pub struct Cache {
    pub heads: [Option<Pfn>; CLASSES_COUNT],
}

impl Cache {
    pub fn alloc_layout(
        &mut self,
        pages: &mut Pages,
        frames: &mut Frames,
        layout: Layout,
    ) -> Option<usize> {
        match class_of(layout) {
            Some(class_idx) => self.alloc_slab(pages, frames, class_idx),
            None => {
                let size_pages = layout.size().div_ceil(PAGE_SIZE);
                let align_pages = layout.align().div_ceil(PAGE_SIZE);
                let order = max(size_pages, align_pages).next_power_of_two().ilog2();

                let page_pfn = frames.alloc(pages, order as usize)?;
                Some(page_pfn.to_addr())
            }
        }
    }

    pub fn free_layout(
        &mut self,
        pages: &mut Pages,
        frames: &mut Frames,
        address: usize,
        layout: Layout,
    ) -> Option<()> {
        match class_of(layout) {
            Some(_) => self.free_slab(pages, frames, address),
            None => {
                frames.free(pages, Pfn::from_addr_down(address));
                Some(())
            }
        }
    }

    fn alloc_slab(
        &mut self,
        pages: &mut Pages,
        frames: &mut Frames,
        class_idx: usize,
    ) -> Option<usize> {
        let head = self.heads.get(class_idx)?;
        let head = match *head {
            Some(head) => head,
            None => {
                let page_pfn = frames.alloc(pages, 0)?;
                init(pages, page_pfn, class_idx)?;
                self.heads[class_idx] = Some(page_pfn);
                page_pfn
            }
        };

        let address = alloc(pages, head)?;

        let Entry::Slab { free_head, .. } = pages.read(head) else {
            return None;
        };

        if free_head.is_none() {
            self.pop(pages, class_idx)?;
        };

        Some(address)
    }

    fn free_slab(&mut self, pages: &mut Pages, frames: &mut Frames, address: usize) -> Option<()> {
        let pfn = Pfn::from_addr_down(address);

        let Entry::Slab {
            free_head, class, ..
        } = pages.read(pfn)
        else {
            return None;
        };

        free(pages, pfn, address)?;

        if free_head.is_none() {
            self.push(pages, class as usize, pfn)?;
        }

        let Entry::Slab { in_use, .. } = pages.read(pfn) else {
            return None;
        };

        if in_use == 0 {
            self.unlink(pages, pfn);
            frames.free(pages, pfn);
        }

        Some(())
    }

    fn links(pages: &Pages, pfn: Pfn) -> (Option<Pfn>, Option<Pfn>) {
        let Entry::Slab {
            next_partial,
            prev_partial,
            ..
        } = pages.read(pfn)
        else {
            return (None, None);
        };

        (
            next_partial.map(|index| pages.pfn_of(index)),
            prev_partial.map(|index| pages.pfn_of(index)),
        )
    }

    fn set_next(&mut self, pages: &mut Pages, pfn: Pfn, value: Option<Pfn>) {
        let Entry::Slab {
            class,
            free_head,
            in_use,
            prev_partial,
            ..
        } = pages.read(pfn)
        else {
            return;
        };

        let next_partial = value.map(|pfn| pages.index_of(pfn));

        pages.write(
            pfn,
            Entry::Slab {
                class,
                free_head,
                in_use,
                next_partial,
                prev_partial,
            },
        );
    }

    fn set_prev(&mut self, pages: &mut Pages, pfn: Pfn, value: Option<Pfn>) {
        let Entry::Slab {
            class,
            free_head,
            in_use,
            next_partial,
            prev_partial: _,
        } = pages.read(pfn)
        else {
            return;
        };

        let prev_partial = value.map(|pfn| pages.index_of(pfn));

        pages.write(
            pfn,
            Entry::Slab {
                class,
                free_head,
                in_use,
                next_partial,
                prev_partial,
            },
        );
    }

    fn push(&mut self, pages: &mut Pages, class_idx: usize, new_pfn: Pfn) -> Option<()> {
        let head = *self.heads.get(class_idx)?;

        self.set_next(pages, new_pfn, head);
        self.set_prev(pages, new_pfn, None);

        if let Some(head) = head {
            self.set_prev(pages, head, Some(new_pfn));
        }

        self.heads[class_idx] = Some(new_pfn);

        Some(())
    }

    fn unlink(&mut self, pages: &mut Pages, pfn: Pfn) -> Option<()> {
        let Entry::Slab { class, .. } = pages.read(pfn) else {
            return None;
        };

        let class_idx = class as usize;
        let (next, prev) = Cache::links(pages, pfn);

        if let Some(prev) = prev {
            self.set_next(pages, prev, next);
        } else if self.heads.get(class_idx).copied().flatten() == Some(pfn) {
            self.heads[class_idx] = next;
        } else {
            return None;
        }

        if let Some(next) = next {
            self.set_prev(pages, next, prev);
        }

        self.set_next(pages, pfn, None);
        self.set_prev(pages, pfn, None);

        Some(())
    }

    fn pop(&mut self, pages: &mut Pages, class_idx: usize) -> Option<Pfn> {
        let head = self.heads.get(class_idx).cloned().flatten()?;

        self.unlink(pages, head);

        Some(head)
    }
}
