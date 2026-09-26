use core::sync::atomic::{AtomicBool, Ordering};

use crate::{
    context::{Context, Task, switch},
    lock::SpinLock,
};

type Tasks = [Option<Task>; TASKS_COUNT];

const TASKS_COUNT: usize = 4;

pub struct Scheduler {
    task_table: Tasks,
    current: usize,
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            task_table: [const { None }; TASKS_COUNT],
            current: 0,
        }
    }
}

pub fn spawn(task: Task) -> Result<(), ()> {
    let mut sched = SCHED.lock();

    let Some(empty_idx) = sched.task_table.iter().position(|t| t.is_none()) else {
        return Err(());
    };

    sched.task_table[empty_idx] = Some(task);
    Ok(())
}

pub fn init() {
    let mut sched = SCHED.lock();
    sched.task_table[0] = Some(Task::boot(0));
}

static SCHED: SpinLock<Scheduler> = SpinLock::new(Scheduler::new());

static NEED_RESCHEDULE: AtomicBool = AtomicBool::new(false);

pub fn tick() {
    NEED_RESCHEDULE.store(true, Ordering::Relaxed);
}

pub fn preempt() {
    let need_reschedule = NEED_RESCHEDULE.swap(false, Ordering::Relaxed);

    if !need_reschedule {
        return;
    }

    let mut sched = SCHED.lock();
    let current = sched.current;
    let mut next_idx = current;

    for i in 0..TASKS_COUNT {
        let idx = (current + i + 1) % TASKS_COUNT;
        if sched.task_table[idx].is_some() {
            next_idx = idx;
            if idx == current {
                return;
            }
            break;
        }
    }

    let from: *mut Context = sched.task_table[current].as_mut().unwrap().context_mut();
    let to: *const Context = sched.task_table[next_idx].as_ref().unwrap().context();

    sched.current = next_idx;
    drop(sched);

    unsafe {
        switch(from, to);
    }
}
