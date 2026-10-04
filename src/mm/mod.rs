use crate::println;

mod frames;
mod gaps;
mod heap;
pub mod image;
mod map;
mod pages;
mod pfn;
mod region;
mod slab;

pub use gaps::gaps;
pub use heap::{HEAP, Heap};
pub use pfn::PAGE_SIZE;
pub use region::Region;

use map::MemoryMap;

pub fn init(memory: Region, dtb: Region) -> Result<(), &'static str> {
    let mut map = MemoryMap::new(memory).ok_or("The device tree reported no usable memory")?;

    if map.reserve(image::image()).is_err() || map.reserve(dtb).is_err() {
        return Err("No room to reserve the kernel image and the device tree");
    }
    HEAP.init(&mut map);

    println!("memory: {memory}");
    HEAP.with(|h| println!("heap: {h}"));

    Ok(())
}
