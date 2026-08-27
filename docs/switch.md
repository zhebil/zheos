# SWITCH - putting a running program down and picking another up

## 1. What this is

A function that is called by one thread of execution and returns into a different one. Everything
the processor was holding for the first - registers, stack pointer, the address it was going to
return to - gets written to memory, and a second set gets loaded in its place.

Nothing about it is magic and nothing about it is a hardware feature. There is no "switch context"
instruction. The category is **software, constrained by a hardware contract**: the procedure call
standard says which registers a function must preserve, and that document is what decides how big
a context is.

This is the smallest of the tier 5 skills and the one everything else in tasking rests on.

## 2. The idea, which is uncomfortable at first

A normal function returns to whoever called it, because the return address is in `x30` and `ret`
jumps there. A context switch is a function that changes the stack pointer in the middle, so the
`ret` at the end reads a different `x30` and lands somewhere else entirely.

Written from the switching function's point of view: you are called, you save your registers on
the current stack, you write the current stack pointer into the old task's record, you read the new
task's stack pointer out of its record, you pop registers off *that* stack, and you return. The
return goes to wherever the new task was when it was last switched away from.

The first task ever switched to has never been switched away from, so its stack has to be
**faked** - written by hand to look exactly like a stack that was saved. Section 5.

## 3. What has to be saved, and what does not

The Procedure Call Standard for the Arm 64-bit Architecture, usually called **AAPCS64**, divides
the registers in two:

**Callee-saved.** `x19` through `x28`, plus `x29` the frame pointer and `x30` the link register.
A function that uses these must put them back before returning. These are the ones a context switch
must save, because from the compiler's point of view the switch is an ordinary function call and it
expects them preserved.

**Caller-saved.** `x0` through `x18`. Any function is free to destroy these. The compiler already
assumed they were gone across the call, so it has already spilled anything it needed. **A context
switch does not save them**, and that is not a shortcut - it is the calling convention being used
correctly.

That is 12 registers, 96 bytes, plus the stack pointer. Compare with what an *interrupt* has to
save: every register, because an interrupt is not a function call and the interrupted code agreed
to nothing. Your exception handler already does that work, and the difference between the two is
worth understanding, because it is the difference between a cooperative switch and a preemptive
one.

Two more that this kernel does not need yet, and will:

- **Floating point and SIMD registers**, `v0` to `v31`. 512 bytes, and untouched as long as the
  kernel never uses them. `-C target-feature=+strict-align` and the absence of floating point in
  kernel code is what makes that true, and the day something uses a `f64` it stops being true
  silently.
- **`TPIDR_EL0` and thread-local state.** Nothing uses it yet.

Write down, in the guide comment for the switch function, which of these you chose not to save and
why. That comment is what stops the silent failure later.

## 4. Where the saved state lives

Two designs, and the choice shows up in every later skill:

**On the task's own stack.** The switch pushes the callee-saved registers onto the outgoing task's
stack, and the only thing stored in the task's record is the stack pointer. One field per task, and
the "context" is a stack layout rather than a struct.

**In the task's record.** A struct with a named field per register, and the switch stores into it
by offset.

The first is what Linux does and what this kernel should do. It is smaller, it makes the faked
first entry in section 5 a matter of writing a stack rather than a struct, and it means a task's
entire saved state travels with its stack.

The cost is that the layout is implicit - it lives in the assembly and in whatever writes the fake
stack, and those two must agree exactly. Getting them out of step is the central bug of this skill
and section 8 is mostly about it.

## 5. Starting a task that has never run

`switch` restores registers and returns. A brand new task has nothing to restore, so its stack is
constructed to look like it does:

- 12 words of saved registers, all of which can be zero except one.
- That one is the slot where `x30` lands. Put the task's entry point address there. When `switch`
  pops it and executes `ret`, the processor jumps to the entry point, and from the entry point's
  perspective it was simply called.

The stack pointer stored in the record points at the bottom of that faked frame.

Two details that are easy to get wrong and hard to see:

- **The stack pointer must be 16-byte aligned.** The architecture requires it for `SP`, and a
  misalignment does not fail at the switch, it fails at the first thing the new task does that
  touches memory relative to `SP`.
- **The task needs somewhere to return to.** If the entry point ever returns, `x30` holds whatever
  the fake stack said, which is zero, and the task jumps to address 0. Put the address of a
  function that cleanly ends the task there instead, or make the entry point a function that
  cannot return.

## 6. What you are building

- A `Context` that is, in the on-stack design, a single `stack_pointer: usize`.
- A `Task` holding that, its stack region, an identifier, and a state. The state field is not used
  by this skill and is what SCHED will fill in, so keep it minimal now.
- `switch(from: *mut Context, to: *const Context)` in assembly, in its own `.s` file or a
  `global_asm!` block. Twelve pushes, one store, one load, twelve pops, one `ret`.
- A function that builds a task: allocate a stack from the heap - which by now works and can free -
  write the fake frame, and return the `Task`.

The switch is genuinely assembly and this is one of the few places in the project where that is
not a choice. The stack pointer is being changed under the compiler's feet, and there is no way to
express that in Rust that is not a lie to the optimiser.

Keep it to `stp` and `ldp` pairs, which store and load two registers at once and are the reason 12
registers cost 6 instructions rather than 12.

## 7. When nothing happens

| symptom | almost certainly |
| --- | --- |
| the first switch to a new task jumps to address 0 | the entry point is not in the `x30` slot of the fake frame, or the frame layout in the builder does not match the pop order in the assembly. |
| the switch works once and faults on the way back | the outgoing stack pointer was not stored, so the record still holds a stale value. |
| a task's locals are wrong after a switch | a callee-saved register is not in the save list. `x19` to `x28`, `x29`, `x30`. Missing `x29` is the common one because it looks like bookkeeping. |
| a fault at an address that looks like a stack pointer with the low bits set | 16-byte alignment. Section 5. |
| everything works with two tasks and breaks with three | the stack allocation, not the switch. Two stacks can overlap by exactly the amount you would not notice. |
| the machine hangs with no fault | a task returned. Its `x30` was zero or garbage and it jumped somewhere that does not fault, usually a loop in flash. |
| stack overflow inside a task corrupts another task | expected without guard pages. LOCKDOWN's guard page applies per task stack, and this is where that becomes a per-task problem rather than a one-stack problem. |
| a floating point value is wrong after a switch | something started using the SIMD registers. Section 3, and it is the failure that arrives months late. |

## 8. How you will know it worked

Two tasks, ping and pong, each with its own stack from the heap, switching to each other a hundred
times, each printing its own counter at the end and both counters reading 100. Then a clean return
into `kmain` and the monitor prompt.

The stronger observable, worth the extra ten minutes: give each task a local variable it sets
before the switch and checks after, with a different value per task. That is the direct evidence
that callee-saved registers survived, and it fails specifically and readably when one is missing,
rather than as a crash.

And print each task's stack pointer at each switch. Two numbers that alternate, each staying inside
its own allocated region, is the whole skill visible in one column of output.

---

## Optional reading

- Procedure Call Standard for the Arm 64-bit Architecture, ARM IHI 0055. Section 6.1 is the
  register roles table from section 3, and it is the specification the switch is implementing.
- `arch/arm64/kernel/entry.S` in Linux, `cpu_switch_to`, which is section 6 in twenty lines.
- `arch/arm64/kernel/process.c`, `copy_thread`, for the faked first frame.
