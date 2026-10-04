use crate::{
    arch::{
        cpu,
        psci::{self, Conduit},
    },
    drivers::pl011::uart,
    println,
};

/// Stops, but stays readable: powering off would leave nothing to attach to,
/// and a dead machine looks exactly like a hung one from the outside.
pub fn halt() -> ! {
    uart().flush();

    loop {
        cpu::wait_for_interrupt();
    }
}

pub fn shutdown(conduit: Conduit) -> ! {
    psci::system_off(conduit);

    println!("PSCI refused to power the machine off");
    halt()
}
