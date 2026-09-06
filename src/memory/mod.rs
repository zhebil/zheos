pub mod map;
pub mod pages;
pub mod pfn;
pub mod region;

use crate::memory::region::Region;

unsafe extern "C" {
    static __image_start: u8;
    static __text_end: u8;
    static __vectors_start: u8;
    static __vectors_end: u8;
    static __exec_end: u8;
    static __rodata_end: u8;
    static __data_end: u8;
    static __bss_start: u8;
    static __stack_bottom: u8;
    static __stack_top: u8;
}

fn between(start: *const u8, end: *const u8) -> Region {
    let base = start as usize;
    Region {
        base,
        size: end as usize - base,
    }
}

/// Region taken by kernel image itself.
pub fn image() -> Region {
    between(&raw const __image_start, &raw const __stack_top)
}

/// Everything the kernel executes: instructions and the exception vectors.
/// Read-only, executable at exception level 1.
pub fn executable() -> Region {
    between(&raw const __image_start, &raw const __exec_end)
}

/// Constants. Read-only and never executed.
pub fn rodata() -> Region {
    between(&raw const __exec_end, &raw const __rodata_end)
}

/// Everything the kernel writes: data, zeroed data, and the stack.
/// Writable and never executed.
pub fn writable() -> Region {
    between(&raw const __rodata_end, &raw const __stack_top)
}

/// Instructions alone. Inside [`executable`], for reporting only.
pub fn text() -> Region {
    between(&raw const __image_start, &raw const __text_end)
}

/// The exception vector table `VBAR_EL1` points at. Inside [`executable`].
pub fn vectors() -> Region {
    between(&raw const __vectors_start, &raw const __vectors_end)
}

/// Initialised data. Inside [`writable`].
pub fn data() -> Region {
    between(&raw const __rodata_end, &raw const __data_end)
}

/// Zeroed data. Inside [`writable`].
pub fn bss() -> Region {
    between(&raw const __bss_start, &raw const __stack_bottom)
}

/// The kernel stack. Inside [`writable`].
pub fn stack() -> Region {
    between(&raw const __stack_bottom, &raw const __stack_top)
}
