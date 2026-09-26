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

**Caller-saved.** `x0` through `x17`. Any function is free to destroy these. The compiler already
assumed they were gone across the call, so it has already spilled anything it needed. **A context
switch does not save them**, and that is not a shortcut - it is the calling convention being used
correctly.

**`x18` is neither.** AAPCS64 reserves it as the platform register and hands it to the operating
system to define. This kernel does not use it, so it is scratch here and the switch can ignore it -
but it is the one register whose role is yours to choose rather than to obey.

That is 12 registers, 96 bytes, plus the stack pointer. Compare with what an *interrupt* has to
save: every register, because an interrupt is not a function call and the interrupted code agreed
to nothing. You have already written that half: `save_all_registers` in `src/vectors.s` pushes `x0`
through `x30`, and takes 256 bytes to hold 248 bytes of register, for the same alignment reason as
the fake frame in section 5.

Put the two numbers side by side. 96 bytes because the compiler promised something; 256 bytes
because nobody promised anything. That is the whole difference between a cooperative switch and a
preemptive one, and it is already in your source twice.

Three more the frame needs an answer for:

- **Floating point and SIMD registers** - **S**ingle **I**nstruction **M**ultiple **D**ata -
  `v0` to `v31`, 128 bits each. AAPCS64 splits them exactly the way it splits the general purpose
  registers, and that split is the whole cost: *"Registers v8-v15 are Callee-saved and the
  remaining registers (v0-v7, v16-v31) are Caller-saved. Additionally, only the bottom 64 bits of
  each value stored in v8-v15 need to be Callee-saved; it is the responsibility of the caller to
  preserve larger values."* So a cooperative switch owes **`d8` to `d15`: 8 registers, 64 bits
  each, 64 bytes**, and nothing else. Not 512. 512 is what the *preemptive* path owes, for the same
  reason it owes 31 general purpose registers instead of 12.
- **`FPSR` and `FPCR`** - the **F**loating **P**oint **S**tatus and **C**ontrol **R**egisters.
  AAPCS64 says the status register's cumulative exception flags "are not preserved across a public
  interface and may have any value on entry to a subroutine", so a cooperative switch may ignore
  it. The control register holds the rounding mode and the manual calls it *global* to the program,
  which is an argument for one shared value rather than one per task. Ignore both for now, and
  write down that you did.
- **`TPIDR_EL0`** - **T**hread **P**o**I**nter / **ID** **R**egister for **E**xception **L**evel
  **0** - and thread-local state. Nothing uses it yet.

That makes the switch frame **20 registers and 160 bytes**: `x19`-`x30` and `d8`-`d15`.

### Before any of that works, the unit is off

`d8` cannot be saved while floating point is disabled, because the `stp` that saves it is itself a
floating point instruction and traps like any other. So enabling comes first, and it is one field
in one register.

`CPACR_EL1` - **C**o**P**rocessor **A**ccess **C**ontrol **R**egister for **E**xception **L**evel
**1** - carries `FPEN`, **F**loating **P**oint **EN**able, at bits `[21:20]`:

| `FPEN` | what it does |
| --- | --- |
| `0b00` | traps these instructions at EL1 and EL0. This is where you are now. |
| `0b01` | traps at EL0 only; EL1 runs them untrapped |
| `0b10` | same as `0b00` - traps at EL1 and EL0 |
| `0b11` | traps nothing |

`0b11` is the one to write while the kernel is the only thing running, which makes the whole value
`3 << 20`. `0b01` is what you come back for once there are user tasks and you want the trap to tell
you *which* task touched floating point, so you can save its registers only when it does. That is
lazy FP switching, and it is a skill of its own rather than part of this one.

Two constraints on where the write goes, and both are hard:

- **In `src/kernel.s`, before `bl kmain`.** The compiler may put a SIMD instruction inside any Rust
  function once the target allows one - including the function that would have enabled the unit.
- **Before the first `stp d8`.** Same reason, one level down.

### What flipping the target costs, which is the part to weigh

`.cargo/config.toml` pins `aarch64-unknown-none-softfloat`, whose calling convention has no
floating point registers in it, so today the compiler cannot emit a `v` register even if `FPEN`
allows it. Flipping to plain `aarch64-unknown-none` lets it emit them *anywhere* - including inside
`uart::handle_interrupt` and the timer handler, which sit on a path that **returns**.

`save_all_registers` in `src/vectors.s` saves `x0`-`x30` and nothing else. The day the target
flips, that path has to grow 512 bytes of `v0`-`v31` plus the status register, or kernel interrupt
handlers have to be kept provably free of floating point. That is the real price of floating point
in a kernel: it is charged on every interrupt, not on every switch, and it is why Linux makes
kernel code ask permission before using SIMD instead of letting the compiler decide.

So split the decision. **Enable `FPEN` and reserve `d8`-`d15` in the frame now** - that costs four
`stp`/`ldp` pairs per switch, nothing at all on the interrupt path, and it settles the frame layout
permanently, which is the thing section 4 warns is expensive to change later. **Flip the target
when something actually needs a float**, and treat the interrupt frame as the piece of work it is.

Write down, in the comment on the switch function, which registers you chose not to save and why.
That comment is what stops the silent failure later.

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

`docs/diagrams/switch.tldx.jsx` draws it: both stacks, the twelve slots on each, and the `ret`
leaving through a stack it did not arrive on.

## 5. Starting a task that has never run

`switch` restores registers and returns. A brand new task has nothing to restore, so its stack is
constructed to look like it does:

- 20 saved registers, which is 160 bytes: `x19`-`x30` and `d8`-`d15`. Not 20 words - a word on
  aarch64 is 4 bytes, and this is the one place where the layout has to match the assembly exactly.
  160 is already a multiple of 16, so the frame needs no padding. All 20 slots can be zero except
  one.
- That one is the slot where `x30` lands. Put the task's entry point address there. When `switch`
  pops it and executes `ret`, the processor jumps to the entry point, and from the entry point's
  perspective it was simply called.

The stack pointer stored in the record points at the bottom of that faked frame.

Two details that are easy to get wrong and hard to see:

- **The stack pointer must be 16-byte aligned.** AAPCS64 requires it at every public interface,
  and the hardware *can* enforce it: `SCTLR_EL1.SA`, bit 3 - **S**tack **A**lignment check - faults
  on any `SP`-relative access made from a misaligned `SP`, and bit 4 `SA0` does the same for
  exception level 0. **This kernel sets neither.** `src/mmu/init.rs` builds its `SCTLR_EL1` -
  **S**ystem **C**on**T**ro**L** **R**egister - out of `M | C | I | WXN` (**W**rite permission
  implies e**X**ecute **N**ever) and nothing else, so unless the reset value already carries `SA`, a
  misaligned stack will not fault at all - it will hand every task a frame that is off by eight and
  surface somewhere unrelated much later. You already print `sctlr_el1` in binary either side of
  `mmu::enable`; read bit 3 there before trusting either outcome. Setting `SA` is a one-bit change
  and turns this from silent into a fault at the first access, which is worth doing *before* you
  write the fake frame rather than after.
- **The task needs somewhere to return to.** If the entry point ever returns, `x30` holds whatever
  the fake stack said, which is zero, and the task jumps to address 0. Put the address of a
  function that cleanly ends the task there instead, or make the entry point a function that
  cannot return.

## 6. What you are building

- A `Context` that is, in the on-stack design, a single `stack_pointer: usize`.
- A `Task` holding that, its stack region, an identifier, and a state. The state field is not used
  by this skill and is what SCHED will fill in, so keep it minimal now.
- `switch(from: *mut Context, to: *const Context)` in assembly, in its own `.s` file or a
  `global_asm!` block. Ten pairs down, one store, one load, ten pairs back, one `ret`.
- A function that builds a task: allocate a stack from the heap, write the fake frame, and return
  the `Task`. The heap is real now - `src/heap.rs` registers a `#[global_allocator]`, and `kmain`
  already builds a `Vec`, drops it, and prints the heap returning to its previous size. A task stack
  is the first allocation in this kernel whose lifetime is not simply "the rest of boot", so it is
  also the first real exercise of `free`.

The switch is genuinely assembly and this is one of the few places in the project where that is
not a choice. The stack pointer is being changed under the compiler's feet, and there is no way to
express that in Rust that is not a lie to the optimiser.

Keep it to `stp` and `ldp` pairs, which store and load two registers at once and are the reason 20
registers cost 10 instructions rather than 20. The same two mnemonics take `d` registers as well as
`x` ones, so the floating point half of the frame is written exactly like the general purpose half -
`stp d8, d9, [sp, #96]` sits next to `stp x19, x20, [sp]` and needs nothing new learned.

## 7. When nothing happens

| symptom | almost certainly |
| --- | --- |
| the first switch to a new task jumps to address 0 | the entry point is not in the `x30` slot of the fake frame, or the frame layout in the builder does not match the pop order in the assembly. |
| the switch works once and faults on the way back | the outgoing stack pointer was not stored, so the record still holds a stale value. |
| a task's locals are wrong after a switch | a callee-saved register is not in the save list. `x19` to `x28`, `x29`, `x30`. Missing `x29` is the common one because it looks like bookkeeping. |
| a fault at an address that looks like a stack pointer with the low bits set | 16-byte alignment. Section 5. |
| everything works with two tasks and breaks with three | the stack allocation, not the switch. Two stacks can overlap by exactly the amount you would not notice. |
| the machine hangs with no fault | a task returned. Its `x30` was zero or garbage and it jumped somewhere that does not fault, usually a loop in flash. |
| stack overflow inside a task corrupts another task | expected. LOCKDOWN planted exactly one guard page, and the linker planted it: `__stack_guard_start`, the `guard:` line `kmain` prints, sits below the **boot** stack. A stack allocated from the heap inherits none of that - the page below it is ordinary readable, writable heap. Giving each task stack its own unmapped page is a decision this skill has to make, not a protection it already has. |
| the very first `stp d8` traps, before anything has been saved | `CPACR_EL1.FPEN` is still `0b00`. It has to be written in `src/kernel.s` before `bl kmain`, not from Rust. |
| a floating point value is wrong after a switch | `d8`-`d15` are missing from the frame, or they are in it in a different order than the builder wrote. `d0`-`d7` and `d16`-`d31` are not the suspect - they are caller-saved, and the compiler already spilled whatever it needed. |
| a `f64` is wrong only in code that was interrupted | the target was flipped to `aarch64-unknown-none` without extending `save_all_registers`. Section 3. This one does arrive months late. |

## 8. How you will know it worked

Two tasks, ping and pong, each with its own stack from the heap, switching to each other a hundred
times, each printing its own counter at the end and both counters reading 100. Then a clean return
into `kmain`.

Mind the order when you place it: `kmain` ends with `zhemon::Zhemon::new().start()` and then powers
the machine off through PSCI - the **P**ower **S**tate **C**oordination **I**nterface, the firmware
call that ends the run - so the monitor never gives control back. The ping-pong has to run
above that line.

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
