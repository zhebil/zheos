use crate::{
    frames::Frames,
    memory::pages::Pages,
    slab::{CLASSES_COUNT, Slab},
};

pub struct Cache {
    heads: [Option<Slab>; CLASSES_COUNT],
}

impl Cache {
    pub const fn new() -> Self {
        Self {
            heads: [None; CLASSES_COUNT],
        }
    }

    pub fn alloc(
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
            self.pop(pages, class_idx);
        };

        Some(address)
    }

    pub fn free(&mut self, pages: &mut Pages, frames: &mut Frames, address: usize) {
        let slab = Slab::from_addr(pages, address).expect("Cache::free: slab not found");
        let class_idx = slab.class(pages);

        let was_full = slab.is_full(pages);

        slab.free(pages, address);

        if was_full {
            self.push(pages, class_idx, slab);
        }

        if slab.is_empty(pages) {
            self.unlink(pages, slab);
            frames.free(pages, slab.pfn);
        }
    }

    fn push(&mut self, pages: &mut Pages, class_idx: usize, slab: Slab) {
        let old_slab = self.heads[class_idx];

        slab.set_next(pages, old_slab);
        slab.set_prev(pages, None);

        if let Some(old_slab) = old_slab {
            old_slab.set_prev(pages, Some(slab));
        }

        self.heads[class_idx] = Some(slab);
    }

    fn unlink(&mut self, pages: &mut Pages, slab: Slab) {
        let class_idx = slab.class(pages);
        let (next, prev) = slab.links(pages);

        if let Some(prev_slab) = prev {
            prev_slab.set_next(pages, next);
        } else {
            assert!(
                self.heads[class_idx] == Some(slab),
                "slab is not on its class list"
            );
            self.heads[class_idx] = next;
        }

        if let Some(next_slab) = next {
            next_slab.set_prev(pages, prev);
        }

        slab.set_next(pages, None);
        slab.set_prev(pages, None);
    }

    fn pop(&mut self, pages: &mut Pages, class_idx: usize) {
        let head = self.heads[class_idx].expect("popping a class with no partial slab");

        self.unlink(pages, head);
    }
}
