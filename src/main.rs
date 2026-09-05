#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use core::{num::NonZeroU32, time::Duration};

use crate::{
    board::{Board, Conduit},
    frames::MAX_ORDER,
    heap::HEAP,
    memory::{image, map::MemoryMap, region::Region},
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
mod heap;
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

    let Some(mut table) = HEAP.with(Table::new) else {
        println!("No room to allocate a page table");
        halt();
    };

    // Everything below RAM: every device on the machine, in one 1 GiB block.
    let devices = Region {
        base: 0,
        size: board.memory.base,
    };

    HEAP.with(|h| {
        if let Err(error) = table.identity_map(h, devices, Descriptor::DEVICE_BLOCK) {
            println!("Failed to map devices: {error}");
            halt();
        }

        if let Err(error) = table.identity_map(h, board.memory, Descriptor::NORMAL_BLOCK) {
            println!("Failed to map memory: {error}");
            halt();
        }
    });

    println!("mair_el1: {:b}", cpu::mmu::read_mair_el1());
    println!("tcr_el1: {:b}", cpu::mmu::read_tcr_el1());
    println!("ttbr0_el1: {:b}", cpu::mmu::read_ttbr0_el1());
    println!("sctlr_el1: {:b}", cpu::mmu::read_sctlr_el1());

    mmu::enable(&mut table);

    println!("sctlr_el1: {:b}", cpu::mmu::read_sctlr_el1());

    println!("table: {:#012x}", table.base());

    HEAP.with(|h| {
        println!("heap: {h}");
    });

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
