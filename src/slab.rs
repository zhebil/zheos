use core::{alloc::Layout, cmp::max, fmt::Display};

use crate::{
    frames::Frames,
    memory::{
        map::MemoryMap,
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

pub struct Cache {
    pub heads: [Option<Slab>; CLASSES_COUNT],
}

impl Cache {
    pub fn new() -> Self {
        Self {
            heads: [None; CLASSES_COUNT],
        }
    }

    fn alloc_slab(
        &mut self,
        pages: &mut Pages,
        frames: &mut Frames,
        class_idx: usize,
    ) -> Option<usize> {
        let slab = match self.heads.get(class_idx)? {
            Some(slab) => *slab,
            None => {
                let page_pfn = frames.alloc(pages, 0)?;
                let slab = Slab::init(pages, page_pfn, class_idx)?;
                self.heads[class_idx] = Some(slab);
                slab
            }
        };

        let address = slab.alloc(pages)?;

        if slab.is_full(pages) {
            self.pop(pages, class_idx)?;
        };

        Some(address)
    }

    fn free_slab(&mut self, pages: &mut Pages, frames: &mut Frames, address: usize) -> Option<()> {
        let slab = Slab::from_addr(pages, address)?;
        let class_idx = slab.class(pages)?;

        let was_full = slab.is_full(pages);

        slab.free(pages, address)?;

        if was_full {
            self.push(pages, class_idx, slab)?;
        }

        if slab.is_empty(pages) {
            self.unlink(pages, slab);
            frames.free(pages, slab.pfn);
        }

        Some(())
    }

    fn push(&mut self, pages: &mut Pages, class_idx: usize, slab: Slab) -> Option<()> {
        let old_slab = *self.heads.get(class_idx)?;

        slab.set_next(pages, old_slab);
        slab.set_prev(pages, None);

        if let Some(old_slab) = old_slab {
            old_slab.set_prev(pages, Some(slab));
        }

        self.heads[class_idx] = Some(slab);

        Some(())
    }

    fn unlink(&mut self, pages: &mut Pages, slab: Slab) -> Option<()> {
        let class_idx = slab.class(pages)?;
        let (next, prev) = slab.links(pages);

        if let Some(prev_slab) = prev {
            prev_slab.set_next(pages, next);
        } else if self.heads.get(class_idx).copied().flatten() == Some(slab) {
            self.heads[class_idx] = next;
        } else {
            return None;
        }

        if let Some(next_slab) = next {
            next_slab.set_prev(pages, prev);
        }

        slab.set_next(pages, None);
        slab.set_prev(pages, None);

        Some(())
    }

    fn pop(&mut self, pages: &mut Pages, class_idx: usize) -> Option<Slab> {
        let head = self.heads.get(class_idx).copied().flatten()?;

        self.unlink(pages, head);

        Some(head)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slab {
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

pub struct Heap {
    pages: Pages,
    frames: Frames,
    cache: Cache,
}

impl Heap {
    pub fn new(map: &mut MemoryMap) -> Option<Self> {
        let mut pages = Pages::new(map)?;
        let frames = Frames::new(map, &mut pages);
        let cache = Cache::new();

        Some(Self {
            pages,
            frames,
            cache,
        })
    }

    pub fn alloc_layout(&mut self, layout: Layout) -> Option<usize> {
        match class_of(layout) {
            Some(class_idx) => self.slab_alloc(class_idx),
            None => {
                let size_pages = layout.size().div_ceil(PAGE_SIZE);
                let align_pages = layout.align().div_ceil(PAGE_SIZE);
                let order = max(size_pages, align_pages).next_power_of_two().ilog2();

                self.alloc_pages(order)
            }
        }
    }

    pub fn free_layout(&mut self, address: usize, layout: Layout) -> Option<()> {
        match class_of(layout) {
            Some(_) => self.slab_free(address),
            None => self.free_pages(address),
        }
    }

    fn alloc_pages(&mut self, order: u32) -> Option<usize> {
        let page_pfn = self.frames.alloc(&mut self.pages, order as usize)?;
        Some(page_pfn.to_addr())
    }

    fn free_pages(&mut self, address: usize) -> Option<()> {
        self.frames
            .free(&mut self.pages, Pfn::from_addr_down(address));
        Some(())
    }

    pub fn frames(&self) -> &Frames {
        &self.frames
    }

    fn slab_alloc(&mut self, class_idx: usize) -> Option<usize> {
        self.cache
            .alloc_slab(&mut self.pages, &mut self.frames, class_idx)
    }

    fn slab_free(&mut self, address: usize) -> Option<()> {
        self.cache
            .free_slab(&mut self.pages, &mut self.frames, address)
    }
}

impl Display for Heap {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.frames)
    }
}
