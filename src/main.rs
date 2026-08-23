#![no_std]
#![no_main]

use core::{
    alloc::Layout,
    arch::global_asm,
    fmt::Write,
    num::NonZeroU32,
    panic::PanicInfo,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use crate::{
    board::{Board, Conduit},
    bump::Bump,
    uart::uart,
};

global_asm!(include_str!("kernel.s"));

static ALREADY_PANICKED: AtomicBool = AtomicBool::new(false);

#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
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
mod bump;
mod console;
mod cpu;
mod dtb;
mod exception;
mod gic;
mod input;
mod irq;
mod mmio;
mod print;
mod psci;
mod ring_buffer;
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

    let image = bump::image();

    println!("image: {}", image);
    println!("dtb: {}", dtb.region());
    println!("memory: {}", board.memory);

    let mut bump = Bump::new(board.memory); // memblock_add
    match bump.reserve(bump::image()) {
        Ok(_) => {}
        Err(_) => {
            println!("no space for kernel");
            halt();
        }
    };
    match bump.reserve(dtb.region()) {
        Ok(_) => {}
        Err(_) => {
            println!("no space for dtb");
            halt();
        }
    };
    let remain_mem = bump.remaining();
    println!("remains memory: {}", remain_mem);

    let l = Layout::new::<u64>();
    match bump.alloc(l) {
        Some(ptr) => println!("ptr: {:#012x}", ptr.addr()),
        None => println!("no space for 8 bytes"),
    };

    let remain_mem = bump.remaining();
    println!("remains memory: {}", remain_mem);

    println!("Hello, ZheOS!");
    println!("Type 'exit' to shutdown the system");
    println!("----------------------------------");

    timer::sleep(Duration::from_secs(1));
    zhemon::Zhemon::new().start();

    shutdown(board.psci)
}

/// Stops, but stays readable: powering off would leave nothing to attach to,
/// and a dead machine looks exactly like a hung one from the outside.
fn halt() -> ! {
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
