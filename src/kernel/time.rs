use core::num::NonZeroU32;
use core::sync::atomic::{AtomicU64, Ordering};
use core::time::Duration;

use crate::arch::{cpu, timer};
use crate::kernel::{irq, sched};

const NANOSECONDS_PER_SECOND: u64 = 1_000_000_000;

static INTERVAL: AtomicU64 = AtomicU64::new(0);
static HZ: AtomicU64 = AtomicU64::new(0);
static KERNEL_TICKS: AtomicU64 = AtomicU64::new(0);

pub fn init(hz: NonZeroU32, intid: u32) {
    let interval = timer::frequency() / hz.get() as u64;

    INTERVAL.store(interval, Ordering::Relaxed);
    HZ.store(hz.get() as u64, Ordering::Relaxed);

    irq::register(intid, handle_interrupt);

    timer::start(interval);
}

fn handle_interrupt(_intid: u32) {
    timer::rearm(INTERVAL.load(Ordering::Relaxed));

    sched::tick();

    KERNEL_TICKS.fetch_add(1, Ordering::Relaxed);
}

/// Rounds up: `delay` and `sleep` both promise *at least* the requested time.
fn ticks_from(duration: Duration, rate: u64) -> u64 {
    let whole = duration.as_secs() * rate;
    let fraction = (duration.subsec_nanos() as u64 * rate).div_ceil(NANOSECONDS_PER_SECOND);

    whole + fraction
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
