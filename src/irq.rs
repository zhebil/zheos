use crate::cpu;
use crate::gic;
use crate::println;
use core::cell::UnsafeCell;

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

    fn set(&self, intid: u32, handler: fn(u32)) -> bool {
        let Some(slot) = (unsafe { (*self.0.get()).get_mut(intid as usize) }) else {
            return false;
        };

        *slot = Some(handler);
        true
    }

    fn get(&self, intid: u32) -> Option<fn(u32)> {
        unsafe { *(*self.0.get()).get(intid as usize)? }
    }
}

static HANDLERS: HandlerTable = HandlerTable::new();

pub fn register(intid: u32, handler: fn(u32)) {
    if !HANDLERS.set(intid, handler) {
        println!("IRQ {intid} is past the {IRQ_COUNT} the handler table holds");
        return;
    }

    gic::enable(intid);
}

#[unsafe(no_mangle)]
pub extern "C" fn handle_interrupt() {
    let Some(interrupt) = gic::acknowledge() else {
        return;
    };

    let intid = interrupt.intid();

    match HANDLERS.get(intid) {
        Some(handler) => handler(intid),
        None => {
            println!("No handler for IRQ {}", intid);
        }
    }

    // Unconditional: an interrupt nobody wanted still has to be deactivated,
    // or the running priority never drops and nothing is ever delivered again.
    interrupt.end();
}
