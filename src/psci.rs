use core::arch::asm;

use crate::board::Conduit;

// Fixed by the PSCI specification from 0.2 on, which is why the device tree
// does not carry it. Only 0.1 let each platform pick its own function IDs.
const SYSTEM_OFF: u32 = 0x8400_0008;

/// Asks whatever implements PSCI to cut the power. Returns only if it refuses,
/// which the specification allows.
pub fn system_off(conduit: Conduit) {
    match conduit {
        Conduit::Hvc => unsafe {
            asm!(
                "hvc #0",
                in("x0") SYSTEM_OFF,
                lateout("x1") _,
                lateout("x2") _,
                lateout("x3") _,
                lateout("x0") _,
                options(nomem, nostack),
            );
        },
        Conduit::Smc => unsafe {
            asm!(
                "smc #0",
                in("x0") SYSTEM_OFF,
                lateout("x1") _,
                lateout("x2") _,
                lateout("x3") _,
                lateout("x0") _,
                options(nomem, nostack),
            );
        },
    }
}
