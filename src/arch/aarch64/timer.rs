use core::arch::asm;

fn read_freq() -> u32 {
    let mut freq: u64 = 0;
    unsafe { asm!("mrs {}, CNTFRQ_EL0", out(reg) freq) };
    freq as u32
}

fn read_count() -> u64 {
    let mut count: u64 = 0;
    unsafe { asm!("isb; mrs {}, CNTPCT_EL0", out(reg) count) };
    count
}

fn read_compare() -> u64 {
    let mut compare: u64 = 0;
    unsafe { asm!("mrs {}, CNTP_CVAL_EL0", out(reg) compare) };
    compare
}

fn write_compare(compare: u64) {
    unsafe { asm!("msr CNTP_CVAL_EL0, {}", in(reg) compare) };
}

fn write_timer_value(timer_value: u64) {
    unsafe { asm!("msr CNTP_TVAL_EL0, {}", in(reg) timer_value) };
}

fn write_control(control: u64) {
    unsafe { asm!("msr CNTP_CTL_EL0, {}", in(reg) control) };
}

mod ctl {
    pub const ENABLE_BIT: u8 = 0;
    #[allow(unused)]
    pub const IMASK_BIT: u8 = 1;
    #[allow(unused)]
    pub const ISTATUS_BIT: u8 = 2;
}

pub fn frequency() -> u64 {
    read_freq() as u64
}

pub fn start(interval: u64) {
    // TVAL is right for the first arm only: "interval from now" is what is meant
    // here. The reload below has to be absolute or the handler latency accumulates.
    write_timer_value(interval);

    write_control(1 << ctl::ENABLE_BIT);
}

pub fn rearm(interval: u64) {
    let mut deadline = read_compare();
    loop {
        deadline += interval;
        if deadline > read_count() {
            break;
        }
    }

    write_compare(deadline);
}
