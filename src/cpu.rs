use core::arch::asm;

pub fn wait_for_interrupt() {
    unsafe { asm!("wfi") }
}

/// The current stack pointer. `sp` is not a general register, so it has to be
/// moved into one before it can be read.
pub fn stack_pointer() -> usize {
    let sp: usize;
    unsafe { asm!("mov {}, sp", out(reg) sp, options(nomem, nostack, preserves_flags)) };
    sp
}

/// Unmasks IRQ and FIQ. `daifclr` *clears* mask bits - the opposite of `daifset`.
pub fn unmask_irqs() {
    unsafe {
        asm!("msr daifclr, #3", options(nostack, preserves_flags));
    }
}

/// Runs `f` with IRQs masked, then puts DAIF back exactly as it was.
///
/// Restoring rather than unmasking is what makes this safe to call from a
/// handler, where exception entry has already masked everything.
pub fn without_interrupts<T>(f: impl FnOnce() -> T) -> T {
    let daif = read_daif();
    mask_irqs();

    let result = f();

    restore_daif(daif);

    result
}

fn read_daif() -> u64 {
    let daif: u64;
    // No nomem on any of these: they must act as compiler barriers, or the
    // optimiser is free to hoist a read of the data they protect out of the
    // surrounding loop and spin forever on a stale value.
    unsafe {
        asm!("mrs {}, daif", out(reg) daif, options(nostack, preserves_flags));
    }
    daif
}

fn mask_irqs() {
    unsafe {
        asm!("msr daifset, #2", options(nostack, preserves_flags));
    }
}

fn restore_daif(daif: u64) {
    unsafe {
        asm!("msr daif, {}", in(reg) daif, options(nostack, preserves_flags));
    }
}

pub mod generic_timer {
    use core::arch::asm;

    pub fn read_freq() -> u32 {
        let mut freq: u64 = 0;
        unsafe { asm!("mrs {}, CNTFRQ_EL0", out(reg) freq) };
        freq as u32
    }

    pub fn read_count() -> u64 {
        let mut count: u64 = 0;
        unsafe { asm!("isb; mrs {}, CNTPCT_EL0", out(reg) count) };
        count
    }

    pub fn read_compare() -> u64 {
        let mut compare: u64 = 0;
        unsafe { asm!("mrs {}, CNTP_CVAL_EL0", out(reg) compare) };
        compare
    }

    pub fn write_compare(compare: u64) {
        unsafe { asm!("msr CNTP_CVAL_EL0, {}", in(reg) compare) };
    }

    pub fn write_timer_value(timer_value: u64) {
        unsafe { asm!("msr CNTP_TVAL_EL0, {}", in(reg) timer_value) };
    }

    pub fn write_control(control: u64) {
        unsafe { asm!("msr CNTP_CTL_EL0, {}", in(reg) control) };
    }
}
