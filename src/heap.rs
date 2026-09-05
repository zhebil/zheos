use core::{
    alloc::{GlobalAlloc, Layout},
    cmp::max,
    fmt::Display,
};

use crate::{
    frames::Frames,
    lock::SpinLock,
    memory::{
        map::MemoryMap,
        pages::{Entry, Pages},
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

    pub fn init(&mut self, map: &mut MemoryMap) {
        if self.pages.len() != 0 {
            // Heap was already initialized, do not reinitialize it.
            return;
        }
        let Some(mut pages) = Pages::new(map) else {
            panic!("Failed to initialize heap pages");
        };

        let frames = Frames::new(map, &mut pages);
        let cache = Cache::new();

        self.pages = pages;
        self.frames = frames;
        self.cache = cache;
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

    pub fn free_layout(&mut self, address: usize, layout: Layout) {
        let entry = self.pages.read(Pfn::from_addr_down(address));

        assert!(
            class_of(layout).is_some() == matches!(entry, Entry::Slab { .. }),
            "free with a layout that does not match the page it points at"
        );

        match entry {
            Entry::Slab { .. } => self.slab_free(address),
            Entry::Buddy { .. } => self.free_pages(address),
        }
    }

    fn alloc_pages(&mut self, order: u32) -> Option<usize> {
        let page_pfn = self.frames.alloc(&mut self.pages, order as usize)?;
        Some(page_pfn.to_addr())
    }

    fn free_pages(&mut self, address: usize) {
        self.frames
            .free(&mut self.pages, Pfn::from_addr_down(address));
    }

    pub fn frames(&self) -> &Frames {
        &self.frames
    }

    fn slab_alloc(&mut self, class_idx: usize) -> Option<usize> {
        self.cache
            .alloc(&mut self.pages, &mut self.frames, class_idx)
    }

    fn slab_free(&mut self, address: usize) {
        self.cache.free(&mut self.pages, &mut self.frames, address)
    }
}

impl Display for Heap {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.frames)
    }
}

#[global_allocator]
pub static HEAP: SpinLock<Heap> = SpinLock::new(Heap::empty());

impl SpinLock<Heap> {
    pub fn init(&self, map: &mut MemoryMap) {
        let mut heap = self.lock();
        heap.init(map);
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut Heap) -> R) -> R {
        let mut heap = self.lock();
        f(&mut heap)
    }
}

unsafe impl GlobalAlloc for SpinLock<Heap> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut heap = self.lock();
        match heap.alloc_layout(layout) {
            Some(addr) => addr as *mut u8,
            None => core::ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut heap = self.lock();
        heap.free_layout(ptr as usize, layout);
    }
}
