use crate::cpu;
use crate::gic;
use crate::println;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

static LIVE: AtomicBool = AtomicBool::new(false);

pub fn unmask() {
    LIVE.store(true, Ordering::Release);
    cpu::unmask_irqs();
}

const IRQ_COUNT: u32 = 64;

type HandlerCells = [Option<fn(u32)>; IRQ_COUNT as usize];
struct HandlerTable(UnsafeCell<HandlerCells>);

// SAFETY: register() refuses once unmask() has run, and handle_interrupt()
// only runs after it, so a write and a read can never overlap. The phase is
// enforced by LIVE rather than promised, and it holds however many cores are
// running as long as every register() happens before any core unmasks.
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
    if LIVE.load(Ordering::Acquire) {
        println!("IRQ {intid} registered after interrupts went live");
        return;
    }

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
