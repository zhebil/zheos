#![no_std]
#![no_main]

use core::{
    arch::{asm, global_asm},
    fmt::Write,
    panic::PanicInfo,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{
    board::{PSCI_SYSTEM_OFF, UART_INTID},
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
mod console;
mod cpu;
mod exception;
mod gic;
mod irq;
mod mem;
mod std;
mod uart;
mod zhemon;

pub fn irq0_handler(intid: u32) {
    let _ = writeln!(uart(), "Received interrupt {}", intid);
}

#[unsafe(no_mangle)]
pub extern "C" fn kmain() -> ! {
    uart().init();

    exception::install_vectors();

    gic::init();
    irq::register(0, irq0_handler);

    irq::register(UART_INTID, uart::handle_interrupt);
    uart().enable_interrupt();

    irq::unmask();

    let _ = writeln!(uart(), "Hello, ZheOS!");
    let _ = writeln!(uart(), "Type 'exit' to shutdown the system");
    let _ = writeln!(uart(), "----------------------------------");

    zhemon::Zhemon::new().start();

    shutdown()
}

pub fn shutdown() -> ! {
    unsafe {
        asm!("hvc #0", in("x0") PSCI_SYSTEM_OFF, options(noreturn));
    }
}

const fn bit_mask(bit: u32) -> u32 {
    1 << bit
}
