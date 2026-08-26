# Turning the MMU on

## 1. What this is

Right now every address your kernel uses is a real address. When you store a byte at
`0x0900_0000`, that number goes out on the bus and the UART sees it. There is no step in
between.

The **MMU** - **Memory Management Unit**, a block of hardware inside the CPU - is that step in
between. Once it is on, every address the CPU produces is looked up in a table first, and
whatever the table says is what goes out on the bus. You have already built that table: it is
the tree in `src/mmu/`.

This document is about the four registers that switch the lookup on. That is the whole job. The
table already exists and already works - `translate()` proves it - so nothing about the tree
changes here.

## 2. Why bother, given the table already returns the right answer

Three things you get, in order of how soon you will care:

**Unaligned loads start working.** With the MMU off, the CPU treats _all_ memory as Device
memory, and Device memory forbids reading 8 bytes from an address that is not a multiple of 8.
That is the alignment fault you hit in the DTB parser. Once RAM is marked Normal in the table,
the restriction goes away.

**Caches start working.** With the MMU off, the data cache is off too - every load and store
goes to actual RAM. That is roughly 100x slower than a cache hit.

**Bad addresses become faults.** An unmapped address stops being "whatever the bus does" and
starts being a clean exception with a report, which your vector table already prints.

## 3. These registers are not memory

This is the part that has no analogy in anything you have written so far.

The UART is **MMIO** - **Memory-Mapped Input/Output**. It lives at address `0x0900_0000` and you
talk to it with ordinary loads and stores. Everything in `src/uart.rs` works this way.

The MMU's registers are not like that. They have **no address at all**. You cannot store to them,
you cannot point at them, `make mem` cannot dump them. They are named directly inside the
instruction encoding, and there are exactly two instructions that touch them:

```
mrs x0, tcr_el1     // Move to Register from System register: x0 = TCR_EL1
msr tcr_el1, x0     // Move to System register from Register: TCR_EL1 = x0
```

That is it. Always via a general-purpose register, never a constant inline. In Rust:

```rust
unsafe { asm!("msr tcr_el1, {}", in(reg) value, options(nostack)) };
```

The consequence you will feel immediately: to change one bit of a system register, you `mrs` it
into a register, modify it, and `msr` it back. Writing a bare constant clobbers every bit you
did not think about. This matters for exactly one of the four registers here - `SCTLR_EL1`,
which the CPU boots with several bits already set.

## 4. Reading the names

Every one of these names is an acronym with a suffix. The suffix is always `_ELn`.

**EL** is **Exception Level** - the privilege ring. EL0 is user code, EL1 is the kernel, EL2 is a
hypervisor, EL3 is firmware. QEMU drops you at EL1, so every register here ends in `_EL1`. The
same register exists separately at EL2 and EL3 with a different suffix; they are unrelated
storage, not views of one thing.

The four registers:

| Name        | Letters                                               | What it holds                        |
| ----------- | ----------------------------------------------------- | ------------------------------------ |
| `MAIR_EL1`  | **M**emory **A**ttribute **I**ndirection **R**egister | Eight memory _types_, one byte each  |
| `TCR_EL1`   | **T**ranslation **C**ontrol **R**egister              | The shape of the address space       |
| `TTBR0_EL1` | **T**ranslation **T**able **B**ase **R**egister **0** | Where the root table is              |
| `SCTLR_EL1` | **S**ystem **C**on**t**ro**l** **R**egister           | The on switch, among 40 other things |

And the acronyms inside them, all in one place so nothing below is a surprise:

|                     |                                                                                                                               |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| **VA**              | Virtual Address - the number the CPU produces                                                                                 |
| **PA**              | Physical Address - the number that reaches the bus                                                                            |
| **TLB**             | Translation Lookaside Buffer - a small cache of recent VA-to-PA answers, so the CPU does not re-walk the tree on every access |
| **ASID**            | Address Space Identifier - see §7                                                                                             |
| **TG**              | Translation Granule - the page size                                                                                           |
| **SH**              | Shareability                                                                                                                  |
| **IRGN** / **ORGN** | Inner / Outer Region cacheability                                                                                             |
| **IPS**             | Intermediate Physical Address Size                                                                                            |
| **TBI**             | Top Byte Ignore                                                                                                               |
| **EPD**             | walk disable. ARM never expands the E and the P; treat `EPD1` as "turn off walks for TTBR1"                                   |
| **DSB**             | Data Synchronization Barrier                                                                                                  |
| **ISB**             | Instruction Synchronization Barrier                                                                                           |
| **TLBI**            | TLB Invalidate                                                                                                                |

## 5. MAIR_EL1 - what "kind" of memory each region is

A descriptor in your tree has an `AttrIndex` field, three bits wide. It does not describe memory;
it is an _index_. `MAIR_EL1` is the array it indexes into: 64 bits, split into eight one-byte
slots. `AttrIndex::Normal = 0` means slot 0, `AttrIndex::Device = 1` means slot 1. That is the
"Indirection" in the name.

You need exactly two slots.

**Slot 0 = `0xFF` = Normal, cached.** The byte splits into two nibbles: bits 7:4 describe the
outer cache, bits 3:0 the inner cache. `0b1111` in either means Write-Back, non-transient,
read-allocate and write-allocate. Plain fast RAM. This is what your `NORMAL_BLOCK` template
selects.

**Slot 1 = `0x00` = Device-nGnRnE.** When the top nibble is `0000`, the byte is a Device type, and
the low nibble picks which flavour. `0x00` is the strictest one. The letters:

- **G** - Gathering. Merging two adjacent byte writes into one wider write. `nG` forbids it.
- **R** - Reordering. `nR` forbids it.
- **E** - Early write acknowledgement. `nE` means a store is not "done" until the device says so.

All three forbidden is what you want for a UART, where writing two bytes as one halfword sends
the wrong thing and reordering the enable bit after the data bit breaks initialisation. The
looser flavours are `0x04` (nGnRE), `0x08` (nGRE), `0x0C` (GRE); they exist for framebuffers and
similar, where throughput matters and each write is not a command.

So:

```
MAIR_EL1 = 0x0000_0000_0000_00FF
             slot7..slot2 unused    slot1=0x00  slot0=0xFF
```

## 6. TCR_EL1 - the shape of the address space

This is the one that looks like a magic number. It is not; it is fourteen small fields packed
into one word. Here is the value split apart, with what each field means, what you set it to, and
what else it could have been.

```
TCR_EL1 = 0x0000_0002_8080_3519
```

| Bits  | Field                 | Yours   | Meaning, and the other options                                                                                                                                                                                                                                                                                         |
| ----- | --------------------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 5:0   | `T0SZ`                | `25`    | VA size = 64 - T0SZ = **39 bits**. Any value 16..39 is legal; 16 gives you a 48-bit space and forces a level-0 table on top, 25 is the largest space that still starts the walk at level 1.                                                                                                                            |
| 7     | `EPD0`                | `0`     | 0 = walk the TTBR0 tables. 1 = do not walk, fault immediately.                                                                                                                                                                                                                                                         |
| 9:8   | `IRGN0`               | `0b01`  | Cacheability of _the table memory itself_, inner. `00` non-cacheable, **`01` Write-Back read+write-allocate**, `10` Write-Through, `11` Write-Back read-allocate only. Non-cacheable would work and be slower, because the table walker re-reads RAM.                                                                  |
| 11:10 | `ORGN0`               | `0b01`  | Same four options, outer cache. Match `IRGN0`.                                                                                                                                                                                                                                                                         |
| 13:12 | `SH0`                 | `0b11`  | Shareability of the table memory. `00` non-shareable, `01` reserved, `10` outer shareable, **`11` inner shareable**. Must agree with the `SH` you put in the descriptors covering that memory, or the walker and your code disagree about the same bytes.                                                              |
| 15:14 | `TG0`                 | `0b00`  | Granule for TTBR0. **`00` = 4 KiB**, `01` = 64 KiB, `10` = 16 KiB.                                                                                                                                                                                                                                                     |
| 21:16 | `T1SZ`                | `0`     | Same as T0SZ but for the top half. Irrelevant here - see EPD1.                                                                                                                                                                                                                                                         |
| 22    | `A1`                  | `0`     | Which register the ASID comes from. 0 = TTBR0, 1 = TTBR1. See §7.                                                                                                                                                                                                                                                      |
| 23    | `EPD1`                | `1`     | **Do not walk the TTBR1 tables.** You have no top-half tables, so any access up there should fault rather than walk garbage.                                                                                                                                                                                           |
| 29:24 | `IRGN1`/`ORGN1`/`SH1` | `0`     | The TTBR1 versions of the three above. Dead, because EPD1 = 1.                                                                                                                                                                                                                                                         |
| 31:30 | `TG1`                 | `0b10`  | Granule for TTBR1. **This is the trap: TG1 encodes 4 KiB as `0b10`, not `0b00`.** `01` = 16 KiB, `11` = 64 KiB. `00` is reserved. Two adjacent fields, same purpose, different encodings, for no reason other than history. Dead here, but set it right anyway - a reserved value is undefined behaviour, not a no-op. |
| 34:32 | `IPS`                 | `0b010` | Intermediate Physical Address Size - the widest physical address the machine may produce. `000` 32 bits, `001` 36, **`010` 40**, `011` 42, `100` 44, `101` 48. Your RAM ends at `0x4800_0000`, so 32 would do; 40 costs nothing.                                                                                       |
| 36    | `AS`                  | `0`     | ASID is 8 bits, not 16.                                                                                                                                                                                                                                                                                                |
| 38:37 | `TBI0`/`TBI1`         | `0`     | Top Byte Ignore: strip bits 63:56 before translating, so software can stash a tag in the top byte of a pointer. Off.                                                                                                                                                                                                   |

Everything above bit 38 is reserved and stays zero.

The two fields that actually decide your design are `T0SZ` and `TG0`, and they are what your
`Level` enum encodes:

- `TG0 = 4 KiB` gives 12 offset bits and 9 index bits per level - 512 slots, one page per table.
- `T0SZ = 25` gives 39 bits of VA. 39 - 12 = 27 = three lots of 9. Three levels, so the walk
  starts at level 1 and there is no level 0. That is why `Level::Level0` does not exist.

## 7. What ASID is, and why you can ignore it

**ASID** - **Address Space Identifier** - is a tag for the TLB.

The problem it solves: when you eventually have two processes, each has its own table, and
switching between them means every cached VA-to-PA answer in the TLB is now wrong. The naive fix
is to invalidate the whole TLB on every context switch, which throws away good entries too.

Instead, `TTBR0_EL1` carries an 8- or 16-bit number in its top bits alongside the table address.
Every TLB entry is tagged with the ASID that was live when it was created, and a lookup only hits
if the tags match. Switch process, switch ASID, and the old entries are still there but invisible

- and still valid when you switch back.

The `nG` bit in your descriptors is the other half of this. **nG** is **not Global**. `nG = 0`
means the mapping is global: it applies in every address space and is never ASID-tagged. Kernel
mappings are global. `nG = 1` means per-process, and gets tagged.

You have one address space and no processes. Leave the ASID field at zero, leave `nG = 0`
everywhere, and none of this does anything. It is in the guide only because `A1` and `AS` in TCR
are otherwise unexplainable.

## 8. TTBR0_EL1 - where the tree starts

The simplest of the four. Bits 47:1 hold the physical address of the root table; bits 63:48 hold
the ASID; bit 0 is reserved.

You write `table.base()` into it. Because your root table is 4 KiB-aligned - that is what the
`#[repr(align(4096))] struct Page` is for - the low twelve bits are already zero and the ASID
field is untouched.

**Zero has no special meaning here.** There is no "TTBR0 not set" state. The register boots as
whatever it boots as, and if you turn the MMU on without writing it, the CPU walks whatever
address is sitting there.

The `0` in the name is the pair: `TTBR0` covers the bottom of the address space (VAs starting
with zeros), `TTBR1` covers the top (VAs starting with ones). A kernel with userspace puts user
mappings in TTBR0 and the kernel in TTBR1, so a context switch changes one register and leaves
the kernel mapped. You use only TTBR0, which is why `EPD1 = 1`.

## 9. SCTLR_EL1 - the on switch

The System Control Register is a grab-bag of about 40 unrelated enable bits. Three matter:

- **bit 0, `M`** - MMU enable. This is the one that changes everything.
- **bit 2, `C`** - data cache enable. Without it, Normal memory is still treated as
  non-cacheable, so you would get the address translation but none of the speed.
- **bit 12, `I`** - instruction cache enable.

Bit 1 is `A`, alignment check. Leave it 0: with it set, unaligned access faults even in Normal
memory, which is exactly the restriction you are trying to escape.

This is the register from §3 that you must read-modify-write. The CPU arrives at your `_start`
with several SCTLR bits already set - reserved bits that must stay 1, endianness, exception
behaviour. `msr sctlr_el1, xzr` would clear them and the machine would misbehave in ways that
look nothing like an MMU bug.

```
mrs  x0, sctlr_el1
mov  x1, #0x1005          // M | C | I
orr  x0, x0, x1
msr  sctlr_el1, x0
```

The extra `mov` is not style. `orr` with an immediate only accepts a value that is a run of
contiguous ones, rotated - `0x1005` is bits 0, 2 and 12, so the assembler rejects it outright.
If you build the value in Rust and `msr` it, this never comes up.

## 10. Barriers - why the order is not arbitrary

The CPU is allowed to reorder loads and stores, and to fetch and decode instructions far ahead of
the one it is executing. Both are invisible to normal code. Both break here, because the "other
observer" reading your memory is the hardware table walker, and it is not part of your
instruction stream.

Two barrier instructions fix this.

**`dsb <domain>`** - Data Synchronization Barrier. Nothing after it starts until every memory
access before it has completed and is visible to the domain you name. `ish` means the _inner
shareable_ domain - all the CPUs that share a cache level, which is the set that includes the
table walker. `ishst` is the same but only waits on stores, which is all you need after building
a table.

**`isb`** - Instruction Synchronization Barrier. Throw away everything fetched or decoded ahead
and start again. Needed after any system register write whose effect the _next instruction_
depends on, because that instruction may already have been decoded under the old setting.

**`tlbi vmalle1`** is not a barrier; it is the invalidate itself. The name reads as
"**TLB I**nvalidate, by **VM**ID, **all** entries, **E**L**1** regime" - wipe the whole TLB for
kernel and user translations. You do it before turning the MMU on because you do not know what
firmware left in there.

## 11. The order

1. `dsb ishst` - the table you wrote is really in memory
2. `tlbi vmalle1` - nothing stale can answer a lookup
3. `dsb ish` - the invalidate has finished
4. `msr mair_el1` - memory types exist before anything indexes them
5. `msr tcr_el1` - the shape is set before anything walks
6. `msr ttbr0_el1` - the root address is set before anything walks
7. `isb` - the next instruction sees 4, 5 and 6
8. read-modify-write `SCTLR_EL1 |= M | C | I` - **the MMU is now on**
9. `isb` - the next instruction after this is the first one fetched through the MMU

Step 8 is the interesting one. The instruction after `msr sctlr_el1` is fetched from a virtual
address, translated through your table. Your table is an identity map, so that virtual address
resolves to the same number it was before, and the CPU carries on into the next instruction as if
nothing happened. That is the entire reason the map has to be an identity map: **the program
counter has to mean the same thing on both sides of one instruction.** Get the map wrong and the
CPU fetches its next instruction from somewhere else - usually nowhere - and you get a fault with
no code left to handle it.

The same applies to your stack pointer, your UART, and your vector table. All three are already
covered: RAM and the device window are both mapped.

## 12. How you will know it worked

Boot it. If the MMU came up, the machine keeps printing and `make run` behaves exactly as it does
today. That is the good outcome and it is deliberately boring.

To prove it is actually on rather than accidentally still off, do something that only works with
translation:

- Read `SCTLR_EL1` back after the write and print it. Bit 0 set means the MMU is on.
- Do an unaligned 8-byte load from RAM. It faults today; it works with the MMU on.
- Touch an address you did not map - anything above `0x4800_0000` - and confirm you get your
  own fault report rather than a hang.

Three failure signatures worth recognising:

**Silence, and `make regs` shows `PC = 0x200`.** A fault was taken and your vector table was not
reachable. If the vectors are installed, this means the fetch after `msr sctlr_el1` went
somewhere unmapped - the identity map is wrong or incomplete.

**Silence, and `PC` is somewhere in your init code.** The table walk itself faulted, usually
`TTBR0_EL1` pointing at something that is not a table, or `TCR_EL1` with a reserved field set.

**It prints, but slowly, or garbled output from the UART.** The UART region got `AttrIndex::Normal`
instead of `Device`, so writes to it are being cached and gathered.

---

## Optional reading

- ARM Architecture Reference Manual for A-profile, section D8 (VMSA). The tables are in D8.3, the
  registers in the register index at the back.
- The register descriptions online: `developer.arm.com/documentation/ddi0601/latest` - one page
  per register, with every field and every encoding. `TCR_EL1` there is the source for §6.
- `virt.dts` for the memory map you are mapping.
