use core::sync::atomic::{AtomicBool, Ordering};

use crate::{
    context::{Context, State, Task, switch},
    cpu,
    heap::HEAP,
    lock::SpinLock,
    println,
};

type Tasks = [Option<Task>; TASKS_COUNT];

const TASKS_COUNT: usize = 4;
const BOOT_TASK_ID: usize = 0;
const IDLE_TASK_ID: usize = 1;

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

    pub fn prune(&mut self) {
        for i in 0..TASKS_COUNT {
            if self.task_table[i]
                .as_ref()
                .is_some_and(|t| t.state() == State::Dead && i != self.current)
            {
                self.task_table[i] = None;
            }
        }
    }

    pub fn next_idx(&self) -> Option<usize> {
        for i in 0..TASKS_COUNT {
            let idx = (self.current + i + 1) % TASKS_COUNT;
            if self.task_table[idx]
                .as_ref()
                // Skip idle task during scheduling
                .is_some_and(|t| t.state() == State::Running && t.id() != IDLE_TASK_ID)
            {
                return Some(idx);
            }
        }
        None
    }

    pub fn free_id(&self) -> Option<usize> {
        self.task_table.iter().position(|t| t.is_none())
    }
}

pub enum SpawnError {
    RanOutOfIds,
    RanOutOfMemory,
}

pub fn spawn(entry: fn()) -> Result<(), SpawnError> {
    let mut sched = SCHED.lock();
    let Some(empty_idx) = sched.free_id() else {
        return Err(SpawnError::RanOutOfIds);
    };

    let Some(task) = Task::new(empty_idx, entry) else {
        return Err(SpawnError::RanOutOfMemory);
    };

    sched.task_table[empty_idx] = Some(task);
    Ok(())
}

pub fn init() {
    let mut sched = SCHED.lock();
    sched.task_table[0] = Some(Task::boot(BOOT_TASK_ID));
    let idle_task: Task = Task::new(IDLE_TASK_ID, idle).unwrap();
    sched.task_table[1] = Some(idle_task);
}

static SCHED: SpinLock<Scheduler> = SpinLock::new(Scheduler::new());

static NEED_RESCHEDULE: AtomicBool = AtomicBool::new(false);

pub fn tick() {
    NEED_RESCHEDULE.store(true, Ordering::Relaxed);

    let mut sched = SCHED.lock();
    let current = sched.current;
    sched.task_table[current].as_mut().unwrap().tick();
}

pub fn ps() {
    let sched = SCHED.lock();
    for task in sched.task_table.iter().flatten() {
        println!("Task {}: ticks = {}", task.id(), task.ticks());
    }
    HEAP.with(|h| println!("Heap: {}", h))
}

pub fn preempt() {
    let need_reschedule = NEED_RESCHEDULE.swap(false, Ordering::Relaxed);

    if !need_reschedule {
        return;
    }

    schedule();
}

pub fn schedule() {
    cpu::without_interrupts(|| {
        schedule_inner();
    })
}

fn schedule_inner() {
    let mut sched = SCHED.lock();

    // Remove dead tasks from the task table
    sched.prune();

    let current = sched.current;

    let next_idx = sched.next_idx().unwrap_or(IDLE_TASK_ID);

    if current == next_idx {
        return;
    }

    let from: *mut Context = sched.task_table[current].as_mut().unwrap().context_mut();
    let to: *const Context = sched.task_table[next_idx].as_ref().unwrap().context();

    sched.current = next_idx;
    drop(sched);

    unsafe {
        switch(from, to);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn task_exit() -> ! {
    let mut sched = SCHED.lock();
    let current = sched.current;

    let current_task = sched.task_table[current].as_mut().unwrap();
    current_task.die();

    drop(sched);
    schedule();

    unreachable!()
}

pub fn idle() {
    loop {
        cpu::wait_for_interrupt();
    }
}
