use alloc::vec::Vec;

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

use frames::MAX_ORDER;
use image::{bss, data, executable, rodata, stack, stack_guard, text, vectors, writable_data};
use map::MemoryMap;

pub fn init(memory: Region, dtb: Region) -> Result<(), &'static str> {
    println!("image: {}", image::image());
    println!("  exec:     {}", executable());
    println!("    text:    {}", text());
    println!("    vectors: {}", vectors());
    println!("  rodata:   {}", rodata());
    println!("  writable: {}", writable_data());
    println!("    data:   {}", data());
    println!("    bss:    {}", bss());
    println!("  guard:    {}", stack_guard());
    println!("  stack:    {}", stack());
    println!("dtb: {}", dtb);
    println!("memory: {}", memory);

    let mut map = MemoryMap::new(memory).ok_or("The device tree reported no usable memory")?;

    if map.reserve(image::image()).is_err() || map.reserve(dtb).is_err() {
        return Err("No room to reserve the kernel image and the device tree");
    }
    HEAP.init(&mut map);

    for region in map.reserved() {
        println!("reserved: {region}");
    }

    for order in 0..=MAX_ORDER {
        let blocks = HEAP.with(|h| h.frames().free_blocks(order));

        if blocks > 0 {
            println!("order {order}: {blocks} x {} pages", 1usize << order);
        }
    }

    HEAP.with(|h| {
        println!("heap: {h}");
    });

    let mut counted = Vec::new();

    for i in 0..10u32 {
        counted.push(i);
    }

    println!(
        "vec: len {} cap {} sum {}",
        counted.len(),
        counted.capacity(),
        counted.iter().sum::<u32>()
    );

    HEAP.with(|h| {
        println!("heap: {h} with the vec live");
    });

    drop(counted);

    HEAP.with(|h| {
        println!("heap: {h}");
    });

    Ok(())
}
