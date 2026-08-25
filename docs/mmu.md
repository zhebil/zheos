# MMU ON - making the CPU actually use the table

## 1. What this is for

The table is built and nothing reads it. `translate()` walks it in software, which proves the
shape is right, but the hardware has never looked at it. This skill hands it over.

Three registers describe the setup, one bit turns it on, and after that every address the CPU
touches - every instruction fetch, every load, every store, every stack push - goes through the
table first.

It is the shortest skill in tier 4 and the one with the least room for error. There is no partial
success: either the instruction after the switch is fetched correctly, or the machine stops in a
way that looks exactly like a hang.

## 2. What changes at the moment you flip the bit

Before: an address is a physical address. `0x0900_0000` is the UART because that is where the UART
is wired.

After: an address is a *virtual* address, and the MMU rewrites it using the table before it reaches
the bus. `0x0900_0000` is still the UART, but only because slot 0 of the level 1 table says so.

Because the map is an identity map, nothing observable changes. That is the entire reason this is
survivable. The program counter is the same number on both sides of the switch, so the very next
instruction is fetched from the same physical address it would have been fetched from anyway.

If the map were not an identity map, the instruction after `msr sctlr_el1` would be fetched from
whatever physical address the new table maps the old PC to - almost certainly not your code - and
you would need a jump to the new virtual address as part of the switch. Kernels that run at a high
address do exactly that. You are not doing it, and building the identity map first is what buys
that.

Two side effects worth naming:

**Unaligned access starts working.** With the MMU off, every address is Device memory, and Device
memory forbids unaligned access. That is the `ESR 0x9600_0021` alignment fault in the debugging
notes. Once RAM is Normal memory, an unaligned load is just a load. This is the acceptance
criterion for the skill, and it is the cleanest possible proof that the table is really in use -
the same instruction that faulted before now runs.

**Caches become possible.** Not automatic: they are separate bits. Device memory is never cached
regardless, which is what keeps the UART correct.

## 3. The three registers, and the order

```
MAIR_EL1    what the eight memory types actually mean
TCR_EL1     the shape of the address space
TTBR0_EL1   where the table is
SCTLR_EL1   the switch
```

The first three have to be written before the switch, and it does not otherwise matter which order
they go in - none of them does anything until the M bit is set. The switch goes last, on its own,
after a barrier sequence.

## 4. MAIR_EL1 - naming the two memory types

A descriptor does not say "this is Device memory". It carries `AttrIndx`, three bits, a *number
between 0 and 7*. `MAIR_EL1` is the table that number indexes into: eight slots, eight bits each,
slot N in bits `8N+7 : 8N`.

`descriptor.rs` already committed to a numbering:

```rust
Self::Normal => 0,
Self::Device => 1,
```

So slot 0 has to describe Normal memory and slot 1 has to describe Device memory. Get this
backwards and the kernel runs out of uncached memory while the UART gets cached and reordered - it
usually still prints in QEMU, which is exactly why it is worth checking rather than assuming.

**Device-nGnRnE = `0x00`.** The three letters are three separate promises the hardware gives up:

- **non-Gathering** - two byte writes to the same register are not merged into one.
- **non-Reordering** - accesses reach the device in program order.
- **no Early write acknowledgement** - a store is not "done" until the device has it.

That is the strictest kind, and it is what a UART register needs. Writing a character and then
reading the flags register has to be two real bus transactions in that order.

**Normal, Write-Back, Read+Write allocate = `0xFF`.** The top nibble is the outer cache attribute,
the bottom nibble the inner. `0b1111` in both means write-back, non-transient, allocate on read and
on write. Normal memory can be cached, reordered, gathered and speculatively read, all of which are
fine for RAM and fatal for a device.

Slot 0 = `0xFF`, slot 1 = `0x00`, everything else zero:

```
MAIR_EL1 = 0x0000_0000_0000_00FF
```

## 5. TCR_EL1 - the shape of the address space

This is the register with the most fields and the most ways to be quietly wrong.

| Field | Bits | Value | Why |
|---|---|---|---|
| `T0SZ` | 5:0 | 25 | 64 - 25 = 39-bit virtual addresses, which is what starts the walk at level 1 |
| `EPD0` | 7 | 0 | walks on TTBR0 are enabled - this is the one you want |
| `IRGN0` | 9:8 | `0b01` | the *tables themselves* are inner write-back cacheable |
| `ORGN0` | 11:10 | `0b01` | same, outer |
| `SH0` | 13:12 | `0b11` | tables are inner shareable |
| `TG0` | 15:14 | `0b00` | 4 KiB granule |
| `EPD1` | 23 | 1 | walks on TTBR1 are **disabled** |
| `TG1` | 31:30 | `0b10` | 4 KiB granule, and note the encoding differs from TG0 |
| `IPS` | 34:32 | `0b010` | physical addresses are 40 bits |

```
TCR_EL1 = 0x0000_0002_8080_3519
```

Four of these deserve more than a table row.

**`T0SZ = 25` is the same 25 that `level.rs` mentions.** It says the input address is 39 bits.
Each level of a 4 KiB-granule table resolves 9 bits, and the last 12 are the offset within a page:
level 3 resolves bits 20:12, level 2 bits 29:21, level 1 bits 38:30, level 0 bits 47:39. A 39-bit
address has its top bit at 38, which is exactly the top of level 1's range - so the walk starts at
level 1 and level 0 never exists. Set `T0SZ` to 24 instead and the top bit becomes 39, the walk
starts at level 0, and your level 1 table is now being read as a level 0 table. Everything in it
means something different.

**`EPD1 = 1` matters more than it looks.** The 64-bit address space is split in two: TTBR0 handles
addresses with the top bits clear, TTBR1 handles addresses with the top bits set. You have no
high-half table. `TTBR1_EL1` at reset holds whatever it holds. If a stray pointer with the high bit
set is ever dereferenced and `EPD1` is 0, the CPU walks a table that does not exist. With `EPD1`
set, the same access is a clean translation fault that your decoder prints.

**`TG1` uses a different encoding from `TG0`.** 4 KiB is `0b00` for TG0 and `0b10` for TG1. This is
not a typo in the manual. Leaving TG1 at its reset value can leave a reserved encoding in it, which
is architecturally UNPREDICTABLE even though TTBR1 walks are disabled, so set it anyway.

**`IPS` is a ceiling, not a request.** It says how many bits of *physical* address the translation
regime may produce. It must not exceed what the CPU actually supports, which lives in
`ID_AA64MMFR0_EL1.PARange`. Cortex-A72 reports 44 bits, so 40 is comfortably inside. 40 bits also
covers `virt`'s PCIe window at `0x4010_0000_00`, which a 32-bit `IPS` would not.

## 6. TTBR0_EL1 - where the table is

The physical address of the level 1 table, which is `table.base()`. It is page-aligned already, and
`Table::new` guarantees that through `#[repr(align(4096))]` on `Page`, so there is nothing to mask.

The top 16 bits are the ASID, which tags TLB entries so a context switch does not have to flush
everything. There is one address space and no context switching, so leave it zero.

## 7. The barrier dance

This is the part that is pure ceremony until the day it is not.

```
dsb ishst        every descriptor store is visible to the table walker
tlbi vmalle1     throw away every EL1 TLB entry
dsb ish          the invalidate has finished everywhere
                 ... write MAIR_EL1, TCR_EL1, TTBR0_EL1 ...
isb              the new configuration is in effect
                 ... set the M bit in SCTLR_EL1 ...
isb              the next instruction is fetched through translation
```

What each one is for:

**`dsb ishst`** - a Data Synchronization Barrier, store variant, inner-shareable domain. The
descriptors were written with ordinary stores. The table walker is a separate piece of hardware
that reads memory on its own. This is the instruction that guarantees it will see what you wrote.
`write_volatile` stops the *compiler* reordering the stores; it does nothing about the hardware.

**`tlbi vmalle1`** - invalidate all TLB entries for EL1. The TLB is the cache of recent
translations. It should be empty right now, but "should be" is doing a lot of work: firmware ran
before you, and a stale entry here is a bug that reproduces once every fifty boots.

**`isb`** - an Instruction Synchronization Barrier. The CPU is allowed to have already fetched and
partly executed the instructions after the one you are running. After changing a system register
that affects how instructions are fetched, that pipeline has to be thrown away and refetched.
Without the final `isb`, the instruction after the M bit may have already been fetched under the
old rules, and whether that matters depends on how far ahead the CPU happened to be - which is not
a thing you want to depend on.

## 8. SCTLR_EL1 - the switch

**Read, modify, write.** Never write a whole value. `SCTLR_EL1` has RES1 bits - bits the
architecture requires to be 1 and does not tell you about in the field list. Writing a constant
clears them.

| Bit | Name | What to do |
|---|---|---|
| 0 | `M` | set - this is the MMU |
| 1 | `A` | leave clear - setting it re-enables alignment faults, which is the opposite of the goal |
| 2 | `C` | set - data cache |
| 3 | `SA` | leave as found - stack alignment check |
| 12 | `I` | set - instruction cache |
| 19 | `WXN` | leave clear - it would make every writable page non-executable, and the RAM block is `rwx` |

Turning `C` and `I` on in the same instruction as `M` is safe here, and it is safe for a specific
reason worth understanding rather than copying: the caches have been *off* the whole time, so every
write so far went straight to memory and there are no dirty lines to lose. A cold cache reads from
memory. The dangerous version of this is the other direction - turning caches off with dirty lines
in them - which is a problem for the day you add a second CPU, not today.

## 9. Steps

1. **`cpu.rs`: read and write the four registers.** `mrs`/`msr` wrappers in the same shape as
   `generic_timer`. They are the only new unsafe code in this skill.

2. **A module for the enable.** `mmu::enable(&table)` or similar, taking the table so `TTBR0_EL1`
   cannot be pointed at the wrong thing. It is one function and it never returns a value - either
   it worked or you are not running.

3. **Write MAIR, TCR, TTBR0. Do not set M yet.** Boot. Nothing should change at all, because none
   of them do anything until the switch. If output changes here, something is wrong with a
   register write itself rather than with the table.

4. **Read the three registers back and print them.** Same rule as the UART: QEMU accepting a write
   is not evidence it stored what you meant. Compare against the values in sections 4 and 5.

5. **Add the barriers and set the M bit.** Boot. The kernel should keep printing. This is the whole
   skill.

6. **Prove it with an unaligned load.** The load that raised `ESR 0x9600_0021` before the switch
   should now complete. Doing it *after* the enable and printing the result is the acceptance
   criterion, and it is better evidence than "it still prints" because it is a behaviour that could
   only have changed if the table is genuinely in use.

## 10. When it goes wrong

**Silence, and the machine looks hung.** The usual failure. `make regs` will tell you where the PC
is. `PC = 0x200` means a fault was taken and the vectors were not reached - but you install vectors
early, so more likely you will see the fault report print normally. If the PC is somewhere in flash
or zero, the instruction fetch after the switch failed, which means the level 1 table does not map
your own code.

**`translation, level 1`** from your decoder - the address is not mapped at all. Print the table
before the switch and look for the slot.

**`access flag, level N`** - the descriptor is there but bit 10 is clear. `Descriptor::NORMAL_BLOCK`
and `DEVICE_BLOCK` both set `af: true`, so this would mean a descriptor was built some other way.

**`address size, level N`** - the address is outside what `T0SZ` allows, or the output address
exceeds `IPS`. Both are TCR problems, not table problems.

**Output turns to garbage, or stops after a few characters.** MAIR indices swapped. The UART is now
Normal cacheable memory, so writes are being gathered and reordered. Read `MAIR_EL1` back: slot 0
in bits 7:0 must be `0xFF`, slot 1 in bits 15:8 must be `0x00`.

**It works, and you do not know why.** QEMU does not model caches. The `C` and `I` bits have no
observable effect, so a cache configuration that would corrupt data on real silicon looks perfect
here. The MMU itself *is* modelled properly - unlike the UART enable bit - so translation failures
are real. Cache correctness is not testable on this machine at all, which is worth knowing before
trusting any of it on hardware.

## 11. Reference: the bits

```
MAIR_EL1  = 0x0000_0000_0000_00FF
              slot 1 (Device-nGnRnE) = 0x00, bits 15:8
              slot 0 (Normal WB RWA) = 0xFF, bits  7:0

TCR_EL1   = 0x0000_0002_8080_3519
              IPS   0b010  bits 34:32   40-bit physical addresses
              TG1   0b10   bits 31:30   4 KiB   (encoding differs from TG0)
              EPD1  1      bit  23      no TTBR1 walks
              TG0   0b00   bits 15:14   4 KiB
              SH0   0b11   bits 13:12   inner shareable
              ORGN0 0b01   bits 11:10   outer write-back
              IRGN0 0b01   bits  9:8    inner write-back
              EPD0  0      bit   7      TTBR0 walks enabled
              T0SZ  25     bits  5:0    39-bit virtual addresses -> start at level 1

TTBR0_EL1 = table.base()

SCTLR_EL1 |= (1 << 12) | (1 << 2) | (1 << 0)      I, C, M
              read-modify-write, never a whole value
```

Order:

```
dsb ishst
tlbi vmalle1
dsb ish
msr mair_el1  / tcr_el1 / ttbr0_el1
isb
msr sctlr_el1, <read value | M | C | I>
isb
```

## 12. Done when

- The kernel survives the enable and keeps printing.
- An unaligned load that raised an alignment fault before the switch completes after it.
- `MAIR_EL1`, `TCR_EL1` and `TTBR0_EL1` read back as the values you meant to write.
- You can say out loud why an identity map is what makes the instruction after the switch work, and
  what would have to happen instead if it were not one.

## Optional reading

- **ARM Architecture Reference Manual (ARMv8-A)**, chapter D8, "The AArch64 Virtual Memory System
  Architecture". The enable sequence and the register field definitions.
- **TCR_EL1** in the Arm A-profile Architecture Registers reference:
  <https://developer.arm.com/documentation/ddi0601/latest/AArch64-Registers/TCR-EL1--Translation-Control-Register--EL1->
- **`target/arm/ptw.c`** in the QEMU source - the table walker as actually implemented, including
  which fault status codes it produces for which mistake.
