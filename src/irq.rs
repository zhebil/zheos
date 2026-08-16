use crate::gic;
use crate::uart;
use core::arch::asm;
use core::fmt::Write;

pub fn unmask() {
    unsafe {
        asm!("msr daifclr, #3", options(preserves_flags, nostack));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn handle_interrupt() {
    let mut uart = uart::UARTDriver::new();
    let Some(interrupt) = gic::acknowledge() else {
        return;
    };

    let _ = writeln!(uart, "Interrupt ID: {}", interrupt.intid());
    interrupt.end();
}
