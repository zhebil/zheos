# LOCKDOWN - permissions that mean something

## 1. What this is

Right now the kernel can write to its own instructions and execute its own stack. Not by accident
- your descriptors say so. `Descriptor::NORMAL_BLOCK` in `src/mmu/descriptor.rs:53` sets
`AccessPermissions::KernelReadWrite` and leaves `pxn: false`, and `kmain` applies it uniformly to
all 128 mebibytes of memory. Every byte is readable, writable, and executable by the kernel.

LOCKDOWN gives each region of the image the permissions it should have had: instructions
executable and not writable, constants readable and neither, data writable and not executable,
stack the same.

The category is **hardware format plus a policy choice**. The bits are fixed by the architecture
and you have already implemented all of them - `ap`, `pxn` and `uxn` are fields on your
`Descriptor` today and are encoded correctly. Nothing new gets decoded. What is new is deciding
which region gets which, and getting the translation table fine-grained enough to say so.

## 2. Why this matters more than it looks

Two reasons, and the second is the one that makes it a prerequisite rather than a nicety.

**It turns silent corruption into a fault at the instruction that caused it.** A stack overflow
today runs down into `.bss` and keeps going, and the first symptom is unrelated output going wrong
much later. With a guard page it is a data abort with the faulting address in `FAR_EL1` - **F**ault
**A**ddress **R**egister, **E**xception **L**evel 1 - reported by the handler you already wrote.

**Exception level 0 is meaningless without it.** TWO WORLDS depends on this skill, and the
dependency is hard. Dropping to exception level 0 while every descriptor says
`AccessPermissions::KernelReadWrite` gives you a "user" mode that can still read and write every
byte of the kernel. That is a demonstration of `eret`, not a privilege boundary. This skill is
what makes the boundary real.

## 3. The bits, all of which you already have

Three fields on the descriptor, from `src/mmu/descriptor.rs`:

**AP\[2:1]**, **A**ccess **P**ermissions, two bits starting at position 6 (`bits::AP = 6`). Your
`AccessPermissions` enum already spells out all four encodings:

| encoding | name in your enum | exception level 1 | exception level 0 |
| --- | --- | --- | --- |
| `0b00` | `KernelReadWrite` | read, write | none |
| `0b01` | `AllReadWrite` | read, write | read, write |
| `0b10` | `KernelReadOnly` | read only | none |
| `0b11` | `AllReadOnly` | read only | read only |

Note what is not in that table: execute. Execution is controlled separately, which surprises
people every time.

**PXN**, **P**rivileged e**X**ecute **N**ever, bit 53. Set it and exception level 1 - the kernel -
cannot fetch instructions from this memory.

**UXN**, **U**nprivileged e**X**ecute **N**ever, bit 54. The same for exception level 0.

`NORMAL_BLOCK` today sets `uxn: true` and `pxn: false`, which is correct for `.text` and wrong for
everything else.

One system register bit is worth knowing about: **SCTLR_EL1.WXN** - **S**ystem **C**on**T**ro**L**
**R**egister, exception level 1, **W**rite implies e**X**ecute **N**ever - bit 19. It is a system
register, so it is reached with `msr` and `mrs` and `make mem` cannot see it, exactly like the
registers `src/mmu/init.rs` already writes. Set it and any writable region becomes
non-executable regardless of what the descriptor says. Turning it on after the table is correct is
a good way to prove the table is correct: if anything breaks, something was relying on memory that
is both writable and executable.

## 4. What each region should get

`linker.ld` lays out five things, and there are six rows because the stack is not a section:

| region | AP | PXN | UXN |
| --- | --- | --- | --- |
| `.text` | `KernelReadOnly` | **false** | true |
| `.rodata` | `KernelReadOnly` | true | true |
| `.data` | `KernelReadWrite` | true | true |
| `.vectors` | `KernelReadOnly` | **false** | true |
| `.bss` | `KernelReadWrite` | true | true |
| stack | `KernelReadWrite` | true | true |
| devices | `KernelReadWrite` | true | true |

Two rows carry the whole skill.

**`.vectors` is executable.** It is easy to miss because it is not called `.text`, but it holds the
exception vector table that `install_vectors` points `VBAR_EL1` at - **V**ector **B**ase
**A**ddress **R**egister, exception level 1 - and the processor fetches
instructions from it. Give it `pxn: true` and the machine dies at the first exception it takes,
which on this kernel is immediately.

**`.rodata` is neither writable nor executable.** Read-only and non-executable are different
properties, and this is the row most often left as merely read-only.

Everything not in the list stays unmapped, which is already true and is the strongest permission
of all.

## 5. The three things standing in the way

None is about permissions. All three are about being able to *say* anything per-region.

### The linker script does not name the regions

`linker.ld` plants `__image_start`, `__bss_start`, `__bss_end` and `__stack_top`. That is enough to
know where the image is and nothing about what is inside it. You need a symbol at each section
boundary: the start and end of `.text`, of `.rodata`, of `.data`, of `.vectors`.

And they have to be **page aligned**, because a descriptor covers 4096 bytes and cannot give the
first half of a page different permissions from the second. Today the sections abut with no
alignment between them - `linker.ld` has `ALIGN(8)` after `.vectors` and `ALIGN(16)` before the
stack, and nothing else - so `.text` and `.rodata` share a page. Each boundary needs an
`ALIGN(4096)`, which costs up to 4095 bytes of padding per boundary and is the entire price of
this skill in memory.

`src/memory/mod.rs:9` already does the Rust half: `image()` declares `__image_start` and
`__stack_top` in an `unsafe extern` block and takes `&raw const` of each. Section 4 of `docs/bump.md`
explains why that shape and not a plain `static`. This is that technique used a second time, which
is the point at which it stops being a trick.

### The image is currently mapped in 2 mebibyte blocks

Follow what `identity_map` does with `board.memory`. The region is `0x4000_0000` to `0x4800_0000`,
128 mebibytes. `Level::size` is `1 << offset` and `offset` is `12 + 9 * (3 - level)`, so a level 1
slot covers `1 << 30` (1 gibibyte), a level 2 slot `1 << 21` (2 mebibytes), a level 3 slot
`1 << 12` (4 kibibytes). The region does not fill a gibibyte, so `map_range` descends. At level 2,
128 mebibytes is exactly 64 slots of 2 mebibytes, each aligned and each filling its slot, so every
one becomes a block and the walk stops. Memory is mapped in 2 mebibyte blocks.

`docs/diagrams/tables.tldx.jsx` draws that walk for `0x4008_1234`, an address inside your image.
Run `tldx serve` on it if the level-by-level descent is not yet automatic.

The kernel image lives at `0x4008_0000`, inside the first of those blocks. To give `.text` its own
permissions you need 4 kibibyte pages over that range, and asking for them now returns
`MapError::BlockInTheWay` - `src/mmu/mod.rs:133` refusing, correctly, to rewrite a mapping that is
already live.

So the fine regions must be mapped **before** the coarse one. That much is ordering, and it is
cheaper than implementing block splitting.

### Ordering alone silently destroys the fine mapping

This is the one the design notes only hinted at, and it is the trap in the skill.

`child_table` guards one direction: it refuses to turn a live **block** into a table. Nothing
guards the other direction. Look at `map_range` at `src/mmu/mod.rs:151`:

```rust
if level.is_aligned(addr) && chunk_end == slot_end {
    self.set(level.slot_of(addr), Descriptor { ... });
```

It writes a leaf into the slot without ever reading what was there.

Now walk the coarse pass after you have mapped the image finely. `map_range` for
`0x4000_0000..0x4800_0000` at level 1 does not fill the gibibyte slot, so it descends into the
level 2 table you already built. At level 2 the first chunk is `0x4000_0000..0x4020_0000`: aligned,
and exactly filling the slot. The condition is true, so `set` overwrites that slot.

That slot was holding the **table** descriptor pointing at your level 3 table for the image. It
becomes a 2 mebibyte block again. Every permission you just set is gone, the level 3 table is
leaked, and nothing reports anything - `map_range` returns `Ok`.

So you need one of two fixes, and the first is smaller:

- **Guard the write.** `map_range` reads the existing descriptor before `set`, and if it is
  `Kind::Table`, that is an error - a `TableInTheWay(usize, Level)` variant next to
  `BlockInTheWay`, refusing for the mirror-image reason. Then the coarse pass fails loudly instead
  of silently, and you split `board.memory` into the ranges that are actually still free.
- **Or skip.** Have the coarse pass take a list of already-mapped regions and step over them, the
  way `MemoryMap::unreserved` already steps over reservations.

Take the guard. It is a few lines, it turns a silent failure into a named one, and it is the same
lesson as the allocator work: the check you can afford is the one at the write.

Mapping the image at page granularity also means a level 3 table for each 2 mebibyte block it
touches, so the table count goes up and the heap is asked for more pages at boot. The
`heap: N of 32768 pages free` line will drop by a few pages, and you should predict roughly how
many before you see it.

## 6. The guard page

A page below the stack, left unmapped. A stack that runs off the end touches it and takes a data
abort with the faulting address just below the stack base, instead of silently eating `.bss`.

One detail `linker.ld` makes awkward: the stack is `. = ALIGN(16); . = . + 0x8000;`, so the base
is 16-byte aligned, not page aligned. A guard page needs a 4 kibibyte boundary to sit on, so the
stack base needs `ALIGN(4096)` too, and a `__stack_bottom` symbol to name it.

This is the cheapest thing in the skill - one region left out of the map - and it is the one that
will save you the most debugging time, because stack overflow is the failure that looks like
everything else.

## 7. What you are building

No new module. Changes in four places:

- **`linker.ld`**: `ALIGN(4096)` at each section boundary and before the stack, plus a symbol pair
  per section and `__stack_bottom`.
- **`src/memory/mod.rs`**: functions beside `image()` that read those symbols into `Region`s. Same
  `unsafe extern` and `&raw const` shape, one per section.
- **`src/mmu/descriptor.rs`**: new constants beside `NORMAL_BLOCK` and `DEVICE_BLOCK`. Same shape
  with different `ap`, `pxn` and `uxn` - the policy table from section 4, written down once.
- **`src/mmu/mod.rs`**: the `TableInTheWay` guard in `map_range`, and its `Display` arm.
- **`kmain`**: `identity_map` calls with the per-section templates inside the existing
  `HEAP.with(|h| ...)` block, before the whole-memory call.

Nothing in the descriptor encoding needs to change. That is worth noticing: the format was built
general enough that a skill three tiers later adds no code to it.

One thing does want changing. `translate` returns `Option<usize>` - an address, not the
permissions. This skill wants it to return the `Descriptor` instead, or to gain a second method
that does. Without it there is no way to ask the table what it actually granted, and section 10
needs exactly that.

## 8. Bring-up order

Each step prints something before the next one starts. Steps 1 to 4 are one sitting.

1. **One boundary, one symbol.** Add `. = ALIGN(4096); __text_end = .;` after `.text` in
   `linker.ld` only. Declare it in Rust, print it. Compare against `make syms`. The two must be
   identical, and `__text_end & 0xFFF` must be zero. This is the step that fails, and it fails at
   the start where it is cheap.
2. **The rest of the symbols.** All boundaries, all page aligned, plus `__stack_bottom`. Print
   every region as a `Region` - they already `Display` as `base: size bytes`. Check by eye that
   each starts where the previous ended, with no gaps and no overlaps, and that the whole set
   covers exactly `image()`.
3. **Predict the padding.** `make sections` before and after. The image grows by up to 4095 bytes
   per boundary. If it grew by much more or by nothing, a boundary is in the wrong place.
4. **Templates, still uniform.** Add the new `Descriptor` constants but give them all the same
   permissions `NORMAL_BLOCK` has today. Map the sections individually, before the coarse call.
   Nothing about the machine should change. This separates "my mapping calls are wrong" from "my
   permissions are wrong", and you want those two failures to arrive on different days.
5. **Watch the trap fire.** Before adding the guard, print `translate(__image_start)` after the
   coarse call. It will report a 2 mebibyte block, not a page, because step 4's work was already
   overwritten. Seeing this once is worth more than reading section 5 twice.
6. **The `TableInTheWay` guard.** Add it. Now the same boot fails with a named error naming the
   address and the level. Then narrow the coarse call to the memory above the image so it stops
   colliding, and get back to a clean boot.
7. **One real permission.** `.rodata` to `KernelReadOnly`, nothing else. Boot. If `println!` still
   works, format strings are mapped correctly. Then write to a `.rodata` address on purpose and
   confirm the data abort.
8. **The rest of the table.** All rows from section 4 at once, `.vectors` included. The failure to
   expect here is taking an exception and finding the vector table non-executable, which looks
   like a hang.
9. **The guard page.** Leave the page below `__stack_bottom` unmapped. Recurse until it faults.
   Confirm the faulting address in `FAR_EL1` is inside that page and not somewhere in `.bss`.
10. **`SCTLR_EL1.WXN`.** Last, and only once everything above is clean. If it breaks the machine,
    something is executing from writable memory and finding it is the point.

Steps 5 and 8 are where the bugs that survive live.

## 9. When nothing happens

| symptom | almost certainly |
| --- | --- |
| `MapError::BlockInTheWay` at boot | the coarse mapping ran first. Section 5. |
| permissions silently ignored, `translate` reports a block | the coarse pass overwrote your level 3 table. That is the trap in section 5, and it is why step 5 exists. |
| the kernel faults on its first instruction after the tables change | `.text` got `pxn: true`. It is one of the two regions where privileged execute must stay allowed. |
| the machine hangs the moment anything takes an exception | `.vectors` got `pxn: true`. Same mistake, different section. |
| a fault on a string literal or a `match` jump table | `.rodata` is mapped as part of `.data` and is fine, or as part of nothing and is not. Check the symbols, not the permissions. |
| everything works until the first `println!` | the `.rodata` case above, since format strings live there. |
| a data abort at an address just below the stack | the guard page working. Confirm the address, then celebrate. |
| a data abort inside the allocator | memory the buddy allocator handed out fell inside a region you mapped read-only. The fine regions must be exactly the sections, not rounded outward into free memory. |
| turning on `SCTLR_EL1.WXN` breaks the machine | something is executing from writable memory. That is the bit doing its job. |
| permissions look right in `translate` and the machine faults anyway | the translation lookaside buffer still holds the old entry. Invalidate after changing a live mapping. |

## 10. How you will know it worked

Four deliberate crimes, each of which should now be a fault with a report rather than a success:

- write to an address inside `.text`
- execute from an address inside `.data`
- write to an address inside `.rodata`
- recurse deep enough to run off the stack

Each should produce your own exception report naming a data abort or an instruction abort, with
the faulting address in the region you aimed at. Four faults, four reports, four addresses you
predicted before you ran it.

Two more that cost nothing, once `translate` hands back a descriptor:

- print `ap`, `pxn` and `uxn` for one address in each of the six rows of section 4, and check them
  against the table by eye
- print `heap: {h}` before and after, and confirm the page count dropped by the number of level 3
  tables you predicted in section 5

Then remove all four crimes and confirm `make run` still reaches the zhemon prompt, since the
interesting failure mode of this skill is locking the kernel out of something it legitimately
needs.

---

## Optional reading

- ARM Architecture Reference Manual for A-profile, section D8.4, on memory access controls. The
  AP, PXN and UXN interaction table is there, including the cases where a higher exception level
  overrides.
- `arch/arm64/mm/mmu.c` in Linux, `map_kernel_segment`, which is this skill with more segments.
- `arch/arm64/kernel/vmlinux.lds.S`, the aligned section boundaries from section 5, in production
  form.
