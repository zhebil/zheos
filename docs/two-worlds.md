# TWO WORLDS - the boundary between kernel and program

## 1. What this is

Everything the machine has run so far has been the kernel, at exception level 1, able to touch
anything. This skill runs code at exception level 0, where it cannot, and gives it a way to ask the
kernel for the things it can no longer do itself.

The category is **hardware mechanism plus a designed interface**. The mechanism is fixed: two
instructions, `eret` to go down and `svc` to come back up, and three registers that carry the state
between them. The interface - what the calls are, what they take, what they return - is entirely
yours, and it is the first thing in this project that is a design rather than an implementation.

**EL** is **E**xception **L**evel. `EL0` is where programs run, `EL1` is the kernel, `EL2` a
hypervisor, `EL3` firmware. Your kernel has been at `EL1` since boot.

**SVC** is **S**uper**V**isor **C**all, the instruction a program executes to enter the kernel
deliberately.

## 2. Why LOCKDOWN comes first, and it is not negotiable

Dropping to exception level 0 changes what the processor *reports* about itself. On its own it
changes nothing about what memory can be touched.

Look at the access permission table in LOCKDOWN section 3. `AccessPermissions::KernelReadWrite` is
`0b00`, which grants exception level 0 **no access at all** - so with the current descriptors, a
program at exception level 0 would fault on its own first instruction, and the fix that makes it
run is `0b01`, which grants read and write to *both* levels. Applied to a whole 2 mebibyte block,
as memory is mapped today, that means the program can read and write the kernel.

So without LOCKDOWN there are two outcomes and neither is a privilege boundary: nothing runs, or
everything is permitted. The bits that make it real - per-region permissions at page granularity,
`PXN` and `UXN` set correctly - are LOCKDOWN's whole content.

## 3. Going down

`eret` returns from an exception. There has not been an exception, and that does not matter: you
set up the machine to look as though there has been one, and return from it.

Three registers carry everything:

- **`ELR_EL1`**, **E**xception **L**ink **R**egister - the address to start executing at. The
  program's entry point.
- **`SPSR_EL1`**, **S**aved **P**rogram **S**tatus **R**egister - the processor state to restore.
  This is where the exception level lives. Writing `0b0000` into its low four bits means "return to
  exception level 0, using `SP_EL0`". The `DAIF` bits are in here too, and clearing them is what
  leaves interrupts enabled in the program.
- **`SP_EL0`** - the program's stack pointer, a separate register from the kernel's `SP_EL1`. The
  processor keeps both, which is what stops a program from being able to corrupt the kernel's stack
  by moving its own.

That last point is worth sitting with. The exception levels have separate stack pointers *in
hardware*, and that is why the kernel can take an exception from a program with a wildly broken
stack pointer and still have somewhere to run.

## 4. Coming back up

`svc #0` raises a synchronous exception, and the processor lands in your vector table - which
`src/exception.rs` already installs and which already decodes `ESR_EL1`.

What is new is the entry it will now take. The vector table has four groups of four, and which
group depends on where the exception came from. Everything so far has come from `EL1` with
`SP_EL1`, which is the second group. An `svc` from a program arrives in the **fourth** group,
"lower exception level, aarch64". If that slot is unpopulated or points at the same handler,
system calls will look like faults.

The syndrome register distinguishes them. `ESR_EL1`'s exception class field holds `0b010101` for an
`svc` from aarch64, and the low 16 bits carry the immediate from the instruction. So `svc #0`
against `svc #1` is readable without the program having to put a number in a register - though
putting it in a register is what everyone does anyway, because the immediate has to be a constant
and a register does not.

## 5. Designing the calls

This is the part that is yours, and the part worth spending time on rather than copying.

**The convention.** Which register holds the call number, which hold the arguments, which holds the
result. Linux on aarch64 uses `x8` for the number, `x0` to `x5` for arguments, `x0` for the return.
Following it costs nothing and means anything you read about Linux applies directly.

**Errors.** A system call cannot panic into the kernel and it cannot be trusted to have succeeded.
Linux returns negative values as errors, which works because addresses and counts are never
negative, and is the reason error numbers exist. Rust would prefer a `Result`, and a `Result` does
not survive a register boundary. Decide how errors cross, once, before writing three calls that
each do it differently.

**Validating arguments.** This is the thing that makes system calls hard, and it deserves its own
paragraph.

A program passes a pointer. The kernel must not dereference it. Not "should check first" - the
pointer might be a kernel address, might be unmapped, might be mapped but not to that program, and
the program may be lying deliberately. Every pointer coming from exception level 0 has to be
checked against what that program is allowed to touch before a single byte is read through it. A
kernel that trusts a user pointer has no boundary at all, however correct its page tables are.

With a single identity-mapped address space, "what that program is allowed to touch" is a range
check against the region you gave it. That is genuinely enough for now, and it is worth writing as
a single function that every call goes through, because the day address spaces become separate is
the day you want exactly one place to change.

**The first three calls**, which are enough to prove everything: write a byte to the console, read
a byte from it, and exit. Those three exercise output, blocking input, and task teardown - and the
second one is the interesting one, because it blocks, which means it goes through SCHED.

## 6. What you are building

- Vector table entries for the fourth group, and an `svc` decoder alongside the fault decoder in
  `src/exception.rs`.
- A drop-to-user function: set `SPSR_EL1`, `ELR_EL1`, `SP_EL0`, then `eret`. Assembly, and short.
- A user region: memory mapped with `AccessPermissions::AllReadWrite` and `pxn: true`, and a
  separate stack. Allocated from the heap, which by now can free it when the program exits.
- A system call table, an argument validator, and the three calls from section 5.
- A test program. It has no standard library, no allocator, and no way to do anything except
  through your calls - which is what makes it the honest test of the interface.

## 7. Testing it

Host tests cover the argument validator, which is where the security-relevant bugs are and which is
pure arithmetic: a range that starts inside and ends outside is rejected, a range that wraps around
is rejected, a zero-length range is decided one way deliberately, a range exactly filling the
region is accepted.

Write those before the calls. The validator is the one piece of this skill where being wrong is
worse than being incomplete.

Everything else is on the machine.

## 8. When nothing happens

| symptom | almost certainly |
| --- | --- |
| the program faults on its first instruction | permissions. The user region needs `AllReadWrite` for data and `uxn: false` for the code, and those are different regions. |
| `eret` returns to exception level 1 instead of 0 | `SPSR_EL1`'s low four bits. `0b0000` is exception level 0 with `SP_EL0`; `0b0100` is exception level 1 and is the value that looks like it worked. |
| the program runs and `svc` looks like a fault | the fourth vector group. Section 4. |
| the program runs one instruction and stops | interrupts left masked in `SPSR_EL1`, so the timer never preempts it, or the opposite - unmasked with the vector for the wrong group. |
| the kernel faults inside a system call | a user pointer dereferenced without validation. Section 5, and this is the bug the whole skill exists to prevent. |
| the program can write to kernel memory | the region is mapped `AllReadWrite` at 2 mebibyte granularity and covers more than you intended. LOCKDOWN's page granularity, applied to the user region too. |
| a system call works from one task and faults from another | the validator is checking against a global region rather than the calling task's. |
| `SP_EL0` looks correct and the program's stack is corrupt | the kernel is running on `SP_EL0` somewhere, usually because `SPSR_EL1` selected `SP_EL0` for exception level 1 by mistake. |

## 9. How you will know it worked

A program, running at exception level 0, that prints a line, reads a keystroke, echoes it, and
exits cleanly back into the kernel.

Then three deliberate crimes, each of which must be a fault reported by your handler rather than a
success:

- the program writes to a kernel address
- the program executes an instruction from its own stack
- the program passes a kernel address to the write call

The third is the one that matters most, because the first two are the hardware refusing and the
third is **your code** refusing. A kernel that passes the first two and fails the third has page
tables that work and a boundary that does not.

Then confirm the machine survives all three: the program dies, the kernel reports, and the monitor
prompt comes back.

---

## Optional reading

- ARM Architecture Reference Manual, section D1 on exception levels, and the `SPSR_EL1` field
  description for the mode bits in section 3.
- `arch/arm64/kernel/entry.S` in Linux, `el0_svc`, and `arch/arm64/kernel/syscall.c`.
- `include/linux/uaccess.h` and `arch/arm64/include/asm/uaccess.h` for `access_ok` and
  `copy_from_user`, which are section 5's validator in production form and are worth reading for
  how much care they take.
