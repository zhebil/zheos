use crate::uart::uart;
use crate::{cpu, gic};
use core::cell::UnsafeCell;
use core::fmt::Write;

pub fn unmask() {
    cpu::unmask_irqs();
}

const IRQ_COUNT: u32 = 64;

struct HandlerTable(UnsafeCell<[Option<fn(u32)>; IRQ_COUNT as usize]>);

// SAFETY: register() only runs before unmask(), and handle_interrupt() only
// after, so on one core an access can never overlap another. Registering a
// handler once interrupts are live breaks that and would need a lock.
unsafe impl Sync for HandlerTable {}

impl HandlerTable {
    const fn new() -> Self {
        Self(UnsafeCell::new([None; IRQ_COUNT as usize]))
    }

    fn set(&self, intid: u32, handler: fn(u32)) {
        unsafe { (*self.0.get())[intid as usize] = Some(handler) }
    }

    fn get(&self, intid: u32) -> Option<fn(u32)> {
        unsafe { (*self.0.get())[intid as usize] }
    }
}

static HANDLERS: HandlerTable = HandlerTable::new();

pub fn register(intid: u32, handler: fn(u32)) {
    if intid >= IRQ_COUNT {
        panic!("Invalid IRQ number: {}", intid);
    }

    HANDLERS.set(intid, handler);

    gic::enable(intid);
}

fn lookup_handler(intid: u32) -> Option<fn(u32)> {
    if intid >= IRQ_COUNT {
        return None;
    }

    HANDLERS.get(intid)
}

#[unsafe(no_mangle)]
pub extern "C" fn handle_interrupt() {
    let Some(interrupt) = gic::acknowledge() else {
        return;
    };

    let intid = interrupt.intid();

    match lookup_handler(intid) {
        Some(handler) => handler(intid),
        None => {
            let _ = writeln!(uart(), "No handler for IRQ {}", intid);
        }
    }

    // Unconditional: an interrupt nobody wanted still has to be deactivated,
    // or the running priority never drops and nothing is ever delivered again.
    interrupt.end();
}
