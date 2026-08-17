pub const UART_BASE: usize = 0x0900_0000;

pub const GICD_BASE: usize = 0x0800_0000;
pub const GICC_BASE: usize = 0x0801_0000;

pub const UART_INTID: u32 = 33;

// PSCI (Power State Coordination Interface) function ID for SYSTEM_OFF.
pub const PSCI_SYSTEM_OFF: u64 = 0x8400_0008;
