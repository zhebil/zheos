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

`linker.ld` lays out five sections, but the descriptor has only three opinions to give, so group
by permission rather than by name:

| segment | holds | AP | PXN | UXN |
| --- | --- | --- | --- | --- |
| executable | `.text`, `.vectors` | `KernelReadOnly` | **false** | true |
| read-only | `.rodata` | `KernelReadOnly` | true | true |
| writable | `.data`, `.bss`, stack | `KernelReadWrite` | true | true |
| devices | the MMIO window | `KernelReadWrite` | true | true |

Grouping is not a tidiness choice, it is what the hardware charges for. A boundary between two
segments has to be page aligned, and every `ALIGN(4096)` wastes up to 4095 bytes. Five sections
means five boundaries; three segments means two. Linux does the same and calls the groups
*segments*, matching the `PT_LOAD` entries of an ELF file - `_stext`/`_etext` bound one, `_sdata`
through `.bss` another.

`docs/diagrams/lockdown.tldx.jsx` draws the finished map - the image top to bottom with each
segment's permissions, the unmapped guard page, and what the descriptor field is that says each
letter. Run `tldx serve` on it.

Two rows carry the whole skill.

**`.vectors` is executable.** It is easy to miss because it is not called `.text`, but it holds the
exception vector table that `install_vectors` points `VBAR_EL1` at - **V**ector **B**ase **A**ddress
**R**egister, exception level 1 - and the processor fetches instructions from it. Give it
`pxn: true` and the machine dies at the first exception it takes, which on this kernel is
immediately. Putting it in the same segment as `.text` makes that mistake unrepresentable: it
cannot get `pxn: true` unless `.text` does too, and that kills the kernel loudly rather than
subtly.

**`.rodata` is neither writable nor executable.** Read-only and non-executable are different
properties, and this is the row most often left as merely read-only.

`uxn: true` everywhere does nothing yet. Every row uses a `Kernel*` access permission, which gives
exception level 0 no access of any kind, and memory EL0 cannot read is memory it cannot execute.
It starts mattering at the first `AllReadWrite` region, which is TWO WORLDS.

Everything not in the list stays unmapped, which is the strongest permission of all.

## 5. The three things standing in the way

None is about permissions. All three are about being able to *say* anything per-region.

### The linker script does not name the regions

`linker.ld` plants `__image_start`, `__bss_start`, `__bss_end` and `__stack_top`. That is enough to
know where the image is and nothing about what is inside it. You need a symbol at each **permission**
boundary - after the executable group, after `.rodata` - plus one naming the stack guard.

They have to be **page aligned**, because a descriptor covers 4096 bytes and cannot give the first
half of a page different permissions from the second. Today the sections abut with no alignment
between them, so `.text` and `.rodata` share a page.

`src/memory/mod.rs` already does the Rust half: `image()` declares its symbols in an
`unsafe extern` block and takes `&raw const` of each. Section 4 of `docs/bump.md` explains why that
shape and not a plain `static`. This is that technique used a second time, which is the point at
which it stops being a trick.

### The image is currently mapped in 2 mebibyte blocks

Follow what `identity_map` does with `board.memory`. The region is `0x4000_0000` to `0x4800_0000`,
128 mebibytes. `Level::size` is `1 << offset` and `offset` is `12 + 9 * (3 - level)`, so a level 1
slot covers `1 << 30` (1 gibibyte), a level 2 slot `1 << 21` (2 mebibytes), a level 3 slot
`1 << 12` (4 kibibytes). The region does not fill a gibibyte, so `map_range` descends. At level 2,
128 mebibytes is exactly 64 slots of 2 mebibytes, each aligned and each filling its slot, so every
one becomes a block and the walk stops. Level 3 is never reached.

`docs/diagrams/tables.tldx.jsx` draws that walk for `0x4008_1234`, an address inside your image.
Run `tldx serve` on it if the level-by-level descent is not yet automatic.

The kernel image lives at `0x4008_0000`, inside the first of those blocks. Giving `.text` its own
permissions means 4 kibibyte pages over that range, so this skill is the first time the walk
descends twice and the first time a level 3 table is allocated. Asking for that *after* the coarse
pass returns `MapError::BlockInTheWay` - `child_table` refusing, correctly, to split a mapping that
is already live.

So the fine regions must be mapped **before** the coarse one. That much is ordering, and it is
cheaper than implementing block splitting.

### Ordering alone silently destroys the fine mapping

This is the trap in the skill.

`child_table` guards one direction: it refuses to turn a live **block** into a table. Nothing
guards the other direction. `map_range` writes its leaf like this:

```rust
if level.is_aligned(addr) && chunk_end == slot_end {
    self.set(level.slot_of(addr), Descriptor { ... });
```

It writes into the slot without ever reading what was there.

Now walk the coarse pass after the image is mapped finely. `map_range` for
`0x4000_0000..0x4800_0000` at level 1 does not fill the gibibyte slot, so it descends into the
level 2 table you already built. At level 2 the first chunk is `0x4000_0000..0x4020_0000`: aligned,
and exactly filling the slot. The condition is true, so `set` overwrites that slot.

That slot was holding the **table** descriptor pointing at your level 3 table. It becomes a 2
mebibyte block again. Every permission you just set is gone, the level 3 table is leaked, and
nothing reports anything - `map_range` returns `Ok`.

Both halves of the fix are needed, and they do different jobs.

- **`TableInTheWay(usize, Level)`**, beside `BlockInTheWay`. `map_range` reads the slot before
  `set` and refuses if it holds a table. One subtlety decides whether it works: at level 3 the two
  bits that mean *table* higher up mean a *page*, so the check has to be "kind is `Table` **and**
  there is a level below". Test it for `Kind::from_level(level)` instead and it is inverted - silent
  at level 2 where the bug is, loud at level 3 where there is nothing to protect.
- **Stop the collision.** The guard turns the stomp into a boot failure; something still has to
  give the coarse pass a range that does not overlap. Splitting `board.memory` by hand works for
  one region and stops scaling at the second - see section 7.

Mapping the image at page granularity costs one level 3 table per 2 mebibyte block it touches, so
`heap: N of 32768 pages free` drops by a few pages at boot. Predict roughly how many before you see
it.

## 6. The guard page

A page below the stack, left unmapped. A stack that runs off the end touches it and faults, instead
of silently eating `.bss`.

**It works by absence, not by permissions.** With no descriptor the walk fails before the access
permissions are ever consulted, so the report says *translation* fault rather than *permission*
fault, and the two stay distinguishable. Setting `pxn`/`uxn` on the page instead would leave it
mapped and writable, and the overflow would run straight through it.

The stack grows **down** from `__stack_top`, so the guard goes below `__stack_bottom`, between
`.bss` and the stack. `linker.ld` needs `ALIGN(4096)` there - the stack base was only 16-byte
aligned - and a `__stack_guard_start` symbol to name it.

That puts a hole in the middle of the writable segment, so it maps as two calls with the same
template rather than one.

One page is enough only while no single stack frame is larger than a page. A prologue allocates its
whole frame with one `sub sp, sp, #N` and then writes at offsets from the new `sp`, so a function
with an 8 kibibyte local array steps clean over a 4 kibibyte guard and lands in `.bss` with no
fault at all. The real fix is stack probing - the compiler touching each page as it walks `sp` down,
which is what `-fstack-clash-protection` does - not a bigger guard. Until then the rule is: large
buffers go in `.bss` or on the heap, never on the kernel stack.

## 7. What you are building

`linker.ld` and `src/memory/mod.rs` are the obvious half: page-aligned boundaries at the two
permission edges plus the guard, and functions beside `image()` reading them into `Region`s.
`src/mmu/descriptor.rs` gets three constants beside `NORMAL_BLOCK` - the same shape with different
`ap`, `pxn` and `uxn`, the table from section 4 written down once. Nothing in the descriptor
encoding changes, which is worth noticing: the format was built general enough that a skill three
tiers later adds no bits to it.

The interesting half is what `kmain` does with them. The naive version is one `identity_map` call
per region and a hand-split `board.memory` around the image. That works, and then the device tree
also wants read-only, so `board.memory` splits a second time, and every future region splits it
again. The splitting is the part that does not scale, and it is exactly the sort of arithmetic that
goes wrong quietly.

Describe the memory instead:

- **`src/mmu/policy.rs`**: a `Mapping` - a name, a `Region`, and an `Option<Descriptor>`. `None`
  means *deliberately unmapped*, which is how the guard page is expressed.
- **`Table::apply`**: takes an arena, a slice of `Mapping`, and a `fill` template. Maps every
  `Some`, then maps everything the plan did not claim with `fill`.
- **`src/memory/gaps.rs`**: given an arena and an iterator of taken regions, yield the parts nothing
  covers. This is `MemoryMap::unreserved` generalised - the cursor walk already existed for
  reservations, so lift it out rather than writing it twice.

Three things about that design earn their keep:

**`None` is load-bearing.** The guard page has to appear in the plan. Leave it out and it is not a
claimed region, so the fill pass sweeps it up as ordinary memory and the guard is silently gone.

**The complement comes from the plan, never from the reservation list.** They look
interchangeable and are not. `Pages::new` reserves the frame allocator's own bookkeeping, and the
page tables sit in reserved memory too - both are read and written after the MMU is on. Reserved
means *do not hand this out as a free frame*; it does not mean *do not map this*.

**Devices stop being special.** An empty plan yields one gap covering the whole arena, so the MMIO
window is the same call with a different `fill`.

Two smaller things fall out. `identity_map` becomes private, since every mapping now goes through a
`Mapping` and the compiler can say so. And a plan wants an overlap check: two regions sharing a page
is invisible otherwise, because `TableInTheWay` only fires on table-versus-block and the second
mapping just replaces the first one's permissions.

One thing does want changing regardless. `translate` returns `Option<usize>` - an address. Under an
identity map that is the same number whether it came from a 2 mebibyte block or a 4 kibibyte page,
so it cannot show you what you need to see. It wants a second method returning the level as well.

## 8. Bring-up order

Each step prints something before the next one starts.

1. **One boundary, one symbol.** Add `. = ALIGN(4096); __text_end = .;` after `.text` in
   `linker.ld` only. Declare it in Rust, print it. Compare against `make syms`. The two must be
   identical, and `__text_end & 0xFFF` must be zero. This is the step that fails, and it fails at
   the start where it is cheap.
2. **The rest of the symbols.** Both permission boundaries, page aligned, plus the guard. Print
   every region - they `Display` as `base: size bytes`. Each must start where the previous ended,
   with no gaps and no overlaps, and the set must tile `image()` exactly. **`image()` itself must
   still span `__image_start` to `__stack_top`**: it backs `map.reserve(image())`, and shortening
   it hands `.rodata` and the stack to the page allocator, which overwrites your string literals
   and prints garbage.
3. **Predict the padding.** `make sections` before and after. Grouped by permission the image
   should barely grow - `.vectors` moves into padding `.text` was already paying for. If it grew by
   a lot, the boundaries are still per-section.
4. **Templates, still uniform.** Add the three `Descriptor` constants but give them all the
   permissions `NORMAL_BLOCK` has today. Map the three segments before the coarse call. Nothing
   about the machine should change, and the heap should drop by one page: the first level 3 table.
   This separates "my mapping calls are wrong" from "my permissions are wrong", and you want those
   two failures on different days.
5. **Watch the trap fire.** Print the level for `image().base` after the segments and again after
   the coarse call. `Level3`, then `Level2` - same address both times, which is why this bug
   survives. Seeing it once is worth more than reading section 5 twice.
6. **The `TableInTheWay` guard.** Add it. The same boot now fails with a named error giving the
   address and the level. Prove it fires before trusting it: with the guard in and the coarse call
   un-narrowed, the boot **must** fail. Then split the coarse call around the image and get back to
   `Level3` twice.
7. **One real permission.** `.rodata` to `KernelReadOnly`, nothing else. Boot. If `println!` still
   works, format strings are being read through a read-only mapping - that is the check, there is
   nothing else to look at. Then write to a `.rodata` address on purpose and confirm the data abort.
8. **The rest of the table.** `ap` on the executable segment, `pxn` on the other two. Two probes:
   a write to `.text` is a data abort, a jump into the stack is an instruction abort. That either
   one *reports* is the other half of the check - the handler runs out of `.vectors`, which is now
   read-only.
9. **The guard page.** Check `translate` returns `None` for it and `Some` either side, which needs
   no fault at all. Then recurse until it faults and confirm the address is inside the guard and the
   fault is *translation*, not permission.
10. **`SCTLR_EL1.WXN`.** Last, and only once everything above is clean. If it breaks the machine,
    something is executing from writable memory and finding it is the point.

The plan-and-fill rewrite of section 7 goes after step 9, once the regions are known good. Doing it
first means debugging the complement and the permissions at the same time.

Steps 5 and 8 are where the bugs that survive live.

## 9. When nothing happens

| symptom | almost certainly |
| --- | --- |
| garbage output, words with letters missing | `image()` was shortened to one section, so the page allocator is handing out `.rodata` and writing over the string literals. Step 2. |
| `MapError::BlockInTheWay` at boot | the coarse mapping ran first. Section 5. |
| permissions silently ignored, `translate` reports a block | the coarse pass overwrote your level 3 table. That is the trap in section 5, and it is why step 5 exists. |
| `TableInTheWay` firing on a page the plan owns | the guard is missing its "and there is a level below" clause, so it fires at level 3 where `Kind::Table` means a page. |
| the kernel faults on its first instruction after the tables change | the executable segment got `pxn: true`. |
| the machine hangs the moment anything takes an exception | `.vectors` is outside the executable segment and got `pxn: true`. Same mistake, different section. |
| everything works until the first `println!` | `.rodata` is mapped as part of nothing. Check the symbols, not the permissions - format strings live there. |
| a data abort just below the stack, *translation* | the guard page working. Confirm the address, then celebrate. |
| a data abort just below the stack, *permission* | not the guard - the guard is unmapped, so it cannot raise a permission fault. Something mapped it. |
| a data abort inside the allocator | memory the buddy allocator handed out fell inside a region you mapped read-only. The regions must be exactly the segments, not rounded outward into free memory. |
| the first heap call after `mmu::enable` faults | the fill pass was driven off the reservation list instead of the plan, so the allocator's own bookkeeping is unmapped. Section 7. |
| turning on `SCTLR_EL1.WXN` breaks the machine | something is executing from writable memory. That is the bit doing its job. |
| permissions look right in `translate` and the machine faults anyway | the translation lookaside buffer still holds the old entry. Invalidate after changing a live mapping. |
| a boot that prints nothing when you expected a fault report | the pipeline buffered it. A kernel that ends in a halt loop may not flush through `make feed | grep`; redirect QEMU's serial to a file instead. |

## 10. How you will know it worked

Four deliberate crimes, each of which should now be a fault with a report rather than a success:

- write to an address inside `.text` - **data** abort, `Access: write`, permission, level 3
- jump to an address inside the stack - **instruction** abort, permission, level 3, and no
  `Access:` line at all, because fetching is not a read
- write to an address inside `.rodata` - data abort, permission
- recurse off the end of the stack - data abort, **translation**, at the first byte of the guard

Each names the address you aimed at in `FAR_EL1`. Four crimes, four reports, four addresses you
predicted before running them. The instruction abort is the one worth staring at: `FAR_EL1` and
`ELR_EL1` hold the same value, because the CPU faulted fetching the instruction it was about to
run, so the faulting address and the return address coincide.

That any of them *reports* rather than hanging is a second result for free: the handler fetches from
`.vectors`, so a report coming back rather than a fault loop at `PC=0x200` proves the vector table
is still executable.

Two more that cost nothing, once `translate` hands back the level:

- print it for every entry in the plan. Every mapped region reports `Level3`; the guard page reports
  `None`. One loop over the plan, and it stays correct as the plan grows.
- print `heap: {h}` before and after, and confirm the page count dropped by the number of level 3
  tables you predicted in section 5

Then remove all four crimes and confirm `make run` still reaches the zhemon prompt, since the
interesting failure mode of this skill is locking the kernel out of something it legitimately needs.

---

## Optional reading

- ARM Architecture Reference Manual for A-profile, section D8.4, on memory access controls. The
  AP, PXN and UXN interaction table is there, including the cases where a higher exception level
  overrides.
- `arch/arm64/mm/mmu.c` in Linux, `map_kernel_segment`, which is this skill with more segments.
- `arch/arm64/kernel/vmlinux.lds.S`, the aligned section boundaries from section 5, in production
  form.
