# LOCK - one thing at a time, proven rather than assumed

## 1. What this is

A lock is a promise that two pieces of code will not touch the same bytes at the same time. You
have been making that promise since REFLEX, but you have been making it in comments. This skill
turns it into a type the compiler enforces.

The category is mixed, which is worth knowing up front. The lock itself is **software** - a
struct, a loop, and a `Drop`. But it rests on two pieces of **hardware** that exist for exactly
this purpose: the exclusive monitor, which lets one core notice that another wrote to an address
between its read and its write, and the four mask bits in the processor state that stop your own
interrupt handler from cutting in. Neither is a library. Both are instructions.

Nothing in the machine changes. What changes is that `src/irq.rs:18` and `src/input.rs:25` stop
being arguments you have to re-read and start being code that cannot be used wrongly.

## 2. Why now, when there is only one core

Because of what a wrong safety argument costs later.

Read the comment above `unsafe impl Sync for InputBuffer` in `src/input.rs:20-24`. It is a good
comment. It names its assumption exactly, distinguishes itself from the neighbouring case in
`irq.rs`, and says which part is load-bearing. It is also, the instant a second core boots,
**silently false**, and nothing will tell you.

That comment does not get edited when MANY CORES lands. It gets **re-derived** - by you, months
from now, on code you no longer have in your head, one file at a time, with nothing that complains
if you get it wrong. That is the expensive part, and it is expensive in proportion to how much code
exists when you pay it.

An uncontended lock costs one atomic read-modify-write, a handful of nanoseconds, and it is
correct on one core and on sixteen. Writing it now means no such comment is ever created. Doing
it before FRAMES rather than after means the allocator, which is the largest piece of shared
mutable state you will ever write, is born correct.

FRAMES does not technically require this. It is first because it is cheap now and expensive later.

## 3. Two hazards, not one

This is the part that decides the design, so it comes before any mechanism.

**Hazard one: your own interrupt handler.** One core, one instruction stream. An interrupt can
land between any two instructions, including between the read of a field and the write back. The
handler runs on the same stack, in the same address space, and can call the same function.

**Hazard two: another core.** A genuinely simultaneous access. Not "interleaved" - simultaneous.

They need different mechanisms and neither one covers the other:

| | stops your handler | stops another core |
|---|---|---|
| masking interrupts | yes | no - the mask bits are per-core |
| a lock | **no, it deadlocks** | yes |

The lock cell of that table is the one worth staring at. On one core, a handler that finds the
lock held is spinning on a lock owned by the code it interrupted, which cannot run again until the
handler returns, which it never will. That is not slow. That is a stop.

So the primitive is both, in a fixed order:

```
mask interrupts
take the lock
  ... work ...
release the lock
restore the previous interrupt state
```

Linux calls this `spin_lock_irqsave`, and it is the ordinary way to lock in kernel code precisely
because kernel code always has both hazards.

**The order matters and the failure is not obvious.** Take the lock first and there is a window
where you hold it with interrupts still enabled. An interrupt landing in that window whose handler
wants the same lock is hazard one, and you have just built the deadlock the masking was there to
prevent. The window is two instructions wide, which means you will not hit it while you are looking
and you will hit it in a month.

**Restore, never unmask.** `cpu::without_interrupts` at `src/cpu.rs:24` already gets this right,
and its doc comment says why: it puts the processor state back exactly as it found it. If it
unmasked unconditionally, calling it from inside an interrupt handler would re-enable interrupts
partway through a handler that the hardware had deliberately masked on entry.

## 4. Reading the names

**DAIF** - the four mask bits in the processor state, one letter each: **D**ebug, **A**bort,
**I**RQ, **F**IQ. **IRQ** is Interrupt **R**e**q**uest, the ordinary kind. **FIQ** is
**F**ast Interrupt **R**e**q**uest, a second, higher-priority line that this machine does not use.
`msr daifset, #2` sets the I bit, masking interrupt requests; `daifclr` clears. Your `cpu.rs`
already does both.

**LDXR** / **STXR** - **L**oa**D** e**X**clusive **R**egister and **ST**ore e**X**clusive
**R**egister. The pair in section 5.

**LDAXR** / **STLXR** - the same two with an **A**cquire and a re**L**ease added. Section 6.

**STLR** - **ST**ore-re**L**ease **R**egister. A plain store that also carries release ordering.

**Exclusive monitor** - a small piece of hardware per core that remembers "this core is watching
this address" and forgets it if anybody writes there. Section 5.

**LSE**, **L**arge **S**ystem **E**xtensions - a set of single-instruction atomics added in
ARMv8.1. Your `cortex-a72` is ARMv8.0 and does not have them, which is why your atomics compile
into loops. Section 5 shows both.

**CAS** - **C**ompare **A**nd **S**wap. Read a value, and write a new one only if the old one is
what you expected. `AtomicBool::compare_exchange` in Rust.

**Guard** - a value whose only job is to undo something when it is dropped. The lock returns one.
Section 8.

## 5. What the hardware actually does

There is no "lock" instruction. There is a pair of instructions that lets you detect
interference and try again.

`ldxr` loads a value **and** records the address in this core's exclusive monitor. `stxr` stores a
value **only if** the monitor is still set, and reports which happened by writing 0 or 1 into a
register you name. Anything that writes to that address in between - another core, this core's own
interrupt handler, a context switch - clears the monitor, and the store fails without writing.

So an atomic read-modify-write is a retry loop, not an instruction:

```
1: ldxr  w10, [x8]        // read, and start watching
   ... decide ...
   stxr  w11, w9, [x8]    // try to write
   cbnz  w11, 1b          // w11 is 1 if somebody interfered, so go around again
```

This is real, not a paraphrase. Compiling `AtomicBool::compare_exchange(false, true, Acquire,
Relaxed)` for `cortex-a72` produces exactly that shape:

```
ldaxrb  w9, [x8]
cbnz    w9, .LBB3_4        // already true, give up
mov     w0, #1
stxrb   w9, w0, [x8]
cbnz    w9, .LBB3_1        // interfered, retry
```

The `b` suffix is byte, because `AtomicBool` is one byte. Compile the identical Rust for a
processor with LSE and the whole loop collapses to one instruction, `casab` - compare and swap,
acquire, byte. Same source, same semantics, different instruction count, chosen by the target.
Your machine gets the loop.

Two consequences worth carrying:

**The failure path needs `clrex`.** If you abandon a `ldxr` without a matching `stxr`, the monitor
stays set and can cause a later unrelated `stxr` to behave oddly. The compiler emits `clrex` for
you on the give-up path, which you can see in the assembly above the `.LBB3_4` label.

**The monitor watches a region, not a byte.** The architecture calls it the exclusive reservation
granule, and it may be as large as 2048 bytes. Two locks that land in the same granule will clear
each other's monitors and make both retry, even though they protect different things. This is why
Linux aligns hot locks to a cache line. Not worth doing now, worth knowing why it is done.

## 6. Memory ordering, which is not about the lock at all

This is the half of the skill that is genuinely hard, and it is hard because it is not about
mutual exclusion.

ARM is **weakly ordered**. The processor is allowed to complete loads and stores in a different
order than you wrote them, as long as a single-threaded program cannot tell. Another core can
tell. So a lock that is perfectly mutually exclusive is still useless if the reads of the
protected data are allowed to float *above* the instruction that took the lock.

Three orderings cover everything you need:

**Relaxed** - atomic, and nothing else. The value is not torn, and that is the whole promise.
Nothing around it is ordered. Correct for a statistic nobody makes decisions on. Your
`KERNEL_TICKS` in `src/timer.rs:27` is the honest use.

**Acquire** - on a load. Nothing written after it in program order may be moved before it. This is
what you want when *taking* a lock: everything in the critical section stays inside.

**Release** - on a store. Nothing written before it may be moved after it. This is what you want
when *releasing*: every write you made is visible to the next core that acquires, before it sees
the lock go free.

Acquire and Release are two halves of one fence, and together they make a box that leaks in
neither direction. That is the entire reason those two names exist.

Which is why the take is `compare_exchange(false, true, Acquire, Relaxed)`, and the two orderings
are different on purpose. The first applies if the swap **succeeded**, and you are now entering
the critical section, so you need Acquire. The second applies if it **failed**, and you are not
entering anything, so there is nothing to order and Relaxed is not a shortcut - it is the correct
answer.

The release is a plain `store(false, Release)`, and it compiles to a single instruction, `stlrb`.
No loop, because a release does not have to check anything - you hold the lock, so nobody else can
be writing it.

If you take one thing from this section: the atomic makes the lock **exclusive**, and the ordering
makes the lock **useful**. Get the first right and the second wrong and the code works on your
machine and breaks on hardware that reorders more aggressively than QEMU.

## 7. Spinning, and the cheap version of it

The naive loop hammers `ldaxr`/`stxr` forever, and every attempt is an exclusive access that
clears every other core's monitor on the same granule. Under contention that is worse than useless
- the cores spend their time cancelling each other.

The standard fix is **test, then test-and-set**: spin on a plain, relaxed load until the lock looks
free, and only then attempt the exclusive operation. The plain load takes no reservation and
disturbs nobody.

`core::hint::spin_loop()` is the marker for the inner loop. On aarch64 it emits `isb`, which is
cheap and gives the core a hint. There is a better answer for a real multi-core spin - the monitor
can be paired with `wfe`, so a waiting core sleeps until the monitor is cleared and a `sev` wakes
it - but that only pays off when contention is possible. Leave it for MANY CORES, and leave a note
saying so.

**One rule that is not an optimisation: never hold a lock across `wfi`.** Look at
`src/input.rs:41-56`. `getc` calls `wait_for_interrupt()` inside `without_interrupts`, which is
correct and subtle - `wfi` wakes on a pending interrupt even while it is masked, so this is the
classic way to sleep without losing a wakeup. If converting that to a lock puts the `wfi` inside
the critical section, the core sleeps holding the lock, and the handler that would wake it needs
the lock. Take the value out, drop the guard, then sleep.

## 8. Why the lock holds the data

A lock beside the data it protects is a convention. A lock **containing** the data is a rule.

```rust
lock.lock().push(byte);
```

If `SpinLock<T>` owns a `T` and the only way to reach it is a guard returned by `lock()`, then
there is no expression that touches the data without holding the lock. Not "we checked" - there
is no such expression to write. That is the whole reason to build it this way rather than as a
bare flag.

The guard is a value with a `Drop`, and it does three things in order when it goes out of scope:
release the lock, then restore the saved processor state, then vanish. It carries the saved DAIF
itself rather than reading a global, and that is what makes locks nestable: an inner guard restores
to "masked", because that is what it found, and the outer one restores to whatever the caller
originally had.

`Deref` and `DerefMut` on the guard are what let `push` be called on it directly. Note that
`DerefMut` gives `&mut T` from a `&mut Guard` - the borrow checker then stops the guard's `T` from
outliving the guard for free, with no unsafe involved.

## 9. What you are building

One module, `src/lock.rs`, plus edits to two existing files.

- `SpinLock<T>` holding an `AtomicBool` and an `UnsafeCell<T>`, with a `const fn new(value: T)` so
  a `static` can be built from it.
- `unsafe impl<T: Send> Sync for SpinLock<T>` - and the bound is the interesting part. `Send` and
  not `Sync`, because the lock hands the value to one core at a time rather than sharing it, which
  is exactly what `Send` means.
- `lock(&self) -> SpinLockGuard<'_, T>` doing mask, then acquire, in that order.
- `try_lock(&self) -> Option<SpinLockGuard<'_, T>>` - one attempt, no spin. Needed by anything
  that must not block.
- `SpinLockGuard<'a, T>` holding a reference to the lock and the saved processor state, with
  `Deref`, `DerefMut` and `Drop`.

Then convert the two existing cases. `src/input.rs` is the one to do first: it already uses
`without_interrupts`, its `RingBuffer` is a real shared structure with three fields that move
together, and its `Sync` comment currently argues from the core count. `src/irq.rs` is the easier
one and should be done second, because its honest answer may be that the handler table wants a
different tool - it is written once at boot and read forever after, which is a case a lock is
heavier than.

The hard part is not any of those signatures. It is deciding, for each of the two conversions,
whether a lock is actually the right answer, and writing down why. A lock on something that is
never written concurrently is cost with no benefit.

## 10. Detecting the deadlock instead of hanging

Taking the same lock twice on one core is a permanent stop, and from outside it looks exactly like
any other hang: no output, one QEMU process at 100 percent.

A bounded spin turns that into a report. Count the attempts, and past some large number, print
through the same path `halt()` in `src/main.rs:162` uses and stop. It costs a compare per
iteration on a path that is already the slow one, and it converts your worst failure mode from
"the machine is silent" into a line naming the lock.

Keeping the repo's no-panic rule: report and halt, do not panic.

## 11. When nothing happens

| symptom | almost certainly |
| --- | --- |
| the machine stops on the first `lock()`, no output | the lock is already held. On one core that means re-entry: something inside the critical section called something that locks the same lock. Section 10 turns this into a message. |
| stops the first time a key is pressed, but only sometimes | masking and locking are in the wrong order. The window is two instructions wide and an interrupt has to land inside it. Section 3. |
| interrupts stay off after a guard is dropped | the guard unmasked instead of restoring, or it restored a state captured at the wrong moment. Save before masking, not after. |
| nothing ever wakes up, `getc` never returns | a `wfi` inside a critical section. Section 7. |
| a shared counter ends short, rarely, and only in release builds | the orderings. `Relaxed` on the take, or `Relaxed` on the release. The exclusion is right and the fences are missing, and this is the failure mode that only shows up under real contention. |
| `stxr` never succeeds, loop spins forever | writing to the same granule from the loop body. Anything stored between the `ldxr` and the `stxr` can clear the monitor, including a `println!` for debugging. Print outside the loop. |
| the guard is dropped and the lock stays held | an early `return` or `?` in the critical section that moved the guard, or a `let _ = lock.lock()` which drops it immediately. `let _guard =`, never `let _ =`. |

## 12. How you will know it worked

The lock is invisible when it works, so test it by making it visible.

Convert `src/input.rs` first, then hold `make run` and type continuously for a few seconds while
the kernel prints. Every keystroke should come back, in order, with none dropped and none
duplicated. The ring buffer is written by the interrupt handler and read by `getc`, so a lock that
is wrong in either direction shows up as characters that are lost, doubled, or out of order.

Then prove the lock is actually doing something rather than being decorative. Add a temporary
counter of how many times `lock()` had to spin before it got in, and print it. On one core with
correct masking that number is **zero, always** - nothing can contend, because the only thing that
could is masked out. A non-zero spin count on a single core is a bug report: it means something
took the lock with interrupts on.

That zero is a stronger result than it looks. It says the masking is load-bearing and the lock is
currently free insurance, which is exactly the state you want to be in the day a second core boots
and the number stops being zero.

---

## Optional reading

- ARM Architecture Reference Manual for A-profile, section B2 on the memory model, and the
  description of the exclusive monitors. The reservation granule is defined there.
- `Documentation/memory-barriers.txt` in the Linux source. Long, and the best explanation of
  acquire and release semantics written for people who write kernels rather than compilers.
- `include/linux/spinlock.h` for the `spin_lock_irqsave` family, and why there are five variants.
- The Rustonomicon chapter on atomics for the Rust spelling of the same ideas.
