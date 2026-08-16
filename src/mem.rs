use core::ptr::{read_volatile, write_volatile};

pub fn read_byte(address: u64) -> u8 {
    unsafe { read_volatile(address as *const u8) }
}

pub fn read_32(address: u64) -> u32 {
    unsafe { read_volatile(address as *const u32) }
}

pub fn write_byte(address: u64, byte: u8) -> () {
    unsafe { write_volatile(address as *mut u8, byte) };
}

pub fn write_32(address: u64, byte: u32) -> () {
    unsafe { write_volatile(address as *mut u32, byte) };
}
