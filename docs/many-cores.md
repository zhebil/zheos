# MANY CORES - waking the other processors

## 1. What this is

QEMU's `virt` machine has been running one core because you asked for one. This skill starts the
others, gives each its own stack, and gets each into the same state core 0 reached at the end of
`kmain`: translation on, interrupts routed, ready to be given work.

The category is **firmware convention plus per-core hardware state**. There is no "start core"
instruction. You ask the firmware, through the same PSCI interface `src/psci.rs` already uses to
power the machine off, and it releases the core at an address you name. What follows is hardware:
a list of registers that are per-core rather than shared, each of which the new core has in its
reset state and needs set.

**PSCI** is **P**ower **S**tate **C**oordination **I**nterface, an ARM specification for asking
whatever is below you - firmware, a hypervisor, or QEMU pretending to be both - to change a core's
power state.

## 2. The machine only has one core right now

Literally. `virt.dts` in this repo has exactly one `cpu@0` node, because it was dumped from a QEMU
invocation with the default `-smp 1`. Before anything in this skill can work:

```sh
qemu-system-aarch64 -M virt,dumpdtb=virt.dtb -cpu cortex-a72 -smp 4 -nographic
dtc -I dtb -O dts virt.dtb -o virt.dts
```

and `-smp 4` has to join the QEMU flags in the `Makefile`. The device tree will then carry four
`cpu@N` nodes, and the `reg` property of each is the value to pass to PSCI. Read them rather than
assuming they count 0, 1, 2, 3, because on a machine with clusters they do not.

Getting this wrong is the first failure of the skill and it looks like PSCI refusing every request.

## 3. What a freshly woken core looks like

It is not a copy of core 0. It is a core at reset, and everything core 0 configured during boot is
unconfigured on it:

- **Translation off.** `SCTLR_EL1.M` clear, so it runs with physical addresses and every access is
  Device memory. The same constraint the guides have been warning about since BUMP.
- **No stack.** `SP` is whatever reset left. This is why the entry point has to be assembly: there
  is no stack, so there is nothing to call a Rust function with.
- **Interrupts masked**, and its own interrupt controller interface disabled.
- **`x0` holds the context identifier** you passed to `CPU_ON`, and nothing else is promised.

What it does share with core 0: memory, including `.bss`, `.data`, every static, and the
translation table. That is the whole point, and it is also the whole hazard.

**The secondary must not zero `.bss`.** `src/kernel.s` does that on the way to `kmain`, and a
secondary running the same path would wipe every static core 0 has set up, including the allocator.
The secondary entry point is a different symbol, not a reuse of `_start`.

## 4. Getting a core started

`CPU_ON` is PSCI function 3. Two encodings exist and the choice is not cosmetic:

- `0x8400_0003` is the 32-bit calling convention.
- `0xC400_0003` is the 64-bit one, and **it is the one you need**, because the entry point you pass
  is a 64-bit address. Bit 30 of the function identifier is what distinguishes them.

Note that `SYSTEM_OFF` in `src/psci.rs:7` is `0x8400_0008`, in the 32-bit range, and is correct
there because it takes no arguments. Copying its shape without changing the range is the mistake
this paragraph exists to prevent.

Three arguments: `x1` is the target core's identifier from the device tree, `x2` is the physical
address of your entry point, `x3` is a context identifier that arrives in the new core's `x0`. Pass
the core's index in `x3` and the secondary knows who it is without decoding anything.

The return value in `x0` is 0 for success, and negative for everything else. The ones you will
actually see: `-2` invalid parameters, meaning the target identifier is wrong; `-4` already on;
`-9` invalid address, meaning the entry point is not somewhere the firmware will jump to. Decode
them rather than printing the raw number - a `-4` from a retry loop reads very differently from a
`-2`.

## 5. Which registers are per-core

This is the checklist the secondary's startup path has to work through, and getting it from a list
rather than from debugging is most of the value of this guide.

**Per-core, must be set on every core:**

- `MAIR_EL1`, `TCR_EL1`, `TTBR0_EL1`, `SCTLR_EL1` - all four. The *table* is shared, its address is
  not. `src/mmu/init.rs` already does exactly this sequence and should be callable as-is.
- `VBAR_EL1`, the vector table base. Same table, per-core register.
- `SP`, obviously, and it must be a different stack per core.
- `DAIF`, the interrupt masks.
- The generic timer comparison and control registers. Each core has its own timer.
- The interrupt controller's **CPU interface**: `GICC_CTLR`, `GICC_PMR`. Banked per core at the
  same address, so the code is identical and has to run four times.

**Shared, set once by core 0:**

- The translation table itself.
- The interrupt controller's **distributor**: `GICD_CTLR` and the routing of shared interrupts.
- Every `static` in the kernel.

**The subtle one:** interrupt identifiers below 32 are per-core. Identifiers 0 to 15 are software
generated, 16 to 31 are private peripheral interrupts, and the generic timer is one of the latter.
So the timer interrupt is enabled per core, in the distributor's banked registers, and enabling it
once on core 0 leaves the other three deaf to their own timers. This catches everyone once.

## 6. The one-word bug already in the tree

`src/cpu/mmu.rs:140` is:

```
tlbi vmalle1
```

That invalidates **this core's** translation lookaside buffer. With four cores, a mapping changed
on core 0 stays live in the other three until something else evicts it, which is a bug that
appears as one core seeing stale memory and is close to undebuggable from the symptom.

The fix is one word:

```
tlbi vmalle1is
```

The `is` suffix means inner shareable, and it broadcasts the invalidation to every core in the
domain. Worth noticing that the barriers on either side of it, at lines 136 and 144, are already
`dsb ishst` and `dsb ish` - the inner-shareable forms. The code is currently inconsistent with
itself, and this skill is where it stops being.

## 7. Everything shared is now genuinely shared

LOCK exists for this moment. Every `unsafe impl Sync` in the tree was written with a single core in
mind, and this is the skill where those arguments have to be true rather than plausible.

If LOCK was done first as planned, this is mostly already handled: `src/irq.rs` and `src/input.rs`
have real locks, and the allocator was born with one. What remains is auditing rather than
converting - walk every `static` in the tree and, for each, say which of three categories it is in:
never written after boot, atomic, or behind a lock. There is no fourth category.

The one new hazard the lock does not cover: **anything core 0 sets up before the secondaries start
must be finished before they start.** A secondary that reaches the allocator while core 0 is still
initialising it will find a half-built structure and a lock that is not held. `CPU_ON` is the
barrier, so the ordering is a matter of where you call it, not of synchronisation.

## 8. What you are building

- A secondary entry point in `src/kernel.s`, separate from `_start`. It reads its index from `x0`,
  loads its own stack from a table, and calls into Rust. It does not touch `.bss`.
- An array of stacks in `.bss`, one per core, with the count fixed at compile time.
- `psci::cpu_on(conduit, target, entry, context)`, alongside `system_off`, with the return value
  decoded into an error type rather than a number.
- A secondary Rust entry that runs the per-core list from section 5 and then parks in `wfi`.
- Core identity: `MPIDR_EL1` gives the hardware identifier, and a small function turning it into a
  0-based index is worth having early, because everything per-core wants an array index rather than
  an affinity value.
- A count of cores that have reported in, so core 0 can wait for them.

What you are **not** building is anything for them to do. They park. That is the correct end state
for this skill, and SCHED is what makes them useful.

## 9. When nothing happens

| symptom | almost certainly |
| --- | --- |
| every `CPU_ON` returns `-2` | the target identifier. Read it from the device tree's `cpu@N` `reg` property, not from a counter. Section 2. |
| `CPU_ON` returns 0 and nothing prints | the secondary faulted before it could print. It has no vector table yet, so `VBAR_EL1` is 0 and the fault loops in flash. `make regs` and look for `PC=0x200` on any core. |
| core 0's statics are all zero after the secondaries start | a secondary ran `_start` and zeroed `.bss`. Section 3. |
| a secondary faults on its first Rust call | no stack, or its stack pointer is not 16-byte aligned. The architecture requires 16 on `SP`. |
| the secondaries run and the timer never fires on them | the timer interrupt is a private peripheral interrupt and is enabled per core. Section 5. |
| one core sees memory that another core changed, sometimes | `tlbi vmalle1` instead of `vmalle1is`, or a missing barrier. Section 6. |
| the machine locks up as soon as two cores are live | a lock taken without masking, or a `static` that is in none of the three categories in section 7. |
| output is interleaved character by character | the serial port is shared and nothing is serialising it. Correct behaviour, and the fix is a lock around the writer, not around the formatting. |
| everything works with `-smp 2` and hangs with `-smp 4` | a fixed-size per-core array sized for two, or a spin that assumes it is the only waiter. |

## 10. How you will know it worked

Four lines at boot, one per core, each naming its index and its `MPIDR_EL1`, printed without
interleaving. Then core 0 continues past its wait and the monitor prompt appears as it always did.

Have each core increment a shared counter under the lock as it reports in, and have core 0 wait
until the counter reaches the expected number before continuing. That turns a missing core from a
silent hang into a hang that names how many are missing.

Two further checks that are worth the five minutes:

- Have each secondary read `SCTLR_EL1` and print it. Bit 0 set on all four is proof that
  translation is on everywhere, and not just where you were watching.
- Have each secondary enable its own timer and print a tick count after a second. Four
  independently ticking cores means the private peripheral interrupts are routed per core, which
  is the item from section 5 that is easiest to get subtly wrong.

`info registers` in the QEMU monitor will show all four cores, which is a cross-check from outside
the kernel entirely.

---

## Optional reading

- ARM Power State Coordination Interface specification, DEN 0022. `CPU_ON` is section 5.1.4, and
  the return code table is worth having open.
- ARM Architecture Reference Manual, `MPIDR_EL1` in the register index, for the affinity fields.
- `arch/arm64/kernel/smp.c` and `arch/arm64/kernel/head.S` in Linux, `secondary_entry`, which is
  section 8 in production form.
- GICv2 Architecture Specification, on which registers are banked per core.
