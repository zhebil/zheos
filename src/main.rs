#![no_std]
#![no_main]

use core::{
    arch::{asm, global_asm},
    fmt::Write,
};

global_asm!(include_str!("kernel.s"));

#[panic_handler]
fn panic_handler(_info: &core::panic::PanicInfo) -> ! {
    loop {
        unsafe { asm!("wfi") } // wait for interrupts
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn kmain() -> ! {
    let mut uart = uart::UARTDriver::new();
    uart.init();

    let _ = writeln!(uart, "Hello, ZheOS!");
    let _ = writeln!(uart, "Test: {:#010x}", 42u32);
    let _ = writeln!(uart, "Type 'exit' to shutdown the system");
    let _ = writeln!(uart, "----------------------------------");

    listen_for_exit(&uart);

    let _ = writeln!(uart, "\n");
    let _ = writeln!(uart, "System is shutting down");

    uart.flush();

    shutdown();
}

pub fn listen_for_exit(uart: &uart::UARTDriver) {
    let mut matched = 0usize;
    loop {
        let c = uart.getc();
        uart.putc(c);

        if c == b"exit"[matched] {
            matched += 1;
        } else {
            matched = if c == b'e' { 1 } else { 0 };
        }

        if matched == 4 {
            break;
        }
    }
}

pub fn shutdown() -> ! {
    unsafe {
        asm!("hvc #0", in("x0") 0x84000008u64, options(noreturn));
    }
}

mod uart;

const fn bit_mask(bit: u32) -> u32 {
    1 << bit
}
