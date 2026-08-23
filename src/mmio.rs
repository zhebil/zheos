use core::ptr::{read_volatile, write_volatile};

pub fn read_byte(address: usize) -> u8 {
    unsafe { read_volatile(address as *const u8) }
}

pub fn read_32(address: usize) -> u32 {
    unsafe { read_volatile(address as *const u32) }
}

pub fn write_byte(address: usize, byte: u8) -> () {
    unsafe { write_volatile(address as *mut u8, byte) };
}

pub fn write_32(address: usize, byte: u32) -> () {
    unsafe { write_volatile(address as *mut u32, byte) };
}
