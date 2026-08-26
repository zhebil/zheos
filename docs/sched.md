# SCHED - deciding who runs next

## 1. What this is

SWITCH can move from one task to another. SCHED is what decides which one, and when, and does it
without the task's cooperation.

The category is **policy**, and it is the first skill in the project that is mostly a decision
rather than a mechanism. Everything it needs mechanically exists by now: a timer that interrupts,
an exception path that saves state, a context switch, an allocator that can hand out stacks and
take them back.

The word for taking the processor away from a task that did not ask to give it up is
**preemption**, and it is the entire difference between this skill and a `yield` function.

## 2. Preemption is the whole idea

A cooperative scheduler needs every task to call `yield` often enough. One task that does not -
one infinite loop, one long computation - and the machine stops responding. Every task is trusted,
and the trust is not enforceable.

A preemptive scheduler takes the processor back on a timer interrupt. The task does not know it
happened. That is the property that makes a kernel a kernel rather than an event loop, and it is
why the timer interrupt is the load-bearing piece.

Your `src/timer.rs` already fires at 100 hertz and `irq::handle_interrupt` already dispatches it.
The change is what the handler does at the end.

## 3. The part where it stops being SWITCH

There is a real subtlety here, and it is the reason this skill is separate from the last one.

SWITCH saves 12 callee-saved registers, because it is a function call and the calling convention
already dealt with the rest. A **timer interrupt** is not a function call. The interrupted code
agreed to nothing, so every register has to be preserved, and your exception entry path already
does that - it saves the full register set on the way in and restores it on the way out.

So a preemptive switch is two saves stacked on top of each other: the exception entry saves
everything for the interrupted task, and then, from inside the handler, SWITCH saves the
callee-saved set for the handler's own frame. When the task is resumed, the exception return
restores the full set.

That layering works, and it is what makes it possible to reuse SWITCH unchanged. The thing to
verify rather than assume is that the exception return happens on the **new** task's stack with the
new task's saved frame, which is exactly what the stack pointer swap arranges.

The alternative design - switching by rewriting the saved exception frame rather than by calling
SWITCH - is what some kernels do. It is fewer instructions and much harder to reason about. Not
worth it here.

## 4. Round robin, and why it is enough

Every runnable task gets a turn, in order, for one tick. When the tick fires, the running task goes
to the back of the queue and the front of the queue runs.

That is it, and it is enough for a long time. Worth knowing what it does not do, so that you know
what you are choosing:

- **No priorities.** A task that matters more does not get more.
- **No fairness over time.** A task that blocks on input and wakes up gets one tick like everyone
  else, even though it used almost nothing.
- **No accounting.** Nobody knows how much processor each task has had.

Linux's Completely Fair Scheduler tracks accumulated runtime per task and always runs the one with
the least, which fixes all three at the cost of a red-black tree and a lot of tuning. That is a
later skill if it is ever a skill at all. Round robin first, and it is what most real-time
schedulers still are.

## 5. Task states

Three, and the third is what makes the scheduler more than a rotation:

- **Running.** On a processor right now. With four cores, up to four tasks are.
- **Ready.** Could run, waiting for a turn. These are the queue.
- **Blocked.** Waiting for something that has not happened, and must not be given a turn. Waiting
  for a keystroke is the case you already have, in `src/input.rs`.

`getc` today spins in `wfi` inside `without_interrupts` until a byte arrives. Once tasks exist,
that is a task that should be **blocked**, not one that should be spinning, and converting it is
the first genuinely useful thing the scheduler does. That conversion is what turns "several things
take turns" into "several things wait on different events", which is the point of the exercise.

## 6. It has to be multi-core from the start

MANY CORES comes before this skill deliberately, and the reason is that a single-core scheduler and
a multi-core scheduler are not the same design with a loop around it.

Three questions that have no single-core answer:

- **One queue or one per core?** One shared queue is simple and every scheduling decision contends
  on one lock. Per-core queues scale and need work stealing so an idle core does not sit next to a
  busy one.
- **Can a task move between cores?** If yes, everything per-task must be genuinely per-task and not
  accidentally per-core. If no, a core's queue can starve while another is idle.
- **Who runs the idle task?** Each core needs something to run when its queue is empty, and it must
  be a real task with a real stack, because it will be interrupted and switched away from like any
  other.

Discovering these after writing a single-core version is a rewrite, which is the specific cost this
ordering avoids. Start with one shared queue behind the lock from LOCK, know that it is the
contended design, and leave a note saying what would replace it.

## 7. The reentrancy problem this skill creates

The scheduler runs inside an interrupt handler and it allocates - stacks, task records, queue nodes.
The allocator masks interrupts and takes a lock. That is fine and was designed for.

What is not automatically fine: **a switch that happens while a lock is held.** If a task takes the
allocator's lock, and the timer preempts it, and the scheduler runs another task that wants the
allocator, that task spins on a lock held by a task that is not currently running. On one core that
is permanent. On four it resolves only if the holder happens to be running elsewhere.

Two standard answers, and this is the design decision of the skill:

- **Do not preempt while a lock is held.** A per-core counter, incremented when any lock is taken
  and decremented when released; the timer handler declines to switch when it is non-zero. This is
  Linux's `preempt_count`, and it is why that counter exists.
- **Make the locks not disable preemption but be sleepable.** A much larger change, and the wrong
  one for a kernel this size.

Take the first. It means the guard from LOCK grows a second responsibility, which is a change to a
finished skill and should be made deliberately rather than discovered.

## 8. What you are building

- `Task`, extending SWITCH's with a state, an identifier, and its stack region so it can be freed.
- A run queue behind the lock. A `VecDeque` from `alloc` is the honest first answer now that the
  heap works, and using it here is the payoff for the three allocator skills.
- An idle task per core.
- `schedule()` - pick the next task, mark states, call `switch`. Callable both from the timer
  handler, for preemption, and directly, for blocking.
- `block()` and `wake(task)`, and the conversion of `input::getc` to use them.
- A preemption counter, per section 7, wired into the lock guard.
- Task exit: a task's entry point returning has to free its stack and never come back, which is the
  fake `x30` slot from SWITCH section 5 finally getting a real value.

## 9. Testing it

The queue and the state machine are host-testable and worth testing hard, because they are ordinary
data structures and the bugs in them are ordinary bugs:

- round robin order is actually round robin, over several rotations
- a blocked task is never picked
- waking a blocked task puts it at the back, not the front
- an empty queue picks the idle task
- exiting the last task does not leave the queue in a state that picks a freed task

What only the machine can test is the preemption itself, and the test that proves it is a task with
no yield in it at all: a tight infinite loop that increments a counter. If another task still runs,
preemption works. That single test is worth more than the rest combined, because it is the property
the whole skill exists for.

## 10. When nothing happens

| symptom | almost certainly |
| --- | --- |
| the first switch out of `kmain` never comes back | `kmain` is not a task. It needs a task record to be switched *away from*, even if nothing ever switches back to it. |
| tasks run once each and then the machine hangs | the running task is not being put back on the queue, or is being put back in the wrong state. |
| a task resumes with the wrong registers | the layering in section 3. The exception frame and the switch frame are on the same stack and one of them is being restored from the wrong place. |
| everything works until a task allocates | the lock-and-preempt interaction. Section 7. |
| the machine hangs the moment a second core is scheduling | one queue, no lock, or a lock taken without masking. |
| a task's stack is freed while it is running | exit ordering. A task cannot free its own stack while standing on it; the next task, or the idle task, has to do it. |
| ticks stop entirely after the first switch | the timer comparison register was not reprogrammed, because the handler switched away before reaching the line that does it. Reprogram before scheduling, not after. |
| keystrokes are lost once `getc` blocks | the wake is happening from the interrupt handler into a queue behind a lock that the handler cannot take. The wake path has to be safe from interrupt context, which is a different constraint from the allocation path. |

## 11. How you will know it worked

Three tasks printing their own names in a loop with no yield anywhere in them, interleaving evenly,
while the monitor still responds to keystrokes.

The details that make it convincing rather than plausible:

- One of the three is a tight loop with no input, output or system call of any kind. It cannot
  cooperate, so if the other two keep running, the processor is being taken from it.
- The interleaving is even. Roughly equal counts over a second is round robin working; wildly
  unequal counts mean something is yielding early or the tick is not firing on every core.
- A task exits and the others carry on, and `heap::free_bytes()` goes back up by that task's stack.
  That is the allocator and the scheduler agreeing, and it is the first moment the whole system
  behaves like one.

---

## Optional reading

- `kernel/sched/core.c` in Linux, `__schedule`. Long, and the top of it is recognisably section 8.
- `include/linux/preempt.h` for `preempt_count` and section 7.
- Operating Systems: Three Easy Pieces, chapters 7 through 10, on scheduling policy. Free online,
  and the clearest writing on why round robin is where everyone starts.
