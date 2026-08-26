# FRAMES - the first allocator that can give memory back

## 1. What this is

A page allocator. It owns every page of physical memory that is not spoken for, hands them out in
blocks, and takes them back. It is the bottom of the real allocator stack, and everything above it
- slab, `Box`, `Vec`, task stacks, page tables - eventually asks it for memory.

The category is **entirely software**. There is no register to write and no device to poke. The
only thing the hardware contributes is the page size, and even that is a number you chose: `TG0`
in `TCR_EL1` says 4 kibibytes, so 4 kibibytes is the unit. The rest is a data structure.

What makes it worth a guide is not the code, which is short. It is that this is the first
structure in the kernel that has to store its own bookkeeping in the memory it manages, and the
first that has to solve fragmentation rather than avoid it.

## 2. What `Bump` was, and what it becomes

`Bump` hands out addresses and can never take one back. That was correct for what it was for, and
it stops being correct here, because from FRAMES onward memory is recycled: a task exits and its
stack goes back, a page table is torn down, a `Vec` is dropped.

The obvious next question is whether `Bump` stays as a layer underneath FRAMES, the way memblock
sits under Linux's buddy allocator. It does not, and the reasoning is worth following because it
is the difference between copying a design and understanding one.

FRAMES needs a record for every page in the machine before it can hand out its first page. On a
128 mebibyte arena that is 32768 pages and 32 kibibytes of array, and it has to come from
somewhere. But it can come from FRAMES itself:

```
page_count = arena.size / 4096
metadata   = page_count bytes
place it in the first aligned gap that is neither the image nor the device tree
mark those pages used
build the free lists from what is left
```

Ten lines, needing nothing that is not already available. **The bump allocator does not disappear,
it becomes an unnamed loop inside `Frames::new`.**

So what is `Bump` actually for? Two different things were hiding under one name:

**A bump allocator** - hand out arbitrary memory before the page allocator exists. This kernel has
exactly one customer for that, the page metadata, and it can serve itself. This half goes away.

**The memory map** - the authoritative record of what physical memory exists and which parts are
spoken for: the kernel image from the linker, the device tree blob, anything firmware reserved.
This is not an allocator at all, it is a list, and FRAMES cannot free a single page without it.
This half stays, and it is `reserve`, `reserved`, and the overlap test that `Bump` already has.

Linux keeps memblock alive after boot on arm64 for the second reason, not the first. `pfn_valid`,
memory hotplug and kexec all ask it whether a physical address exists, long after
`memblock_free_all` has given every free page away.

The allocator half is genuinely needed on a real machine and genuinely not here. Linux needs it
because memory arrives in several banks with node affinity and each node's metadata must come from
that node; because reservations arrive in stages and the picture is incomplete when the first
allocation is needed; because memory can be hot-added later. None of that is true on `virt`, where
there is one memory node and one set of reservations, all known at the top of `kmain`.

## 3. FRAMES goes before the page tables

There is a consequence of this that improves the boot order rather than just simplifying it.

Today `kmain` builds translation tables out of `Bump`:

```
Bump::discover  ->  Table::new(&mut bump)  ->  identity_map  ->  mmu::enable
```

Tables allocated from a bump allocator can never be freed. That is invisible now and it is not
invisible in TWO WORLDS, where every program gets its own address space and tearing one down means
returning its tables.

`Frames::new` needs the arena, the reservations, and nothing else. No translation, no tables. So
it can be step one:

```
Frames::new  ->  Table::new(&mut frames)  ->  identity_map  ->  mmu::enable
```

A translation table is exactly one 4 kibibyte page, which is order 0, so it is a better fit for
FRAMES than for `Bump`'s align-up arithmetic. And tables become freeable three skills before
anything needs them to be.

## 4. Fragmentation, and which kind this fights

Two different problems share the word, and confusing them is why allocator designs look arbitrary.

**External fragmentation** is having plenty of memory free and not being able to use it, because
what is free is scattered. 100 mebibytes free in single pages, and a 2 mebibyte request fails.
The free memory is real; the contiguity is gone.

**Internal fragmentation** is being given more than you asked for. Ask for 33 bytes, get 64, waste
31. The memory is not lost to the system, it is lost inside a block you hold.

FRAMES fights external fragmentation and does not care about internal at all, because everything
it hands out is a whole number of pages. SLAB, next skill, is the reverse. That is the division of
labour between them, and it is the reason there are two layers rather than one.

## 5. Reading the names

**PFN**, **P**age **F**rame **N**umber - a page's index, not its address. Page frame number `n`
starts at `arena_base + n * 4096`. Every piece of arithmetic in this skill is easier in page frame
numbers, and the conversion happens only at the edges.

**Frame** and **page** - a frame is the physical slot, a page is the contents. In an identity-
mapped kernel with no swapping they are the same thing, and the distinction only starts to matter
at demand paging. The skill is called FRAMES because it allocates the slots.

**Order** - the base-two logarithm of a block's size in pages. Order 0 is 1 page, order 1 is 2
pages, order 4 is 16 pages. Blocks come in orders and nothing else.

**Buddy** - the other half of the block a block was split from. Section 5, and the whole design
turns on how cheap it is to find.

**Arena** - the range of physical memory this allocator owns. Comes from the device tree, minus
whatever `Bump` reserved.

**Granule** - the page size the translation tables use, fixed by `TG0` in `TCR_EL1` at 4 kibibytes.
Same word ARM uses for the exclusive monitor's region in LOCK, unrelated meaning.

## 6. The buddy algorithm

The split and the merge are drawn in [`diagrams/frames.tldx.jsx`](diagrams/frames.tldx.jsx) - open
it with `tldx serve docs/diagrams/frames.tldx.jsx` and read it alongside this section.

Keep one free list per order. Eleven lists, orders 0 through 10.

**Allocating order `n`:** if list `n` is non-empty, pop it and you are done. If it is empty,
allocate order `n + 1` recursively, split the result in half, put one half on list `n`, and return
the other. Splitting recurses upward until it finds something or runs out of orders.

**Freeing** is where the design earns its name. When a block of order `n` comes back you would
like to reunite it with the block it was split from, so the arena does not slowly grind down into
single pages. Finding that block takes one instruction:

```
buddy_pfn = pfn ^ (1 << n)
```

Exclusive-or with the block's size in pages, which flips exactly the one bit that distinguishes
the two halves of the parent. A block at page 12 of order 2 covers pages 12 to 15; its buddy is
`12 ^ 4 = 8`, covering 8 to 11. And that block's buddy is `8 ^ 4 = 12`. Each is the other's,
symmetrically, so nothing has to remember who was split from whom.

Merge only if **both** hold:

- the buddy is free
- the buddy is the same order

The second condition is not optional and is the one that gets skipped. If the buddy was itself
split into smaller pieces, it is not a free block of order `n`, it is a region containing some
free and some used pages, and merging with it would hand out memory that is in use. Both facts
have to be readable from the metadata, which is what section 8 is about.

After a merge, the combined block is order `n + 1` at the lower of the two addresses, and you try
again at the next order up. The loop stops when the buddy is unavailable or you reach order 10.

Sixteen lines, no search, no sorted list, no scan. That is the whole allocator.

## 7. The numbers, and where each comes from

**Page size 4096.** From `TG0` in `TCR_EL1`, which `src/mmu/init.rs:33` sets to `0b00`. Not a
choice you make here.

**Maximum order 10, so the largest block is 1024 pages, which is 4 mebibytes.** This is a policy
number, not a hardware one. It is what arm64 Linux uses with a 4 kibibyte page size, and it is
chosen as the point past which contiguous allocations are rare enough not to be worth the extra
lists. Two warnings for when you read Linux source: the constant is `MAX_ORDER` in older kernels
and `MAX_PAGE_ORDER` in newer ones, and around 6.4 its meaning changed from exclusive to
inclusive, so a tutorial's arithmetic may be off by one against the source in front of you. The
4 mebibyte figure is the same either way.

**32768 pages.** `128 mebibytes / 4096`, from the device tree at run time, not a constant.

**Eleven free lists.** Orders 0 through 10 inclusive.

**One byte of metadata per page.** You need two facts per page: is it free, and if it heads a
block, what order. Order 0 to 10 needs four bits, the free flag needs one, so a byte is
comfortable. That is 32768 bytes for the whole arena, which is 32 kibibytes, which is 8 pages,
which is **0.024 percent** of the memory it describes.

Worth comparing: Linux's `struct page` is about 64 bytes, or 1.5 percent of memory. The difference
is not cleverness on your part - `struct page` carries a reference count, a mapping pointer, list
links and flags because Linux pages are shared, swapped, mapped into several address spaces and
reclaimed. Yours are not, yet. Knowing which of those fields you are choosing not to have is worth
more than the byte count.

**The free lists cost nothing.** A free page can hold its own list pointer, because it is by
definition unused memory. Eleven list heads is 88 bytes; the links live inside the free pages. The
same trick SLAB uses next skill, met here first.

## 8. Bringing it up, in order

The bootstrap has a fixed sequence and every step exists for a reason:

1. **Collect the reservations.** The kernel image from the linker symbols, the device tree blob
   from wherever it landed. Both are already available at the top of `kmain`.
2. **Compute the metadata size** from the arena the device tree reported. `arena.size / 4096`
   bytes, rounded up to a page.
3. **Find a home for it.** Walk the arena from the base, skipping any reserved region, and take
   the first page-aligned gap large enough. This is the ten lines from section 2, and it is the
   only bump-allocation this kernel will ever do.
4. **Mark every page used.** Nothing is free until you say so. Starting from "everything used" and
   freeing what is available is safer than the reverse, and it is what Linux does.
5. **Reserve the metadata array itself.** It sits inside the arena, and if you do not mark it used,
   FRAMES will hand out the pages holding its own bookkeeping. This is the step that gets
   forgotten, and the symptom is memory that corrupts itself only under pressure.
6. **Free every page that is not reserved.** This is `memblock_free_all`, and it is more
   interesting than it sounds.

Step 6 is where your real numbers show up.

You cannot just push every free page onto list 0 - that would leave the arena as 32000 order-0
blocks with no large blocks at all, and the merging would have to discover all the structure
afterwards. Instead, walk each free range and greedily take the largest aligned block that fits,
which is capped by three things: the maximum order, the pages remaining, and the alignment of the
current page frame number. That last one is `pfn.trailing_zeros()`.

Here is the real decomposition for this kernel, using the actual symbols out of `kernel.elf`:

- `__image_start` is `0x4008_0000` and `__stack_top` is `0x4008_f3d0`, so the image is 62416
  bytes, which rounds up to 16 pages.
- The arena starts at `0x4000_0000`, which is page frame number 0. The image begins 512 kibibytes
  in, at page 128. So pages 0 to 127 are free.
- 128 pages, starting at an address aligned to 1 gibibyte, is **exactly one order-7 block**. One
  entry, not 128.
- Pages 128 to 143 are the image. That is 16 pages at page 128, exactly one order-4 block, though
  nothing requires a reservation to land so neatly.
- Free resumes at page 144. `144` is `0b10010000`, four trailing zeros, so the largest aligned
  block that can start there is order 4. Take 16 pages, and you are at page 160.
- `160` is `0b10100000`, five trailing zeros: order 5. Take 32, arrive at 192.
- `192` is `0b11000000`: order 6. Take 64, arrive at 256.
- `256` has eight trailing zeros, so order 8. Then 512 gives order 9, and from 1024 onward every
  block is order 10 until the arena runs out.

The staircase at the start is not a bug, it is the alignment telling you the truth. Print that
decomposition once during bring-up and you will never wonder whether the allocator understood its
own arena.

The device tree blob is a second reserved range and will punch a similar hole wherever it landed,
plus the metadata array itself is a third. Measure them rather than assuming.

## 9. What you are building

One new module, `src/frames.rs`, and a refactor of `src/bump.rs`.

**New in `frames.rs`:**

- A per-page metadata type small enough to be one byte, holding a free flag and an order. Whether
  that is a packed `u8` or a small enum is a real choice: a struct with two fields is clearer and
  the compiler will not pack it for you, and 32 kibibytes against 64 is not worth obscurity.
- `Frames` holding the arena `Region`, a `NonNull` to the metadata array, the page count, and the
  eleven list heads.
- `Frames::new(arena: Region, reserved: &[Region]) -> Option<Frames>` - the whole bootstrap from
  section 8, in one call, taking no allocator.
- `alloc(&mut self, order: usize) -> Option<Pfn>` and `free(&mut self, pfn: Pfn, order: usize)`.
- `free_pages(&self) -> usize`, because you cannot test any of this without it.

**Changed in `bump.rs`:** the pointer-that-goes-up is now ten lines inside `Frames::new`, so what
is left of `Bump` is the memory map from section 2 - the reserved list, `reserve`, and the overlap
test. Whether that keeps the name `Bump`, becomes a `MemoryMap`, or gets folded into `frames.rs`
as a private helper is a naming decision, and the honest test is whether anything outside FRAMES
still calls it. Nothing should.

Note the signature: **`Frames::new` takes `Region` and a slice of `Region`, not a `&Dtb`.**
`Bump::discover` today reaches into the device tree parser to find the blob's own extent, which is
the memory layer calling sideways into board discovery. Collecting the reservations belongs to
whoever already knows the machine. FRAMES then depends on `Region` and nothing else, which is
what makes it testable with a synthetic arena and no device tree at all.

**Also changed in `main.rs`:** the reordering from section 3, and `Table::new` and `identity_map`
now take `&mut Frames` instead of `&mut Bump`. Their bodies need one page at a time, which is
order 0, so the change is at the call sites rather than in the walking logic.

Two shapes worth deciding deliberately rather than by accident:

**A newtype for the page frame number.** Nearly every bug in this skill is a physical address used
where a page frame number belongs, or the reverse. They are both `usize` and the compiler will not
help you unless you ask it to. A `Pfn(usize)` with `to_addr` and `from_addr` costs nothing at run
time and turns a whole class of bug into a compile error.

**How the free list is threaded.** The links live inside the free pages, which means writing a
pointer into a page you have just declared free, and reading it back later. It is the first place
in the kernel where a data structure lives in the memory it manages, and it needs the same
`write_volatile` care and the same explicit reasoning as `Table::set` in `src/mmu/mod.rs:85`.

Do not wrap this in a lock. LOCK is done, and `Frames` is a plain `&mut self` type; whoever owns
it decides. That keeps it testable on the host and keeps the locking decision in one place.

**Where this sits in the architecture:** [`diagrams/architecture.tldx.jsx`](diagrams/architecture.tldx.jsx)
has the layers. FRAMES joins the same one `Bump` and `mmu` are in already, and that
layer touches nothing beneath it. `Frames` never reads a device register and never executes an
`msr`; it does arithmetic on `Region`s it was handed. The only part of memory management that
reaches down to the architecture layer is `mmu/init.rs`, for four system registers. Moving the
bootstrap into FRAMES changes nothing about that, because `Bump` never talked to hardware either -
it read linker symbols, which are compile-time constants, and a `&Dtb` that somebody else had
already parsed.

## 10. Testing it, starting with the tests

Everything in this skill is host-testable, and the tests are unusually good ones, because the
allocator has an invariant that is easy to state and easy to check exhaustively.

Reuse the arena helper from `src/mmu/mod.rs:218` - real page-aligned memory from `std::alloc`,
leaked.

- An arena added as one aligned power-of-two region shows exactly one block at that order and
  nothing at any other.
- The staircase: a region that is not aligned decomposes into the orders section 8 predicts. Assert
  the exact list, not just the total.
- Allocating order 0 from an arena that has only a large block splits all the way down, and the
  free counts at each order are what splitting predicts.
- Allocate every page, one at a time, and the count matches `free_pages()` before you started.
  One more allocation returns `None`.
- Free them all in a **scrambled** order, and the arena comes back to exactly the block list it
  started with. This is the single most valuable test in the skill, it catches merge bugs nothing
  else catches, and it should run over several scramble orders.
- The merge does not fire when the buddy is a different order. Split a block, allocate one of the
  small halves, free its sibling, and assert nothing merged.
- The merge does not fire when the buddy is in use.
- Allocating and freeing at the maximum order does not merge past order 10.

The scrambled free-and-recombine test is worth writing before the merge code, because it is the
test that tells you the merge is right, and writing it first stops you from writing a merge that
only handles the order it was debugged against.

## 11. When nothing happens

| symptom | almost certainly |
| --- | --- |
| the arena decomposes into thousands of order-0 blocks | the greedy loop is not capping by alignment. The cap is the minimum of the maximum order, the pages left, and `pfn.trailing_zeros()`. |
| everything works until memory is exhausted, then random corruption | the metadata array is not reserved. Section 8, step 5. It is allocated from `Bump` inside the arena and it looks free unless you say otherwise. |
| freeing everything does not return one big block | the merge is not looping. Merging is repeated at each order, not done once. |
| freeing merges two blocks that are not buddies | using `pfn - (1 << order)` or a sign trick instead of exclusive-or, which is only correct for the upper half of a pair. |
| freeing merges a block with one that is in use | the same-order check is missing. Free is necessary, same-order is not optional. Section 5. |
| a block comes back at the wrong address after a merge | the merged block starts at the **lower** of the two page frame numbers, which is `pfn & !(1 << order)`, not whichever one was passed in. |
| `alloc` returns a page inside the kernel image | step 6 freed a range it should not have. The image reservation is in `Bump`, and the free walk has to respect it. |
| an address is off by exactly the arena base | a page frame number used as an address, or the reverse. This is the bug the newtype in section 9 exists to prevent. |
| a data abort reading a free list link | the link was written before the memory management unit mapped that page, or into a page that is not in the arena. Every free page is inside `board.memory`, which `identity_map` covered. |
| the count is right and the addresses repeat | a block was pushed onto two lists, usually by splitting and forgetting to remove the parent from its own list before splitting it. |

## 12. How you will know it worked

Print the decomposition at bring-up: the count of free blocks at each order, and the total free
memory in bytes.

Three things in that output prove it worked, and they are checkable by hand:

- The total free bytes equals the arena size minus the image, minus the device tree, minus the
  metadata array. Every one of those four numbers is already printed by `kmain` today, so this is
  arithmetic you can do in your head against the boot log you already have.
- The low orders match the staircase in section 8. Seeing exactly one order-7 block, then a 4, 5,
  6, 8, 9 run, is the allocator telling you it understood its own arena's alignment.
- Allocate one order-10 block, print the free counts, free it, print again, and the two match
  exactly. That single round trip exercises split, merge and the list bookkeeping in one line of
  output.

Then take the arena down to a few pages by lying to it about the size, and confirm `alloc` returns
`None` rather than handing out something outside the arena. Exhaustion is a real path here in a way
it never was for `Bump`, because everything above FRAMES will lean on it.

---

## Optional reading

- `mm/page_alloc.c` in the Linux source. `__free_one_page` is the merge, and it is worth reading
  once precisely because it is recognisably the sixteen lines from section 5 buried in thirty
  years of policy.
- `mm/memblock.c`, `memblock_free_all`, for step 6 of section 8, and `memblock_alloc` for the
  half of memblock this kernel does not need.
- Knuth, The Art of Computer Programming, volume 1, section 2.5, where the buddy system was first
  written down.
- `include/linux/mmzone.h` for `free_area` and the per-order lists, which is the same eleven lists
  with more names on them.
