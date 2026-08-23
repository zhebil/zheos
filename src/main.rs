#![no_std]
#![no_main]

use core::{
    arch::{asm, global_asm},
    fmt::Write,
    panic::PanicInfo,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{
    board::{DTB_BASE, PSCI_SYSTEM_OFF, UART_INTID},
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
mod dtb;
mod exception;
mod gic;
mod input;
mod irq;
mod mmio;
mod print;
mod ring_buffer;
mod uart;
mod zhemon;

#[unsafe(no_mangle)]
pub extern "C" fn kmain() -> ! {
    uart().init();

    exception::install_vectors();

    gic::init();

    irq::register(UART_INTID, uart::handle_interrupt);
    uart().enable_interrupt();

    irq::unmask();

    println!("Hello, ZheOS!");
    println!("Type 'exit' to shutdown the system");
    println!("----------------------------------");

    let memory = unsafe { dtb::Dtb::from_ptr(DTB_BASE) }.and_then(|dtb| dtb.memory());

    if let Some(memory) = memory {
        println!("{:#010x} {:x} bytes", memory.addr, memory.size);
    } else {
        println!("Memory not found");
    }

    zhemon::Zhemon::new().start();

    shutdown()
}

pub fn shutdown() -> ! {
    unsafe {
        asm!("hvc #0", in("x0") PSCI_SYSTEM_OFF, options(noreturn));
    }
}
