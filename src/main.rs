#![no_std]
#![no_main]

core::arch::global_asm!(include_str!("kernel.s"));

#[panic_handler]
fn panic_handler(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn kmain() -> ! {
    let uart = uart::UARTDriver::new();
    uart.init();

    for b in b"Hello\n" {
        uart.putc(*b)
    }

    loop {}
}

mod uart;

const fn bit_mask(bit: u32) -> u32 {
    1 << bit
}
