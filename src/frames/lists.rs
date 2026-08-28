use crate::{frames::MAX_ORDER, memory::pfn::Pfn};

const ORDERS: usize = MAX_ORDER + 1;

const EMPTY: usize = usize::MAX;

pub struct FreeLists {
    heads: [Option<Pfn>; ORDERS],
    pages: usize,
}

struct Links {
    prev: Option<Pfn>,
    next: Option<Pfn>,
}

impl FreeLists {
    pub const fn empty() -> FreeLists {
        FreeLists {
            heads: [None; ORDERS],
            pages: 0,
        }
    }

    pub fn pages(&self) -> usize {
        self.pages
    }

    pub fn head(&self, order: usize) -> Option<Pfn> {
        *self.heads.get(order)?
    }

    // Slow method, debug only
    pub fn blocks(&self, order: usize) -> usize {
        let mut count = 0;
        let mut current = self.head(order);

        while let Some(pfn) = current {
            count += 1;
            current = unsafe { read_links(pfn) }.next;
        }

        count
    }

    pub fn push(&mut self, pfn: Pfn, order: usize) {
        let Some(slot) = self.heads.get_mut(order) else {
            return;
        };
        let head = *slot;
        *slot = Some(pfn);

        unsafe {
            write_links(
                pfn,
                Links {
                    prev: None,
                    next: head,
                },
            )
        };

        // Update head's prev pointer
        if let Some(head) = head {
            let mut links = unsafe { read_links(head) };
            links.prev = Some(pfn);
            unsafe { write_links(head, links) };
        }

        self.pages += 1 << order;
    }

    pub fn pop(&mut self, order: usize) -> Option<Pfn> {
        let pfn = self.head(order)?;

        self.unlink(pfn, order);

        Some(pfn)
    }

    pub fn unlink(&mut self, pfn: Pfn, order: usize) {
        let Links { prev, next } = unsafe { read_links(pfn) };

        match prev {
            // Update prev's pinter to next entry
            Some(prev) => {
                let mut links = unsafe { read_links(prev) };
                links.next = next;
                unsafe { write_links(prev, links) };
            }
            // No prev means this was the head, so the head moves on.
            None => {
                if let Some(slot) = self.heads.get_mut(order) {
                    *slot = next;
                }
            }
        }

        // Update next's prev pointer
        if let Some(next) = next {
            let mut links = unsafe { read_links(next) };
            links.prev = prev;
            unsafe { write_links(next, links) };
        }

        self.pages -= 1 << order;
    }
}

impl Links {
    fn encode(&self) -> [usize; 2] {
        [
            Self::addr_from_pfn(self.prev),
            Self::addr_from_pfn(self.next),
        ]
    }

    fn decode(raw: [usize; 2]) -> Links {
        Links {
            prev: Self::pfn_from_addr(raw[0]),
            next: Self::pfn_from_addr(raw[1]),
        }
    }

    fn addr_from_pfn(pfn: Option<Pfn>) -> usize {
        pfn.map_or(EMPTY, Pfn::to_addr)
    }

    fn pfn_from_addr(addr: usize) -> Option<Pfn> {
        (addr != EMPTY).then(|| Pfn::from_addr_down(addr))
    }
}

/// SAFETY: `pfn` must be a page currently on one of these lists, so that its
/// first two words are this module's to read.
unsafe fn read_links(pfn: Pfn) -> Links {
    Links::decode(unsafe { (pfn.to_addr() as *const [usize; 2]).read() })
}

/// SAFETY: same as `read_links`.
unsafe fn write_links(pfn: Pfn, links: Links) {
    unsafe { (pfn.to_addr() as *mut [usize; 2]).write(links.encode()) }
}
