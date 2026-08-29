# SLAB - cutting a page into objects

## 1. What this is

The layer that makes small allocations affordable. FRAMES works in pages; SLAB takes one page from
FRAMES, cuts it into equal-size slots, and hands those out. It is what `kmalloc` is built on in
Linux, and it is where almost every allocation in a running kernel actually lands.

Entirely software, like FRAMES, and for the same reason: the hardware has no opinion about objects.

The reason it exists is a ratio. Ask FRAMES for 32 bytes and you get 4096, wasting 99.2 percent.
Ask FRAMES to track 32-byte units instead and its metadata array grows by a factor of 128, from
32 kibibytes to 4 mebibytes, to describe the same memory. Neither is acceptable, and no adjustment
to FRAMES fixes it - the two requirements pull in opposite directions. That is why there are two
layers rather than one good one.

## 2. The idea, which is one sentence

**Take one page, decide it will only ever hold 64-byte objects, and you no longer need to track
where the objects are.** They are at every multiple of 64 from the start of the page. There are 64
of them. Allocation is "which one is free", and that is a much smaller question than "where is
there 64 bytes".

Everything else in this skill is consequence.

The free list costs nothing, by the same trick FRAMES used for its own lists: a free slot is
unused memory, so the pointer to the next free slot lives **inside** the free slot. An empty page
of 64 free slots is a chain of 64 pointers threaded through itself, and the only state outside the
page is the head.

Allocation becomes: take the head, follow it to get the new head, return the old one. Three
instructions and no search. Freeing is the reverse, and it is the same three instructions. That
speed is the second reason slab exists, and in a busy kernel it matters as much as the space.

## 3. Internal fragmentation, which is the cost you are choosing

FRAMES fought external fragmentation, which was memory that exists but is not contiguous. SLAB has
the other problem.

Ask for 33 bytes from a 64-byte class and 31 bytes are wasted. They are not lost to the system;
they are lost inside a block you were given. That is **internal fragmentation**, and unlike the
external kind it cannot be fixed by rearranging anything. It is priced in when you pick the class
table.

The trade is explicit: fewer classes means simpler code and more waste, more classes means less
waste and more partly-used pages sitting around. Linux settled on powers of two plus two extra
classes at 96 and 192, which exist for one empirical reason - so many kernel structures land just
over 64 and just over 128 that a straight power-of-two table wasted a measurable fraction of
memory on them.

That is the part worth copying: **the class table is a measurement, not a formula.** Do not inherit
Linux's because it is Linux's. Print the sizes this kernel actually asks for once `Vec` and `Box`
are live, and choose against that. You will have very different answers, because your kernel
allocates `RingBuffer`s and page tables and Linux allocates `dentry`s and `sk_buff`s.

## 4. Reading the names

**Slab** - historically, one or more contiguous pages devoted to objects of a single size. In this
kernel a slab is one page, because one page from FRAMES is the natural unit and multi-page slabs
buy nothing until objects get large.

**Cache**, in the slab sense - all the slabs for one size class, plus the bookkeeping. Nothing to
do with the processor's caches, and the collision is unfortunate and permanent. Linux says
`kmem_cache`. When this guide says cache it means a size class.

**Size class** - one of the fixed sizes SLAB rounds requests up to.

**Slot** - one object-sized division of a slab.

**Partial**, **full**, **empty** - the three states a slab can be in. A slab with some free slots
is partial, and those are the only ones worth looking at when allocating. Section 5.

**SLUB** - the name of the surviving implementation in Linux. Historical note that will save
confusion: Linux had three, called SLAB, SLOB and SLUB. SLOB was removed in 6.2 and SLAB in 6.8,
so modern Linux has only SLUB, but every tutorial written before 2024 compares all three as if
they are live options.

## 5. The three states, and why a slab has to know its own

Allocation looks at partial slabs first, because a partial slab has a free slot ready and needs no
page from FRAMES. Only when there is no partial slab does the class ask FRAMES for a page, cut it
up, and make it the new partial slab.

Freeing is where the states earn their keep. When the last object in a slab is freed, that page is
now entirely unused, and it should go back to FRAMES so some other class - or a page table, or a
task stack - can have it. A slab allocator that never returns pages is a slab allocator that
slowly converts all of memory into empty 64-byte slots.

That gives the rule that shapes the design: **freeing an object has to be able to find its slab.**
You are handed a bare pointer and nothing else. Two ways to get from the pointer to the slab:

- **Mask the address.** The slab is one page, so the slab's header is at `ptr & !0xFFF`. One
  instruction, no lookup, and it requires the header to live inside the page it describes.
- **Look it up.** Keep the slab metadata beside the page metadata FRAMES already has, indexed by
  page frame number. No header inside the page, so no alignment surprises, at the cost of a memory
  access.

Linux's SLUB uses the second, via `struct page`, and **so does this kernel.** The decision is made
here rather than left open, because it changes where the first slot starts and therefore every
number in section 6.

The reason is the arithmetic in section 6: a header inside the page cannot divide a power-of-two
class, so it always costs a whole object, and at the large classes that is a quarter of the page.
Out of band, a page holds exactly `4096 / size` objects and the tail waste is zero for every
power-of-two class. The masking trick is one `and` against one load from the frame table, and a
load that hits cache is not worth a quarter of a page.

The cost is real and it is paid up front: the frame table has one entry per page of RAM, and those
entries now have to be wide enough for the slab fields, whether or not the page ever becomes a
slab. That is section 6 as well.

## 6. The numbers, and where they come from

**Slots per slab.** With the metadata out of the page, the whole page is slots: `4096 / size`, and
the leftover is `4096 mod size`. Every power-of-two class divides 4096, so every power-of-two class
wastes nothing. Only 96 and 192 have a tail, and it is small.

| class | slots | wasted |
|---|---|---|
| 16 | 256 | 0 |
| 32 | 128 | 0 |
| 64 | 64 | 0 |
| 96 | 42 | 64 |
| 128 | 32 | 0 |
| 192 | 21 | 64 |
| 256 | 16 | 0 |
| 1024 | 4 | 0 |

Compare that with a 16-byte header living in the page, which leaves 4080 - not a power of two, so
no power-of-two class divides it. 64-byte objects give `4080 / 64 = 63.75`, so 63 slots and 48
bytes wasted; 1024-byte objects give 3 slots and 1008 bytes wasted, a quarter of the page. The
header does not cost 16 bytes, it costs one whole object, because it pushes the last one out and
nothing else can use the room. That is what section 5 bought by moving the metadata out.

**What the frame table pays for it.** The slab fields have to sit in the per-page entry FRAMES
already keeps, and that entry exists for every page of RAM whether or not it ever becomes a slab.
Today it is one byte - a free flag and a four-bit order - so 128 MiB costs 32768 entries and 32 KiB
of table. Widening it to eight bytes makes that 256 KiB, or 0.2 percent of RAM, paid always.

That is the honest trade, and it is not free: the tail waste it removes is only paid by pages that
actually become slabs, while the wider table is paid by all of them. It wins here because 0.2
percent is smaller than the fragmentation of even a few dozen large-class slabs, and because it
gets *better* as the kernel allocates more, not worse. Copy `struct page`'s other trick to keep the
entry at eight bytes: a page is either a buddy page or a slab page and never both, so the slab
fields and the order field are a union over the same bytes, not fields beside each other.

**Alignment.** Every object must be aligned to at least its class size when the class is a power of
two, and to 16 bytes otherwise. 16 because that is the alignment `Layout` will ask for on aarch64
for anything containing a `u128` or a pointer pair, and returning something less aligned is
undefined behaviour rather than a performance problem. With the metadata out of the page, slot 0
sits at the page base, a page is 4096-aligned, and every class divides 4096, so every slot is
already aligned to its own class size. Alignment costs nothing here - it is one more thing the
header was paying for.

**The largest class.** Above some size, cutting a page up stops making sense: at 2048 bytes a page
holds two objects, at 4096 it holds one, and you have gained nothing over calling FRAMES. Linux's cutoff is
twice the page size, past which `kmalloc` forwards straight to the page allocator. Take the same
rule: above the largest class, round the request up to a whole number of pages and call FRAMES
with the matching order.

**Minimum class.** 8 bytes is the floor for the free list to work at all, because a free slot has
to hold a pointer, and a pointer is 8 bytes on this machine. That is a hard constraint, not a
policy: a 4-byte class cannot thread its own free list.

## 7. What you are building

One module, `src/slab.rs`, plus a change to `src/frames/metadata.rs`.

`docs/diagrams/slab.tldx.jsx` draws all of it - the slots, the lookup, the entry, and the three
states. Open it with `tldx serve docs/diagrams/slab.tldx.jsx` and keep it beside the guide.

**The frame-table entry, derived bit by bit.** Section 5 put the slab bookkeeping in the per-page
table, so `Entry` in `src/frames/metadata.rs` has to carry both meanings. It is one byte today:

```
bit 0      free
bits 1..4  order
```

A slab page needs five things: which class it belongs to, how many slots are handed out, the head
of its free list, and the two links that keep it on the class's partial list. Count the bits each
one actually needs, against a 4 KiB page and 128 MiB of RAM:

| field          | range                                      | bits |
| -------------- | ------------------------------------------ | ---- |
| `is_slab`      | buddy page or slab page                    | 1    |
| `class`        | one of about 11 classes                    | 4    |
| `free_head`    | slot index 0..511, plus a "none" sentinel  | 10   |
| `in_use`       | 0..512                                     | 10   |
| `next_partial` | a page frame number, within the arena      | 19   |
| `prev_partial` | same                                       | 19   |
|                |                                            | 63   |

512 is the slot ceiling because the smallest class is 8 bytes and `4096 / 8 = 512`. 10 bits holds
0..1023, so it covers 0..512 and leaves room for the sentinel. Those four fields are fixed by the
page size and the class table, not by how much memory the machine has, and they come to 25 bits.

**The two links get whatever is left, and the arithmetic is the point.** `64 - 25 = 39`, so 19 bits
each with one bit spare, and `2^19 = 524288` pages is a 2 GiB arena. Reach for `u16` because it is a
Rust integer and you get 65536 pages instead, a 256 MiB cap, for no reason at all - the field is
hand-packed either way, so a 19-bit mask costs exactly what a 16-bit mask costs. Pick the width from
the budget, not from the type.

2 GiB is far past anything `-M virt` will be asked to boot, so this is a limit that never needs a
configuration knob. It does need to be **checked**, because the failure is silent: a truncated link
gives two slabs the same successor and corrupts a partial list, and nothing prints. One constant and
one refusal in `Frames::new`:

```
const MAX_ARENA_PAGES: usize = 1 << 19;   // 2 GiB of 4 KiB pages
```

If the arena is bigger, refuse to boot and say so. The day that fires, widen the entry to 16 bytes
and the links to 32 bits - a type change, not a redesign. A limit you check is not a limit you have
to design around.

63 bits fit in a `u64`, and the buddy meaning needs 5, so **one `u64` per page carries both**. The two meanings never coexist - a page is a buddy page or a slab page, never both -
so they overlay the same bits rather than sitting beside each other. Write it as a Rust `enum` with
two variants and let the compiler do the packing, or pack it by hand into a `u64` the way `Entry`
already packs into a `u8`. The enum is clearer and costs 16 bytes per page instead of 8, because
Rust adds a discriminant word; the hand-packed `u64` is what earns the number above.

**The module.**

- `class_of(layout: Layout) -> Option<usize>` - rounds a request to a class index, satisfying both
  the size and the alignment, or `None` above the largest class.
- Per-slab `alloc` and `free`, working on one page: pop the free head, push it back.
- A `Cache` per class: the class size and the head of its partial list.
- A top-level type owning the array of caches, with `alloc(&mut self, &mut Frames, Layout)` and
  `free(&mut self, &mut Frames, ptr, Layout)`. It needs `&mut Frames` for two reasons now, not one:
  to get and return pages, and because the bookkeeping it reads and writes lives in the frame
  table.

The interface takes `Layout`, not a size, because that is what `GlobalAlloc` will hand it next skill
and because alignment is part of the request rather than a detail. A class that satisfies the size
but not the alignment is the bug section 9 is about.

## 8. Building it, in order

Eight steps. Each one ends in something the machine prints, so you never write two steps' worth of
code before finding out the first was wrong.

**Step 1 - widen the entry.** In `src/frames/metadata.rs`, make `Entry` a `u64` instead of a `u8`.
`to_byte`/`from_byte` become the `u64` versions, `byte()` indexes by 8 bytes instead of 1, and
`required_size` multiplies by 8 before the `div_ceil`. Nothing else changes yet: the entry still
only ever holds the buddy meaning.

*Signal:* boot and read `frames:`. The free page count drops by exactly 56, because the metadata
region grew from 8 pages to 64. `32768 pages x 1 byte = 32 KiB = 8 pages`; `x 8 bytes = 256 KiB =
64 pages`. If the number moved by something other than 56, `required_size` and `byte()` disagree
about the stride, and that is a bug you want now rather than after the free list is threaded
through it.

**Step 2 - add the slab meaning, and a way in.** Give `Entry` its second variant with the fields
from section 7. Then give `Frames` two methods so `slab.rs` can reach the table without
`metadata` becoming public: something like `page(&self, pfn) -> Entry` and
`set_page(&mut self, pfn, Entry)`. This is also where `MAX_ARENA_PAGES` goes in, and where
`Frames::new` starts refusing an arena it cannot address.

*Signal:* at boot, allocate one page with `frames.alloc(0)`, print `frames.page(pfn)`, write a slab
entry to it, print it again. Then boot once with `-m 4G` and confirm it refuses with a message
instead of truncating a link. The Makefile already takes the size as a variable, so it is one
command: `make run MEM=4G`. 4 GiB is 1048576 pages against a ceiling of 524288. That is the only
time you will ever exercise the check, so exercise it now. You should see the buddy meaning turn into the slab meaning and the
order field stop being readable as an order. That is the union working.

**Step 3 - the class table.** `const CLASSES: [usize; N]` and `class_of`. Start with powers of two
from 8 to 2048 plus 96 and 192, and expect to change it later - section 3 says the table is a
measurement, and you have nothing to measure until HEAP.

*Signal:* print `class_of` for a row of layouts and read them: 1 byte lands in 8, 65 lands in 96,
100 lands in 128, 2049 gives `None`. Then print it for `Layout::new::<u128>()`, which asks for 16
bytes with 16-byte alignment. If your `class_of` only compares sizes it will happily return the
8-byte class for something that needs 16-byte alignment, and this is the print that catches it.

**Step 4 - cut a page into slots.** A function that takes a page frame number and a class, threads
the free list through the page, and writes the slab entry. Slot `i` is at `page_base + i * size`,
slot `i`'s first 8 bytes hold the index of slot `i + 1`, and the last slot holds the sentinel.

*Signal:* walk the chain from `free_head` to the sentinel and print the count. For class 64 it must
be exactly 64, for class 1024 exactly 4, for class 96 exactly 42. Those are section 6's numbers, and
walking the chain is the only way to find out whether the page actually holds them. A count of 63
means you subtracted a header that no longer exists.

**Step 5 - alloc and free one slab.** Pop the head, follow it to the new head, bump `in_use`. Free
is the reverse. No cache, no partial list, no FRAMES - one page, by hand, from step 4.

*Signal:* allocate all 64 objects of a class-64 slab. `in_use` reads 64 and `free_head` reads the
sentinel, and the 65th allocation returns `None` rather than a wild pointer. Free all 64 and walk
the chain again: back to 64. Write a recognisable byte to each object as you go and read them back
before freeing, because two allocations returning the same address is the failure this step exists
to rule out, and it is silent otherwise.

**Step 6 - the cache and the partial list.** Now the state machine from section 5. Allocating asks
the class's partial list first; an empty list means `frames.alloc(0)`, step 4, and push. Freeing
puts the slab back on the partial list if it was full, and when `in_use` hits 0 it unlinks the slab
and calls `frames.free(pfn)`.

*Signal:* the round trip. Print `frames`, allocate 64 objects of one class, print `frames` again -
exactly one page gone. Free all 64, print - the page is back. Then allocate 64, free 63, print: the
page is still gone, because a slab with one live object is not empty. Those three prints are the
whole state machine, and the third is the one that catches an empty slab being confused for a
partial one.

**Step 7 - the top level.** The array of caches, `alloc(Layout)` and `free(ptr, Layout)`, and the
large path: above the largest class, round the size up to pages, work out the order, and call
FRAMES directly. Freeing a large allocation has to take the same branch, which is why `free` takes
the `Layout` and not just the pointer.

*Signal:* allocate 3000 bytes. FRAMES loses one page, not two, because 3000 rounds to one page and
order 0. Allocate 9000: FRAMES loses 4 pages, because 9000 rounds to 3 pages and the smallest order
that holds 3 pages is 2. Free both and watch the count come back. The 4-versus-3 is worth predicting
before you run it - if you expected 3, you have found the difference between rounding to pages and
rounding to an order.

**Step 8 - the statistics.** Per class: slabs held, objects live, bytes asked for against bytes
handed out. This is not decoration; section 3's internal fragmentation is invisible without it, and
the class table cannot be tuned against anything else.

*Signal:* run the workload from step 6 and read the ratio. 64 objects of 40 bytes asked for 2560 and
got 4096, so 37 percent is lost to the class table. That is the number that tells you whether to add
a class at 48.

## 9. When nothing happens

| symptom                                                                | almost certainly                                                                                                                                                                                     |
| ---------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| two live objects at the same address                                   | the free list head was updated before the slot was read, or a slot was pushed onto the list twice by a double free.                                                                                  |
| the free list runs off into memory that is not the page                | the chain was built with the wrong stride, or the last slot was given a next pointer instead of the sentinel.                                                                                        |
| the chain walks 63 slots instead of 64                                 | a header was subtracted that no longer exists. Section 5 put the metadata in the frame table; the whole page is slots.                                                                               |
| a `u128` or a pointer pair written to an allocation faults or corrupts | alignment. The class satisfied the size and not the `Layout`'s alignment. Section 6.                                                                                                                 |
| pages are never returned to FRAMES                                     | the free counter per slab is not reaching zero, usually because it counts allocations rather than free slots, or because objects freed into a different class's slab are decrementing the wrong one. |
| memory use grows without bound under a steady workload                 | empty slabs stay on the partial list instead of going back to FRAMES. An empty slab is not a partial slab.                                                                                           |
| freeing works for the first slab and faults for later ones             | the pointer-to-slab step: the page frame number is off by the arena base, so `table[pfn]` reads the wrong page's entry.                                                                               |
| everything works and FRAMES reports the wrong free count               | a page taken from FRAMES for a slab and then returned at the wrong order. Slabs are one page, so order 0, always.                                                                                    |

## 10. How you will know it worked

Print the class table, then run a workload and print the per-class statistics: slabs held, objects
live, bytes requested against bytes handed out.

That last pair is the number worth having, because it is the internal fragmentation from section 3
made visible. If the workload asked for 100 kilobytes and SLAB gave out 160, you are wasting 37
percent, and the class table is wrong for this kernel rather than wrong in general.

The concrete round trip: allocate 63 objects of one class, confirm FRAMES lost exactly one page,
free all 63, and confirm FRAMES got it back. Then do the same 63 allocations, free 62 of them, and
confirm FRAMES lost exactly one page still. Those two together prove the state machine in section
5 in four lines of output.

Once HEAP lands next skill, the real observable arrives: a `Vec` that grows, is dropped, and gives
its memory back, with `free_pages()` returning to where it started. That is the first time this
kernel is not slowly leaking, and it is worth checking on the day it becomes true.

---

## Optional reading

- Jeff Bonwick, "The Slab Allocator: An Object-Caching Kernel Memory Allocator", USENIX 1994. The
  original, short, and readable. Object caching and constructors are the parts Linux kept and this
  kernel is skipping.
- `mm/slub.c` in the Linux source, and `include/linux/slub_def.h` for the per-class structure.
- `mm/slab_common.c` for the size class table and `kmalloc_index`, which is the rounding in
  section 6 with the odd classes visible.
