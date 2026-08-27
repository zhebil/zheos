# BUMP - a pointer that only goes up, and the ranges it steps over

The machine has 128 MiB of RAM. The kernel occupies the first 53 KiB of it. The other 127.9
MiB has no name, no owner, and nothing that can hand any of it out. Every byte the kernel has
used so far was placed by the linker before boot: a global, a buffer, the stack. Nothing has
ever been decided at runtime.

An allocator is the thing that decides at runtime. This one is close to the simplest that
exists: a single pointer to the next free byte, moved forward on every request, never moved
back. There is no free. There is no reuse. Memory handed out is gone until reboot.

That is not a toy. It is what real kernels use for the first few seconds of their life, because
the first thing you need memory for is the page tables that make a real allocator possible, and
you cannot use a real allocator to build them. Linux's version is `memblock`, and this skill is
deliberately shaped like it - not because the shape is required at 128 MiB, but because the
shape is the lesson.

Sections 3 and 4 are the ones that decide whether this works. Section 3 is the memblock model
and what parts of it are worth keeping. Section 4 is the one new Rust technique in the whole
skill.

---

## 1. Vocabulary

**Allocator** - anything that hands out memory on request. It answers one question: "give me
`n` bytes I can use, aligned to `a`". Everything else is refinement.

**Arena** - the block of memory an allocator hands out of. A start, an end, and nothing else.

**Bump pointer** - the one piece of state. It starts at the beginning of the arena, and each
allocation moves it forward past the bytes just given away. Also called a pointer-bump or a
linear allocator.

**Reservation** - a range inside the arena that is already spoken for and must never be handed
out. The kernel image is one. The device tree blob is another. A list of these is what turns a
naive bump pointer into something that survives contact with a real machine.

**Alignment** - a requirement that an address be a multiple of some power of two. `align_of::<u64>()`
is 8; a page table needs 4096. Alignment is not a preference, it is a correctness requirement
the CPU enforces.

**`align_up`** - round an address forward to the next multiple of an alignment. The one piece
of arithmetic in this skill that is easy to get subtly wrong.

**Layout** - `core::alloc::Layout`, a (size, alignment) pair that has already checked that the
alignment is a power of two and that the size does not overflow when rounded up. It is in
`core`, not in `alloc`, so you can use it with no allocator at all. Which is convenient,
because you are writing the allocator.

**Linker symbol** - a name the linker script plants at an address, like `__stack_top`. It has
an address and no value. Getting that distinction wrong is the classic bug of this skill and
section 4 is entirely about it.

**Physical address** - the number the CPU puts on the bus. Right now, with the MMU off, every
address in this kernel is physical. That changes exactly once, and this allocator is what makes
the change possible.

**`memblock`** - Linux's boot-time allocator, `mm/memblock.c`. Two lists of ranges and a search
between them. Section 3.

**`GlobalAlloc`** - the Rust trait that makes `Box`, `Vec` and `String` work. Not this skill.
That is HEAP (`zheos-9ka`), and it sits on top of what you build here.

---

## 2. Where you are now

Two facts already exist in the kernel and this skill is the join between them.

The linker knows where the image ends. `make syms` prints it:

```
0000000040004f48 D __bss_start
0000000040005390 B __bss_end
000000004000d390 B __stack_top
```

The device tree knows where RAM ends. `kmain` already prints it:

```rust
println!("{:#010x} {:x} bytes", board.memory.base, board.memory.size);
```

which at `-m 128M` is `0x40000000` and `0x8000000`, so RAM is `[0x4000_0000, 0x4800_0000)`.

```
0x4000_0000 ┌──────────────────┐  memory.base      ← arena starts here
            │ .text .rodata    │
            │ .data .bss       │  the image, 20 KiB   ┐
0x4000_5390 ├──────────────────┤  __bss_end           ├ RESERVED
            │ stack (grows ↓)  │  32 KiB              │
0x4000_d390 ├──────────────────┤  __stack_top         ┘
            │                  │
            │   free RAM       │  ~112 MiB
            │                  │
0x4700_0000 ├──────────────────┤  DTB_BASE            ┐
            │ device tree blob │  1 MiB               ├ RESERVED
0x4710_0000 ├──────────────────┤                      ┘
            │   free RAM       │  ~15 MiB
0x4800_0000 └──────────────────┘  memory.base + memory.size
```

The arena is **all of RAM**. The two blocks that are already taken are reservations, not
boundaries. That is the whole design decision of this skill and section 3 is why.

The stack is inside the first reservation, and it grows **down**, away from the free space. If
it instead grew up you would be handing out the return addresses of the function doing the
handing out. It is 32 KiB with no guard page - a deep enough recursion walks down into `.bss`
and corrupts a global, silently. Not this skill's problem, but the number `0x8000` in
`linker.ld` is a guess someone made once.

---

## 3. The memblock model

Linux does not have a hole to deal with. It has a **second list**, and every hole - the DTB,
the kernel image, the initrd, firmware regions - is just an entry in it.

### What memblock actually is

Two lists of `(base, size, flags)` ranges:

- **`memblock.memory`** - what is physically RAM at all, built from the DTB's `/memory` nodes
  by `memblock_add()`.
- **`memblock.reserved`** - the sub-ranges of that RAM already spoken for.

Allocation is "find a gap in `memory` that does not intersect `reserved`, then add the gap you
picked to `reserved`." `memblock_alloc_range_nid()` walks both lists in lockstep via
`for_each_free_mem_range_reverse()` - top-down by default - and calls `memblock_reserve()` on
whatever it takes.

So the DTB is not skipped by a special case. It is reserved by the same call that reserves
everything else, and the allocator never knew there was anything special about it. That is the
idea worth stealing.

The blob reserves itself, using its own header field:

```c
/* drivers/of/fdt.c, early_init_fdt_reserve_self() */
memblock_reserve(__pa(initial_boot_params),
                 fdt_totalsize(initial_boot_params));
```

`fdt_totalsize` is the same `totalsize` you can read at offset 4 of the blob. On your padded
`virt.dtb` that is `0x0010_0000` - QEMU's `dumpdtb` pads the file to 1 MiB and writes the padded
length into the header:

```
$ xxd -l 8 virt.dtb
00000000: d00d feed 0010 0000
          ^magic    ^totalsize = 0x0010_0000
```

So Linux would reserve the full megabyte here too, not the ~8 KiB the tree actually needs. Your
`Dtb` already parses this field.

### The boot order, which resolves the chicken and egg

arm64's sequence, and it is the same three constraints you have:

1. `setup_machine_fdt()` maps the blob at a fixed virtual address through the fixmap, using
   statically allocated page tables. No allocator needed.
2. `early_init_dt_scan()` reads `/memory` and calls `memblock_add()` per range. Now memblock
   knows what RAM is.
3. `early_init_fdt_reserve_self()` - the blob protects itself.
4. `early_init_fdt_scan_reserved_mem()` - the `/memreserve/` header entries and the
   `/reserved-memory` node.
5. The kernel image, `_text` to `_end`.
6. Only now does anything allocate.

Parse before you can allocate, reserve before you allocate, and the parse needs no allocator
because the format is read-in-place. Your `kmain` already does steps 1 and 2 in a different
form; steps 3, 5 and 6 are this skill.

### The part of memblock that really is a bump allocator

The region arrays have to live somewhere before there is an allocator, so they start as fixed
static arrays in `.init` data - `INIT_MEMBLOCK_REGIONS` is 128 entries. When one fills up,
`memblock_double_array()` allocates a bigger array *out of memblock itself*, copies over, and
reserves the new one. The abandoned static array is in `.init` and gets freed wholesale later.

That is the honest answer to "how does an allocator allocate its own metadata": it does not,
until it can, and the bootstrap case is a fixed array someone sized by guessing.

You are going to copy the fixed array and skip the doubling. That is not a shortcut around
Linux's design, it *is* Linux's design, minus the escape hatch you will never hit.

### What to keep and what to drop

| memblock has | you build | why |
| --- | --- | --- |
| `memory` list, N ranges | one `Region` | `virt` has one `/memory` node with one `reg` pair. A list of one is a list. |
| `reserved` list, N ranges | `[Region; 8]` + a length | This is the mechanism. Two entries today, and the second one is the entire point of the skill. |
| dynamic array growth | fixed array, error when full | `memblock_double_array` needs the allocator to allocate its own metadata. Two reservations, eight slots. |
| sorted, merged regions | unsorted, unmerged | Sorting buys an early exit in a loop that runs twice. Section 7 shows why unsorted still terminates. |
| top-down first-fit | forward-only bump | Top-down exists to keep low memory free for DMA. There is no DMA. |
| `memblock_free` | nothing | Free is the next allocator's job. |
| flags: `NOMAP`, `MIRROR`, `HOTPLUG` | nothing | All mean "this RAM is special in a way `virt` has no way to be". |
| NUMA nodes | nothing | One node. |
| handoff to the buddy allocator | nothing yet | `zheos-9ka`. |

The rows you keep are the two lists and the search between them. Everything dropped is a
response to a machine you do not have.

### Two things this buys immediately

**The image stops being a boundary.** If the arena started at `__stack_top` you would be
encoding "the image is at the bottom" into the arena's start, which is true by accident of
`linker.ld` and not by anything the machine says. Reserving it instead means the fact lives in
one `reserve()` call where a reader can see it.

**The skip loop is exercised on allocation number one.** `next` starts at `memory.base`,
immediately hits the image reservation, and jumps to `__stack_top`. You cannot ship a broken
skip loop without noticing on the first print. A design where the interesting path only runs
after 112 MiB of allocation would hide the bug until TABLES.

---

## 4. Getting a linker symbol into Rust

`kernel.s` reads `__stack_top` in one instruction and no ceremony:

```asm
ldr     x0, =__stack_top
```

Rust needs four lines and one idea. The idea is the whole section.

**A linker symbol has an address and no value.** `__stack_top = .` in `linker.ld` does not
create a variable holding `0x4000_d390`. It creates a *name for the address* `0x4000_d390`.
There are no bytes there that belong to it. Whatever is at that address is unrelated memory -
in this case the first byte past your reservation, which is uninitialised.

So the number you want is the address of the symbol, never its contents. In assembly the
distinction is visible in the syntax: `ldr x0, =__stack_top` loads the address, `ldr x0,
__stack_top` loads what is there, and they are different instructions. In Rust the same
distinction is `&raw const X` versus `X`, and both compile.

The declaration, edition 2024:

```rust
unsafe extern "C" {
    static __stack_top: u8;
}
```

`unsafe extern` is required in edition 2024 - you are promising this symbol exists, and the
compiler cannot check that. The type is `u8` and it is a lie, deliberately: there is no value,
so the type is meaningless, and `u8` is the smallest lie available. Some codebases write
`static __stack_top: [u8; 0];` to make the lie explicit. Either works.

Then take the address:

```rust
let image_end = &raw const __stack_top as usize;
```

`&raw const` builds a pointer without forming a reference and without reading anything, so this
compiles with no `unsafe` block even though the static is `extern`. Verified with rustc 1.97 on
`aarch64-unknown-none`.

What you must not write is `unsafe { __stack_top }`. That reads a byte of RAM and gives you a
number somewhere in 0..255. It compiles, it runs, it produces a plausible-looking small number,
and your image reservation becomes 200 bytes long instead of 53 KiB. Section 11 has the
symptom.

### The other end of the image

The reservation is `[image_start, image_end)`, and `image_end` is `__stack_top`. For
`image_start`, `linker.ld` sets `. = 0x40000000` and `memory.base` is also `0x4000_0000`, so
using `memory.base` works - by coincidence. Linux does not rely on the coincidence; it uses
`_text` and `_end`, two symbols the linker script plants.

One line at the top of `linker.ld`, next to the existing `. = 0x40000000`, gives you the same
thing:

```
__image_start = .;
```

Worth it. It costs nothing and it turns an assumption into a fact.

---

## 5. Alignment, and why bumping is not `p += n`

If every allocation were byte-aligned this would be two lines. It is not, for two reasons that
are both real on this machine.

**The MMU is off, so all memory is Device memory.** Device memory forbids unaligned access
entirely. An 8-byte load from a 4-byte-aligned address raises an Alignment fault, ESR DFSC
`0x21`. This is already in `CLAUDE.md` because you have hit it once. Hand out a misaligned
address and the fault happens at the *use*, not at the allocation, which is a long way from the
bug.

**Page tables need 4096-byte alignment.** This is the actual customer. A translation table on
arm64 must start on a 4 KiB boundary because the low 12 bits of the table's address are not
stored in the descriptor that points at it - they are assumed zero. Ask for 4096-aligned memory
and get a 16-aligned address, and the hardware silently walks a table 4 KiB away from the one
you filled in. TABLES (`zheos-rel`) is the next skill and this is the requirement it brings.

The arithmetic:

```rust
fn align_up(address: usize, align: usize) -> usize {
    (address + align - 1) & !(align - 1)
}
```

Three things about it.

It only works if `align` is a power of two, because `!(align - 1)` is a mask of the high bits
only when `align - 1` is a run of low ones. For `align = 3` this returns garbage with no
complaint. `Layout` has already checked this for you, which is most of the reason to use
`Layout`.

`address + align - 1` can overflow at the top of the address space and wrap to a small number,
which turns "out of memory" into "here is address 8". Not reachable at `0x4800_0000`, very
reachable the moment something passes `usize::MAX`. `checked_add` turns it into a `None` you
were going to return anyway.

The order matters, and it matters more now that reservations exist. Align first, then check,
then add the size. Section 7 spells out the exact loop.

`usize::next_multiple_of` in `core` does the same thing with a division, and is correct for
non-powers-of-two as well, which you do not need. Writing the mask yourself is worth it once,
in a kernel, because the mask is the thing you want to recognise when you read someone else's
allocator.

---

## 6. Where the state lives

The only design question here is a Rust question rather than a hardware one. Mutable state that
outlives one function is the thing Rust is most opinionated about, and a kernel has nowhere to
put it except a global.

**`static mut`.** Every access is `unsafe`, and edition 2024 makes taking a reference to one a
hard error (`static_mut_refs`). Workable with raw pointers, unpleasant, and the unpleasantness
is the compiler correctly pointing out that you have not said how concurrent access is
prevented.

**`AtomicUsize`.** One `fetch_update` closure does the whole allocation, no `unsafe`, and it
stays correct the day MANY CORES (`zheos-5x5`) wakes a second CPU. It also stops working the
moment there is a reservation array to consult, because the skip loop is not a single word of
state.

**A plain struct, owned by `kmain`.** No global, no atomics, no `unsafe`, `&mut self` on
`alloc` and `reserve`, and the borrow checker enforces exclusivity for free. Whoever needs to
allocate takes `&mut Bump` as an argument.

Take the third. It is the least machinery that works, it is trivially testable, and the code
that needs it - TABLES - is called from `kmain` anyway, so passing it down is one parameter.

The global is not avoidable forever: `GlobalAlloc` is a trait implemented on a `static`, so
HEAP has no choice. That is HEAP's problem, and by then you will know whether you want a
spinlock, because you will know whether interrupts allocate.

Skipped: the global. Add when `zheos-9ka` needs `GlobalAlloc`.

---

## 7. The shape of it

```rust
pub struct Bump {
    next: usize,
    end: usize,
    reserved: [Region; MAX_RESERVED],
    reserved_len: usize,
}

impl Bump {
    pub fn new(memory: Region) -> Bump;
    pub fn reserve(&mut self, region: Region) -> Result<(), Full>;
    pub fn alloc(&mut self, layout: Layout) -> Option<NonNull<u8>>;
    pub fn remaining(&self) -> usize;
}
```

### The allocation loop

This is the whole algorithm and it is worth writing out before writing it:

1. `start = align_up(next, layout.align())`
2. `finish = start + layout.size()`
3. if any reservation intersects `[start, finish)`, set `next = that reservation's end` and go
   back to 1
4. if `finish > end`, return `None` - **without touching `next`**
5. `next = finish`, return `start`

Step 3 is the memblock gap-walk, degenerate to forward-only. Note that it re-aligns after
jumping: a reservation can end at any address, and the caller asked for 4096.

**It terminates without sorting the reservations.** Intersection means the reservation's end is
strictly greater than `start`, which is at least `next`, so every retry moves `next` strictly
forward. There are finitely many reservations and each can be jumped over at most once. Bound
the loop by `reserved_len + 1` iterations anyway and return `None` if it ever runs out - a loop
that cannot terminate is better than a loop you believe cannot terminate.

Sorting and merging is what memblock does instead, and it is what you would add if the array
had thirty entries rather than two.

**Step 4's "without touching `next`" is the invariant that hides.** The natural way to write
this is to bump the pointer and then check the end, which loses the alignment padding and every
skipped reservation on each failed allocation. Compute into locals, check, then store.

### Notes on each signature

**`new` takes the whole `Region`, not a start and end.** The arena is all of RAM. Everything
that is not RAM-you-can-use is a `reserve()` call, and putting it that way means every claim on
memory is stated in the same vocabulary at the same place in `kmain`.

**`Region` already exists** in `src/dtb/mod.rs`, `pub struct Region { base, size }`. `board.rs`
already imports it. Reuse it rather than inventing a second pair of `usize`s; it will need
`Clone, Copy` derived to sit in an array.

**Zero-size regions are harmless.** A `Region { base: 0, size: 0 }` never intersects anything,
because intersection needs `res.base + res.size > start` and `0 > start` is false for any real
address. So the array can be initialised with zeros and scanned in full, and `reserved_len`
exists only so `reserve()` knows where to write and when to fail.

**`reserve` returns `Result`, and the caller must handle it.** A dropped reservation causes
exactly the corruption this whole design exists to prevent, so it cannot be silent. It also
cannot panic - `1f5df33 Remove every panic from the kernel` was deliberate. `kmain` already has
the pattern for this: `Board::discover` returns a `Result`, and the failure path prints and
calls `halt()`.

**`alloc` returns `Option`, not `Result`, not a panic.** Out of memory is a condition a kernel
handles, not a bug. The caller that cannot proceed can fail itself, with a message about what
it wanted, which is a better message than the allocator can write.

**`NonNull<u8>`, not `*mut u8` or `usize`.** The null-pointer niche makes `Option<NonNull<u8>>`
the same size as a pointer, so the `Option` is free. It also documents that success means a
real address. Returning `usize` is simpler to print and throws away pointer provenance, which
does not matter today with the MMU off and does matter to Miri and to future-you.

**`remaining()` is `end - next` and is an upper bound.** Reservations ahead of `next` are not
subtracted. Say so in the doc comment; it is one line versus a scan, and the only consumer is
exhaustion.

**`MAX_RESERVED = 8`.** Two are used. `INIT_MEMBLOCK_REGIONS` is 128 for the same reason and
with the same justification, which is none.

**No `free`.** It is the definition of the allocator, not an omission. State it in the doc
comment so it reads as a choice.

**No zeroing.** Freshly bumped memory contains whatever was there. Zeroing every allocation
costs a `write_bytes` over the whole block; not zeroing means page tables built on this
allocator must zero themselves or the CPU will walk into random descriptors. Do not zero, and
say so loudly enough in the doc comment that TABLES cannot claim it was not warned.

### What `kmain` does

The order mirrors the arm64 sequence from section 3:

```rust
let mut bump = Bump::new(board.memory);      // memblock_add
bump.reserve(image())?;                      // _text .. _end
bump.reserve(dtb.region())?;                 // early_init_fdt_reserve_self
```

`dtb.region()` does not exist yet and is two lines - `Dtb` holds `blob: &'a [u8]`, so the base
is `blob.as_ptr() as usize` and the size is `blob.len()`, which is `totalsize` from the header.
Adding it makes the reservation read exactly like the Linux one instead of like a constant
copied out of the `Makefile`.

Note what this fixes for free: `DTB_BASE` in `board.rs` and `DTB_ADDR` in the `Makefile` are the
same number written twice, and the *size* was written nowhere. Asking the blob where it is and
how big it is removes both problems.

---

## 8. Bring-up order

Each step prints something before the next one starts.

1. Declare `__stack_top`, print `&raw const __stack_top as usize`. Compare against `make syms`.
   They must be identical. This is the step that fails, and it fails at the start where it is
   cheap.
2. `align_up`. Check it against pairs you worked out by hand: `(0x1001, 0x1000)` is `0x2000`,
   `(0x1000, 0x1000)` is `0x1000` - already-aligned must not move.
3. `Bump::new(board.memory)` with no reservations at all. Print `next`, `end`, size in MiB.
   Should be the full 128 MiB starting at `0x4000_0000`.
4. `alloc` for 16 bytes. It will return `0x4000_0000`, on top of your own vector table. Do not
   write to it. This is the "before" picture and it is worth seeing once.
5. `reserve` plus the intersection test. Add the image reservation. Now the same `alloc`
   returns `0x4000_d390`. That one number moving is the skill working.
6. Add the DTB reservation from `dtb.region()`. Print both reservations back.
7. Three allocations of 16 bytes. Distinct, ascending, 16 apart.
8. Alignment. `Layout::from_size_align(1, 4096)` three times. Addresses 4096 apart, twelve zero
   low bits.
9. Write to what you got and read it back. Until this happens you have been printing arithmetic,
   not memory.
10. Exhaustion. Ask for `remaining() + 1` and get `None`. Then ask for 16 more and still get an
    address - the failure must not have consumed anything.
11. Jump the DTB. Allocate until `next` passes `0x4700_0000` and confirm the returned address
    skips to `0x4710_0000`. Section 9 has the cheap way to do this without allocating 112 MiB.

Steps 1-6 are one sitting. Steps 10 and 11 are where the bugs that survive live.

### Optional step 12: parse `/memreserve/`

The one piece of Linux's reservation machinery you can implement and then actually test.

The DTB header's memory reservation block sits between the header and the struct block, at
`off_mem_rsvmap`. It is a list of 16-byte entries - two big-endian `u64`s, address and size -
terminated by a pair of zeros. Walking it is a dozen lines and your `Cursor` already does the
hard part.

On `virt` it is empty, so the code would be dead. Except you can make it non-empty:

```sh
dtc -I dtb -O dts virt.dtb -o virt.dts
# add a line at the top, above the / { ... } node:
#   /memreserve/ 0x46000000 0x00100000;
dtc -I dts -O dtb virt.dts -o virt.dtb
```

Boot that and a third reservation appears, and allocations step over `0x4600_0000` too. Then
throw the modified blob away. This is the whole of `early_init_fdt_scan_reserved_mem`'s legacy
half, it is testable in five minutes, and it is the difference between having read about
`/memreserve/` and having parsed one.

Do it if you want it. It buys nothing on this machine and it is the most faithful thing in the
skill.

---

## 9. Proving it works

**The image reservation moves the first allocation.** Allocate 16 bytes with no reservations,
note the address, add the image reservation, allocate again. `0x4000_0000` becomes
`0x4000_d390`. One number, and it proves the intersection test, the jump, and the re-align in
one shot.

**Distinct, ascending, non-overlapping.** Three allocations. Ascending is not enough - an
off-by-one in the bump gives ascending addresses that overlap by a byte. Check
`addr[1] - addr[0] >= size[0]`.

**Actually aligned.** Print `addr & (align - 1)` rather than asserting `addr % align == 0`. Zero
is the pass, and a non-zero value tells you *how far off* you are, which usually names the bug:
8 means you aligned to `align_of::<u64>()` somewhere, and something mid-page means you aligned
before jumping a reservation instead of after.

**Nothing handed out ever intersects a reservation.** The invariant the type exists for, and
worth an assertion inside `alloc` itself.

**The DTB is stepped over, cheaply.** Allocating 112 MiB to reach `0x4700_0000` works and takes
a while. The fast version is a second `Bump` in the self-check, built over a fake `Region` with
fake reservations - `Bump::new` and `reserve` take plain numbers, so the whole skip loop is
testable on a made-up 4 KiB arena with a hole in the middle, at boot, in microseconds. That is
the argument for `new` taking values rather than reading globals.

**The memory is real.** Write `0xAA` across an allocation, read it back in Rust, then look from
outside:

```
make mem ADDR=0x4000d390 N=16 FMT=xb
```

Rust reading back what Rust wrote proves the pointer round-trips. QEMU's monitor showing the
same bytes proves they are in RAM.

**Exhaustion is graceful.** `remaining() + 1` returns `None`, `remaining()` is unchanged, and a
small allocation still succeeds afterwards. This is the `next`-must-not-move invariant and it is
invisible in every other test.

**`reserve` fails loudly when full.** Nine reservations into an eight-slot array returns `Err`
and `kmain` halts with a message. A dropped reservation is silent corruption 112 MiB later.

**The kernel still works afterwards.** Allocate several MiB, write to all of it, then print
something. If the shell still echoes and the timer still ticks, you have not walked over the
stack or the vectors.

---

## 10. Where the addresses go next

Worth having straight before TABLES, because it is the reason the skill order is what it is.

The MMU is off, so every address in the kernel right now is a physical address - the number that
goes on the bus. Turning the MMU on means the CPU starts translating: code uses *virtual*
addresses, and a set of tables in memory maps each one to a physical address.

Those tables are themselves memory, and every entry in them stores a **physical** address,
because the thing reading them is the hardware page-table walker, which by definition works
below translation. So building page tables needs a source of physical memory. That is this
allocator. It cannot be a normal allocator, because normal allocators hand out virtual
addresses, and there is no virtual anything until the tables exist.

That circularity is why every kernel has a bump allocator, and why it never fully goes away:
arm64 selects `CONFIG_ARCH_KEEP_MEMBLOCK`, so memblock survives boot for `pfn_valid`, kexec and
memory hotplug, long after `memblock_free_all()` has handed every free page to the buddy
allocator.

The practical consequence for you: once the MMU is on, the numbers `alloc` returns are still
physical, and dereferencing them directly stops working unless they happen to be mapped. That is
a TABLES problem with a standard answer - map all of RAM at a fixed offset, add the offset when
you need to touch it. Nothing to do now. Just do not be surprised when `alloc` keeps working and
using the result stops.

---

## 11. When nothing happens

| symptom                                                       | almost certainly                                                                                                                                                       |
| ------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| the image reservation is a couple of hundred bytes long       | you read the symbol instead of its address. `&raw const __stack_top`, never `unsafe { __stack_top }`. Section 4.                                                       |
| first `alloc` returns `0x4000_0000` after reserving the image | the intersection test never fires. Check it half-open: `start < res.base + res.size && res.base < finish`.                                                             |
| first `alloc` returns `None`                                  | `end < next`. `Region` is `(base, size)` and the arena wants an end; `memory.size` is not an address.                                                                  |
| every `alloc` returns the same address                        | computed the new `next` and never stored it. Very easy with `&mut self` and a shadowing `let`.                                                                          |
| addresses ascend but overlap                                  | added the size before aligning, or aligned the result instead of the input. Section 5.                                                                                |
| asked for 4096 alignment, got something 16-aligned            | the skip loop jumped a reservation and returned the reservation's end without re-aligning. Step 3 goes back to step 1, not to step 2.                                  |
| hang inside `alloc`                                           | the retry loop. `next` is not advancing past the reservation, so it re-intersects forever. This is why the loop has an iteration bound.                                |
| data abort, ESR DFSC `0x21`, on first use of the memory       | unaligned 8-byte access with the MMU off. The allocation is misaligned, not the code using it.                                                                          |
| data abort on first *write*, any alignment                    | the address is outside RAM. `end` came from the wrong place - it is `memory.base + memory.size`, not `memory.size`.                                                     |
| output turns to garbage after a lot of allocation             | writing over the device tree. The DTB reservation is missing, or its size came from somewhere other than `totalsize`.                                                   |
| the kernel dies before printing anything                      | writing over your own `.text`. The image reservation is missing entirely, and `alloc` handed out `0x4000_0000`.                                                        |
| silent hang after a large allocation                          | the stack. 32 KiB, no guard, grows down into `.bss`. A big local array in the function that allocates will do it, not the allocation itself.                            |
| `PC=0x200` in `make regs`                                     | a fault before `install_vectors`, per `CLAUDE.md`. Should be impossible here. If you see it, this code runs earlier than you think.                                    |
| `remaining()` shrinks by more than the size requested         | correct, not a bug: alignment padding and skipped reservations are consumed too. Confirm that is what you are seeing before hunting a leak.                             |

The general move: every number in this skill is checkable by hand. `make syms` gives the image
end, `xxd -l 8 virt.dtb` gives the blob size, `make mem` gives the bytes, and the arithmetic is
one add and one mask. If a printed address does not match what you compute on paper, the bug is
in the few lines between them.

---

## 12. Deliberately left out

**`free`, and everything that follows.** Free lists, size classes, coalescing, fragmentation.
memblock does have `memblock_free`, and dropping it is the one place this design is genuinely
simpler rather than smaller. The next allocator will have it.

**Sorting and merging reservations.** `memblock_insert_region` keeps the arrays sorted and
`memblock_merge_regions` coalesces adjacent same-flag entries, which is why 128 slots is enough
for a real machine. With two entries there is nothing to merge and nothing to search.

**Growing the reservation array.** `memblock_double_array`. Needs the allocator to allocate its
own metadata and then reserve it, which is a fun trick and entirely unnecessary at two of eight
slots.

**Top-down allocation.** memblock searches from the top so low memory stays available for
devices with narrow DMA addressing. Nothing on `virt` has that constraint.

**Region flags.** `MEMBLOCK_NOMAP` - memory that exists but must be kept out of the linear map,
usually firmware. `MEMBLOCK_MIRROR`, `MEMBLOCK_HOTPLUG`. All describe ways RAM can be special
that `virt` has no way to be.

**Multiple memory ranges.** `virt` has one `/memory` node with one `reg` pair. A board with two
banks reports two, and `Bump` grows a memory array to match the reserved one - at which point
you have written memblock.

**`/reserved-memory`.** The modern replacement for `/memreserve/`: a real node whose children
carry `reg` or just a `size` for the kernel to place, plus `no-map` and `reusable`. Handled by
`drivers/of/of_reserved_mem.c`. `virt` has none, and finding it needs a tree walk deeper than
`Dtb::find`'s root-children-only search.

**Zeroing on allocation.** Section 7. TABLES zeroes its own tables.

**Alignment larger than a page.** `Layout` supports it and `align_up` handles it. No caller.

**A guard page below the stack.** The right fix for stack-overflow-eats-`.bss`, and it needs the
MMU, so it is downstream of TABLES rather than upstream.

**Statistics, high-water marks, allocation tagging.** All useful in a debugger, all speculative.
`remaining()` is the one number worth having, because exhaustion is a real test.

---

## 13. Done when

- `alloc` returns distinct, correctly aligned addresses inside the region the DTB reported, and
  never inside a reservation.
- The first allocation moves from `0x4000_0000` to `0x4000_d390` when the image reservation is
  added, and you can point at the line that did it.
- The DTB reservation's base and size both come from the blob, not from `DTB_BASE` and not from
  a constant.
- A 4 KiB allocation with 4 KiB alignment comes back with twelve zero low bits, including
  immediately after a reservation jump.
- Writing to allocated memory and reading it back works, and `make mem` agrees from outside.
- Exhaustion returns `None` and does not consume anything.
- `reserve` fails rather than silently dropping the ninth entry.
- The arena's start and end come from the device tree; the image reservation comes from the
  linker. No addresses in `bump.rs`.
- `make lint` is clean and one `self_check` runs at boot.
- You can say out loud: why a linker symbol's address is the value you want, why `align_up`
  needs a power of two, why the skip loop terminates without sorting, and why page tables have
  to be built out of physical memory.

The last one is the one that matters. The others you can check; that one is the skill.

---

## Optional reading

- **Boot time memory management**, `Documentation/core-api/boot-time-mm.rst` in the Linux
  source, or <https://www.kernel.org/doc/html/latest/core-api/boot-time-mm.html>. Six screens,
  and it is the normative description of everything in section 3.
- **`mm/memblock.c`**. The real thing. `memblock_add_range`, `memblock_merge_regions`,
  `memblock_double_array`, `memblock_alloc_range_nid`, and the `__next_mem_range` iterator that
  walks two lists at once. Read `memblock_double_array` even if you read nothing else.
- **`drivers/of/fdt.c`**, `early_init_fdt_reserve_self` and `early_init_fdt_scan_reserved_mem`.
  Forty lines, and they are the two reservations you are making.
- **`core::alloc::Layout`** in the Rust standard library docs. Short, and the invariants section
  is the specification you are implementing against.
- **ARM Architecture Reference Manual (ARMv8-A)**, section D8 on the VMSA - the translation
  table descriptor formats, to see the assumed-zero low bits that make 4 KiB alignment mandatory
  rather than polite. Read when TABLES starts, not now.
- **"Untangling Lifetimes: The Arena Allocator"**, Ryan Fleury. Not kernel-specific, and the
  clearest argument anywhere for why the allocator with no `free` is a design and not a
  limitation.
