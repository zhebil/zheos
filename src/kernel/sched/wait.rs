use core::sync::atomic::{AtomicU32, Ordering};

use super::{NEED_RESCHEDULE, SCHED, TASKS_COUNT};
use crate::kernel::task::State;

pub struct WaitQueue {
    slots: AtomicU32,
}

impl WaitQueue {
    pub const fn new() -> Self {
        Self {
            slots: AtomicU32::new(0),
        }
    }

    pub fn prepare(&self) {
        let mut sched = SCHED.lock();
        let current = sched.current;

        self.slots.fetch_or(1 << current, Ordering::SeqCst);

        sched.task_table[current]
            .as_mut()
            .unwrap()
            .set_state(State::Blocked);
    }

    pub fn finish(&self) {
        let mut sched = SCHED.lock();
        let current = sched.current;

        self.slots.fetch_and(!(1 << current), Ordering::SeqCst);

        sched.task_table[current]
            .as_mut()
            .unwrap()
            .set_state(State::Ready);
    }

    pub fn wake_all(&self) {
        let mut sched = SCHED.lock();
        let waiters = self.slots.swap(0, Ordering::SeqCst);

        if waiters == 0 {
            return;
        }

        for i in 0..TASKS_COUNT {
            if waiters & (1 << i) != 0 {
                sched.task_table[i]
                    .as_mut()
                    .unwrap()
                    .set_state(State::Ready);
            }
        }

        NEED_RESCHEDULE.store(true, Ordering::Relaxed);
    }
}
