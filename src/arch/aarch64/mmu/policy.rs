use super::descriptor::Descriptor;
use crate::mm::Region;

pub struct Mapping {
    pub name: &'static str,
    pub region: Region,
    pub template: Option<Descriptor>,
}
