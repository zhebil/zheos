use crate::mm::region::Region;

unsafe extern "C" {
    static __image_start: u8;
    static __exec_end: u8;
    static __rodata_end: u8;
    static __stack_guard_start: u8;
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
pub fn writable_data() -> Region {
    between(&raw const __rodata_end, &raw const __stack_guard_start)
}

/// The unmapped page below the stack. Nothing maps it, so a stack that
/// overflows faults here instead of running on into .bss.
pub fn stack_guard() -> Region {
    between(&raw const __stack_guard_start, &raw const __stack_bottom)
}

/// The kernel stack. Mapped on its own, above the guard page.
pub fn stack() -> Region {
    between(&raw const __stack_bottom, &raw const __stack_top)
}
