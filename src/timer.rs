use core::sync::atomic::{AtomicU64, Ordering};
use core::time::Duration;

use crate::{
    board::TIMER_INTID,
    cpu::{
        self,
        generic_timer::{
            read_compare, read_count, read_freq, write_compare, write_control, write_timer_value,
        },
    },
    irq,
};

mod ctr {
    pub const ENABLE_BIT: u8 = 0;
    #[allow(unused)]
    pub const IMASK_BIT: u8 = 1;
    #[allow(unused)]
    pub const ISTATUS_BIT: u8 = 2;
}

const NANOSECONDS_PER_SECOND: u64 = 1_000_000_000;

static INTERVAL: AtomicU64 = AtomicU64::new(0);
static HZ: AtomicU64 = AtomicU64::new(0);
static KERNEL_TICKS: AtomicU64 = AtomicU64::new(0);

pub fn init(hz: u32) {
    assert!(hz > 0);

    let freq = read_freq() as u64;
    let interval = freq / hz as u64;

    INTERVAL.store(interval, Ordering::Relaxed);
    HZ.store(hz as u64, Ordering::Relaxed);

    irq::register(TIMER_INTID, handle_interrupt);

    // TVAL is right for the first arm only: "interval from now" is what is meant
    // here. The reload below has to be absolute or the handler latency accumulates.
    write_timer_value(interval);

    write_control(1 << ctr::ENABLE_BIT);
}

fn handle_interrupt(_intid: u32) {
    let interval = INTERVAL.load(Ordering::Relaxed);

    let mut deadline = read_compare();
    loop {
        deadline += interval;
        if deadline > read_count() {
            break;
        }
    }

    write_compare(deadline);

    KERNEL_TICKS.fetch_add(1, Ordering::Relaxed);
}

/// Rounds up: `delay` and `sleep` both promise *at least* the requested time.
fn ticks_from(duration: Duration, rate: u64) -> u64 {
    let whole = duration.as_secs() * rate;
    let fraction = (duration.subsec_nanos() as u64 * rate).div_ceil(NANOSECONDS_PER_SECOND);

    whole + fraction
}

fn duration_from(ticks: u64, rate: u64) -> Duration {
    let seconds = ticks / rate;
    let remainder = ticks % rate;

    Duration::new(seconds, (remainder * NANOSECONDS_PER_SECOND / rate) as u32)
}

/// Ticks of the free-running counter, `read_freq()` of them per second.
struct CpuTicks(u64);

impl CpuTicks {
    fn since_boot() -> Self {
        Self(read_count())
    }

    fn from_duration(duration: Duration) -> Self {
        Self(ticks_from(duration, read_freq() as u64))
    }

    fn to_duration(self) -> Duration {
        duration_from(self.0, read_freq() as u64)
    }
}

/// Timer interrupts handled, `HZ` of them per second.
struct HeartbeatTicks(u64);

impl HeartbeatTicks {
    fn rate() -> u64 {
        let hz = HZ.load(Ordering::Relaxed);
        // assert!(hz != 0, "timer::init has not run");

        hz
    }

    fn since_boot() -> Self {
        Self(KERNEL_TICKS.load(Ordering::Relaxed))
    }

    fn from_duration(duration: Duration) -> Self {
        Self(ticks_from(duration, Self::rate()))
    }
}

/// Heartbeats since `init`. Rises on its own; nothing but the handler writes it.
pub fn ticks() -> u64 {
    KERNEL_TICKS.load(Ordering::Relaxed)
}

/// Time since the machine powered on. Reads the counter, so it needs no `init`.
pub fn now() -> Duration {
    CpuTicks::since_boot().to_duration()
}

/// Busy-waits. Needs no `init`, so it is the one usable during device bring-up.
pub fn delay(duration: Duration) {
    let deadline = read_count() + CpuTicks::from_duration(duration).0;

    while read_count() < deadline {}
}

/// Parks the core until the heartbeat has advanced. Needs `init` and unmasked IRQs.
pub fn sleep(duration: Duration) {
    let start = HeartbeatTicks::since_boot().0;
    // The current tick is already partly spent, so the extra one is what makes
    // this sleep *at least* `duration` rather than up to one tick less.
    let target = start + HeartbeatTicks::from_duration(duration).0 + 1;

    while HeartbeatTicks::since_boot().0 < target {
        cpu::wait_for_interrupt();
    }
}
