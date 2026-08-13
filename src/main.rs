#![no_std]
#![no_main]

use core::{
    arch::{asm, global_asm},
    fmt::Write,
    panic::PanicInfo,
    sync::atomic::{AtomicBool, Ordering},
};

global_asm!(include_str!("kernel.s"));

static ALREADY_PANICKED: AtomicBool = AtomicBool::new(false);

#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    let ap = ALREADY_PANICKED.load(Ordering::Relaxed);
    if !ap {
        ALREADY_PANICKED.store(true, Ordering::Relaxed);

        let mut uart = uart::UARTDriver::new();
        uart.init();
        let _ = writeln!(uart, "ZheOS has panicked!");
        let _ = writeln!(uart, "{}", info);

        uart.flush();
    }

    loop {
        unsafe { asm!("wfi") } // wait for interrupts
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn kmain() -> ! {
    let mut uart = uart::UARTDriver::new();
    uart.init();

    let _ = writeln!(uart, "Hello, ZheOS!");
    let _ = writeln!(uart, "Type 'exit' to shutdown the system");
    let _ = writeln!(uart, "----------------------------------");

    listen_for_exit(&mut uart);

    let _ = writeln!(uart, "\n");
    let _ = writeln!(uart, "System is shutting down");

    uart.flush();

    shutdown();
}

pub fn listen_for_exit(uart: &mut uart::UARTDriver) {
    let mut matched = 0usize;
    loop {
        let c = uart.getc();
        if c.flags.framing() || c.flags.parity() || c.flags.overrun() || c.flags.brk() {
            let _ = writeln!(uart, "ERROR! B:0x{:02x} F: {}", c.byte, c.flags);
        } else {
            uart.putc(c.byte);
        }

        if c.byte == b"exit"[matched] {
            matched += 1;
        } else {
            matched = if c.byte == b'e' { 1 } else { 0 };
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
