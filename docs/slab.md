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

Linux's SLUB uses the second, via `struct page`. The first is simpler and is what a kernel this
size should do. Either way, decide it before you write anything, because it changes what a slab
header is and where the first slot starts.

## 6. The numbers, and where they come from

**Slots per slab.** A page is 4096 bytes. If the header lives in the page, subtract it first. With
a 16-byte header and 64-byte objects, `(4096 - 16) / 64 = 63` slots, and 16 bytes of the page are
header and 0 bytes are wasted. With 96-byte objects, `(4096 - 16) / 96 = 42` slots using 4032
bytes, leaving 48 bytes unusable at the end. That trailing waste is real and it is why the odd
classes are not free.

**Alignment.** Every object must be aligned to at least its class size when the class is a power of
two, and to 16 bytes otherwise. 16 because that is the alignment `Layout` will ask for on aarch64
for anything containing a `u128` or a pointer pair, and returning something less aligned is
undefined behaviour rather than a performance problem. The header size is chosen to keep the first
slot aligned, which is a constraint on the header, not on the objects.

**The largest class.** Above some size, cutting a page up stops making sense: at 2048 bytes a page
holds one object and a header, and you have gained nothing over calling FRAMES. Linux's cutoff is
twice the page size, past which `kmalloc` forwards straight to the page allocator. Take the same
rule: above the largest class, round the request up to a whole number of pages and call FRAMES
with the matching order.

**Minimum class.** 8 bytes is the floor for the free list to work at all, because a free slot has
to hold a pointer, and a pointer is 8 bytes on this machine. That is a hard constraint, not a
policy: a 4-byte class cannot thread its own free list.

## 7. What you are building

One module, `src/slab.rs`.

- A slab header: how many slots are free, the head of this slab's free list, its size class, and
  links to the neighbouring slabs of the same class.
- A `Cache` per size class, holding the class size and the partial slab list.
- `Slab`-level `alloc` and `free` operating on one page.
- A top-level type owning the array of caches, with `alloc(&mut self, layout: Layout)` and
  `free(&mut self, ptr, layout)`, taking `&mut FRAMES` for the cases that need a new page or want
  to return one.

The interface takes `Layout`, not a size, because that is what `GlobalAlloc` will hand it next
skill and because alignment is part of the request rather than a detail. Rounding a `Layout` to a
class has to satisfy both its size and its alignment, and a class that satisfies the size but not
the alignment is the bug section 10 is about.

The hard part is section 5's pointer-to-slab question, and it is hard because it is invisible until
you write `free`. Answer it on paper first.

## 8. Testing it

Host-testable, over a `Frames` built on the leaked arena from `src/mmu/mod.rs:218`.

- A request rounds up to the class the table says, for every class boundary and for one byte
  either side of each.
- Every object handed out is inside its page, aligned to its class, and distinct from every other
  live object.
- Filling a slab exactly consumes one page from FRAMES and no more. Ask FRAMES for its free count
  before and after.
- One more allocation after that takes a second page.
- Freeing every object in a slab returns the page to FRAMES, and the free count comes back to what
  it was.
- Freeing all but one object does **not** return the page.
- Allocation and freeing in a scrambled order, repeatedly, never hands out the same address twice
  while it is live and always returns every page in the end. This is the SLAB counterpart of the
  scrambled test in FRAMES, and it is the one worth writing first.
- A request larger than the biggest class forwards to FRAMES and comes back page-aligned.
- A request with an alignment larger than its size gets something correctly aligned, which is the
  case a size-only class lookup gets wrong.

## 9. When nothing happens

| symptom | almost certainly |
| --- | --- |
| two live objects at the same address | the free list head was updated before the slot was read, or a slot was pushed onto the list twice by a double free. |
| the free list runs off into memory that is not the page | the chain was built with the wrong stride, or the header size was not subtracted before the first slot. |
| objects are correctly spaced but the first one overlaps the header | the first slot's offset is the header size rounded up to the class alignment, not the raw header size. |
| a `u128` or a pointer pair written to an allocation faults or corrupts | alignment. The class satisfied the size and not the `Layout`'s alignment. Section 6. |
| pages are never returned to FRAMES | the free counter per slab is not reaching zero, usually because it counts allocations rather than free slots, or because objects freed into a different class's slab are decrementing the wrong one. |
| memory use grows without bound under a steady workload | pages return to FRAMES but the caches keep their headers, or empty slabs stay on the partial list. An empty slab is not a partial slab. |
| freeing works for the first slab and faults for later ones | the pointer-to-slab step. If it is the masking trick, some slab is not page-aligned; if it is the lookup, the page frame number arithmetic is off by the arena base. |
| everything works and FRAMES reports the wrong free count | a page taken from FRAMES for a slab and then returned at the wrong order. Slabs are one page, so order 0, always. |

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
