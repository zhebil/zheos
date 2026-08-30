#![no_std]
#![no_main]

use core::{alloc::Layout, num::NonZeroU32, time::Duration};

use crate::{
    board::{Board, Conduit},
    frames::{Frames, MAX_ORDER},
    memory::{
        image,
        map::MemoryMap,
        pages::{Entry, Pages, Slot},
        pfn::{PAGE_SIZE, Pfn},
        region::Region,
    },
    mmu::{Table, descriptor::Descriptor},
    uart::uart,
};

core::arch::global_asm!(include_str!("kernel.s"));

static ALREADY_PANICKED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    use core::{fmt::Write, sync::atomic::Ordering};

    let ap = ALREADY_PANICKED.load(Ordering::Relaxed);
    if !ap {
        ALREADY_PANICKED.store(true, Ordering::Relaxed);

        let _ = writeln!(uart(), "ZheOS has panicked!");
        let _ = writeln!(uart(), "{}", info);

        uart().flush();
    }

    loop {
        cpu::wait_for_interrupt();
    }
}

mod board;
mod console;
mod cpu;
mod dtb;
mod exception;
mod frames;
mod gic;
mod input;
mod irq;
mod lock;
mod memory;
mod mmio;
mod mmu;
mod print;
mod psci;
mod ring_buffer;
mod slab;
mod timer;
mod uart;
mod zhemon;

// Checked when the constant is evaluated, so timer::init needs no runtime guard.
const TIMER_HZ: NonZeroU32 = NonZeroU32::new(100).unwrap();

#[unsafe(no_mangle)]
pub extern "C" fn kmain(dtb_ptr: usize) -> ! {
    // On the hardcoded earlycon base, so the two failures below have a console.
    uart().init();

    exception::install_vectors();

    let Some(dtb) = (unsafe { dtb::Dtb::from_ptr(dtb_ptr) }) else {
        println!("No device tree at {dtb_ptr:#010x}");
        halt();
    };

    let board = match Board::discover(&dtb) {
        Ok(board) => board,
        Err(missing) => {
            println!("Device tree has no {missing}");
            halt();
        }
    };

    uart::adopt(board.uart.base);

    gic::init(&board.gic);

    irq::register(board.uart.intid, uart::handle_interrupt);
    uart().enable_interrupt();

    timer::init(TIMER_HZ, board.timer_intid);

    irq::unmask();

    println!("image: {}", image());
    println!("dtb: {}", dtb.region());
    println!("memory: {}", board.memory);

    let Some(mut map) = MemoryMap::new(board.memory) else {
        println!("The device tree reported no usable memory");
        halt();
    };

    if map.reserve(image()).is_err() || map.reserve(dtb.region()).is_err() {
        println!("No room to reserve the kernel image and the device tree");
        halt();
    }

    let Some(mut pages) = Pages::new(&mut map) else {
        println!("No room for the page metadata, or the arena is too large to index");
        halt();
    };

    let mut frames = Frames::new(&map, &mut pages);

    for region in map.reserved() {
        println!("reserved: {region}");
    }

    for order in 0..=MAX_ORDER {
        let blocks = frames.free_blocks(order);

        if blocks > 0 {
            println!("order {order}: {blocks} x {} pages", 1usize << order);
        }
    }

    println!("frames: {frames}");

    let Some(page) = frames.alloc(&mut pages, 0) else {
        println!("No page to allocate");
        halt();
    };

    println!("alloc(0): {:#012x}", page.to_addr());

    pages.write(
        page,
        Entry::Slab {
            class: 0xF,
            free_head: Slot::new(5),
            in_use: 0x3FF,
            next_partial: None,
            prev_partial: None,
        },
    );

    match pages.read(page) {
        Entry::Slab {
            class,
            free_head,
            in_use,
            next_partial,
            prev_partial,
        } => println!(
            "slab entry: class {class:#x} free_head {} in_use {in_use:#x} unlinked {}",
            free_head.map_or(0, Slot::index),
            next_partial.is_none() && prev_partial.is_none()
        ),
        Entry::Buddy { free, order } => println!("buddy entry: free {free} order {order}"),
    }

    frames.free(&mut pages, page);

    println!("frames: {frames}");

    for (size, align) in [(1usize, 1usize), (65, 1), (100, 1), (2049, 1), (1, 64)] {
        let Ok(layout) = Layout::from_size_align(size, align) else {
            continue;
        };

        match slab::class_of(layout) {
            Some(index) => println!("class_of({size}, {align}) = {}", slab::CLASSES[index]),
            None => println!("class_of({size}, {align}) = none"),
        }
    }

    slab_probe(&mut pages, &mut frames);

    cache_probe(&mut pages, &mut frames);

    layout_probe(&mut pages, &mut frames);

    let Some(mut table) = Table::new(&mut frames, &mut pages) else {
        println!("No room for a translation table");
        halt();
    };

    // Everything below RAM: every device on the machine, in one 1 GiB block.
    let devices = Region {
        base: 0,
        size: board.memory.base,
    };

    // Halting on failure rather than carrying on: a half-built table is worse
    // than a stop, because the next step turns the MMU on and walks it.
    if let Err(error) =
        table.identity_map(&mut frames, &mut pages, devices, Descriptor::DEVICE_BLOCK)
    {
        println!("Failed to map devices: {error}");
        halt();
    }

    if let Err(error) = table.identity_map(
        &mut frames,
        &mut pages,
        board.memory,
        Descriptor::NORMAL_BLOCK,
    ) {
        println!("Failed to map memory: {error}");
        halt();
    }

    println!("mair_el1: {:b}", cpu::mmu::read_mair_el1());
    println!("tcr_el1: {:b}", cpu::mmu::read_tcr_el1());
    println!("ttbr0_el1: {:b}", cpu::mmu::read_ttbr0_el1());
    println!("sctlr_el1: {:b}", cpu::mmu::read_sctlr_el1());

    mmu::enable(&mut table);

    println!("sctlr_el1: {:b}", cpu::mmu::read_sctlr_el1());

    println!("table: {:#012x}", table.base());
    println!("frames: {frames}");

    println!("0x0900_0000 -> {:?}", table.translate(0x0900_0000));
    println!("0x4008_0000 -> {:?}", table.translate(0x4008_0000));
    println!("0x4400_0000 -> {:?}", table.translate(0x4400_0000));
    println!("0x4800_0000 -> {:?}", table.translate(0x4800_0000));
    println!("0x9000_0000 -> {:?}", table.translate(0x9000_0000));
    println!("sp          -> {:?}", table.translate(cpu::stack_pointer()));

    println!("Hello, ZheOS!");
    println!("Type 'exit' to shutdown the system");
    println!("----------------------------------");

    timer::sleep(Duration::from_secs(1));
    zhemon::Zhemon::new().start();

    shutdown(board.psci)
}

/// Stops, but stays readable: powering off would leave nothing to attach to,
/// and a dead machine looks exactly like a hung one from the outside.
pub(crate) fn halt() -> ! {
    uart().flush();

    loop {
        cpu::wait_for_interrupt();
    }
}

fn shutdown(conduit: Conduit) -> ! {
    psci::system_off(conduit);

    println!("PSCI refused to power the machine off");
    halt()
}

fn print_slab_entry(pages: &Pages, pfn: Pfn) {
    match pages.read(pfn) {
        Entry::Slab {
            free_head, in_use, ..
        } => match free_head {
            Some(slot) => println!("entry: free_head {} in_use {in_use}", slot.index()),
            None => println!("entry: free_head none in_use {in_use}"),
        },
        Entry::Buddy { free, order } => println!("entry: buddy free {free} order {order}"),
    }
}

fn slab_probe(pages: &mut Pages, frames: &mut Frames) {
    let Some(page) = frames.alloc(pages, 0) else {
        println!("slab: no page for the probe");
        return;
    };

    for class in [0usize, 3, 8] {
        let Some(slab) = slab::Slab::init(pages, page, class) else {
            println!("slab: class index {class} is out of range");
            continue;
        };

        let expected = PAGE_SIZE / slab::CLASSES[class];

        match slab.chain_len(pages) {
            Some(count) => println!(
                "class {}: chain {count} of {expected}",
                slab::CLASSES[class]
            ),
            None => println!("class {}: chain is malformed", slab::CLASSES[class]),
        }
    }

    let Some(slab) = slab::Slab::init(pages, page, 3) else {
        println!("slab: class index 3 is out of range");
        return;
    };

    let mut slots = [0usize; 64];
    let mut handed = 0;

    while handed < slots.len() {
        let Some(address) = slab.alloc(pages) else {
            break;
        };

        slots[handed] = address;
        unsafe { *(address as *mut u8) = handed as u8 };
        handed += 1;
    }

    let full = slab.alloc(pages).is_none();
    let mut wrong = 0;

    for (i, &address) in slots[..handed].iter().enumerate() {
        if unsafe { *(address as *const u8) } != i as u8 {
            wrong += 1;
        }
    }

    println!("class 64: handed {handed}, full {full}, wrong readbacks {wrong}");
    print_slab_entry(pages, page);

    let mut rejected = 0;

    for address in [slots[0] + 1, page.to_addr() + PAGE_SIZE, page.to_addr() - 8] {
        if slab.free(pages, address).is_none() {
            rejected += 1;
        }
    }

    if slab.free(pages, slots[0]).is_some() && slab.free(pages, slots[0]).is_none() {
        rejected += 1;
    }

    let mut refused = 0;

    for &address in slots[1..handed].iter() {
        if slab.free(pages, address).is_none() {
            refused += 1;
        }
    }

    if slab.free(pages, slots[0]).is_none() {
        rejected += 1;
    }

    println!("class 64: bogus frees rejected {rejected} of 5, good frees refused {refused}");

    match slab.chain_len(pages) {
        Some(count) => println!("class 64: chain {count} after freeing all"),
        None => println!("class 64: chain is malformed after freeing"),
    }

    print_slab_entry(pages, page);

    frames.free(pages, page);
}

fn layout_probe(pages: &mut Pages, frames: &mut Frames) {
    let mut cache = slab::Cache {
        heads: [None; slab::CLASSES_COUNT],
    };

    let small = Layout::new::<[u8; 64]>();
    let three_k = Layout::new::<[u8; 3000]>();
    let nine_k = Layout::new::<[u8; 9000]>();

    println!("layout: start         {frames}");

    let Some(a) = cache.alloc_layout(pages, frames, three_k) else {
        println!("layout: 3000 refused");
        return;
    };

    println!("layout: 3000 live     {frames}, at {a:#012x}");

    let Some(b) = cache.alloc_layout(pages, frames, nine_k) else {
        println!("layout: 9000 refused");
        return;
    };

    println!("layout: 9000 live     {frames}, at {b:#012x}");

    let Some(c) = cache.alloc_layout(pages, frames, small) else {
        println!("layout: 64 refused");
        return;
    };

    println!("layout: 64 live       {frames}, at {c:#012x}");

    let mut refused = 0;

    for (address, layout) in [(c, small), (b, nine_k), (a, three_k)] {
        if cache.free_layout(pages, frames, address, layout).is_none() {
            refused += 1;
        }
    }

    println!("layout: 0 live        {frames}, refused {refused}");
}

fn cache_run(
    cache: &mut slab::Cache,
    pages: &mut Pages,
    frames: &mut Frames,
    slots: &mut [usize],
    layout: Layout,
) -> usize {
    let mut handed = 0;

    while handed < slots.len() {
        let Some(address) = cache.alloc_layout(pages, frames, layout) else {
            break;
        };

        slots[handed] = address;
        handed += 1;
    }

    handed
}

fn cache_probe(pages: &mut Pages, frames: &mut Frames) {
    let mut cache = slab::Cache {
        heads: [None; slab::CLASSES_COUNT],
    };

    let mut slots = [0usize; 192];
    let layout = Layout::new::<[u8; 64]>();

    println!("cache: start          {frames}");

    let handed = cache_run(&mut cache, pages, frames, &mut slots[..64], layout);
    println!("cache: {handed} live         {frames}");

    let mut refused = 0;

    for &address in slots[..handed].iter() {
        if cache.free_layout(pages, frames, address, layout).is_none() {
            refused += 1;
        }
    }

    println!("cache: 0 live         {frames}, refused {refused}");

    let handed = cache_run(&mut cache, pages, frames, &mut slots[..64], layout);

    for &address in slots[..handed - 1].iter() {
        if cache.free_layout(pages, frames, address, layout).is_none() {
            refused += 1;
        }
    }

    println!("cache: 1 live         {frames}");

    if cache
        .free_layout(pages, frames, slots[handed - 1], layout)
        .is_none()
    {
        refused += 1;
    }

    println!("cache: 0 live         {frames}");

    let handed = cache_run(&mut cache, pages, frames, &mut slots[..128], layout);
    println!("cache: {handed} live        {frames}");

    for &address in slots[..handed].iter() {
        if cache.free_layout(pages, frames, address, layout).is_none() {
            refused += 1;
        }
    }

    println!("cache: 0 live         {frames}, refused {refused}");

    let handed = cache_run(&mut cache, pages, frames, &mut slots, layout);
    println!("cache: {handed} live        {frames}");

    for i in [0usize, 64, 128] {
        if cache.free_layout(pages, frames, slots[i], layout).is_none() {
            refused += 1;
        }
    }

    for &address in slots[65..128].iter() {
        if cache.free_layout(pages, frames, address, layout).is_none() {
            refused += 1;
        }
    }

    println!("cache: middle empty   {frames}");

    for &address in slots[1..64].iter().chain(slots[129..].iter()) {
        if cache.free_layout(pages, frames, address, layout).is_none() {
            refused += 1;
        }
    }

    println!("cache: 0 live         {frames}, refused {refused}");
    println!("cache: head is none   {}", cache.heads[3].is_none());
}
