#![no_std]
#![no_main]

use core::arch::{asm, global_asm};

global_asm!(include_str!("kernel.s"));

#[panic_handler]
fn panic_handler(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn kmain() -> ! {
    let uart = uart::UARTDriver::new();
    uart.init();

    listen_for_exit(&uart);

    for b in b"t\nSee you later!" {
        uart.putc(*b);
    }

    uart.flush();

    shutdown();
}

pub fn listen_for_exit(uart: &uart::UARTDriver) {
    let mut matched = 0usize;
    loop {
        let c = uart.getc();
        if c == b"exit"[matched] {
            matched += 1;
        } else {
            matched = if c == b'e' { 1 } else { 0 };
        }

        if matched == 4 {
            break;
        }

        uart.putc(c);
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
