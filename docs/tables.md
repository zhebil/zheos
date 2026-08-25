# TABLES - teaching the CPU to rewrite addresses

## 1. What this is for

Right now, when your code says `0x0900_0000`, the number `0x0900_0000` goes out on the wires and
the UART answers. The address in your register *is* the address on the bus. No step in between.

You are about to add a step in between. From now on the CPU will take every address your code
uses, look it up in a table you built, and put a *different* number on the bus.

That sounds like a pointless indirection, and for the first version it literally is - you are
going to build a table that maps every address to itself. Nothing will change. That is on
purpose. Once the lookup step exists, you get things that are impossible without it:

- **Memory that faults instead of silently working.** Today a wild pointer to `0x4900_0000`
  writes into nothing and keeps going. With a lookup, an address you never put in the table
  makes the CPU stop and tell you.
- **Read-only code.** You can mark the region holding your instructions as "reads allowed,
  writes not". A bug that scribbles on `.text` becomes an error message instead of a crash
  twenty minutes later.
- **A guard page under the stack.** Your stack is 32 KiB with nothing below it, so a deep
  recursion quietly eats `.bss`. Leave one page below it out of the table and overflow becomes
  an immediate fault.
- **Two programs at the same address.** Every process thinking it owns `0x1000` is the whole
  reason this machinery exists in the first place. Far away, but that is the destination.

This skill builds the table. It does **not** switch it on - that is the next skill,
`zheos-f27`. Section 8 explains why splitting it that way is the single most useful decision
here.

---

## 2. Is this a hardware thing or a software thing

This was the question, and it deserves a straight answer, because the situation is genuinely
unusual and nothing else in the project works this way.

**The format is hardware. The bytes are yours.**

Inside the CPU there is a lump of circuitry called the **MMU** - Memory Management Unit. Part of
it is a small hardware state machine called the **table walker**. When it is switched on, that
circuit reads bytes out of your RAM and interprets them, on every load, every store, and every
instruction fetch.

So:

- **You build the tables from scratch.** No library gives them to you. No firmware left any
  behind. They are just some `u64` values you write into memory you got out of your bump
  allocator. Right now that memory holds garbage, and if you switched the MMU on this second the
  machine would follow that garbage and die.
- **The layout is not up for discussion.** It is not a convention the community settled on, like
  a file format or a protocol. ARM chose it, wrote it into the transistors, and shipped it. Bit
  10 means "access allowed" because a wire in the chip is connected to bit 10.

You already do exactly this, every day, with the UART. When `uart.rs` writes `0x301` into the
control register, that is not a number anyone agreed on in a mailing list - it is the bit pattern
the PL011's internal wiring reacts to. Page tables are the same deal, just bigger: instead of a
32-bit register you are filling in a few thousand bytes of RAM, and instead of a UART reading
them it is a circuit inside the CPU.

The consequence is that you cannot design this part. Linux, FreeBSD, and your kernel all write
the *identical* bit layout, because none of them has a choice. What you get to design is which
addresses you map and how you organise the code that fills the bytes in.

---

## 3. Why it is a tree and not a list

The obvious design would be a big lookup table: one entry per address, look up the address, get
the answer.

Count how big that is. Even a modest 39-bit address space is 549,755,813,888 addresses. At 8
bytes per entry, that table is 4 terabytes. You have 128 megabytes.

So the real design does two things to shrink it.

**First, it works in chunks, not single bytes.** The smallest thing that gets its own entry is
4096 bytes - a **page**. Every address inside one page gets the same answer, and the low 12 bits
of the address just ride along unchanged. That divides the table size by 4096 straight away.
Still 1 GiB. Not enough.

**Second, it splits the lookup into stages.** Instead of one enormous table, you get a small tree
of small tables, and the address is chopped into pieces that each pick a slot in one of them.

Every table in the tree is the same size and shape: **512 slots, 8 bytes each**. That is 4096
bytes - exactly one page. That is not a coincidence, it is the point: tables are pages, so the
allocator that hands out pages is the allocator that hands out tables.

512 slots means it takes 9 bits of the address to pick one, and now every magic number in this
document falls out of arithmetic you can do yourself:

- The last stage: each slot covers one page. **4 KiB.**
- One stage up: each slot covers 512 pages. 512 × 4 KiB = **2 MiB.**
- One stage up again: 512 × 2 MiB = **1 GiB.**
- One more: 512 × 1 GiB = **512 GiB.**

Each stage up multiplies by 512, because each stage is 9 more bits of address.

---

## 4. What "level 1", "level 2", "level 3" mean

They are just names for those stages. **Level = how deep in the tree, counting down from the
root.** Level 0 is the outermost table, level 3 is the innermost. That is the whole meaning.

The useful way to hold them is by how much ground one slot covers:

| level | one slot covers | you need this level if... |
| --- | --- | --- |
| 0 | 512 GiB | your address space is bigger than 512 GiB |
| 1 | 1 GiB | your address space is bigger than 1 GiB |
| 2 | 2 MiB | you want detail finer than 1 GiB |
| 3 | 4 KiB | you want detail finer than 2 MiB |

**You get to choose where the tree starts.** There is a register that tells the CPU how many bits
of address you intend to use, and that number decides which level is the root. Everything you
care about lives below `0x4800_0000`, which is under 2 GiB, so you will ask for a 39-bit address
space - and with 39 bits the tree starts at **level 1**. Level 0 never exists. You never allocate
it and you can forget it.

So your tree is at most three levels deep, and the first version is one level deep.

### What a slot can say

Each slot is 8 bytes and says one of three things. This is the mechanism, and it is all of it:

1. **"Nothing here."** The slot is zero. Any address landing here makes the CPU stop and raise a
   fault. This is the default, and it is why a fresh table must be zeroed.

2. **"Here is the answer for this whole chunk."** The slot holds a physical address and some
   permission bits. The lookup stops immediately. At level 1 this one slot covers a whole
   gigabyte; at level 2, 2 MiB; at level 3, 4 KiB. The ARM manual calls the big ones **blocks**
   and the 4 KiB ones **pages**; they are the same idea at different zoom levels.

3. **"Too coarse - go look over there."** The slot holds the address of *another table*, one level
   down. The CPU goes and repeats the process there with the next 9 bits of the address.

Option 3 is what makes it a tree. Option 2 is what stops it being enormous: mapping a whole
gigabyte with one slot is why your entire kernel needs two of them instead of thirty-two
thousand.

### The walk, step by step

Suppose the MMU is on and your code does `ldr x0, [x1]` with `x1` holding `0x4008_1234`.

The hardware chops that address up:

```
0x4008_1234

  bits 38..30  →  1        which slot in the level 1 table
  bits 29..21  →  0        which slot in the level 2 table, if it gets that far
  bits 20..12  →  0x81     which slot in the level 3 table, if it gets that far
  bits 11..0   →  0x234    offset inside the final chunk, carried through untouched
```

Then:

1. Read the register holding the root table's address. Call it `R`.
2. Read 8 bytes from `R + 1 × 8`. That is slot 1 of the level 1 table.
3. If that slot is zero → fault, done.
   If it says "here is the answer" → the answer is the address in the slot, plus the bottom 30
   bits of `0x4008_1234`. Done, one memory read.
   If it says "go look over there" → take the table address out of it and repeat at step 2 with
   the level 2 slot number.
4. Same again at level 3 if needed.
5. Check the permission bits on whatever slot ended the walk. Allowed → the access goes through.
   Not allowed → fault.

That is it. A load can therefore cost one, two, or three extra memory reads. That would be
crippling, so the CPU caches the results in something called the **TLB** - Translation Lookaside
Buffer - and the walk usually does not happen at all. The TLB is not your problem in this skill,
and it becomes your problem the first time you *change* a table after the MMU is running.

---

## 5. The map you are actually building

Here is the machine, checked this session:

```
0x0000_0000  flash, unused
0x0800_0000  interrupt controller
0x0900_0000  the UART                      ← devices
0x0901_0000  clock, GPIO, virtio
0x0A00_4000  ...end of devices

0x4000_0000  RAM starts    ┐
0x4008_0000  your kernel   │
0x4400_0000  device tree   │ ← RAM
0x4800_0000  RAM ends      ┘
```

Every device sits below `0x4000_0000`. All RAM sits above it. The level 1 table's slots are 1 GiB
each. So:

```
slot 0  covers 0x0000_0000 .. 0x4000_0000   →  all the devices
slot 1  covers 0x4000_0000 .. 0x8000_0000   →  all the RAM
slots 2..511                                →  zero, nothing there
```

**Two slots.** One table, one page, 4096 bytes from your bump allocator, and two of its 512 slots
are non-zero. That is a complete and correct map of everything this kernel touches.

Both slots say "the answer is the same address you asked for" - that is what makes it an
**identity map**, and it is why switching the MMU on will change nothing observable. That is the
best possible first result: if anything at all changes, you have a bug.

### Then make it less coarse

Slot 1 says all of `0x4000_0000..0x8000_0000` is RAM. Only the first 128 MiB actually exists. So a
stray pointer to `0x5000_0000` would translate fine and go nowhere.

The fix is to turn slot 1 from "here is the answer" into "go look over there", pointing at a
second table. That table's slots are 2 MiB each, and 128 MiB needs 64 of them. Slots 64 through
511 stay zero, so anything above `0x4800_0000` faults.

That is the version worth having, and it is where you actually build a *tree*: two tables, a slot
pointing from one to the other, and two allocations from `Bump`.

---

## 6. What is inside one slot

Eight bytes. Most of the bits are for things you do not have. Here are the ones that matter, and
section 11 has the rest for when you are writing the code.

**The bottom two bits say which of the three kinds of slot it is.**

| bottom 2 bits | meaning |
| --- | --- |
| `00` or `10` | nothing here |
| `01` | here is the answer (a block, at level 1 or 2) |
| `11` | at level 1 or 2: go look over there. At level 3: here is the answer. |

Yes, `11` means two different things depending on the level, and at level 3 the value `01` is
invalid. There is no good reason for this. Everybody trips on it once.

**Bits 47 down to 12 hold the physical address.** The low 12 bits are not stored, because
everything is at least page-aligned so they are known to be zero. In practice you write
`address | flags` and the flags live in the space the address does not use. If your address is
not properly aligned, its low bits land on top of your flags and quietly corrupt them.

**Bit 10 is the Access Flag.** Set it to 1. If it is 0, every single access through this slot
faults, no exceptions. The feature exists so an operating system can watch which pages are being
used; you have no such operating system, so a zero here is only ever a bug. This is the most
common reason a first attempt at page tables does nothing, it costs one bit, and the symptom
looks exactly like "the MMU is broken".

**Bits 4 to 2 say what kind of memory this is.** Not the kind itself - a number from 0 to 7,
which is an index into a list of eight kinds held in a separate register. Section 7.

**Bits 7 and 6 are permissions.** Leave them `00`, which means "readable and writable by the
kernel, no access from user code". Read-only text and the rest are worth doing, and they belong
after the map is proven to work at all. Do not debug two things at once.

**Bits 54 and 53 say "never execute instructions from here".** Set both on the device slot -
letting the CPU speculatively fetch instructions out of a UART's FIFO is a real hazard, not a
theoretical one. On the RAM slot, bit 53 must be 0 or your own code cannot run.

A "go look over there" slot is much simpler: the address of the next table, with `11` in the
bottom bits, and nothing else set.

---

## 7. Two kinds of memory, and why the UART cares

The CPU treats RAM and devices completely differently, and the slot is where you tell it which
is which.

**Normal memory** is RAM. The CPU may cache it, reorder accesses to it, and merge two small
writes into one big one. All of that is fine, because reading RAM twice gives the same answer and
nobody is watching.

**Device memory** is a UART or an interrupt controller. Every access is a real event. The CPU
must not cache it, must not reorder it, and must not merge writes.

Consider what each of those would do to your UART if you got it wrong:

- **Caching**: your write to the transmit register lands in a cache line and never reaches the
  chip. Output stops - but not cleanly. Some of it appears later, out of order, when the cache
  line is eventually evicted.
- **Merging**: two byte-sized writes to the data register get merged into one 16-bit write. Two
  characters become one.
- **Reordering**: the CPU writes the data register before checking whether the FIFO has room, or
  hoists the status-register read out of your spin loop and spins forever on a stale value.

That last one is worth stopping on, because it looks like something you already fixed. Your
`read_volatile` in `mmio.rs` stops the *compiler* from reordering or eliding those accesses. It
does nothing about the *CPU*, which reorders at runtime. The memory type is what constrains the
CPU. You need both, and neither substitutes for the other.

The eight possible memory types are defined in a register called **MAIR_EL1** - Memory Attribute
Indirection Register. It is eight bytes, one per type, and the 3-bit number in a slot picks one.
Writing that register is the next skill's job, but you have to agree the numbering now, because
it is baked into the slots you are about to write:

| number | meaning | byte value |
| --- | --- | --- |
| 0 | Normal RAM, cached | `0xFF` |
| 1 | Device | `0x04` |

Put those two constants in your module with a comment, and use `0` for the RAM slot and `1` for
the device slot.

One thing to look forward to: right now, with the MMU off, the CPU treats *everything* as device
memory. That is why unaligned loads fault today - the note in `CLAUDE.md` about ESR `0x21` is
exactly this. Once the MMU is on and RAM is marked as Normal, unaligned loads start working. That
is a nice, visible confirmation that the map took effect.

---

## 8. Build it, but do not switch it on

The acceptance criterion for this skill says the tables exist and are inspectable, MMU still off.
That is not caution for its own sake.

With the MMU off you can build a completely wrong table and the machine will not react at all.
Your UART works, your timer ticks, your shell responds. You can print the table, walk it, compare
it against numbers you worked out on paper, and fix it, all with a live machine under you.

The moment you switch it on, the *next instruction the CPU fetches* goes through the table. If
that fetch fails, the CPU tries to run the fault handler - and fetching *that* goes through the
table too, and fails as well. The machine locks up with nothing on the screen and no way in.
That is not a debugging session, it is a rebuild-and-guess session.

So: build it here, get every number right while you can still see, then flip the switch next
skill with high confidence.

---

## 9. Steps

**Step 1 - one page, zeroed.** Get 4096 bytes with 4096-byte alignment out of `Bump`, zero all of
it, print the address. Confirm the bottom twelve bits are zero, then `make mem ADDR=<that> N=8
FMT=xg` and confirm the monitor sees eight zeros.

Zeroing is not optional and it is not defensive. `Bump::alloc` deliberately does not zero - that
is written down in `docs/bump.md`. 510 of these slots will never be written by you, and "nothing
here" is spelled *zero*. Leftover garbage in a slot is the CPU following a random pointer with
random permissions.

**Step 2 - write the two slots by hand.** No loop, no general function. Work out the two 64-bit
values from section 6, store them into slot 0 and slot 1, and print them. They should be:

```
slot 0, devices at 0x0000_0000 :  0x0060_0000_0000_0405
slot 1, RAM at 0x4000_0000     :  0x0040_0000_4000_0701
```

Here is where each of those digits comes from, so they do not look magic:

```
slot 0 - devices                        slot 1 - RAM
  bottom bits = 01        0x...0001       bottom bits = 01        0x...0001
  memory type 1  (bit 2)  0x...0004       memory type 0           0x...0000
  access flag    (bit 10) 0x...0400       shareable    (bits 8,9) 0x...0300
  no-execute     (bit 53) 0x0020_...      access flag  (bit 10)   0x...0400
  no-execute     (bit 54) 0x0040_...      no-execute   (bit 54)   0x0040_...
  address 0x0000_0000                     address 0x4000_0000
  ─────────────────────────────           ──────────────────────────────
  0x0060_0000_0000_0405                   0x0040_0000_4000_0701
```

Then `make mem` on the table address and confirm the two values are really in RAM.

At this point you have a complete, correct identity map of the entire machine, in two slots. It
is worth having existed even though the next step generalises it away, because every bit in it is
one you placed yourself.

**Step 3 - walk your own table in software.** Write a function that takes an address and does
exactly what section 4 describes: pick the slot, look at the bottom two bits, either fault,
answer, or descend. Return the physical address it would produce.

This is the most valuable thing in the whole skill. It is how "inspectable" gets satisfied with
the MMU off, and it is the tool you will want at 1am next skill. Check it against four addresses
you work out on paper:

```
0x0900_0000  →  0x0900_0000    the UART, via slot 0
0x4008_0000  →  0x4008_0000    your own code, via slot 1
0x4400_0000  →  0x4400_0000    the device tree, via slot 1
0x9000_0000  →  nothing        slot 2 is zero
```

That last line is the one proving zeroed slots really do mean "nothing here".

One honest limit: this walker shares its slot-picking arithmetic with the code that built the
table, so if that arithmetic is wrong in both places they will agree with each other. The defence
against that is the paper arithmetic above, not more code.

**Step 4 - replace the two hand-written stores with a function.** Something that takes a `Region`
and a memory kind and fills in whatever slots are needed. Run it and get the *same two numbers*
out. Same hex, different code path. If they move, the loop is wrong, and you have the previous
values to diff against - which is the entire reason step 2 was its own step.

The rule the function follows is the one every real kernel uses: at each level, if the chunk you
are mapping is big enough and aligned enough to fit one whole slot at this level, write the
answer and stop. Otherwise allocate a table one level down and go deeper.

**Step 5 - go one level down for RAM.** Map `board.memory` - the real 128 MiB - instead of a
rounded-up gigabyte. Slot 1 becomes "go look over there", pointing at a second table with 64
slots of 2 MiB each.

Three things change, and each is checkable:

- `translate(0x4008_0000)` still gives `0x4008_0000`, now through two levels.
- `translate(0x4800_0000)` now gives *nothing*, where before it succeeded. That change is the
  entire point of this step.
- Slot 1 is now `<address of the second table> | 0x3`. Its bottom two bits are `11`, not `01`.

**Step 6 - print the map.** Loop over the table, skip zero slots, print the address range each
one covers, where it points, and what kind of memory it is. Ten lines, and it turns "inspectable"
into output you can paste into a commit message.

Steps 1 to 3 are one sitting. Steps 4 to 6 are another.

### Worth adding while you are here: say why a fault happened

Not required, but it pays for itself the moment you switch the MMU on.

When the CPU refuses an access it raises an exception and fills in the **ESR** - Exception
Syndrome Register - a number describing what went wrong. `src/exception.rs` already prints it as
one opaque hex value. For memory faults, the bottom 6 bits are the whole answer:

| bottom 6 bits | meaning |
| --- | --- |
| `0x04`-`0x07` | nothing was mapped there |
| `0x08`-`0x0B` | you left the access flag clear |
| `0x0C`-`0x0F` | mapped, but not with the permission you wanted |
| `0x21` | unaligned access |

And within the first three rows, the lowest *two* bits are which level the walk got to. So a
"nothing mapped" fault at level 1 means the root slot is zero, while the same fault at level 2
means you descended fine and the leaf is missing. That is the difference between "I never mapped
it" and "I mapped it into the wrong table", for free.

`FAR_EL1` - Fault Address Register, the address that failed - is already being printed, so with
those 6 bits you have both halves.

---

## 10. When the numbers come out wrong

| what you see | almost certainly |
| --- | --- |
| slot reads `...0404` instead of `...0405` | you built the flags and forgot the bottom two bits. An even-numbered slot means "nothing here". |
| a slot full of plausible-looking garbage | you did not zero the page. `Bump` does not do it for you. |
| the table address is not 4096-aligned | you asked `Layout` for a `[u64; 512]`, whose alignment is 8. Ask for 4096 explicitly. |
| the walker says "nothing there" for an address you mapped | the shift when picking a slot. Level 1 shifts right by 30, level 2 by 21, level 3 by 12. |
| the walker finds the right chunk but the wrong offset | you added back 12 bits of the original address after a level 1 slot. A 1 GiB slot carries 30 bits through, a 2 MiB slot carries 21. |
| slot 63 or 511 is set and you never wrote it | you wrote past the end of the table. 512 slots, and the way to make that impossible is section 12. |
| the mapping function never finishes | each branch has to advance by *its own* chunk size. A branch that descends still advances 2 MiB, not by the whole region. |
| asked to map 128 MiB, got a whole gigabyte | the "does it fit in one slot at this level" test checked alignment but not length. Both matter. |
| the machine dies during this skill | you wrote to memory you did not get from `Bump`. Nothing else in this skill touches anything. |
| a fault with ESR bottom bits `0x21` | unaligned access, MMU still off. Slots are 8 bytes at 8-byte spacing inside a 4096-aligned page, so if you see this, your base address is not what you think. |

The method throughout: compute the number on paper, print it from Rust, read it back with `make
mem`. Three independent views of the same 8 bytes. If they disagree, the bug is in the few lines
between them.

Note that this QEMU has no `info tlb` in the monitor for aarch64 - checked this session - so
there is no oracle to compare against. The paper arithmetic is the oracle.

---

## 11. Reference: the bits, for when you are writing the code

Sections 1 to 10 are the ideas. This is the lookup table.

**A slot holding an answer** (a block at level 1 or 2, a page at level 3):

| bits | name | what to put |
| --- | --- | --- |
| 1:0 | kind | `01` for a block, `11` for a level 3 page |
| 4:2 | AttrIndx | `0` for Normal RAM, `1` for Device |
| 5 | NS | 0 |
| 7:6 | AP | `00` - kernel read/write, no user access |
| 9:8 | SH | `11` for Normal RAM, `00` for Device |
| 10 | AF | **1**, always |
| 11 | nG | 0 |
| 47:12 | address | the physical address; low bits must already be zero |
| 52 | contiguous | 0 |
| 53 | PXN | 1 for devices, 0 for RAM |
| 54 | UXN | 1 |

Alignment of the address: a level 1 block needs its low 30 bits zero, a level 2 block its low 21,
a level 3 page its low 12.

**A slot pointing at another table:** `next_table_address | 0b11`. Bits 63:59 can restrict
everything below; leave them zero so permissions are decided entirely at the leaves.

**Picking a slot from an address:**

```
level 1  →  (address >> 30) & 0x1FF
level 2  →  (address >> 21) & 0x1FF
level 3  →  (address >> 12) & 0x1FF
```

which is `(address >> (12 + 9 * (3 - level))) & 0x1FF`.

That `& 0x1FF` is worth writing even though you can prove it is redundant, because it makes the
result *structurally* in the range 0..512. If the only way to get a slot number is through this
function, there is no out-of-range case, so there is no bounds check, no error path to invent for
something that cannot happen, and no `panic!` in the generated code. `make asm` will show you the
difference: a masked index emits no compare and no branch. That is how this module keeps your
no-panic rule without inventing a `Result` for a bug.

**A rough shape for the module**, since you asked for guidance and not code:

```rust
pub enum Memory { Normal, Device }

pub struct Tables { /* the root table's address */ }

impl Tables {
    pub fn build(bump: &mut Bump) -> Option<Tables>;
    pub fn identity_map(&mut self, region: Region, kind: Memory, bump: &mut Bump)
        -> Result<(), MapError>;
    pub fn root(&self) -> usize;
    pub fn translate(&self, va: usize) -> Option<usize>;
}
```

Three notes on it:

`identity_map`, not `map`, because there is no separate "what address should this appear at"
parameter - it is always the same address. Add that parameter when something actually moves.

`bump` is passed in rather than stored, because the tables only need to allocate while they are
being built. Storing a `&mut Bump` would lock the allocator away from everyone else for as long
as the tables exist.

Failures are `Option` and `Result`, never a panic. Out of memory is a condition. So is being
handed a region whose base or size is not a multiple of 4096, which you cannot map. `kmain`
already has the pattern: print what failed, call `halt()`.

**What the next skill does with this**, so you know the shape is right:

```
MAIR_EL1  = 0x0000_0000_0000_04FF     the two memory types from section 7
TCR_EL1   = 39-bit address space, 4 KiB pages
TTBR0_EL1 = tables.root()             Translation Table Base Register
SCTLR_EL1 |= 1                        System Control Register - the actual switch
```

Four registers. `root()` hands back a plain physical address, the map already covers the code
that will be executing at that instant, and nothing about the tables needs to change. If any of
those three were false, the design would be wrong now rather than later.

---

## 12. Done when

- One 4096-aligned, fully zeroed page comes out of `Bump`, and `make mem` shows the zeros.
- Slot 0 reads `0x0060_0000_0000_0405` and you can say what every set bit is doing.
- After step 5, slot 1 points at a second table, and that table has 64 filled slots and 448 zeros.
- Your walker returns the same address it was given for the UART, your code, the stack and the
  device tree, and returns nothing for `0x4800_0000` and `0x9000_0000`.
- Devices are marked device memory, RAM is marked normal memory.
- Every address and size comes from `board.memory`, the linker, or the device tree. The only
  literals are the device region's base and length, and those are rounded to 1 GiB rather than
  claimed to be exact.
- No `panic!`, no `unwrap`, no `assert!`. Slot numbers are masked, so out of range does not exist.
- `make lint` is clean and the shell still works - the MMU is off, so nothing should have changed.
- You can say out loud: what the hardware does between a `ldr` and the wires, why the tree has
  levels, why bit 10 must be 1, and why the UART must not be marked as normal memory.

The last one is the one that matters. The others you can check.

---

## Optional reading

None of this is required to do the work.

- **ARM Architecture Reference Manual (ARMv8-A), section D8.** The actual specification for every
  bit in section 11. Dense, but it is the only authority - there is nowhere else to look.
- **`target/arm/ptw.c` in the QEMU source.** The table walker, written in C. Far easier to read
  than the manual if you want to check what a specific bit combination really does, and it is the
  exact code that will be interpreting your table.
- **`arch/arm64/mm/mmu.c` in the Linux source**, function `__create_pgd_mapping`. The
  "big enough and aligned enough → write the answer, otherwise go deeper" rule from step 4, in
  about forty lines.
- **`arch/arm64/include/asm/pgtable-hwdef.h`.** Linux's names for the bits - `PTE_AF`,
  `PTE_ATTRINDX`, `PMD_SECT_S`. Useful as a second opinion on your own constants.
