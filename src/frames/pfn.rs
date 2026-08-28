pub const PAGE_SIZE: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pfn(pub usize);

impl Pfn {
    const EMPTY_PATTERN: usize = usize::MAX;

    pub fn to_addr(self) -> usize {
        self.0 * PAGE_SIZE
    }

    pub fn from_addr_down(addr: usize) -> Self {
        Self(addr / PAGE_SIZE)
    }

    pub fn from_addr_up(addr: usize) -> Self {
        Self(addr.div_ceil(PAGE_SIZE))
    }

    pub fn offset(self, offset: usize) -> Self {
        Self(self.0 + offset)
    }

    pub fn pages_until(self, end: Pfn) -> usize {
        end.0 - self.0
    }

    pub fn alignment_order(self) -> usize {
        self.0.trailing_zeros() as usize
    }

    pub fn index_from(self, base: Pfn) -> usize {
        self.0 - base.0
    }

    pub unsafe fn read_links(self) -> Links {
        let raw = unsafe { (self.to_addr() as *const [usize; 2]).read() };

        Links::decode(raw)
    }

    pub unsafe fn write_links(self, links: Links) {
        let raw = links.encode();

        unsafe { (self.to_addr() as *mut [usize; 2]).write(raw) }
    }
}

pub struct Links {
    pub prev: Option<Pfn>,
    pub next: Option<Pfn>,
}

impl Links {
    fn encode(&self) -> [usize; 2] {
        let prev = self.prev.map_or(Pfn::EMPTY_PATTERN, |pfn| pfn.0);
        let next = self.next.map_or(Pfn::EMPTY_PATTERN, |pfn| pfn.0);
        [prev, next]
    }

    fn decode(value: [usize; 2]) -> Self {
        let prev = value[0];
        let next = value[1];
        Self {
            prev: (prev != Pfn::EMPTY_PATTERN).then_some(Pfn(prev)),
            next: (next != Pfn::EMPTY_PATTERN).then_some(Pfn(next)),
        }
    }
}
