# WAIT - tasks that sleep and tasks that end

## 1. What this is and why

After SCHED, every task in the table gets a turn, forever. Two things are wrong with that:

- **zhemon burns its turns waiting.** It sits in `getc` doing `wfi` until a key arrives, and that
  wait eats its whole 10 ms slice, every time around.
- **A finished task never goes away.** When `print_letters` is done, it parks in `halt()`, still in
  the table, still taking turns, still holding its 16 KiB stack.

WAIT gives a task two new ways to leave the CPU: **sleep** until something wakes it, and **exit**
for good. The category is **software**, all of it - no new hardware, no new system registers. It is
the same design as Linux 0.01: a fixed task array, a state per task, a `schedule()` any task can
call, wait queues, and an idle task for when nobody has work.

`docs/diagrams/wait.tldx.jsx` shows the states a task moves through, and the order a task must go
to sleep in.

## 2. What you already have, and what changes

| already works | from |
| --- | --- |
| `switch`, `Task`, `ContextStack` that frees itself on `Drop` | SWITCH |
| the task table, `current`, `preempt()` on the tick, the trampoline | SCHED |
| `input::getc`, fed by the UART interrupt through `push_character` | UART IRQ |
| `SpinLock`, `cpu::without_interrupts` | LOCK |

What changes:

1. Each task gets a **state**: ready, blocked, or dead. The picker skips anything not ready.
   **Section 4.**
2. A task that returns from its function **exits**, and someone else frees its stack.
   **Section 5.**
3. An **idle task** that runs only when nobody else can. **Section 6.**
4. `getc` **sleeps on a wait queue** and the UART interrupt wakes it. **Section 7.**
5. A way to *see* all of it: each task counts the ticks it was running for, and zhemon gets a `ps`
   command. **Section 8.**

## 3. Reading the names

- **`ps`** - **P**rocess **S**tatus, the Unix command that lists running tasks.
- **`wfi`** - **W**ait **F**or **I**nterrupt: the CPU stops until an interrupt is pending.
- **`bl`** - **B**ranch with **L**ink: jump, and put the return address in `x30`. **`blr`** is the
  same with the target in a **R**egister.
- **IRQ** - **I**nterrupt **R**e**Q**uest. **UART** - **U**niversal **A**synchronous
  **R**eceiver-**T**ransmitter, the serial port your keys arrive through.
- **`SeqCst`** - **Seq**uentially **C**on**s**is**t**ent, the strictest atomic ordering in Rust:
  every core sees every `SeqCst` operation in one agreed order.
- **Ready / Blocked / Dead** - our three states. Linux calls them `TASK_RUNNING`,
  `TASK_INTERRUPTIBLE` and `TASK_DEAD`. Linux's "running" means "may run", which is why ours is
  called ready.

## 4. The idea: a state per task

Today the loop in `preempt()` takes the next slot that holds a task. Move it into a method on
`Scheduler`, `next_idx`, and change one rule: take the next slot whose task is **Ready**.

That is the whole mechanism for both new abilities. To sleep, a task marks itself **Blocked** and
calls `schedule()`, which switches to someone else - and the picker will not choose it again until
someone sets it back to Ready. To exit, a task marks itself **Dead** and calls `schedule()`, and it
is never chosen again at all.

`schedule()` is the second way into `switch`, next to `preempt()`. `preempt()` is the timer taking
the CPU; `schedule()` is a task giving it up. Both do the same thing, so `preempt()` becomes "if the
flag is set, call `schedule()`".

**Why still an array, not a run queue.** The SCHED guide said a queue would come with blocking. It
does not have to: with four slots, skipping the ones that are not Ready costs nothing, and Linux
0.01 did exactly this over 64 slots. A run queue - a list holding only the Ready tasks - is what you
build when scanning every slot gets too slow. Not yet.

## 5. Exit: a task cannot free its own stack

When a task's function returns, it has nowhere to go. Today the trampoline parks it. Instead, the
trampoline calls a Rust function, `task_exit`, which marks the task Dead and calls `schedule()`.
The function type becomes `fn()` instead of `fn() -> !`, and `print_letters` just returns.

`task_exit` is called from assembly by name, so it needs `#[unsafe(no_mangle)]` (keep the symbol
called exactly `task_exit`) and `extern "C"` (use the standard calling convention). It returns `!`.
`schedule()` never comes back to a Dead task, but the compiler cannot know that, so the function
ends in `unreachable!()` - if it ever does come back, you get a panic instead of running garbage.

The dead task's stack has to be freed, but **not by the dead task**. It is standing on that stack
while it calls `schedule()`, and `switch` writes its `sp` into its `Context` on the way out. Free it
too early and both of those write into freed memory. So `task_exit` only *marks* the task Dead
(`set_state`). It never empties its own slot.

Someone else frees it, later. At the start of every `schedule()`, before picking, `prune` turns
every slot that is Dead and is **not** `current` into `None`. Dropping the `Task` drops its
`ContextStack`, which gives the 16 KiB back to the heap. By then the dead task is off the CPU for
good.

Linux does the same thing in `finish_task_switch`: the next task, right after the switch, releases
the stack of the one that died.

## 6. Idle: somebody must always be runnable

Once tasks can block, "nobody is Ready" becomes possible: A and B are dead, and zhemon is waiting for
a key. `schedule()` still has to switch to *something*, because the current task just said it cannot
go on.

That something is the **idle task**: a task whose function is `loop { wfi }`. It never blocks and
never exits. The picker only chooses it when no other task is Ready. When a key arrives, the UART
interrupt wakes zhemon, and on the way out of that interrupt `preempt()` switches from idle to
zhemon (section 7 says why it does not wait for the tick).

Put idle in a fixed slot, `IDLE_TASK_ID = 1`, right after boot, so `next_idx` can skip it by
number. Linux keeps an idle task per core for the same reason, and in Linux 0.01 it was task 0.

**Two different "nobody else" answers.** `next_idx` searches every slot after `current`, wrapping
around, and ends on `current` itself. That gives three outcomes, and they must stay apart:

| situation | `next_idx` returns | `schedule()` does |
| --- | --- | --- |
| another task is Ready | `Some(that slot)` | switches to it |
| only `current` is Ready | `Some(current)` | returns, no switch |
| nobody is Ready | `None` | switches to idle (`unwrap_or(IDLE_TASK_ID)`) |

The trap is SCHED's old loop, which returned early when the search landed on `current`. Keep that
as `None` and it now means "run idle": zhemon is Ready, yet every tick swaps it out for idle.
Observed: step 3's `ps` gave zhemon 45 ticks and idle 33, where idle should have 0.

## 7. Sleeping without missing the wakeup

A **wait queue** is the list of tasks waiting for one thing - here, "a byte is in the input buffer".
It needs three operations:

- **prepare**: mark me Blocked and put me on the queue;
- **finish**: mark me Ready and take me off;
- **wake all**: every task on the queue becomes Ready, and the queue is emptied.

`getc` then sleeps like this, in this order:

1. prepare;
2. check the buffer - if a byte is there, finish and return it;
3. otherwise `schedule()`, and when you come back, go to 1.

The UART interrupt pushes the byte first, then calls wake all.

**Why prepare comes before the check.** Suppose the byte arrives at the worst moment:

- after you **checked** but before you **prepared**: the wakeup finds nobody on the queue, and you
  then go to sleep with a byte sitting in the buffer. Nobody will wake you until the *next* key.
  This is the **lost wakeup**, the classic bug of this whole area.
- with prepare first: a byte that arrives before your check is found by the check, and one that
  arrives after it finds you on the queue and wakes you. There is no gap.

**And the whole sequence runs with interrupts masked.** Wrap steps 1-3 in
`cpu::without_interrupts`, the same way `getc` already wraps its `wfi`. Otherwise the timer can
preempt you between step 1 and step 2. You are marked Blocked, so the picker treats you as asleep,
but you never got to check the buffer. Observed: widen that gap with a busy loop, feed `ps` and
`exit`, and zhemon echoes `p` and never reads the rest. Masking is what makes the sequence one step
on this core. The prepare-before-check order is what keeps it right once there are other cores.

**Wake all sets the reschedule flag.** Without that, a woken zhemon waits for the next tick, up to
10 ms. With it, `preempt()` on the way out of the very UART interrupt that woke it switches to it.
Linux does the same, in `resched_curr`.

**The queue itself** is a set of slot numbers. With at most 32 slots, a `u32` holds it: bit *n* set
means "slot *n* is waiting". An `AtomicU32` needs no lock, and wake all is one `swap(0)`. Use
`Ordering::SeqCst` on it: "add myself, then check" has to stay in that order as seen from other
cores, and `SeqCst` is the ordering that promises it. Linux puts a full memory barrier at the same
spot, in `set_current_state`.

The queue lives in `sched.rs`, because every operation also changes a task's state. Each one takes
the `SCHED` lock itself and holds it across both halves - the bit and the state - so wake all can
never land between "I joined" and "I am Blocked". The static in `input.rs` is a plain
`WaitQueue`, not a `SpinLock<WaitQueue>`: the atomic already makes it safe to share.

**Never hold a lock across `schedule()`.** The task that would take it next may be the one that
wakes you. In `getc`, pop in one statement - `INPUT_BUFFER.lock().pop()` - so the guard is a
temporary, dropped at the `;`. Bind the guard to a `let` instead, or wrap the queue in a
`SpinLock`, and zhemon sleeps holding a lock that `push_character` needs. Observed: the first key
never echoes, and about 15 seconds later `failed to acquire lock after 16777216 attempts`.

**`schedule()` masks interrupts itself.** Between dropping the lock and finishing `switch`, an
interrupt would run a second `schedule()` while `current` already names the next task - and it would
save the running task's `sp` into the wrong `Context`. `preempt()` always calls it with interrupts
masked, so the bug stays hidden until a task calls `schedule()` on its own. Observed: `exit` echoes
and the machine never powers off. Wrap the body in `cpu::without_interrupts`. Linux's `__schedule`
starts by masking interrupts too.

## 8. Seeing it: ticks and `ps`

Everything above is invisible from the outside - a sleeping zhemon and a spinning zhemon print the
same things. So measure it:

- each `Task` gets a `ticks` counter;
- `sched::tick()`, which the timer already calls, adds one to the current task's counter;
- a `ps` line in zhemon lists every task in the table - its id and its ticks - and then the heap
  line, `N of M pages free`, that `kmain` already prints at boot.

A tick is a **sample**, not a stopwatch: every 10 ms, one tick goes to whoever is on the CPU at that
instant. Over many ticks, that adds up to how much of the CPU each task used. A task that runs for a
few microseconds per key - zhemon, once it sleeps - is almost never the one caught, so it reads 0.
Linux kept the same sampled count per task, and `ps` read it.

zhemon already treats `exit` as a whole-line command before the parser runs. `ps` goes right next to
it.

## 9. The constants

| value | what | derivation |
| --- | --- | --- |
| `BOOT_TASK_ID = 0` | the boot task's slot | the code already running when `init` is called, adopted as a task in SWITCH. It runs zhemon. |
| `IDLE_TASK_ID = 1` | idle's slot | `init` creates idle right after boot. Any fixed slot works, as long as `next_idx` can skip it by number. |
| `TASKS_COUNT = 4` | table size | unchanged. Boot, idle, A and B now fill it, so a third `spawn` fails. |
| `u32` for the wait queue | 32 bits, one per slot | `TASKS_COUNT` must stay at most 32; a `const` assert holds that. `AtomicU64` would allow 64. |
| `1 << slot` | the bit for one slot | `fetch_or(1 << slot)` adds it, `fetch_and(!(1 << slot))` removes it - `!` flips every bit, so the mask keeps all bits but that one. |
| `swap(0)` | wake all | takes every waiter and empties the queue in one step, so a task cannot be added in between and missed. |
| 4 pages | one task's stack | 16 KiB / 4 KiB pages. Two exited tasks give back 8; idle holds 4 for good. |

## 10. What you are building

In this order. Each step boots and has a number to check.

Every heap check is against the last `heap:` line `kmain` prints at boot, before any task exists -
**in the same run**. That boot number moved by one page between builds of this branch, so the `ps`
numbers below are only comparable with the boot line printed next to them.

**Step 1 - ticks and `ps`.** A `ticks` field on `Task`, counted in `sched::tick()`. A `sched::ps()`
that prints each task's id and ticks, then the heap line. zhemon calls it on the line `ps`.
*Check:* `make feed INPUT='ps'`. The feed types it about one second after boot. Mine printed:

```
Task 0: ticks = 33
Task 1: ticks = 33
Task 2: ticks = 33
Heap: 32412 of 32768 pages free
```

with `heap: 32420 of 32768 pages free` as the boot line: 8 pages below it, for A and B's stacks. A
and B finished their letters long before, and zhemon was only waiting - yet each has a third of the
CPU. Both problems, in numbers.

**Step 2 - exit.** A `State` enum with `Ready` and `Dead` (`Blocked` comes in step 4; clippy
rejects an unused variant), and a `set_state` on `Task`. `next_idx` skips anything not Ready.
Split `schedule()` out of `preempt()`, mask interrupts inside it, and `prune` Dead slots at its
start. In `switch.s`, after `blr x19`, `bl task_exit` and delete the park loop - nothing reaches it
any more. `Task::new` takes `fn()`; `print_letters` returns.
*Check:* `ps` shows only task 0, now with all the ticks, and the heap is **back at the boot line** -
the two stacks came back. Mine: `Task 0: ticks = 74`, `Heap: 32419`, boot line 32419.

**Step 3 - idle.** `init` creates the idle task in slot `IDLE_TASK_ID`. `next_idx` skips it, and
`schedule()` falls back to it on `None` (section 6). Now that `init` owns slot 1, let `spawn` take
the entry function and create the `Task` itself, using the slot number as the id, so ids and slots
cannot drift apart. It can fail two ways - no free slot, or no memory for the stack - so it returns
a `SpawnError` with one variant for each.
*Check:* `ps` shows idle with **0 ticks** - zhemon is always Ready, so idle is never picked - and the
heap 4 pages below the boot line, for idle's stack. Mine: `Task 0: ticks = 76`, `Task 1: ticks = 0`,
`Heap: 32415`, boot line 32419.

**Step 4 - sleep.** Add `Blocked`. A `WaitQueue` in `sched` with `prepare`, `finish` and
`wake_all`. A static `INPUT_WAIT_QUEUE` in `input.rs`; `getc` does the sequence from section 7
inside `without_interrupts`, and `push_character` wakes the queue after pushing.
*Check:* section 12.

## 11. When something goes wrong

All observed on this kernel.

| symptom | almost certainly |
| --- | --- |
| `called Option::unwrap() on a None value` in `schedule` when a task exits | the current task's slot was emptied: `task_exit` set it to `None` instead of marking it Dead, or `prune` did not skip `current` (section 5). |
| `entered unreachable code` in `task_exit` | `next_idx` fell back to `current` instead of idle, so a dead task was switched back into (section 6). |
| idle has ticks while zhemon is still Ready (step 3) | `next_idx` returns `None` for "only `current` is Ready", and `schedule()` reads that as "run idle" (section 6). |
| the first key never echoes; ~15 s later `failed to acquire lock after 16777216 attempts` | `getc` called `schedule()` while holding the input buffer's lock or a lock around the queue (section 7). |
| `exit` echoes, then the machine never powers off | `schedule()` runs with interrupts unmasked when a task calls it directly (end of section 7). |
| zhemon echoes part of a line, then ignores the rest until another key | the sleep in `getc` is not inside `without_interrupts`, and a tick landed mid-sequence (section 7). Rare, since the gap is a few instructions. |

## 12. How you will know it worked

`make feed INPUT='ps\rexit'` - `\r` is Enter, so this types `ps`, then `exit`:

```
Hello, ZheOS!
Type 'exit' to shutdown the system
----------------------------------
\AAABBBBBAAABBBAAAABBBAAAABBAAAABBBBAAAABBBBAAABBBAAABBBBAAABBBAAAABBBAAAABBBABBBps
Task 0: ticks = 0
Task 1: ticks = 75
Heap: 32415 of 32768 pages free
\exit
```

and `make feed` returns after about 3 seconds. A hang runs until the feed's timeout kills QEMU,
which took 16.

Task 0 is the boot task, which runs zhemon; task 1 is idle.

- **zhemon has 0 ticks.** It spent the whole second asleep, not spinning. In step 1 it had 33.
- **idle has the rest.** With A and B gone and zhemon asleep, idle is the only one left, and it
  spends that time in `wfi`.
- **A and B are gone, and their stacks with them.** Only tasks 0 and 1 remain, and the heap line
  matches step 3: 4 below the boot line, for idle.
- **`ps` answered and `exit` worked.** Each key woke zhemon out of its sleep, so wake all reached it
  every time.
- Exact tick counts drift by one or two between runs. The shape - zhemon at or near 0, idle with the
  rest - is what matters.

---

## Optional reading

- Linux 0.01, `kernel/sched.c`: `schedule()` scans a 64-slot `task` array for `TASK_RUNNING`, and
  `sleep_on`/`wake_up` are the first wait queue. Short enough to read in one sitting.
- Linux `include/linux/wait.h`, `___wait_event`: the prepare, check, `schedule()` loop from
  section 7, with the same order.
- Linux `kernel/sched/core.c`, `finish_task_switch`: where a dead task's stack is released by the
  task that runs next.
- *Operating Systems: Three Easy Pieces*, chapter 30 "Condition Variables" - the lost wakeup, from
  the other side. Free online.
