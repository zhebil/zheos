# LOCKDOWN - permissions that mean something

## 1. What this is

Right now the kernel can write to its own instructions and execute its own stack. Not by accident
- your descriptors say so. `Descriptor::NORMAL_BLOCK` in `src/mmu/descriptor.rs:54` sets
`AccessPermissions::KernelReadWrite` and leaves `pxn: false`, applied uniformly to all 128
mebibytes of memory. Every byte of memory is readable, writable, and executable by the kernel.

LOCKDOWN gives each region of the image the permissions it should have had: instructions
executable and not writable, constants readable and neither, data writable and not executable,
stack the same.

The category is **hardware format plus a policy choice**. The bits are fixed by the architecture
and you have already implemented all of them - `ap`, `pxn` and `uxn` are fields on your
`Descriptor` today and are encoded correctly. Nothing new gets decoded. What is new is deciding
which region gets which, and getting the translation table to be fine-grained enough to say so.

## 2. Why this matters more than it looks

Two reasons, and the second is the one that makes it a prerequisite rather than a nicety.

**It turns silent corruption into a fault at the instruction that caused it.** A stack overflow
today runs down into `.bss` and keeps going, and the first symptom is unrelated output going wrong
much later. With a guard page it is a data abort with the faulting address in `FAR_EL1`, reported
by the handler you already wrote.

**Exception level 0 is meaningless without it.** TWO WORLDS depends on this skill, and the
dependency is hard. Dropping to exception level 0 while every descriptor says
`AccessPermissions::KernelReadWrite` gives you a "user" mode that can still read and write every
byte of the kernel. That is a demonstration of `eret`, not a privilege boundary. This skill is
what makes the boundary real.

## 3. The bits, all of which you already have

Three fields on the descriptor, from `src/mmu/descriptor.rs`:

**AP\[2:1]**, **A**ccess **P**ermissions, two bits at position 6. Your `AccessPermissions` enum
already spells out all four encodings:

| encoding | exception level 1 | exception level 0 |
| --- | --- | --- |
| `0b00` | read, write | none |
| `0b01` | read, write | read, write |
| `0b10` | read only | none |
| `0b11` | read only | read only |

Note what is not in that table: execute. Execution is controlled separately, which surprises
people every time.

**PXN**, **P**rivileged e**X**ecute **N**ever, bit 53. Set it and exception level 1 - the kernel -
cannot fetch instructions from this memory.

**UXN**, **U**nprivileged e**X**ecute **N**ever, bit 54. The same for exception level 0.

`NORMAL_BLOCK` today sets `uxn: true` and `pxn: false`, which is correct for `.text` and wrong for
everything else.

One system register bit is worth knowing about: **SCTLR_EL1.WXN**, **W**rite implies e**X**ecute
**N**ever, bit 19. Set it and any writable region becomes non-executable regardless of what the
descriptor says. It is a belt-and-braces switch that makes write-xor-execute a property of the
machine rather than of your table building. Turning it on after the table is correct is a good way
to prove the table is correct: if anything breaks, something was relying on writable and
executable memory.

## 4. What the kernel should get

Four regions, four answers:

| region | AP | PXN | UXN |
| --- | --- | --- | --- |
| `.text` | kernel read only | false | true |
| `.rodata` | kernel read only | **true** | true |
| `.data`, `.bss` | kernel read write | true | true |
| stack | kernel read write | true | true |
| devices | kernel read write | true | true |

Read the `.rodata` row again. Read-only and non-executable are different properties, and constants
should be neither writable nor executable. It is the row most often left as merely read-only.

Everything not in the list stays unmapped, which is already true and is the strongest permission
of all.

## 5. The two things standing in the way

Neither is about permissions. Both are about being able to *say* anything per-region.

### The linker script does not name the regions

`linker.ld` plants `__image_start`, `__bss_start`, `__bss_end` and `__stack_top`. That is enough
to know where the image is and nothing about what is inside it. You need a symbol at each section
boundary: the start and end of `.text`, of `.rodata`, of `.data`.

And they have to be **page aligned**, because a descriptor covers 4096 bytes and cannot give the
first half of a page different permissions from the second. Today the sections abut with no
alignment between them, so `.text` and `.rodata` almost certainly share a page. Each boundary needs
an `ALIGN(4096)`, which costs up to 4095 bytes of padding per boundary and is the entire price of
this skill in memory.

`BUMP` section 4 already covered getting a linker symbol into Rust, and the same
`unsafe extern` and `&raw const` pattern applies unchanged. This is that technique used a second
time, which is the point at which it stops being a trick.

### The image is currently mapped in 2 mebibyte blocks

Follow what `identity_map` actually does with `board.memory`. The region is `0x4000_0000` to
`0x4800_0000`, 128 mebibytes. At level 1 a slot covers 1 gibibyte, the region does not fill one, so
`map_range` descends. At level 2 a slot covers 2 mebibytes, and 128 mebibytes is exactly 64 of
them, so every one is a block and the walk stops. Memory is mapped in 2 mebibyte blocks.

The kernel image lives at `0x4008_0000`, inside the first of those blocks. To give `.text` its own
permissions you need 4 kibibyte pages over that range, and asking for them now returns
`MapError::BlockInTheWay` - which is `src/mmu/mod.rs:132` refusing, correctly, to rewrite a mapping
that is already live.

So `identity_map` has to be called with the fine regions **before** the coarse one, not after.
Map `.text`, `.rodata`, `.data`, `.bss` and the stack first, each with its own template, then map
whatever is left of memory as normal blocks. Ordering is the whole fix, and it is cheaper than
implementing block splitting.

That ordering has one consequence worth predicting: mapping the image at page granularity means a
level 3 table for each 2 mebibyte block it touches, so the table count goes up and `Bump` gets
asked for more pages at boot. `bump.remaining()` will drop by a measurable amount, and you should
know roughly what to expect before you see it.

## 6. The guard page

A page below the stack, mapped as invalid. A stack that runs off the end touches it and takes a
data abort with the faulting address one page below `__stack_top - 0x8000`, instead of silently
eating `.bss`.

This is the cheapest thing in the skill - one region left out of the map - and it is the one that
will save you the most debugging time, because stack overflow is the failure that looks like
everything else.

## 7. What you are building

No new module. Changes in three places:

- `linker.ld`: `ALIGN(4096)` at each section boundary, and a symbol pair per section.
- A small function that reads those symbols into `Region`s, next to `bump::image()` which already
  does exactly this for the whole image.
- `kmain`: a handful of `identity_map` calls with different templates, in the order section 5
  requires, before the current whole-memory call.
- New `Descriptor` constants beside `NORMAL_BLOCK` and `DEVICE_BLOCK`. They are the same shape with
  different `ap`, `pxn` and `uxn`, and they are the whole policy from section 4 written down once.

Nothing in `src/mmu/` needs to change. That is worth noticing: the descriptor encoding was built
general enough that a skill three tiers later adds no code to it.

## 8. Testing it

The table-level tests are host tests and go in `src/mmu/`, where `translate` already gives you a
walker to check against. But `translate` returns an address and not the permissions, so this skill
probably wants it to return the descriptor instead, or a second method that does.

- A region mapped read-only comes back with `AccessPermissions::KernelReadOnly` at every address
  inside it, including the last byte.
- Adjacent regions with different templates do not bleed into each other at the boundary. Check
  the last address of one and the first of the next.
- A page-granularity mapping inside a range that is later mapped coarsely keeps its own
  permissions, which is the ordering rule from section 5 turned into a test.
- The guard page translates to nothing.

## 9. When nothing happens

| symptom | almost certainly |
| --- | --- |
| `MapError::BlockInTheWay` at boot | the coarse mapping ran first. Section 5. |
| the kernel faults on its first instruction after the tables change | `.text` got `pxn: true`. It is the one region where privileged execute must stay allowed. |
| a fault on a string literal or a `match` jump table | `.rodata` got mapped as part of `.data` and is fine, or as part of nothing and is not. Check the symbols, not the permissions. |
| everything works until the first `println!` | the `.rodata` case above, since format strings live there. |
| a data abort at an address just below the stack | the guard page working. Confirm the address, then celebrate. |
| a data abort inside the allocator after LOCKDOWN | memory that FRAMES hands out fell inside a region you mapped read-only. The fine regions must be exactly the sections, not rounded outward into free memory. |
| turning on `SCTLR_EL1.WXN` breaks the machine | something is executing from writable memory. That is the bit doing its job, and finding what is the point. |
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

Then remove all four and confirm `make run` still reaches the monitor prompt, since the
interesting failure mode of this skill is locking the kernel out of something it legitimately
needs.

---

## Optional reading

- ARM Architecture Reference Manual for A-profile, section D8.4, on memory access controls. The
  AP, PXN and UXN interaction table is there, including the cases where a higher exception level
  overrides.
- `arch/arm64/mm/mmu.c` in Linux, `map_kernel_segment`, which is this skill with more segments.
- `arch/arm64/kernel/vmlinux.lds.S`, the aligned section boundaries in section 5, in production
  form.
