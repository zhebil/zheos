# SCHED - the timer takes the CPU away

## 1. What this is and why

In SWITCH, tasks handed the CPU to each other by calling `switch` themselves. SCHED takes that
choice away from them: the **timer interrupt** calls `switch`, a hundred times a second, whether the
running task wants it or not. Taking the CPU from a task that did not offer it is called
**preemption**.

The category is **software**. The hardware gives you two things you already have - a timer that
interrupts every 10 ms, and an interrupt path that saves registers - and everything else is a
choice: which task runs next, where the tasks are kept, and where in the interrupt path the switch
happens.

What you build: two tasks that each print a letter in a busy loop and never call anything but
`print!`, plus zhemon, all sharing one core. Why it matters: until now one runaway loop owned the
machine. After this, nothing does.

`docs/diagrams/sched.tldx.jsx` is the whole idea in one picture: the timeline of turns, what
happens at each tick, and the round-robin order.

## 2. What you already have, and what changes

| already works | from |
| --- | --- |
| timer interrupt every 10 ms, handler in `src/timer.rs` | TIMER |
| `vectors.s` saves 784 bytes on the current stack, calls `irq::handle_interrupt`, restores, `eret` | exceptions, SWITCH |
| `switch`, `Task::new`, `Task::boot`, `ContextStack` | SWITCH |
| `SpinLock`, which masks interrupts while held | LOCK |

Four things change, and each is one section below:

1. The exception frame grows by 16 bytes. **Section 5.**
2. The switch happens at the end of `irq::handle_interrupt`, after the GIC is told the interrupt is
   finished. **Section 6.**
3. A new task starts through a small assembly trampoline instead of jumping straight into its
   function. **Section 7.**
4. Tasks live in one table with a "current" index. **Section 8.**

No new hardware and no new system registers. `ELR_EL1` and `SPSR_EL1` are read and written with
`mrs`/`msr`, the same way `vectors.s` already reads `esr_el1`.

## 3. Reading the names

- **IRQ** - **I**nterrupt **R**e**Q**uest. The timer and the UART both arrive as one.
- **GIC** - **G**eneric **I**nterrupt **C**ontroller. It decides which interrupt reaches the CPU.
- **EOI** - **E**nd **O**f **I**nterrupt: the write that tells the GIC you are done with one.
  `interrupt.end()` does it, by writing the **EOIR** (EOI **R**egister).
- **EL1** - **E**xception **L**evel **1**, where the kernel runs. **`FPSR`/`FPCR`** - **F**loating
  **P**oint **S**tatus / **C**ontrol **R**egister, already in the frame since SWITCH.
- **`ELR_EL1`** - **E**xception **L**ink **R**egister, EL1: the address to go back to after the
  interrupt. The hardware writes it on the way in; `eret` reads it on the way out.
- **`SPSR_EL1`** - **S**aved **P**rogram **S**tatus **R**egister, EL1: a copy of the CPU's state
  flags at the moment of the interrupt, including whether interrupts were masked.
- **`PSTATE`** - **P**rocessor **STATE**, the live version of what `SPSR_EL1` saves a copy of.
- **`DAIF`** - the four mask bits in `PSTATE`: **D**ebug, **A** (SError, "asynchronous abort"),
  **I**RQ, **F**IQ (**F**ast **I**nterrupt re**Q**uest). A set bit means masked.
- **`eret`** - **E**xception **RET**urn: jump to `ELR_EL1` and copy `SPSR_EL1` back into `PSTATE`,
  in one instruction.
- **Hz** - hertz, times per second. `TIMER_HZ = 100` is one tick every 10 ms.

## 4. The idea: a switch called from inside the interrupt

Task A is running. The timer fires. `vectors.s` pushes all of A's registers onto A's stack and calls
the handler. The handler calls `switch(A, B)` - the same `switch` from SWITCH, unchanged. It pushes
a 160-byte switch frame on top, saves `sp` into A's `Context`, and loads B's.

So a preempted task is paused twice over, on its own stack:

- underneath, the **exception frame**: where A was when the timer hit, and every register it had;
- on top, the **switch frame**: where the handler was when it switched away.

Resuming A later undoes both, in reverse. `switch` pops the switch frame and returns into A's
handler. The handler returns into `vectors.s`. `vectors.s` pops the exception frame and `eret`s to
the exact instruction A was on. A never finds out.

`docs/diagrams/sched-stacks.tldx.jsx` draws this with both stacks side by side, in step order.

That is the whole mechanism. What follows are the three places where it breaks if you do only that.

## 5. The exception frame has to carry `ELR_EL1` and `SPSR_EL1`

Today `save_all_registers` saves `x0`-`x30`, `FPSR`, `FPCR` and `q0`-`q31`. It does not save
`ELR_EL1` or `SPSR_EL1`, and until now it did not need to: nothing ran between entry and `eret`
that could change them.

Now something does. `ELR_EL1` is one register per CPU, not per task. Follow it:

1. A is interrupted. `ELR_EL1` = A's address. Switch to B.
2. B runs and is interrupted. `ELR_EL1` = B's address. Switch to A.
3. A's handler finishes and `eret`s - to `ELR_EL1`, which is **B's** address.

A's registers, on A's stack, now running B's code. It does not crash. Observed on this kernel: the
letters still alternate, but zhemon never reacts to `exit`, because the tasks are quietly running
each other's code. That makes it the worst kind of bug to find later.

The fix: read both with `mrs` on entry, store them in the frame, and write them back with `msr`
just before the `eret`. Linux's exception frame, `struct pt_regs`, holds the same two as `pc` and
`pstate`.

## 6. Switch after the EOI, never before

The GIC keeps track of the interrupt being handled. Until you write the EOI, it will not deliver
another interrupt of the same priority, and the timer's next tick is exactly that.

So if the handler switches away *before* `interrupt.end()`, the EOI is stuck inside a paused task.
The new task runs with no timer interrupts at all, and it keeps the CPU forever. Observed: task A
prints all 40 of its letters, then the machine goes silent.

The fix is to split the job. The timer handler only **marks** that a switch is due - one
`AtomicBool`, set after it rearms the timer. `irq::handle_interrupt` calls `interrupt.end()` as it
does now, and only *then* checks the flag and switches. Linux does the same, with a flag called
`TIF_NEED_RESCHED` checked on the way out of every interrupt.

## 7. A brand new task starts with interrupts masked

Taking an IRQ sets the `I` bit in `PSTATE`, so the whole handler - including the `switch` - runs
with interrupts masked. `DAIF` belongs to the CPU, not to a task, so whoever `switch` lands in
inherits it.

- A task that was **preempted before** is fine: it goes back out through `vectors.s`, and `eret`
  restores its own `SPSR_EL1`, in which `I` was clear.
- A task that **never ran** has no exception frame. With the SWITCH forged frame, its `ret` jumps
  straight into its function with interrupts still masked. Nothing can preempt it. Observed: exactly
  the same silence as section 6 - A prints 40 letters, then nothing.

The fix is a trampoline: a few instructions in `switch.s` that every new task passes through once.

1. `msr daifclr, #2` - unmask IRQs.
2. `blr x19` - call the task's function.

The forged frame changes to match: **`x30` = the trampoline, `x19` = the task's function.** `x19` is
a callee-saved register, so `switch` restores it from the frame like any other, and it is sitting
there when the trampoline runs. Linux does exactly this: on arm64 a new thread's `x19` holds its
function and its `pc` is `ret_from_fork`; on x86 `kthread_frame_init` puts the function in `bx`.

Since task functions are `fn() -> !`, the trampoline never gets control back. Park it after the
`blr` anyway, so a mistake shows up as a stopped task rather than a jump into whatever follows.

## 8. The task table

The simplest scheduler that works:

- a fixed-size array of `Option<Task>`, say 4 slots;
- the index of the task running now;
- both inside one `SpinLock`.

Slot 0 is `Task::boot` - the code that is already running, which becomes zhemon. The others come
from `Task::new`. **Round robin**: at every due switch, walk forward from the current index to the
next slot that holds a task, wrapping around. If that is the current slot, there is nobody else,
so return.

Why an array and not a queue: tasks sit still in it, and picking the next one needs no allocation
inside an interrupt handler. A queue that moves tasks around is what you add when tasks can block or
exit, and neither exists yet - that is the next skill, WAIT.

Two lock rules, both inherited:

- **Drop the guard before `switch`.** The same trap as in SWITCH: a guard borrowed straight through
  in a `let` lives to the end of the block, across the switch.
- **A task holding any `SpinLock` cannot be preempted.** `lock()` masks interrupts, so the tick waits
  until the guard drops and is taken then. That is why no "preemption counter" is needed here -
  Linux's `spin_lock_irqsave` gets the same effect the same way.

There is no idle task, because slot 0 always has something to run: zhemon waits for keys in
`input::getc`, which unmasks interrupts every time around its loop, so it can be preempted like
anyone else.

Written for one core. The lock is taken as if other cores existed, and MANY CORES will make "the
current task" one per core.

## 9. The constants

| value | what | derivation |
| --- | --- | --- |
| `800` | exception frame size | 784 + 16 for the `ELR_EL1`/`SPSR_EL1` pair. Still a multiple of 16. |
| `16 * 16` | `ELR_EL1`, `SPSR_EL1` | the first free pair after `x0`-`x30` (pairs 0-15). |
| `16 * 17` | `FPSR`, `FPCR` | moved up one pair. |
| `16 * 18` .. `16 * 48` | `q0`-`q31` | moved up one pair. The last is 768, inside the `q` form's 1008 limit. The `x` pairs must stay at or below 504, the 64-bit `stp` limit, and 16 * 17 = 272 is. |
| `#2` in `msr daifclr, #2` | unmask IRQ only | the immediate is 4 bits, one per `DAIF` letter: D = 8, A = 4, I = 2, F = 1. `cpu::unmask_irqs` uses `#3`, IRQ and FIQ. |
| `100` Hz | `TIMER_HZ` | one tick per 10 ms, so one time slice is 10 ms. |
| `4` | table slots | boot plus two tasks, plus one spare. |

## 10. What you are building

In this order. Each step boots and has something to check.

**Step 1 - save `ELR_EL1` and `SPSR_EL1`** in `save_all_registers` / `restore_all_registers`, with
the new offsets from section 9. Update the frame-size comment.
*Check:* nothing changes. Boot, type a key, `make feed INPUT='exit'` shuts down. That is the point -
the frame grew and nothing noticed.

**Step 2 - the trampoline.** In `switch.s`, a global label that unmasks IRQs and `blr x19`s, then
parks. In `Task::new`, the forged frame gets `x19` = entry and `x30` = the trampoline.
*Check:* the SWITCH ping-pong still prints its twenty lines. It is cooperative and runs with
interrupts on, so the only difference is one extra hop at each task's first start.

**Step 3 - the table**, in a new `sched` module: the locked array and current index, a function that
puts `Task::boot(0)` in slot 0, and one that places a `Task::new` in the first free slot. Delete the
ping-pong from `main.rs`, and `switch_between` with it, since nothing else uses it. Put two tasks
in the table instead: each prints its letter 40 times with
a busy loop between letters, then spins without printing.
*Check:* boot is unchanged and **no letters appear**. The tasks exist, but nothing switches to them
yet.

**Step 4 - preempt.** The timer handler sets the flag after rearming. After `interrupt.end()`,
`irq::handle_interrupt` calls into `sched`, which clears the flag, picks the next slot, takes both
`Context` pointers, drops the guard, and calls `switch`.
*Check:* section 12.

## 11. When something goes wrong

| symptom | almost certainly |
| --- | --- |
| letters alternate, but zhemon ignores `exit` and `make feed` runs to its timeout | `ELR_EL1`/`SPSR_EL1` not in the frame. Section 5. |
| one task prints all 40 letters, then silence | either the switch is before `interrupt.end()` (section 6), or the trampoline does not unmask (section 7). Same symptom, two causes. |
| no letters at all, boot looks normal | nothing calls the scheduler: the flag is never set, never checked, or the tasks never made it into the table. |
| `failed to acquire lock after 16777216 attempts` | the scheduler's guard is alive across `switch`. |
| exception with `ELR` = `0x0` on a new task's first run | `x30` is not the trampoline, or `x19` is not the entry. |
| a task crashes after running for a while | stack overflow. Each preemption puts 800 + 160 bytes plus the handler's own frames on the task's 16 KiB stack. |

## 12. How you will know it worked

`make feed INPUT='exit'` with two tasks printing `A` and `B` 40 times each, a busy loop of 2,000,000
iterations between letters:

```
Hello, ZheOS!
Type 'exit' to shutdown the system
----------------------------------
\AAAABBBBAAAABBBBAAAABBBBAAAABBBAAABBBBBAAAABBBBAAABBBBAAAABBBBAAABBBBAAABBBBAAAAexit
```

and QEMU exits on its own in about 3 seconds.

- **The letters alternate in runs**, 3 to 5 at a time, mostly 4. Neither task calls anything that
  could give the CPU away, so every change of letter is the timer taking it. A run is roughly one
  10 ms slice; how many letters fit in one depends on how fast the host runs QEMU.
- **Exactly 40 of each.** Nothing was lost or repeated when a task was paused mid-loop and resumed.
- **zhemon still echoes `exit` and shuts down**, sharing the CPU three ways. That is also the proof
  that each task returned to its *own* interrupted instruction, since zhemon was preempted like the
  others.

---

## Optional reading

- Linux `arch/arm64/kernel/entry.S`: `kernel_entry` saves `elr_el1` and `spsr_el1` into `pt_regs`
  (section 5), and `ret_from_fork` is the trampoline from section 7.
- Linux `arch/arm64/kernel/process.c`, `copy_thread` - sets a new thread's `x19` and `pc`.
- *Operating Systems: Three Easy Pieces*, chapters 6 and 7 - limited direct execution and round
  robin, free online.
