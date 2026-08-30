use crate::{
    frames::Frames,
    memory::pages::Pages,
    slab::{CLASSES_COUNT, Slab},
};

pub struct Cache {
    heads: [Option<Slab>; CLASSES_COUNT],
}

impl Cache {
    pub fn new() -> Self {
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
            self.pop(pages, class_idx)?;
        };

        Some(address)
    }

    pub fn free(&mut self, pages: &mut Pages, frames: &mut Frames, address: usize) -> Option<()> {
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
