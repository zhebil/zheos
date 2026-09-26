# SWITCH - putting a running program down and picking another up

## 1. What this is and why

A **task** is a function running on a stack of its own, which can be paused and later resumed
exactly where it stopped. `switch` is the function that does the pausing and resuming: it is called
by one task and returns into a different one.

There is no "switch" instruction and no hardware feature behind this. The category is **software,
constrained by a hardware contract**: the calling convention says which registers a function must
leave intact, and that document is what decides how much a switch has to save.

What you build here: two tasks, `ping` and `pong`, that hand the CPU back and forth ten times by
hand, each keeping its own counter, and then give it back to the boot code so zhemon starts as
usual. Why: SCHED does exactly this, except the timer decides when to call `switch` instead of the
tasks themselves.

## 2. What you touch

No device, so no MMIO address like the UART's `0x0900_0000`. A switch is general purpose registers,
the stack pointer and ordinary RAM.

There is one system register, and only because the frame includes floating point registers:
`CPACR_EL1`, written once with `msr` in `src/kernel.s` before `bl kmain`. A system register lives
inside the CPU rather than at an address, so `make mem` cannot read it - that is what `mrs` is for,
and what `cpu::mmu::read_cpacr_el1` wraps.

## 3. Reading the names

- **AAPCS64** - **A**rm **A**rchitecture **P**rocedure **C**all **S**tandard, **64**-bit. The
  document that says which register holds which argument and who must preserve what.
- **`x29`** is the **F**rame **P**ointer and **`x30`** the **L**ink **R**egister: `bl` writes the
  return address into `x30`, and `ret` jumps to whatever `x30` holds.
- **`sp`** - **S**tack **P**ointer.
- **`stp` / `ldp`** - **ST**ore **P**air / **L**oa**D** **P**air. Two registers to or from memory
  in one instruction.
- **`v0`-`v31`** are the 128-bit floating point and SIMD registers - **S**ingle **I**nstruction
  **M**ultiple **D**ata. **`q8`** is all 128 bits of `v8` (**Q**uad word), **`d8`** is its bottom
  64 (**D**ouble word).
- **`CPACR_EL1`** - **C**o**P**rocessor **A**ccess **C**ontrol **R**egister, **E**xception
  **L**evel **1**. **`FPEN`** inside it is **F**loating **P**oint **EN**able.
- **`FPSR` / `FPCR`** - **F**loating **P**oint **S**tatus / **C**ontrol **R**egister.
- **`SCTLR_EL1`** - **S**ystem **C**on**T**ro**L** **R**egister. Its **`SA`** bit is **S**tack
  **A**lignment check, and **`SA0`** is the same check for EL**0**.

## 4. The idea: a `ret` that lands somewhere else

A normal function returns to whoever called it, because `bl` put the return address in `x30` and
`ret` jumps there. `switch` changes `sp` halfway through, so the registers it pops at the end -
`x30` among them - come off a *different* stack. The `ret` then lands wherever the other task was
when it last called `switch`.

From the paused task's side nothing odd happened. It called `switch`, and much later `switch`
returned, with every register it cared about intact.

## 5. What gets saved: 20 registers, 160 bytes

To the compiler, `switch` is an ordinary function call, so AAPCS64 is the whole rulebook:

- **`x0`-`x17` are caller-saved.** Any call may destroy them, so the compiler already moved
  anything it still needed out of them before calling. `switch` leaves them alone. That is not a
  shortcut, it is the convention used correctly.
- **`x18`** is the platform register, reserved for the OS to define. This kernel does not use it.
- **`x19`-`x28`, `x29`, `x30` are callee-saved.** The caller expects them back unchanged. That is 12
  registers.
- **`d8`-`d15`.** AAPCS64 §6.1.2: *"Registers v8-v15 are Callee-saved and the remaining registers
  (v0-v7, v16-v31) are Caller-saved. Additionally, only the bottom 64 bits of each value stored in
  v8-v15 need to be Callee-saved."* So 8 registers of 64 bits, not all of `q0`-`q31`.
- **`FPSR` and `FPCR`** are left out. AAPCS64 says the status flags may have any value on entry to a
  function, and the control register (rounding mode) is one setting for the whole program.

12 + 8 = **20 registers × 8 bytes = 160 bytes**.

Compare with the interrupt path in `src/vectors.s`, which saves **784 bytes**: all of `x0`-`x30`
(256 with padding), `FPSR` and `FPCR` (16), and all of `q0`-`q31` (512). An interrupt is not a
function call, so the interrupted code agreed to nothing and everything has to survive. 160
because the compiler promised something, 784 because nobody promised anything - that is the
difference between a cooperative switch and a preemptive one.

Saving `d8` needs the floating point unit on, because `stp d8, d9` is itself a floating point
instruction and traps like one. `src/kernel.s` sets `CPACR_EL1.FPEN` before `bl kmain`, and it has
to be there rather than in Rust: once the target is `aarch64-unknown-none`, the compiler may use a
`v` register inside any function, including the one that would have enabled them.

## 6. Where the saved registers live

Two designs exist, and real kernels use both:

- **On the task's own stack.** `switch` pushes the registers onto the outgoing stack, and the only
  thing kept in the task's record is `sp`. Linux on x86-64 does this: `__switch_to_asm` pushes six
  registers, and `struct inactive_task_frame` describes them, with the comment *"This must match
  the order in inactive_task_frame"*.
- **In the task's record.** A struct with a field per register. Linux on arm64 does this:
  `cpu_switch_to` stores `x19`-`x30` and `sp` into `task->thread.cpu_context`, at offsets
  generated from the C struct so the assembly never hardcodes one.

This kernel uses the first. It makes starting a new task (section 8) a matter of writing a stack.
The cost is that the frame layout exists twice - in `switch.s` and in the Rust that forges frames -
and the two must agree exactly. `SwitchFrame` plus a size assertion is how you keep them honest.

`docs/diagrams/switch.tldx.jsx` draws it: two stacks, the ten pairs on each with their offsets, and
the `ret` leaving through a stack it did not arrive on.

## 7. The switch itself, in four moves

`switch(from, to)` gets `from` in `x0` and `to` in `x1`, both pointing at a `Context` whose first
and only field is a saved `sp`.

1. **Push** `x19`-`x30` and `d8`-`d15` onto the current stack: `sub sp, sp, #160`, then ten `stp`.
2. **Store `sp` into `*from`.** Only now, so it points at the frame just pushed.
3. **Load `sp` from `*to`.**
4. **Pop** the same ten pairs, `add sp, sp, #160`, `ret`.

Steps 2 and 3 go through a scratch register (`mov x9, sp` then `str x9, [x0]`), because `sp` cannot
be the data register of a load or store - register number 31 in that position means the zero
register `xzr`. `x9` is safe to clobber because it is caller-saved.

The layout, offsets from the new `sp`:

| offset | pair |
| --- | --- |
| `+0` .. `+80` | `x19,x20` `x21,x22` `x23,x24` `x25,x26` `x27,x28` `x29,x30` |
| `+96` .. `+144` | `d8,d9` `d10,d11` `d12,d13` `d14,d15` |

So `x30` sits at 80 + 8 = **88**.

## 8. A task that has never run: the forged frame

`switch` pops a frame and returns. A brand new task has no frame to pop, so you write one by hand -
exactly the 160 bytes a `switch` *would* have left if this task had run before and paused:

- every slot zero, except
- **`x30` = the entry function's address**,
- and the task's saved `sp` = `stack.top() - 160`.

When `switch` pops that frame, `ret` jumps into the entry function. Zeros in the other slots are
not required by anything, but a zero `x29` ends the frame-pointer chain cleanly, and every boot
starts from the same state.

Two consequences:

- **The entry function must never return.** On entry `x30` still holds the entry function's own
  address, so a `ret` from it would jump back to its first instruction and start again. Typing it
  `fn() -> !` makes the compiler refuse any path that returns.
- **`sp` must stay 16-byte aligned.** 160 is a multiple of 16 and the stack top is page-aligned, so
  the forged `sp` is too. On this machine a misaligned `sp` faults rather than corrupting quietly:
  the boot log's first `sctlr_el1:` line ends in `...111000`, so `SA` (bit 3) and `SA0` (bit 4) are
  already 1 in QEMU's reset value. The architecture leaves that reset value to the implementation,
  so it is QEMU's choice, not a guarantee.

## 9. The boot code is a task too - adopted, not created

`kmain` is already running when the first switch happens, on the stack `linker.ld` reserves
(`__stack_top`). To switch *away* from it, it needs a `Context` for `switch` to save into, and to
switch *back*, that `Context` has to be findable.

It does not need a heap stack or a forged frame. Forging one would be wrong: if anything ever
switched into it, `kmain` would start again from the top. So the boot task gets its own
constructor: a `Context` holding 0, which the first `switch` overwrites with the real `sp`, and no
stack of its own. Linux has the same special case - `init_task` is the one task that is never
forked, it is the boot flow wrapped up after the fact.

## 10. Where tasks live, and the lock-guard trap

`pong` takes no arguments, yet it has to name its own `Context` and the boot one to switch back.
So tasks live in statics, each a `SpinLock<Option<Task>>`, filled in by `kmain` before the first
switch. `Option` because a task allocates its stack at runtime.

A static also fixes a quieter problem: `switch` holds raw pointers into `Context`s, and a `Task`
that moves after its pointer was taken leaves that pointer aimed at the old address. A static never
moves. Rust catches the local version of this too - use a `task` after moving it into a static and
it is error E0382.

**The trap.** Each `.lock()` returns a guard; the lock is held until the guard is dropped. Normally
a guard made inside a `let` dies at the end of that `let`. But when the `let` *borrows straight
through it*, Rust extends the guard's life to the end of the enclosing block - so the borrow does
not dangle:

| the `let` looks like | guard dropped |
| --- | --- |
| `&mut *LOCK.lock()` or `&*LOCK.lock()`, coerced to a raw pointer | end of the **block** - across the switch |
| `LOCK.lock().as_mut().unwrap().context_mut()`, a method chain | end of the `let` |
| a named guard, then `drop(guard)` | exactly where you say |

A guard held across `switch` stays held inside a paused task. Two ways it shows up:

- The other task asks for the same lock and spins forever. `SpinLock` gives up after 2^24 tries
  and prints `failed to acquire lock after 16777216 attempts`.
- Nobody asks for it, but `SpinLock` masks interrupts while held, and a guard that never drops
  never unmasks them. The UART interrupt stops, zhemon never sees a key, and `make feed` sits until
  its 15-second guard.

Take the two pointers in one helper, with every guard gone before the call, and every switch goes
through it.

The lock protects *getting* the pointers, not the switch: `switch` writes `sp` after the guards
are gone. On one core with hand-picked switches that is fine. SCHED needs a real answer, and Linux's
is to hold the run-queue lock *across* the switch and have the task on the other side release it.

## 11. The constants

| value | where | derivation |
| --- | --- | --- |
| `160` (`0xa0`) | frame size | 20 registers × 8 bytes. A multiple of 16, so no padding. |
| `88` | `x30`'s offset | pair 5 (`x29,x30`) at 5 × 16 = 80, second register of the pair +8. |
| `16 KiB` (`0x4000`) | task stack size | four pages. Plenty for a function that prints; the interrupt frame alone is 784 bytes and lands on whichever stack was running. |
| `4096` | stack alignment | one page, so the top is page-aligned and therefore 16-aligned. |
| `3 << 20` = `0x300000` | `CPACR_EL1` | `FPEN` is bits `[21:20]`; `0b11` there is 3 shifted up 20. `kernel.s` writes it as `#0x30 << 16`, the same number. |

`FPEN`'s four encodings:

| `FPEN` | effect |
| --- | --- |
| `0b00` | traps floating point at EL1 and EL0 - the reset state |
| `0b01` | traps at EL0 only |
| `0b10` | same as `0b00` |
| **`0b11`** | **traps nothing** - what `kernel.s` writes |

`0b01` becomes interesting once there are user tasks: trapping their first floating point
instruction is how a kernel saves the 512 bytes of `q` registers only for tasks that use them.

## 12. What you are building

- **`ContextStack`** - a 16 KiB, page-aligned heap allocation with `top()`, freed in `Drop`.
- **`Context`** - `#[repr(C)]`, one field: the saved `sp`. `repr(C)` because `switch.s` reads
  offset 0, and default Rust layout promises nothing.
- **`SwitchFrame`** - `#[repr(C)]`, the 20 registers in exactly the order `switch.s` pushes them,
  `#[derive(Default)]` for the zeros, and a compile-time assertion that its size is 160.
- **`Task`** - an id, a `Context`, and an `Option<ContextStack>`. Two constructors: `new(id,
  entry: fn() -> !)` allocates a stack and forges the frame; `boot(id)` adopts the running code with
  no stack. The stack field is never read, only dropped - its `Drop` frees the memory the saved
  `sp` points into, so it has to live as long as the task.
- **`switch`** in `src/switch.s`, included by `global_asm!` from `context.rs` and declared in an
  `unsafe extern "C"` block as `(from: *mut Context, to: *const Context)`. `*const` for `to`
  because the assembly only reads it.
- **One helper** that takes two task statics, gets both pointers with every guard dropped, and calls
  `switch`.
- **`BOOT`, `PING`, `PONG`** statics in `main.rs`, and `ping`/`pong` that each keep a counter and a
  float, print them, and switch to the other. After ten rounds `ping` switches to `BOOT`.

## 13. When something goes wrong

| symptom | almost certainly |
| --- | --- |
| exception with `ELR` (**E**xception **L**ink **R**egister, the address that faulted) = `0x0` on the first switch into a task | the entry address is not at `+88`, or the frame builder and `switch.s` disagree on order. |
| `ELR` is an odd address that is not the entry function | the saved `sp` points somewhere that is not a frame. |
| `invalid asm template string: expected }, found /` | a `{` or `}` in a comment inside a `global_asm!` file. Braces are format placeholders there, even in comments. |
| `switch` missing from `llvm-nm kernel.elf` | nothing calls it yet. rustc links with `--gc-sections`, and only `_start` is `KEEP`. It appears once Rust references it. |
| `failed to acquire lock after 16777216 attempts` right after a switch | a lock guard held across the switch. Section 10. |
| everything prints, but zhemon ignores input and `make feed` hangs | the same guard, quieter: interrupts stay masked. |
| a task's locals are wrong after a switch | a callee-saved register is missing from the frame, or the frame is not the size `switch.s` thinks. |
| the first `stp d8` traps | `CPACR_EL1.FPEN` is still `0b00`. |
| a task overflows its stack and corrupts its neighbour | expected. Heap stacks have no guard page; unmapping one needs a live mapping change, which is `zheos-e7a`. |

## 14. How you will know it worked

`make feed INPUT='exit'`, after the translate lines:

```
ping 0 1
pong 0 1
ping 1 2
pong 1 2
...
ping 9 512
pong 9 512
Hello, ZheOS!
Type 'exit' to shutdown the system
```

and then `exit` shuts the machine down in about three seconds, not fifteen.

What each part proves:

- **Strict alternation** means each `switch` stored and loaded the right `sp`.
- **Each side counts 0 to 9 on its own** means each task got its own stack back intact, and the
  compiler did keep `n` and `x` there: disassembling `ping` shows them at `[sp, #0xc]` and
  `[sp, #0x10]`, reloaded right after `bl switch`. It also keeps pointers it needs after the call
  in `x19`-`x28`, so a lost callee-saved register would crash the next `println!` rather than print
  a wrong number.
- **`Hello, ZheOS!` after `pong 9`** means the adopted boot task came back through the same path.
- **`exit` working** means no lock guard outlived its switch.

What it does not prove: `d8`-`d15`. The float was spilled to the stack, so no value crossed a switch
in those registers. The frame saves them; this output does not exercise them.

---

## Optional reading

- AAPCS64, ARM IHI 0055, §6.1 - the register roles table from section 5.
- Linux `arch/x86/entry/entry_64.S`, `__switch_to_asm`, and `struct inactive_task_frame` in
  `arch/x86/include/asm/switch_to.h` - the same on-stack design as this kernel.
- Linux `arch/arm64/kernel/entry.S`, `cpu_switch_to` - the record design.
