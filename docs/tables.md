# TABLES - the tree that turns an address into a different address

Every address this kernel has ever used went straight onto the bus. `0x0900_0000` meant the
UART because the UART is wired at `0x0900_0000`. There has been no indirection anywhere, which
is why the machine has been so easy to reason about: `make mem ADDR=0x09000018` and the kernel
are looking at the same thing through the same eyes.

That ends here. A **translation table** is a tree in memory that the CPU's hardware walks on
every single load, store and instruction fetch, to convert the address the code used into the
address that reaches the bus. Once it is on, a pointer is a lookup key rather than a location.

This skill builds the tree and stops. `zheos-f27` turns it on. That split is deliberate and it
is the single most important thing about the ordering: with the MMU off you can build a
completely wrong set of tables and the machine will not so much as flinch, which means you get
to inspect and correct them with a working UART, a working timer and a working shell. The
instant you flip `SCTLR_EL1.M`, a mistake in bit 10 of one descriptor means the next
instruction fetch faults, the fault handler's own instruction fetch faults, and the machine is
gone with nothing on the screen.

So: build it, print it, walk it by hand, check the hex against numbers you computed on paper.
Then, next skill, turn it on.

Sections 3 and 4 are the ones that decide whether this works. Section 3 is what the hardware
actually does on every access. Section 4 is the 64 bits that tell it to.

---

## 1. Vocabulary

**Virtual address (VA)** - the number in your code, in a register, in the program counter. With
the MMU off there is no such thing; with it on, it is every address.

**Physical address (PA)** - the number that reaches the bus and selects RAM or a device. What
you have been using all along.

**Translation** - the mapping from one to the other. Not a function you call. A thing the
hardware does, invisibly, on every access, using tables you left in memory.

**Translation table** - one node of the tree. On this machine it is exactly one 4 KiB page
holding 512 entries of 8 bytes. Sometimes called a page table, or by its Linux level names
(pgd, pud, pmd, pte).

**Descriptor** - one 8-byte entry in a table. Section 4 is the whole of it.

**Level** - how deep in the tree. Levels are numbered 0, 1, 2, 3 going *down*, and each level
resolves 9 bits of the address. Level 0 is the root; level 3 is the leaf.

**Granule** - the smallest mappable unit, and the table size. 4 KiB here. The architecture also
offers 16 KiB and 64 KiB, which change every number in this document.

**Block descriptor** - a leaf that appears above level 3, mapping a large aligned chunk in one
entry: 1 GiB at level 1, 2 MiB at level 2. The reason a 128 MiB identity map is 64 entries
rather than 32768.

**Page descriptor** - a leaf at level 3, mapping one 4 KiB granule.

**Table descriptor** - a non-leaf, holding the physical address of the table one level down.

**Identity map** - a map where VA equals PA for every address. It changes nothing about what
any address means, which is exactly why it is the right first map: turning the MMU on with an
identity map is a no-op you can observe.

**TTBR0_EL1 / TTBR1_EL1** - the two registers holding the root table's physical address. The
CPU picks between them by the top bits of the VA: low addresses use TTBR0, high addresses
(`0xFFFF_...`) use TTBR1. Kernels normally live in TTBR1. You will use TTBR0 only, because your
kernel is at `0x4008_0000` and it is staying there.

**MAIR_EL1** - eight bytes in one register, each describing a *type* of memory: cacheable,
device, write-through. A descriptor does not carry the type; it carries a 3-bit index into
this register. Section 6.

**Access Flag (AF)** - one bit in every leaf descriptor. If it is zero, the access faults.
Section 11 exists mostly for this bit.

**TLB** - the cache of recently walked translations inside the CPU. Not this skill's problem
while the MMU is off, and the first thing to suspect once it is on and you change a table.

**Data abort** - the exception raised when a load or store cannot be translated or is not
permitted. Every mistake in this document arrives as one. `src/exception.rs` already prints
them; section 8's optional step makes them say why.

---

## 2. Where you are now

The machine, verified this session:

```
0x0000_0000 ┌──────────────────┐
            │ pflash 0 and 1   │  unused with -kernel
0x0800_0000 │ gic_dist         │
0x0801_0000 │ gic_cpu          │
0x0900_0000 │ pl011  ← UART    │   DEVICE MEMORY
0x0901_0000 │ pl031  rtc       │
0x0903_0000 │ pl061  gpio      │
0x0A00_0000 │ virtio-mmio x32  │
            ├──────────────────┤
            │   unmapped       │
0x4000_0000 ├──────────────────┤  RAM base, from /memory
            │ free, 512 KiB    │
0x4008_0000 │ kernel image     │  __image_start        NORMAL MEMORY
0x4008_dbd0 │ .. __stack_top   │
            │ free RAM         │
0x4400_0000 │ device tree blob │  1 MiB, from x0
0x4410_0000 │ free RAM         │
0x4800_0000 └──────────────────┘  RAM end
```

`PSTATE` says `EL1h`: exception level 1, using `SP_EL1`. EL1 is where a kernel belongs, and it
is the level whose translation regime you are about to configure. There is no EL2 or EL3
software in the way - QEMU dropped you straight here.

Two things you built are the reason this skill can happen now:

`board.memory` is `Region { 0x4000_0000, 0x0800_0000 }`, from the device tree. That is what to
map as RAM, and it is not a constant in your source.

`Bump` hands out 4 KiB-aligned physical memory. Page tables are 4 KiB and must be built out of
physical memory, because the thing that reads them is the hardware walker, which by definition
works underneath translation. That is the sentence from `docs/bump.md` section 10, and this is
where it gets cashed.

One thing you built that is about to become load-bearing in a way it has not been:
`Bump::alloc` **does not zero**. A table is 512 descriptors and you are going to write two of
them. The other 510 have to be zero, because zero means "invalid" and anything else means "here
is a page, go read it". Uninitialised memory in a translation table is the hardware following a
random pointer into a random address with random permissions. Zero the table yourself, in
`Table::new`, immediately.

---

## 3. What the hardware does on every access

This is the model. Everything else in this document is encoding details.

Take a 39-bit virtual address and cut it into four fields:

```
 38     30 29     21 20     12 11        0
┌─────────┬─────────┬─────────┬───────────┐
│ L1 idx  │ L2 idx  │ L3 idx  │  offset   │
│ 9 bits  │ 9 bits  │ 9 bits  │  12 bits  │
└─────────┴─────────┴─────────┴───────────┘
```

Then:

1. Read `TTBR0_EL1`. That is the physical address of the level 1 table.
2. Index it with the L1 field. 9 bits, 512 entries, 8 bytes each - so the offset into the table
   is `idx * 8`, which is why a table is 4096 bytes.
3. Look at the bottom two bits of the descriptor you found:
   - `0b00` or `0b10` - **invalid**. Translation fault. Done, badly.
   - `0b01` - **block**. The walk stops here. This entry maps the whole 1 GiB. Take the output
     address from the descriptor, glue on the low 30 bits of the VA, done.
   - `0b11` - **table**. Take the next-level table address out of the descriptor, go to step 4.
4. Same thing one level down with the L2 field. A block here is 2 MiB.
5. Same again with the L3 field. At level 3 there are no blocks - `0b11` means a 4 KiB **page**,
   and `0b01` is invalid. This is a genuine architectural wart and it catches everyone once.
6. Physical address = the leaf's output address, plus the low bits of the VA that the leaf's
   size does not cover.

Then it checks the permissions and the memory type from the leaf, and either lets the access
through or raises an abort. Alongside that it fills a TLB entry so the next access to the same
page skips steps 1-6 entirely.

The whole tree is nothing more than a 512-way trie keyed on the address, with an early-out at
every node. That early-out is what makes it practical: a 1 GiB block is one descriptor, and the
alternative encoding of the same fact is 262144 leaf pages in 512 tables.

### The shape you are building

The address space you care about ends at `0x4800_0000`, which is under 2 GiB. There is no point
having 48 bits of VA to describe it. Set `TCR_EL1.T0SZ = 25`, giving a 39-bit VA space, and
with a 4 KiB granule the walk **starts at level 1** - level 0 does not exist and you never
allocate it.

So the root table is one page, 512 entries, each covering 1 GiB. Two of those 512 entries do
all the work:

```
L1[0]  covers 0x0000_0000 .. 0x4000_0000   → all devices      → Device block
L1[1]  covers 0x4000_0000 .. 0x8000_0000   → all RAM          → Normal block
L1[2..512]                                  → invalid, i.e. zero
```

Two descriptors, one 4 KiB allocation, and every address the kernel currently touches is
identity-mapped with the correct memory type. That is a complete and correct first map, and it
is milestone A in section 8.

It is also too coarse in one specific way, which is milestone B: entry 1 maps the whole
gigabyte `0x4000_0000..0x8000_0000` as RAM, but only the first 128 MiB is real. A stray pointer
to `0x4800_0000` would be translated successfully and go to a hole in the bus instead of
faulting. Replacing that one block with a table descriptor and 64 × 2 MiB blocks at level 2
maps exactly the RAM that exists and leaves the rest invalid - and it is the step where you
actually build a *tree* rather than an array.

### Why not 48-bit VA, or 64 KiB granule

`T0SZ = 16` gives the full 48 bits and adds a level 0 table containing exactly one non-zero
entry. It is one more allocation and one more indirection to describe nothing. Linux does it
because Linux needs the address space; you do not.

A 64 KiB granule needs fewer levels but its level 2 block is 512 MiB, which is bigger than your
RAM. It also makes every published example and every Linux constant not apply. 4 KiB is what
everything assumes.

`T0SZ = 32` (a 32-bit VA, so a level 1 table with just 4 entries and 32 bytes long) is legal
and even smaller. Skip it - a truncated root table has its own alignment rule and you would be
the only person on the internet with one.

---

## 4. The descriptor

64 bits. Here is a leaf - a block or a page - with only the fields that matter at EL1 on this
machine:

```
 63  59 58    55 54  53  52 51        47                12 11 10 9 8 7 6 5 4  2 1 0
┌──────┬────────┬───┬───┬───┬──┬────────────────────────┬──┬──┬───┬───┬─┬─────┬───┐
│      │  soft  │UXN│PXN│Con│  │     output address     │nG│AF│ SH│ AP│ │Attr │ 01│
└──────┴────────┴───┴───┴───┴──┴────────────────────────┴──┴──┴───┴───┴─┴─────┴───┘
```

Bottom up:

**bits[1:0] - what this is.** `0b01` block, `0b11` table-or-page, anything even is invalid.
Getting this wrong at level 3 is the wart from section 3.

**bits[4:2] - AttrIndx.** Which of MAIR_EL1's eight byte-fields describes this memory. Not the
type itself, an index into a table of types. Section 6.

**bit[5] - NS.** Non-secure. You are already in non-secure state, where it is ignored. Zero.

**bits[7:6] - AP[2:1] - permission.**

| value | EL1 | EL0 |
| --- | --- | --- |
| `0b00` | read/write | no access |
| `0b01` | read/write | read/write |
| `0b10` | read only | no access |
| `0b11` | read only | read only |

`0b00` for everything, for now. There is no EL0 until `zheos-gnt`, and read-only text is a
`zheos-f27` refinement - do not add permissions to a map you have not yet proven translates.

**bits[9:8] - SH - shareability.** `0b11` inner shareable for Normal memory, which is what
makes the cache coherent with other cores and with DMA. `0b00` for Device, where it is ignored
because Device accesses are never cached in the first place. Getting this wrong on Normal
memory is invisible on one core and a disaster on `zheos-5x5`.

**bit[10] - AF - Access Flag.** Set it to 1. If it is 0, every access through this descriptor
raises an Access Flag fault, immediately, unconditionally. The hardware's intent is that an OS
leaves it clear and uses the resulting faults to learn which pages are being used; you have no
such OS, so a zero here is purely a bug. It is the single most common reason a first page table
does not work, it costs one bit, and it looks exactly like "the MMU is broken".

**bit[11] - nG - not global.** Zero: this mapping belongs to every address space. Relevant once
there is more than one.

**bits[47:12] - output address.** The physical address, with the low bits implied zero. For a
level 3 page that means bits[47:12] and 4 KiB alignment. For a **level 2 block** the low 21
bits must be zero, and for a **level 1 block** the low 30 bits. The hardware does not mask them
for you and the architecture calls a non-zero value there reserved. In practice you build the
descriptor as `pa | flags`, so a misaligned `pa` silently corrupts the flags underneath it.

**bit[52] - Contiguous.** A hint that this and 15 neighbours are one run, so the TLB can hold
them in one entry. An optimisation. Zero.

**bits[54:53] - UXN and PXN - execute never.** UXN blocks execution at EL0, PXN at EL1. Set
both on device memory - speculatively fetching instructions from a UART FIFO is a genuine
hazard, not a theoretical one. On RAM, PXN must be 0 or your kernel cannot run; UXN is 1
because EL0 does not exist yet.

A **table descriptor** is much simpler: `next_table_physical_address | 0b11`, and the upper
bits `[63:59]` hold NSTable/APTable/UXNTable/PXNTable, which restrict everything below. Leave
them zero and permissions are decided entirely at the leaves, which is far easier to reason
about.

### Two worked values

The two milestone-A descriptors, so you have something to check against.

Device block, level 1, output `0x0000_0000`, AttrIndx 1:

```
  0b01                  = 0x0000_0000_0000_0001   block
  AttrIndx = 1  (<<2)   = 0x0000_0000_0000_0004
  AF = 1       (<<10)   = 0x0000_0000_0000_0400
  PXN = 1      (<<53)   = 0x0020_0000_0000_0000
  UXN = 1      (<<54)   = 0x0040_0000_0000_0000
  output address        = 0x0000_0000_0000_0000
                        ─────────────────────────
                          0x0060_0000_0000_0405
```

Normal block, level 1, output `0x4000_0000`, AttrIndx 0:

```
  0b01                  = 0x0000_0000_0000_0001   block
  AttrIndx = 0          = 0x0000_0000_0000_0000
  SH = 0b11     (<<8)   = 0x0000_0000_0000_0300
  AF = 1       (<<10)   = 0x0000_0000_0000_0400
  UXN = 1      (<<54)   = 0x0040_0000_0000_0000
  output address        = 0x0000_0000_4000_0000
                        ─────────────────────────
                          0x0040_0000_4000_0701
```

Those two numbers are the acceptance criterion of this skill. When `make mem ADDR=<root> N=2
FMT=xg` prints them, the tables are built. Note the byte order the monitor shows you - `xp
/2xg` prints them as 64-bit values, so they read the same way round as above.

---

## 5. What Linux does, and what to keep

The Linux anchor for this skill is `arch/arm64/mm/mmu.c`, not `mm/`. The relevant function is
`__create_pgd_mapping()`, which descends `alloc_init_pud` → `alloc_init_pmd` → `alloc_init_cont_pte`,
allocating each missing table with a callback - and during boot that callback is
`early_pgtable_alloc()`, which calls `memblock_phys_alloc()`. Linux's page table builder sits
directly on the allocator you wrote last skill, for exactly the reason you wrote it.

Its central optimisation is `use_1G_block()` / `pmd_set_huge()`: at each level, if the region
being mapped is aligned to that level's block size and at least that big, install a block and
stop descending. Otherwise allocate the next table down. That single rule is what turns "map
128 MiB" into 64 descriptors instead of 32768, and it is the one piece of the algorithm worth
copying verbatim.

| Linux arm64 has | you build | why |
| --- | --- | --- |
| 4 levels, 48-bit VA, TTBR1 | 2 levels, 39-bit VA, TTBR0 | Nothing lives above 2 GiB and nothing has moved. |
| a separate `idmap_pg_dir` | nothing | The idmap exists to keep the few instructions around `msr sctlr_el1` addressable while the world changes underneath them. Your whole map is an identity map, so the problem it solves does not occur. Name this - it is the reason your `zheos-f27` is going to be six instructions. |
| `swapper_pg_dir` in `.bss` | one `Bump` allocation | Linux needs a root table before memblock is usable. Your `Bump` is already up. |
| `pgprot_t`: `PAGE_KERNEL`, `PAGE_KERNEL_RO`, `PAGE_KERNEL_EXEC` | `Normal` and `Device` | Permissions are `zheos-f27`. Two memory types is the minimum that is not wrong. |
| block-or-descend at every level | block-or-descend at every level | The one rule to keep. |
| `map_mem()` walking every memblock region, honouring `NOMAP` | one `/memory` region | One bank, no NOMAP, as in `docs/bump.md`. |
| break-before-make when changing a live mapping | nothing | Changing a mapping the TLB already holds needs invalidate-then-write-then-invalidate or the hardware may see both old and new. You are building a map that has never been live. Becomes real the first time you change one after `zheos-f27`. |
| `dsb ishst` after writing tables | one `dsb` before enabling | Section 10. |
| contiguous-bit runs, `CONFIG_RODATA_FULL`, KPTI, KASAN shadow | nothing | Each one is a response to a problem you can name and do not have. |

The rows worth keeping are: the block-or-descend rule, tables allocated from the boot allocator,
and two memory types. Everything else is deferred with a reason.

---

## 6. MAIR, and why the memory type is not optional

A descriptor's AttrIndx is 3 bits, so there are 8 possible memory types, and their definitions
live in `MAIR_EL1` - eight bytes, byte *n* defining type *n*.

The two you need:

| index | MAIR byte | meaning |
| --- | --- | --- |
| 0 | `0xFF` | Normal, inner and outer write-back, read+write allocate, non-transient |
| 1 | `0x04` | Device-nGnRE |

so `MAIR_EL1 = 0x0000_0000_0000_04FF`. Writing it is `zheos-f27`'s job, but the indices are
baked into descriptors you write here, so decide now and put the constants in this module.

`0xFF` is the fully cacheable case: the outer and inner fields are both `0b1111`, meaning
write-back with both allocate hints. Nothing subtle, just an unmemorable encoding.

`Device-nGnRE` unpacks as **n**on-**G**athering, non-**R**e-ordering, **E**arly write
acknowledgement. Those three properties are the entire reason this distinction exists, and each
one of them would break your UART if it were wrong:

- **Gathering** would let the CPU merge two byte writes to `UARTDR` into one halfword write.
  Two characters become one.
- **Re-ordering** would let it write `UARTDR` before checking `UARTFR`, or hoist a `UARTFR`
  read out of your spin loop, which is precisely the hang your `read_volatile` exists to avoid.
  `volatile` constrains the compiler; the memory type constrains the CPU. You need both.
- **Early acknowledgement** is the only one that is a relaxation: the write may be signalled
  complete before it reaches the device. `Device-nGnRnE` (`0x00`) waits for the endpoint. Either
  works for a PL011; `nGnRE` is what Linux uses for ordinary MMIO and `nGnRnE` is what it uses
  where it wants the strictest possible ordering.

The failure mode if you map the UART as Normal cacheable is worth predicting before you cause
it: writes land in a cache line and never reach the device, so output stops - but not
immediately, and not tidily. Some of it appears when the line is eventually evicted, out of
order, mixed with a stale read of `UARTFR` that never sees the FIFO drain. It looks like the
UART broke rather than like the map is wrong.

Conversely, mapping RAM as Device is not a correctness failure, it is a performance one, plus a
familiar side effect: **Device memory forbids unaligned access**. That is the `ESR` DFSC `0x21`
already in `CLAUDE.md`. It is the state you are in right now, with the MMU off - all memory
behaves as Device-nGnRnE. So the moment `zheos-f27` succeeds, unaligned loads start working,
and that is a usable smoke test that the map really took effect.

---

## 7. The shape of it

```rust
pub enum Memory {
    Normal,
    Device,
}

pub struct Table {
    entries: NonNull<u64>,
}

pub struct Tables {
    root: Table,
}

impl Tables {
    pub fn build(bump: &mut Bump) -> Option<Tables>;
    pub fn identity_map(&mut self, region: Region, kind: Memory, bump: &mut Bump)
        -> Result<(), MapError>;
    pub fn root(&self) -> usize;
    pub fn translate(&self, va: usize) -> Option<(usize, u64)>;
}
```

### Notes on each piece

**`identity_map` rather than `map`.** There is no VA parameter because VA equals PA, by
definition, for every mapping this kernel has. A general `map(va, pa, ...)` is two more
parameters and one more thing that can disagree. Add it when something actually moves - which
is a real event, and it is called `zheos-9ka` growing a linear map, or `zheos-f27` deciding to
run the kernel out of TTBR1.

Skipped: the VA parameter. Add when a mapping is not an identity.

**`Region`, again.** `src/region.rs` already holds `(base, size)` with `end()` and
`is_overlapping()`. `identity_map(board.memory, Memory::Normal, ..)` reads as the thing it is.
No new address types.

**`bump` passed in, not stored.** `Tables` needs to allocate only while it is being built. A
stored `&mut Bump` would borrow the allocator for the lifetime of the tables and lock out every
other user, which is the borrow checker correctly objecting to a design where a long-lived
struct owns a shared resource. Pass it per call.

**Indices are computed, never passed.** This is the one design decision that carries weight,
because of your no-panic rule.

```rust
const fn index(va: usize, level: u8) -> usize {
    let shift = 12 + 9 * (3 - level as usize);
    (va >> shift) & 0x1FF
}
```

The `& 0x1FF` means the result is structurally in `0..512`. If `Table::set` takes only a value
produced this way, there is no bounds check to fail, no `Result` to invent for a case that
cannot happen, and no `panic!` in the emitted code. Compare with a `set(&mut self, i: usize,
..)` taking a caller-supplied index: now you owe an error path for something that is a bug
rather than a condition. Design the index out and the question does not arise. `make asm` will
show you the difference - a masked index emits no compare and no branch to a panic block.

**`Table::new` zeroes.** `bump.alloc(Layout::from_size_align(4096, 4096))` gives you 4096 bytes
of whatever was there. `write_bytes(ptr, 0, 4096)` immediately, before anything else touches it,
because 510 of those descriptors are going to stay untouched and they must read as invalid.
This is the one line whose absence produces the most spectacular failures in `zheos-f27`.

`Layout::from_size_align` returns a `Result`, and `Layout::from_size_align_unchecked` is unsafe.
Neither is a problem: `?` the `Result` into your `Option`/`Err`, or note that
`Layout::new::<[u64; 512]>()` gives you size 4096 with alignment 8 - which is *not* enough, so
you do want the explicit 4096 alignment. That mismatch is worth noticing rather than
discovering.

**`build` returns `Option`, `identity_map` returns `Result`.** Same split as `Bump`: running out
of memory is a condition, and a caller asking to map something unmappable is worth naming. The
`MapError` cases that actually exist:

- out of memory - a table allocation failed
- the region's base or size is not a multiple of 4096 - you cannot map a fraction of a granule
- nothing to map - a zero-size region, which is probably a bug upstream

There is no panic anywhere in that list, and `kmain` already has the pattern: print what failed
and `halt()`.

**`translate` is a test, and it is the deliverable.** Given a VA, walk your own tables the way
section 3 says the hardware would, and return the PA and the leaf descriptor. Thirty lines. It
is how you satisfy "inspectable" with the MMU off, it is how you check the map without
believing your own construction code, and it is the tool you will want at 1am during
`zheos-f27` when one address in the middle of RAM aborts.

The honest caveat: a walker that shares `index()` with the builder will agree with a wrong
`index()`. It catches descriptor-format bugs and structure bugs, which is most of them, but not
a shift that is wrong in both places. The defence against that one is section 9's hand
arithmetic, not more code.

### The mapping algorithm

Linux's rule, kept whole:

```
map(region, kind):
  addr = region.base
  while addr < region.end():
      remaining = region.end() - addr
      if addr is 1 GiB aligned and remaining >= 1 GiB and level allows:
          write a level 1 block; addr += 1 GiB
      else if addr is 2 MiB aligned and remaining >= 2 MiB:
          descend to level 2, allocating the table if the L1 entry is empty
          write a level 2 block; addr += 2 MiB
      else:
          descend to level 3, allocating tables as needed
          write a page; addr += 4 KiB
```

"Descend, allocating if the entry is empty" is the one subtlety: if `L1[i]` is already a
*block*, you cannot descend through it, and turning it into a table means splitting it into 512
level 2 entries covering the same range. Linux handles this. Do not - return a `MapError` and
let the caller not do that. Map devices before RAM, or RAM before devices, in disjoint regions,
and the case never arises.

For milestone A the whole loop collapses to two iterations of the first branch. For milestone B,
mapping `board.memory` (128 MiB, 1 GiB-aligned base, but only 128 MiB long) takes branch two 64
times. Both fall out of the same code, which is why writing the general loop is cheaper than
writing the special case twice.

---

## 8. Bring-up order

Each step prints something before the next one starts. Nothing here turns the MMU on.

**Step 1 - one page, zeroed, from `Bump`.** `Table::new`: allocate 4096/4096, zero it, print the
address. Check the low twelve bits are zero, then `make mem ADDR=<that> N=8 FMT=xg` and confirm
eight zeros. You have just used your allocator for its actual customer, and you have proved the
zeroing works - which nothing so far has.

**Step 2 - the two block descriptors, by hand.** Build the constants from section 4 and write
them into `L1[0]` and `L1[1]` directly, with no loop and no `identity_map`. Print them. Compare
against `0x0060_0000_0000_0405` and `0x0040_0000_4000_0701`. Then `make mem` on the root and
confirm the monitor sees the same two values in RAM.

This is milestone A: a complete, correct identity map of everything this kernel touches, in two
entries. It is worth having existed even though the next step generalises it away, because
every number in it is one you computed yourself.

**Step 3 - `translate`.** The walker from section 7. Feed it four addresses and check each
against what you work out on paper:

```
0x0900_0000  → 0x0900_0000, via L1[0], Device
0x4008_0000  → 0x4008_0000, via L1[1], Normal   (your own .text)
0x4400_0000  → 0x4400_0000, via L1[1], Normal   (the DTB)
0x9000_0000  → None                             (L1[2] is zero)
```

That last line is the one that proves invalid entries read as invalid, and it is why zeroing
mattered.

**Step 4 - the general `identity_map`, still at level 1.** Replace step 2's two hand-written
stores with two calls, and get the same two descriptors out. Same hex, different code path. If
the numbers move, the loop is wrong and you have the previous values to diff against - which is
the entire reason step 2 exists as a separate step.

The device region is a judgement call: `virt`'s devices span `0x0800_0000` to about
`0x0A00_4000`, so the honest `Region` is not the whole gigabyte. At level 1 you have no choice
but to round to 1 GiB. Map `Region { 0, 0x4000_0000 }` and note in the code that this is
level-1 granularity, not a claim about the machine.

**Step 5 - descend to level 2.** Now map `board.memory` - 128 MiB - instead of a hardened
gigabyte. `L1[1]` becomes a table descriptor pointing at a second allocation; that table gets
64 block descriptors of 2 MiB each; entries 64..512 stay zero. Then:

- `translate(0x4008_0000)` still gives `0x4008_0000`, now through two levels.
- `translate(0x4800_0000)` gives `None`, where it used to succeed. That change is the point of
  the whole step.
- The root's `L1[1]` is now `<l2 physical> | 0x3`. Check it with `make mem`.
- `bump.remaining()` has dropped by two pages rather than one.

This is milestone B and it is the deliverable. You now have a tree, a table descriptor, two
allocations from `Bump`, and a map that describes the machine rather than a rounding of it.

**Step 6 - print the map.** A loop over the root that skips zero entries and prints level, index,
covered VA range, output address and decoded type. Ten lines, and it turns "inspectable" from a
promise into output you can paste into a commit message. It is also the thing you will read
first when `zheos-f27` misbehaves.

Steps 1-3 are one sitting. Steps 4-6 are another.

### Optional step 7: decode the data abort

From `zheos-dmu`'s notes, and it pays for itself the moment `zheos-f27` starts.

Every mistake in this document arrives as a data abort or an instruction abort, and
`src/exception.rs` currently prints the ESR as one opaque hex number. For EC `0x25` and `0x21`
the low bits of the ISS are the whole answer:

- **bit 6, WnR** - 1 if a write, 0 if a read.
- **bits 5:0, DFSC** - the fault status:

| DFSC | meaning |
| --- | --- |
| `0b0000LL` (`0x00`-`0x03`) | address size fault at level LL |
| `0b0001LL` (`0x04`-`0x07`) | **translation fault** - nothing mapped |
| `0b0010LL` (`0x08`-`0x0B`) | access flag fault - you left bit 10 clear |
| `0b0011LL` (`0x0C`-`0x0F`) | **permission fault** - mapped, but not like that |
| `0x21` | alignment fault |
| `0b0101LL` | external abort on a table walk - your table pointer is garbage |

The low two bits are the level the walk got to, which localises the bug to a table. A
translation fault at level 1 means the root entry is zero; at level 2 it means you descended
correctly and the leaf is missing. That is the difference between "I never mapped it" and "I
mapped it into the wrong table", and it is free.

`FAR_EL1` is already being printed and gives you the address, so with DFSC you have both halves.

### Deliberately not doing here: the exception stack

`zheos-dmu`'s notes also propose splitting `SP_EL0` and `SP_EL1` at this skill so exceptions run
on their own stack. The mechanism is small - `msr spsel, xzr` so the kernel runs on `SP_EL0`,
point `SP_EL1` at a second stack, and move the handler from vector slot `0x200` to `0x000`.

Leave it for `zheos-f27`. The whole value of a separate exception stack is that a stack overflow
faults instead of quietly eating `.bss`, and that requires an *unmapped guard page* below the
stack - which requires the MMU to be on. Doing it here gets you the machinery with none of the
protection, and adds a second variable to whatever goes wrong when you flip `SCTLR_EL1.M`.

---

## 9. Proving it works

**Every number is checkable by hand, and that is the method.** There is no `info tlb` in this
QEMU's monitor for aarch64 - checked this session - so there is no oracle. The check is: compute
the descriptor on paper, print it from Rust, and read it back out of RAM with `make mem`. Three
independent views of the same 64 bits.

**The two constants.** `0x0060_0000_0000_0405` and `0x0040_0000_4000_0701`. If those appear at
the root, the encoding is right.

**`make mem` agrees with Rust.** `make mem ADDR=<root> N=4 FMT=xg`. Rust printing what Rust
wrote proves the pointer arithmetic; the monitor proves the bytes are in RAM at the address you
are about to put in `TTBR0_EL1`.

**`translate` round-trips the identity.** For every address in the four-line table in step 3,
`translate(va)` gives back `va`. An identity map is the only kind where the test is this cheap,
which is another argument for building it first.

**Unmapped means `None`.** `translate(0x9000_0000)` and, after step 5,
`translate(0x4800_0000)`. A map that never says no is a map where you have not actually looked
at bit 0.

**The root is 4 KiB aligned.** `root & 0xFFF == 0`. `TTBR0_EL1` ignores the low bits rather than
complaining, so a misaligned root silently points somewhere else. Print the mask, not an
assertion - a non-zero value tells you how far off, which usually names the bug.

**The tables do not overlap anything.** They came from `Bump`, so they are inside RAM and
outside the image and the DTB by construction. Worth printing the two table addresses next to
`bump::image()` and `dtb.region()` once, because "by construction" is a claim about code you
wrote last week.

**The second table's address appears in the first table.** After step 5, `L1[1] >> 12 << 12`
must equal the level 2 table's address, and `L1[1] & 0x3` must be `0x3`. One line, and it is
the only check that the table descriptor is a table descriptor rather than a block that happens
to point at a table - the difference is one bit, and both are "valid".

**Nothing changed.** The MMU is off. The shell still echoes, the timer still ticks, `zhemon`
still works. If anything at all behaves differently after this skill, you wrote outside your
allocation, and the tables are the least of it.

**One runnable check.** Same as `docs/bump.md`: there is no test harness (`zheos-smj`, still not
a dependency). A `fn self_check()` over a `Tables` built on a synthetic `Bump` - a fake arena
with fake regions - runs the whole builder and the whole walker at boot in microseconds and
prints one line. `Bump::new` takes values, which is what makes this possible, and it is the
second time that choice has paid.

---

## 10. What happens when you turn it on

Not this skill. But the shape of `zheos-f27` decides whether this skill's output is the right
shape, so it is worth having straight.

```
MAIR_EL1  = 0x0000_0000_0000_04FF     the two types from section 6
TCR_EL1   = T0SZ 25, TG0 4 KiB, SH0 inner, IRGN0/ORGN0 write-back,
            EPD1 = 1 (no TTBR1 walks at all), IPS from ID_AA64MMFR0_EL1
TTBR0_EL1 = tables.root()
    isb
SCTLR_EL1 |= M | C | I
    isb
```

Four registers and two barriers. Three things about it:

**The `isb` before is not optional and the `isb` after is the interesting one.** The instruction
after `msr sctlr_el1` has already been fetched, decoded and possibly speculated, using the old
regime. `isb` throws that away and re-fetches - through the MMU. That instruction's address is
the first virtual address the machine ever uses, and it is `pc + 4`, in your `.text`, at
`0x4008_xxxx`. It resolves because your map is an identity map. This is the whole job of
Linux's `idmap_pg_dir`, and you got it for free by never moving anything.

**`TCR_EL1.TG0` uses a different encoding from `TG1`.** `00` is 4 KiB for TG0; `10` is 4 KiB for
TG1. There is no reason for this and it costs people an afternoon.

**A `dsb` before the `msr`,** so the table writes are complete before the walker can be asked to
read them. With the MMU off your stores are Device memory and effectively already ordered, so
this is belt-and-braces here - and it stops being belt-and-braces the moment caches are on and
you edit a live table.

The consequence for this skill: `root()` returns a plain physical `usize`, the map covers the
code that will be running at that instant, and nothing about the tables needs to change when
the MMU comes on. If any of those three were false, the design would be wrong now rather than
later.

---

## 11. When nothing happens

The MMU is off through this whole skill, so nothing will crash. That is the good news and it is
also why the table below is mostly about wrong numbers rather than dead machines - the crashes
are all deferred to `zheos-f27`, where they will be caused by whatever you get wrong here.

| symptom | almost certainly |
| --- | --- |
| descriptor is `...0404` instead of `...0405` | you built the flags and forgot bits[1:0]. An even descriptor is invalid, and invalid is silent until the MMU is on. |
| descriptor is right but the map does not work in `zheos-f27` | bit 10, AF. It is the default answer. Section 4. |
| `translate` returns `None` for an address you mapped | the shift in `index()`. Level 1 shifts by 30, level 2 by 21, level 3 by 12. `12 + 9 * (3 - level)`. |
| `translate` returns the right page but the wrong offset | you glued on 12 bits of VA for a block. A level 1 block keeps 30 bits of offset, a level 2 block 21. |
| the level 2 table address in `L1[1]` is off by a bit or two | you `or`ed `0b11` into an address whose low bits were not zero, or wrote `pa >> 12` when the descriptor wants `pa` itself in bits[47:12]. |
| a table full of plausible-looking garbage | `Table::new` did not zero. `Bump::alloc` does not, by design, and `docs/bump.md` section 7 says so. |
| the root address is not 4 KiB aligned | `Layout::new::<[u64; 512]>()` has alignment 8, not 4096. Section 7. |
| entry 511 is non-zero and you never wrote it | you wrote past the table. 512 entries, `& 0x1FF`, and the allocation is 4096 bytes not 512. |
| `identity_map` loops forever | the advance in the loop. Every branch must add its own block size, and the branch that descends still advances by 2 MiB, not by the region size. |
| `MapError` on a region you believe is fine | `board.memory.size` is `0x0800_0000`, which is 2 MiB aligned; the *device* region is not. Round it or map it at level 1. |
| mapping RAM appears to succeed but only maps 1 GiB | 128 MiB is not 1 GiB, so the "block at this level" test must check the remaining length as well as the alignment. Checking alignment alone maps a gigabyte when asked for 128 MiB. |
| the kernel dies during this skill | you wrote to a table pointer you did not get from `Bump`, or to one you got and then overwrote the variable. Nothing else in this skill touches memory. |
| output goes strange after step 5 | you mapped over the UART, i.e. the device region got `Memory::Normal`. Only observable in `zheos-f27`, so if you see it now, it is not this. |
| `data abort, DFSC 0x21` | unaligned access, MMU still off, as always. A `u64` store to a table entry needs 8-byte alignment, which a 4 KiB-aligned table plus an 8-byte stride always gives you - so if you see it, the base is not the base. |

The general move is the same as last skill: `make mem` on the root table and read the hex. Two
descriptors, computed by hand in section 4, sitting at an address you printed. If it does not
match, the bug is in the few lines between them.

---

## 12. Deliberately left out

**Turning it on.** `zheos-f27`. Section 10 is a preview so the interface is right, not an
instruction.

**Permissions.** Read-only `.text`, non-executable `.data`, a guard page under the stack. All
require level 3 granularity over the image, all are genuinely worth having, and all belong after
the map is proven to translate at all. Adding permissions to a map that has never been live
means debugging two things.

**TTBR1 and a higher-half kernel.** The reason every real kernel lives at `0xFFFF_...` is that
user space then owns the low half and a context switch swaps one register. There is no user
space until `zheos-gnt`.

**Splitting a block into a table.** Needed only if you map overlapping regions at different
granularities. Return a `MapError` instead and map disjoint regions.

**Break-before-make.** The protocol for changing a mapping the TLB may already hold. There is no
TLB in play while the MMU is off and no live mapping to change. It becomes mandatory the first
time you edit a table after `zheos-f27`.

**Cache maintenance.** With the MMU off the stores are already reaching memory. One `dsb` at the
handoff covers it. Real `dc civac` by-VA maintenance becomes a topic when there are two agents
looking at the same memory, which means `zheos-5x5` or DMA.

**The contiguous bit, and TLB shootdown, and ASIDs.** Optimisation, multiprocessing, and
multiple address spaces respectively. None exist yet.

**64 KiB and 16 KiB granules, and 48-bit VA.** Section 3.

**Hardware access-flag and dirty-bit management** (`TCR_EL1.HA`/`HD`). Lets the hardware set AF
for you. You are setting it to 1 permanently, so there is nothing to manage.

**Mapping the PCIe windows at `0x4010_0000_0000`.** Above the 39-bit VA space you chose, so
mapping them would require the level 0 table you skipped. Nothing uses them.

---

## 13. Done when

- One 4 KiB-aligned table comes out of `Bump`, fully zeroed, and `make mem` shows the zeros.
- `L1[0]` reads `0x0060_0000_0000_0405` and you can name every set bit.
- After step 5, `L1[1]` is a table descriptor whose address is the level 2 table's and whose low
  two bits are `0b11`, and the level 2 table holds 64 block descriptors and 448 zeros.
- `translate` returns `va` for the UART, the image, the stack and the DTB, and `None` for
  `0x4800_0000` and `0x9000_0000`.
- The device region is Device-typed and the RAM region is Normal-typed, and the two AttrIndx
  values match the MAIR you will write in `zheos-f27`.
- Every address and size in the call comes from `board.memory`, the linker, or the blob. No RAM
  addresses in `tables.rs`; the device base `0x0` and its 1 GiB length are the only literals, and
  they are level-1 granularity rather than a claim about the machine.
- No `panic!`, no `unwrap`, no `assert!` in the module. Out of memory is an `Option`, a bad
  region is a `MapError`, and the table index cannot be out of range because it is masked.
- `make lint` is clean, one `self_check` runs at boot, and the shell still works.
- You can say out loud: what the hardware does between a `ldr` and the bus, why a block
  descriptor exists, why bit 10 must be 1, why the UART must not be Normal memory, and why an
  identity map means `zheos-f27` does not need Linux's `idmap_pg_dir`.

The last one is the one that matters.

---

## Optional reading

- **ARM Architecture Reference Manual (ARMv8-A), section D8** - the VMSA. D8.2 is the address
  translation process and D8.3 is the descriptor formats. It is the only normative source for
  every bit in section 4, and the tables of "start level for a given T0SZ and granule" are worth
  finding once and never re-deriving.
- **`arch/arm64/mm/mmu.c`** in the Linux source. `__create_pgd_mapping`, `alloc_init_pud`,
  `use_1G_block`. Forty lines carry the whole block-or-descend rule.
- **`arch/arm64/include/asm/pgtable-hwdef.h`** - Linux's names for every bit in section 4
  (`PTE_AF`, `PMD_SECT_S`, `PTE_ATTRINDX`). Useful as a second opinion on your own constants.
- **`arch/arm64/mm/proc.S`**, `__cpu_setup` and the `TCR_EL1` assembly. The seven instructions
  `zheos-f27` is going to be, with all the `CONFIG_` noise that does not apply to you.
- **`arch/arm64/kernel/head.S`** - `create_idmap`, and the comment explaining why it exists.
  Read it to confirm you do not need it.
- **`hw/arm/virt.c`** and **`target/arm/ptw.c`** in QEMU. The second is the page table walker
  itself, and it is far more readable than the ARM ARM if you want to check what a specific bit
  combination actually does.
