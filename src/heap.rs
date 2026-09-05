use core::{alloc::Layout, cmp::max, fmt::Display};

use crate::{
    frames::Frames,
    memory::{
        map::MemoryMap,
        pages::Pages,
        pfn::{PAGE_SIZE, Pfn},
    },
    slab::{Cache, class_of},
};

pub struct Heap {
    pages: Pages,
    frames: Frames,
    cache: Cache,
}

impl Heap {
    pub const fn empty() -> Self {
        Self {
            pages: Pages::empty(),
            frames: Frames::empty(),
            cache: Cache::new(),
        }
    }

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
            .alloc(&mut self.pages, &mut self.frames, class_idx)
    }

    fn slab_free(&mut self, address: usize) -> Option<()> {
        self.cache.free(&mut self.pages, &mut self.frames, address)
    }
}

impl Display for Heap {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.frames)
    }
}
