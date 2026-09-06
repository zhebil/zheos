use crate::{memory::region::Region, mmu::descriptor::Descriptor};

pub struct Mapping {
    pub name: &'static str,
    pub region: Region,
    pub template: Option<Descriptor>,
}
