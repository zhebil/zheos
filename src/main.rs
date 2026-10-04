#![no_std]
#![no_main]
#![allow(special_module_name)]

extern crate alloc;

use core::{fmt::Display, num::NonZeroU32, time::Duration};

use crate::{
    arch::{exception, mmu},
    drivers::{gic, pl011, pl011::uart},
    kernel::{irq, power, sched, time},
};

mod arch;
mod demo;
mod drivers;
mod kernel;
mod lib;
mod mm;
mod platform;
mod tty;
mod zhemon;

// Checked when the constant is evaluated, so time::init needs no runtime guard.
const TIMER_HZ: NonZeroU32 = NonZeroU32::new(100).unwrap();

#[unsafe(no_mangle)]
pub extern "C" fn kmain(dtb_ptr: usize) -> ! {
    // On the hardcoded earlycon base, so the two failures below have a console.
    uart().init();

    exception::install_vectors();

    let (dtb, board) = or_halt(platform::discover(dtb_ptr));

    pl011::adopt(board.uart.base);
    gic::init(&board.gic);

    irq::register(board.uart.intid, pl011::handle_interrupt);
    uart().enable_interrupt();

    time::init(TIMER_HZ, board.timer_intid);

    irq::unmask();

    or_halt(mm::init(board.memory, dtb.region()));
    or_halt(mmu::init(board.memory, dtb.region()));

    sched::init();
    or_halt(demo::start());

    println!("Hello, ZheOS!");
    println!("Type 'exit' to shutdown the system");
    println!("----------------------------------");

    zhemon::Zhemon::new().start();

    time::sleep(Duration::from_secs(1));

    power::shutdown(board.psci)
}

fn or_halt<T>(result: Result<T, impl Display>) -> T {
    result.unwrap_or_else(|error| {
        println!("{error}");
        power::halt()
    })
}
