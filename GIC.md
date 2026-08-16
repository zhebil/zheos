# GIC - the interrupt controller

Everything the kernel has done so far, it did because it asked. This is the skill that lets the
machine speak first.

**Interrupt** - a device saying "I have something for you", and the CPU dropping what it is doing
to go deal with it. **GIC** stands for Generic Interrupt Controller. It is the chip sitting between
every device and the CPU, deciding what gets through. Devices do not wire into the CPU directly.
They all wire into the GIC.

This task is one thing: turn that chip on and prove something reaches your handler. No device work.
No useful handler yet. Just make the wire live.

This document is what you need to know before writing `src/gic.rs`. It describes the hardware, not
the code.

---

## 1. Vocabulary

**Polling** - asking over and over. `Uart::getc` does this: read the flag register, is a byte there,
no, read it again. The CPU is fully occupied doing nothing.

**IRQ** - Interrupt ReQuest. The ordinary interrupt signal. There are two wires into an ARM core,
IRQ and **FIQ** (Fast Interrupt reQuest). FIQ is the higher-priority one, historically reserved for
one urgent device. You will only use IRQ.

**INTID** - Interrupt IDentifier. Every interrupt source in the machine has a number. The UART is 33
on this machine. That is the whole identity of an interrupt as far as the GIC is concerned.

**Distributor** - the global half of the GIC. Knows about every interrupt and every core. Decides
whether an interrupt is enabled at all and which core it goes to.

**CPU interface** - the per-core half. The doorway into *your* core. Filters by priority, tells you
which interrupt fired, and takes your acknowledgement.

**Masked** - switched off, but not forgotten. A masked interrupt stays pending and arrives the
moment you unmask it. This is different from disabled, which means the GIC will not track it at all.

**Pending** - has fired, has not been handled yet. **Active** - you have picked it up and are
handling it. An interrupt walks inactive → pending → active → inactive.

**EOI** - End Of Interrupt. Writing to a register to say "I am finished with this one". Until you do,
the GIC believes you are still busy and will not send you another of the same or lower priority.

**Level-sensitive** - the device holds a wire high for as long as it wants attention. It goes quiet
only when you fix the underlying cause. **Edge-triggered** - the device pulses the wire once and the
GIC remembers. Most devices here are level-sensitive, and that matters: if you handle a
level-sensitive interrupt without silencing the device, it fires again immediately, forever.

**Spurious** - the GIC's way of saying "actually, nothing". INTID 1023.

---

## 2. Where you are now

Three pieces already exist, and they nearly join up.

**`src/uart.rs` polls.** `getc` at line 133 spins on `try_getc` until a byte turns up. Works, costs a
whole core. `init` at line 90 already masks every UART interrupt (`IMSC = 0`) and clears anything
pending (`ICR = ALL_MASK`), so the UART is in a clean state and is not currently shouting at anyone.

**`src/exception.rs` installs the vector table.** `install_vectors` writes the table's address into
`VBAR_EL1` and issues an `isb`. The receiving end of an interrupt is built.

**`src/vectors.s` has a slot for IRQ, and it is wrong for the job.** Line 78:

```
                vector_slot unexpected_entry    // 0x280  IRQ
```

An IRQ taken today lands in `unexpected_entry`, which saves the registers, sets `x3 = 1`, and calls
`handle_exception` - a function whose return type is `!`. It prints a fault report and parks in
`loop { wfi }`. Correct behaviour for something that should never happen. Fatal for something that
happens sixty times a second.

So the shape of the work is: bring the GIC up, and give the IRQ slot a path that *returns*.

---

## 3. The five gates

Between the UART receiving a byte and your Rust code running, there are five switches. Right now
three of them are off.

1. **The device's own interrupt enable.** The UART's `IMSC` register. Off - your `init` sets it to 0.
2. **The distributor: is this INTID enabled?** Off - reset state.
3. **The distributor: which core does it go to?** For the UART, unset - reset state.
4. **The CPU interface: is it enabled, and does it pass the priority mask?** Off, and the mask blocks
   everything - reset state.
5. **The core: is `PSTATE.I` clear?** Off - the CPU boots with IRQ masked.

Five gates, and a shut gate at any one of them produces exactly the same symptom: nothing. No fault,
no message, no hint. This is the single most frustrating thing about interrupt bring-up, and it is
why the test at the end of this document matters more than usual.

This task is gates 2 through 5. Gate 1 belongs to UART IRQ and TIMER, which is why beads has both
blocked on this one.

---

## 4. The two halves of the chip

From `virt.dts`:

```
intc@8000000 {
	reg = <0x00 0x8000000 0x00 0x10000   0x00 0x8010000 0x00 0x10000>;
	compatible = "arm,cortex-a15-gic";
	#interrupt-cells = <0x03>;
};
```

Two address ranges, one device:

| Base | Size | Name | What it is |
|---|---|---|---|
| `0x0800_0000` | 64 KiB | GICD - distributor | Global. Shared by all cores. |
| `0x0801_0000` | 64 KiB | GICC - CPU interface | Per-core. Each core sees its own at the same address. |

`compatible = "arm,cortex-a15-gic"` is how you know this is **GICv2**. There is a GICv3, it is a
different design (system registers instead of memory-mapped, different register names), and its
documentation will not help you. Ignore anything that says `ICC_*`.

The per-core thing is worth pausing on. `0x0801_0000` is not one register block that all cores share.
Every core reading that address reads *its own* CPU interface. Right now you run one core, so it does
not matter, but it explains why the same interrupt has settings in two places.

Every register in both halves is 32 bits wide. Some fields inside them are one byte per interrupt,
and those bytes are individually addressable.

---

## 5. Interrupt numbers on this machine

INTIDs are split into three ranges by what kind of source they are:

| INTID | Kind | Meaning |
|---|---|---|
| 0-15 | **SGI** - Software Generated Interrupt | You cause it, by writing a register. Used for core-to-core messages. |
| 16-31 | **PPI** - Private Peripheral Interrupt | A device that exists once *per core*. The CPU's own timer. |
| 32-1019 | **SPI** - Shared Peripheral Interrupt | An ordinary device on the bus. Any core can be given it. |

The device tree does not print INTIDs. It prints `<kind number flags>`, and you convert:

- kind `0` = SPI → INTID = number **+ 32**
- kind `1` = PPI → INTID = number **+ 16**

So, from `virt.dts`:

| Device | dts `interrupts` | INTID |
|---|---|---|
| PL011 UART | `<0x00 0x01 0x04>` | SPI 1 → **33** |
| PL031 RTC | `<0x00 0x02 0x04>` | SPI 2 → 34 |
| PL061 GPIO | `<0x00 0x07 0x04>` | SPI 7 → 39 |
| Generic timer, EL1 physical | `<0x01 0x0e 0x104>` | PPI 14 → **30** |
| Generic timer, virtual | `<0x01 0x0b 0x104>` | PPI 11 → 27 |

The third number is the trigger type: `4` means level-sensitive, active high. `0x104` is the same `4`
plus a per-core mask in the upper bits, which is a PPI-only thing you can ignore.

The two numbers you will actually use are **33** for the UART and **30** for the timer. The `+32`
offset is the one place people quietly get this wrong: SPI 1 is not INTID 1.

---

## 6. The distributor's registers

Offsets from `0x0800_0000`. This is the full useful set for GICv2; you need four of them.

| Offset | Name | Width per interrupt | What it does |
|---|---|---|---|
| `0x000` | `GICD_CTLR` | - | Global on/off. **Bit 0 = enable.** |
| `0x004` | `GICD_TYPER` | - | Read-only. Bits [4:0] = *N*; the GIC supports 32×(N+1) INTIDs. Good sanity check. |
| `0x080` | `GICD_IGROUPR` | 1 bit | Group 0 or Group 1. Leave at reset. |
| `0x100` | `GICD_ISENABLER` | 1 bit | **Write 1 to enable.** Writing 0 does nothing. |
| `0x180` | `GICD_ICENABLER` | 1 bit | Write 1 to disable. Separate register, same reason. |
| `0x200` | `GICD_ISPENDR` | 1 bit | Write 1 to force pending. Read to see what is pending. |
| `0x280` | `GICD_ICPENDR` | 1 bit | Write 1 to clear pending. |
| `0x400` | `GICD_IPRIORITYR` | 1 **byte** | Priority. Lower number = more urgent. |
| `0x800` | `GICD_ITARGETSR` | 1 **byte** | Bitmask of which cores get it. Bit 0 = core 0. |
| `0xC00` | `GICD_ICFGR` | 2 bits | Upper bit: 1 = edge-triggered, 0 = level-sensitive. |
| `0xF00` | `GICD_SGIR` | - | Write-only. Fires a software interrupt. |

**Set/clear register pairs.** `ISENABLER` and `ICENABLER` look redundant. They are not. If enabling
were a read-modify-write on one register, two pieces of code doing it at once would lose an update.
Write-1-to-set and write-1-to-clear means each writer touches only its own bit. The same pattern
appears for pending and active. It is the standard fix for shared hardware registers, and you will
see it everywhere from here on.

**Finding the bit for INTID *n*.**

For 1-bit-per-interrupt registers, 32 interrupts fit in each 32-bit word:

```
address = base + 0x100 + 4 * (n / 32)
bit     = n % 32
```

For 1-byte-per-interrupt registers, 4 fit in each word, and the byte is directly addressable:

```
address = base + 0x400 + n
```

For 2-bit `ICFGR`, 16 fit per word:

```
address = base + 0xC00 + 4 * (n / 16)
bits    = [2*(n%16) + 1 : 2*(n%16)]
```

Worked out for the UART, INTID 33:

| Thing | Address | Which bits |
|---|---|---|
| Enable | `0x0800_0104` | bit 1 |
| Priority | `0x0800_0421` | the whole byte |
| Target core | `0x0800_0821` | bit 0 for core 0 |
| Trigger type | `0x0800_0C08` | bits [3:2] |

And for the timer, INTID 30 (a PPI):

| Thing | Address | Which bits |
|---|---|---|
| Enable | `0x0800_0100` | bit 30 |
| Priority | `0x0800_041E` | the whole byte |
| Target core | n/a | PPIs are private; `ITARGETSR` is read-only here and always means "this core" |

**The gotcha in this table is `ITARGETSR`.** For SPIs it resets to 0 - target no cores. Enable INTID
33, set its priority, forget the target byte, and the distributor accepts the interrupt and then
routes it nowhere. Everything looks configured and nothing ever arrives. The first 32 bytes of
`ITARGETSR` (the SGIs and PPIs) are read-only, which is a useful tell that this register only ever
concerns SPIs.

---

## 7. The CPU interface's registers

Offsets from `0x0801_0000`. Short list, all of it relevant.

| Offset | Name | What it does |
|---|---|---|
| `0x000` | `GICC_CTLR` | **Bit 0 = enable** this core's interface. |
| `0x004` | `GICC_PMR` | Priority Mask Register. The threshold. **Resets to 0.** |
| `0x008` | `GICC_BPR` | Binary Point. Splits priority into preemption group and subpriority. Ignore. |
| `0x00C` | `GICC_IAR` | Interrupt Acknowledge. **Reading it** returns the INTID in bits [9:0] and moves the interrupt to active. |
| `0x010` | `GICC_EOIR` | End Of Interrupt. Write back exactly what `IAR` gave you. |
| `0x014` | `GICC_RPR` | Running Priority. Diagnostic. |
| `0x018` | `GICC_HPPIR` | Highest Priority Pending. Lets you peek without acknowledging. Diagnostic. |

`GICC_IAR` is the unusual one: **reading it changes the hardware's state**. It is not a query. Read
it once per interrupt, keep the value, and use it for the `EOIR` write. Read it twice and you have
acknowledged something you are not going to handle.

---

## 8. Priority, and the mask that blocks everything

Priority is one byte per interrupt, and it runs backwards from intuition: **0 is the most urgent, 255
the least.** Think of it as a queue position, not a score.

`GICC_PMR` is a threshold. An interrupt reaches the core only if its priority is numerically **lower
than** the mask. Not lower-or-equal. Strictly lower.

`GICC_PMR` resets to `0`. Nothing is strictly below 0. **The reset state of the GIC blocks every
interrupt in the machine.** Write `0xFF` to it and everything passes.

This is the single most common reason a first GIC bring-up produces silence, and it is worth knowing
why the default is that way rather than filing it as an arbitrary magic number: reset is meant to be
a *safe* state. A controller that woke up passing every interrupt to a core that has not yet
installed a vector table would be a worse default than one that passes nothing.

One more wrinkle, which explains a confusing observation later. The GIC is only required to implement
the top bits of each priority byte - at least 4 of the 8, the rest implementation-defined. So writing
`0xFF` and reading back `0xF8` or `0xF0` is normal, not a failed write. Read yours back once and note
what it does; that tells you how many priority levels you actually have.

`GICD_IPRIORITYR` resets to 0 for every interrupt, which is the *most* urgent, so it passes any
non-zero mask. You can leave priorities alone for this task. Set them anyway once you have two
interrupt sources and care which one wins.

---

## 9. `DAIF` - the core's own mask

The last gate is inside the CPU, and the GIC knows nothing about it.

`PSTATE` has four mask bits, remembered by the letters **DAIF**:

| Letter | Masks |
|---|---|
| **D** | Debug exceptions |
| **A** | SError - asynchronous system errors |
| **I** | **IRQ** |
| **F** | FIQ |

Set means blocked. The CPU boots with all four set. You clear `I` with a special instruction:

```
msr daifclr, #2
```

The `#2` is a 4-bit mask over the letters, in the order F=1, I=2, A=4, D=8. So `#2` is exactly the
IRQ bit. `daifset` is the same instruction the other way. There is no ordinary write to these; the
architecture gives them their own instruction because setting and clearing one bit atomically is the
common case.

Two consequences worth having in your head now:

**On taking an exception, the hardware sets all four automatically.** Your handler runs with
interrupts masked, without you doing anything. That is why nesting does not happen by accident, and
why `ELR_EL1` and `SPSR_EL1` survive your handler even though `vectors.s` does not save them.

**`wfi` wakes on a pending interrupt even while `I` is set.** The mask stops the *exception*, not the
wake-up. Once the GIC is live, the current `loop { wfi }` in the panic handler and in
`handle_exception` will spin rather than sleep whenever anything is pending, which is harmless but
will look strange in a trace.

---

## 10. The life of one interrupt

Everything above, in the order it happens. Byte arrives at the UART:

1. **The UART** raises its interrupt line and holds it high. Level-sensitive: it stays high until the
   receive FIFO is drained.
2. **The distributor** checks: is INTID 33 enabled in `ISENABLER`? Is `GICD_CTLR` bit 0 set? What
   does `ITARGETSR[33]` say? It marks 33 **pending** and forwards it to core 0's CPU interface.
3. **The CPU interface** checks: is `GICC_CTLR` bit 0 set? Is priority 33 numerically below `PMR`?
   If yes, it raises the IRQ line into the core.
4. **The core** checks `PSTATE.I`. If clear, it takes the exception: saves the return address into
   `ELR_EL1`, saves `PSTATE` into `SPSR_EL1`, sets D, A, I and F, and jumps to
   `VBAR_EL1 + 0x280` - the IRQ slot of the "current EL, SP_EL1" group, line 78 of `vectors.s`.
5. **Your handler** reads `GICC_IAR`. It gets 33, and the interrupt moves from pending to **active**.
6. **Your handler does the work** - for the UART, reads bytes out of `DR` until the FIFO is empty.
   This is what makes the device drop its line. Skipping it is the classic interrupt storm.
7. **Your handler writes 33 to `GICC_EOIR`.** The interrupt goes back to inactive, and the GIC is
   willing to deliver again.
8. **`eret`.** The CPU restores `PSTATE` from `SPSR_EL1` - which un-masks `I` again, because it was
   clear when the interrupt hit - and jumps back to `ELR_EL1`. The interrupted code resumes without
   knowing anything happened.

Two ways to break step 5-7, both worth recognising on sight:

**Reading `IAR` when nothing is pending returns 1023**, the spurious ID. It means the interrupt went
away between the CPU interface raising the line and you acknowledging - real, and normal at low rates.
The rule is: if `IAR` gives you 1023, return immediately and **do not** write `EOIR`.

**Skipping `EOIR`** does not crash anything. It quietly stops all further interrupts of that priority
or lower, because the GIC still thinks you are busy. The symptom is "the first one works and then it
goes dead" - which reads like a hang, not a bug in the last line of a handler.

---

## 11. What your existing code cannot do yet

Three concrete gaps.

**`src/mem.rs` only does bytes.**

```rust
pub fn read_byte(address: u64) -> u8
pub fn write_byte(address: u64, byte: u8) -> ()
```

Every GIC register is 32 bits. `uart.rs` solved this privately with `read_register` /
`write_register` at lines 146-154, which are `u32` and take an offset from a stored base. You need
the same shape for the GIC, and you have two bases rather than one.

Since the MMU is off, all memory is Device memory, so these writes reach the GIC in program order
without explicit barriers. That stops being true after MMU ON, which is where `dsb` starts appearing
in other people's GIC code and looking mysterious.

**The IRQ vector slot leads to a function that never returns.** `handle_exception` is `-> !` and ends
in `loop { wfi }`. An IRQ needs a second path: save registers, call something that returns, restore
registers, `eret`. `unexpected_entry` is only half of it - the restore side does not exist yet,
because until now nothing ever needed to go back.

`eret` is the instruction that undoes an exception. It restores `PSTATE` from `SPSR_EL1` and jumps to
`ELR_EL1`, in one step. Ordinary `ret` will not do; it does not restore the processor state.

**One naming thing.** `src/gic.rs` currently has `GICD_CTRL`. The spec spells it `GICD_CTLR` -
ConTroL Register. Worth fixing now while there is one of them, because every table you look up will
use the spec spelling and mismatched names are miserable to grep.

---

## 12. Bring-up order

Smallest thing that can be tested, first. Do not wire the UART yet - a device that can only be
verified through a path you have not proven is the wrong first step.

1. **Read `GICD_TYPER`.** Bits [4:0] tell you how many interrupts this GIC supports. A plausible
   number proves you have the right base address and 32-bit access works. A read of all-ones or
   all-zeros means you are talking to nothing.
2. **Set `GICD_CTLR` bit 0.** Distributor on.
3. **Write `0xFF` to `GICC_PMR`.** Mask wide open. Read it back - `0xF8` is correct.
4. **Set `GICC_CTLR` bit 0.** CPU interface on.
5. **Point the IRQ slot at a handler that returns.** Give it its own entry in `vectors.s`, separate
   from `unexpected_entry`, that saves, calls, restores, and `eret`s.
6. **Have that handler read `IAR`, print the INTID, write `EOIR`.** Nothing more. One line of output
   is the entire goal.
7. **Clear `PSTATE.I`** with `msr daifclr, #2`, after everything above. Last, deliberately: unmask
   before the handler is ready and the first interrupt takes you somewhere unfinished.
8. **Fire an SGI at yourself** and see the line print.

Only then enable INTID 33, set its target and trigger type, and let the UART speak. At that point
gate 1 is one bit in `IMSC` and you are into UART IRQ territory.

---

## 13. Proving it works

You do not need a device to make an interrupt. `GICD_SGIR` at `0x0800_0F00` invents one.

| Bits | Field | Meaning |
|---|---|---|
| [25:24] | TargetListFilter | `0b00` = use the CPU list below. `0b01` = all cores except me. `0b10` = **me only**. |
| [23:16] | CPUTargetList | Bitmask of cores, used only when the filter is `0b00`. |
| [15] | NSATT | Security. Not relevant here; leave 0. |
| [3:0] | SGIINTID | Which SGI, 0-15. |

Writing `0x0200_0000` means: filter `0b10` (self), SGI number 0. The GIC marks INTID 0 pending on
your own core, and if the four switches are right, your handler runs and prints `0`.

SGIs need no `ITARGETSR` and no `ICFGR` - they are always edge-triggered and always deliverable. On
GICv2 their `ISENABLER` bits read as one and cannot be cleared, so there is nothing to enable either.
They do still respect priority, `GICD_CTLR`, `GICC_CTLR` and `PMR` - which is exactly why an SGI is a
good test of those four and nothing else.

**The observable:** `make run`, and a line appears in the serial output that nothing typed and no
device caused.

Two supporting checks that need no code changes:

```sh
make mem ADDR=0x08000000 N=1 FMT=xw    # GICD_CTLR - should read 1 once you enable it
make mem ADDR=0x08010004 N=1 FMT=xw    # GICC_PMR  - should read 0xF8 after writing 0xFF
```

`make mem` boots the kernel, waits a second, then dumps physical memory from the monitor, so it reads
the GIC's live state after your init has run. This is the same trick that caught the UART registers
QEMU pretends to accept, and it is the only way to tell "my write worked" from "QEMU ignored it".

And `make trace` logs every exception with its `ESR` - use it if the handler is running but printing
nothing.

---

## 14. When nothing happens

Because every failure looks identical, a checklist beats debugging.

| Symptom | Where to look |
|---|---|
| Absolute silence | Some gate is shut. Walk 1-5 in section 3 in order; do not guess. |
| `PMR` reads 0 | You never wrote it, or wrote to the distributor base by mistake. Reset value blocks everything. |
| SGI works, UART does not | `ITARGETSR[33]` is 0. SGIs do not use it, so this failure only shows up on your first SPI. |
| Handler runs once, then dead | Missing `EOIR` write. |
| Handler runs forever | Level-sensitive interrupt whose device was never silenced. For the UART that means you did not drain the FIFO. |
| `IAR` returns 1023 | Spurious. Return without writing `EOIR`. If it is *always* 1023, the distributor forwarded nothing - gate 2 or 3. |
| The fault report prints, `kind: unexpected slot` | The IRQ arrived but the slot still points at `unexpected_entry`. Actually good news: the whole chain works. |
| The report prints and it was the **FIQ** slot | Group/security configuration. On this machine, with `virt` built without security extensions, everything is Group 0 and signals as IRQ - so this should not happen, and if it does, `GICC_CTLR` has more bits set than you meant. |
| `PC=0x200` and a hang | Nothing to do with the GIC. A fault before `install_vectors`, per CLAUDE.md. |
| Garbage after the first interrupt | The restore side of the IRQ entry is wrong, or the stack is unbalanced. `save_all_registers` moves `sp` by 256; the return path must move it back by exactly that. |

---

## 15. Deliberately left out

Real GICv2 has more, and none of it is needed today. Named here only so that seeing it elsewhere does
not read as a gap in your setup:

- **Groups and security.** GICv2 sorts interrupts into Group 0 and Group 1 and, with the security
  extensions, into secure and non-secure. QEMU's `virt` is built without the security extensions
  unless you ask for them, so there is one view of the world, everything is Group 0, and `IGROUPR`
  can stay at reset.
- **Banking.** With multiple cores, `ISENABLER0`, `IPRIORITYR` for SGIs and PPIs, and `ICFGR0` are
  per-core copies at the same address. Invisible on one core.
- **Preemption and `GICC_BPR`.** Letting a more urgent interrupt cut in on a running handler. Needs
  nested handlers; not a Tier 3 concern.
- **`GICC_AIAR` / `GICC_AEOIR`** and the split EOI mode - alternate registers for the secure/Group 1
  case.
- **The v2m frame** at `0x0802_0000` in `virt.dts` - message-signalled interrupts for PCIe. Devices
  writing to an address instead of pulling a wire. Only matters if you ever do PCI.
- **GICv3.** Different architecture, system registers named `ICC_*`. Not this machine.

---

## 16. Done when

An interrupt you caused with a register write reaches a handler installed by `install_vectors`,
prints its INTID, and returns cleanly to the code it interrupted - which keeps running.

That is `zheos-jpf`'s acceptance criterion, and it unblocks both UART IRQ and TIMER, because from
that point on adding a device is two bits: one in the device, one in `ISENABLER`.

---

## Optional reading

- **ARM Generic Interrupt Controller Architecture Specification, version 2.0** - ARM IHI 0048.
  Chapter 4 is the register map, section 4.3 the distributor, 4.4 the CPU interface.
- **`hw/intc/arm_gic.c`** in the QEMU source. What is *actually* emulated, including which priority
  bits are implemented.
- **ARM Architecture Reference Manual (ARMv8-A)**, section D1 on exception entry and `DAIF`.
- `virt.dts` in this repo - the machine describing its own interrupt numbers.
