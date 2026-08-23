# DTB - the machine describing itself

Every address this kernel knows, you typed in by hand. `board.rs` is five constants copied off
`info mtree -f` and out of `virt.dts`. That works because QEMU's `virt` machine never moves
anything. It stops working the moment the machine is not exactly this machine - a different
`-m`, a different QEMU version, real hardware.

The device tree is where those constants come from. It is a data structure the firmware hands
the kernel at boot that says, in the machine's own words, "here is your RAM, here are your
devices, here are their addresses and interrupt numbers". Reading it is how a single kernel
binary boots on a thousand different boards.

This skill reads one thing out of it: the real RAM base and size. That is a small target on
purpose. The parser you write to reach it is the whole job, and once it works, every other fact
in the tree is one more call away.

Section 3 is the one to read first. It is the reason your first attempt would otherwise fail
with no output and no clue why.

---

## 1. Vocabulary

**Device tree** - a tree of nodes describing hardware. Not code, not executable, just data. One
node per device, nested to mirror how the buses nest.

**Node** - one thing. It has a name, some properties, and children. `/memory@40000000` is a node.

**Property** - a name and a bag of bytes. `reg = <0x00 0x40000000 0x00 0x8000000>` is a property.
The tree format does not know what those bytes mean. The meaning comes from a _binding_, which
is a document, not something in the file.

**DTS** - the text form, what `dtc` prints. Human-readable. `virt.dts` in this repo is one.

**DTB** - the binary form, also called the FDT (flattened device tree). This is what is actually
in memory at boot. Same information, packed for a parser with no allocator.

**Flattened** - the tree is stored as a flat byte stream with begin/end markers instead of
pointers. That is what makes it parseable in place, with no allocation and no fixups. You walk
it; you never build it.

**Cell** - a 32-bit big-endian number. The unit that numeric properties are made of. An address
that needs 64 bits is stored as _two_ cells, not as one 64-bit value.

**`#address-cells` / `#size-cells`** - two properties that say how many cells the children of
this node use for an address and for a length. They live on the **parent**. This is the part
that trips everyone.

**`reg`** - the property that says where a device lives. A list of (address, length) pairs, each
one sized by the parent's cell counts.

**`compatible`** - a list of strings naming what the device _is_, most specific first. This is
how a driver finds its hardware: not by address, by name. `"arm,pl011"` is how you would find
your UART.

**Unit address** - the number after the `@` in a node name. It is a copy of the first address in
`reg`, there so sibling names stay unique. Never parse it; parse `reg`.

**Big-endian** - most significant byte first. The whole DTB is big-endian, always, on every
machine, including this little-endian one. Every multi-byte number in it needs swapping.

---

## 2. Where you are now

`board.rs`, in full:

```rust
pub const UART_BASE: usize = 0x0900_0000;
pub const GICD_BASE: usize = 0x0800_0000;
pub const GICC_BASE: usize = 0x0801_0000;
pub const UART_INTID: u32 = 33;
pub const PSCI_SYSTEM_OFF: u64 = 0x8400_0008;
pub const TIMER_INTID: u32 = 30;
```

Six facts, all true, none checked. RAM base is not even in the list - it lives in `linker.ld` as
`0x40000000`, and the size lives nowhere at all, because nothing has needed to know where RAM
ends yet.

That changes at the next skill. A bump allocator has to know where the free memory _stops_.
Right now the only way to answer that is to hardcode 128 MiB and hope nobody passes `-m 512M`.
The device tree answers it properly.

`kernel.s` starts like this:

```
_start:
                ldr     x0,  =__stack_top
                mov     sp,  x0
```

The first instruction overwrites `x0`. On a normal arm64 boot that would be the bug: `x0` holds
the device tree pointer, and you just lost it. Here it happens to be harmless, for a reason that
is section 3.

---

## 3. The blob QEMU does not give you

The standard arm64 boot protocol says: firmware puts the DTB somewhere in RAM, puts its address
in `x0`, and jumps to the kernel. Every guide you will read assumes this. The beads issue for
this skill assumes it too.

**On this project's setup it does not happen.** Measured, not guessed:

```
$ qemu-system-aarch64 -M virt -cpu cortex-a72 -m 128M -kernel kernel.elf -S -s
(lldb) register read x0 pc
      x0 = 0x0000000000000000
      pc = 0x0000000040000788          <- this is _start, the ELF entry, directly
(lldb) memory find -e '(uint32_t)0xedfe0dd0' -- 0x40000000 0x48000000
data not found within the range.
```

`x0` is zero, and there is no device tree anywhere in the 128 MiB of RAM. Not misplaced, not
byte-swapped. Absent.

The rule QEMU applies (`hw/arm/boot.c`) is that a **raw image is assumed to be a Linux kernel
and an ELF is assumed not to be**. For a raw image it installs a small bootloader stub, loads a
generated DTB, and sets `x0`. For an ELF it does none of that - it just loads the segments and
sets `PC` to the entry point. Which is exactly what you want for bare metal, right up until the
moment you want the DTB.

Adding `-dtb virt.dtb` does not help. Same test, same result:

```
A: -kernel kernel.elf -dtb virt.dtb   ->  x0 = 0, no magic anywhere in RAM
```

The flag is parsed and then ignored, because the not-Linux path never loads a DTB at all.

This is worth sitting with for a second, because it is the general shape of a whole class of
bare-metal bug: the tutorial is correct, your machine is correct, and they are correct about
different configurations. The only way through is to check what your machine actually did.

### Except there is one, at address zero

The search above covered RAM, `0x4000_0000` upward. It did not cover the flash window, and
there is a live device tree sitting at physical address **`0x0`**:

```
(qemu) xp /6xw 0x0
00000000: 0xedfe0dd0 0x00001000 0x40000000 0xc81b0000
```

No `-dtb`, no generic loader, plain `-kernel kernel.elf`. It is real, and it is generated per
run - the memory node's size cell follows `-m`:

| `-m` | size cell at offset `0x180` |
|---|---|
| `128M` | `0x0800_0000` |
| `256M` | `0x1000_0000` |
| `512M` | `0x2000_0000` |

So the accurate statement is not "QEMU gives an ELF kernel no device tree". It is **"QEMU gives
an ELF kernel no way to find the device tree"**. The blob exists; `x0` is zero and nothing points
at it.

**Why zero.** `arm_setup_direct_kernel_boot` in `hw/arm/boot.c` tries to tuck the DTB in at the
base of RAM, but only if the kernel image left room there:

```c
if (image_low_addr > info->loader_start || image_high_addr < info->loader_start) {
    info->dtb_start = info->loader_start;   /* 0x4000_0000 on virt */
    info->dtb_limit = image_low_addr;
}
```

`linker.ld` starts the kernel at `. = 0x40000000`, which is `loader_start` exactly, so neither
half of the condition holds. `dtb_start` is never assigned, keeps the zero it was allocated
with, and `arm_load_dtb(0, ...)` drops the blob at physical address 0.

Take the kernel away and the intended placement shows up:

| command line | `0x0` | `0x4000_0000` |
|---|---|---|
| `-kernel kernel.elf` | `edfe0dd0` - the DTB | `140001ff` - your `_start` |
| no `-kernel` at all | zeroes | `edfe0dd0` - the DTB |

So `x0 = 0` and "the DTB is at `0x0`" are the same fact twice over: a field nobody assigned.
They agree by accident. Nothing pointed `x0` at that blob.

The address is not a promise. `0x0` is `virt.flash0`, the pflash window, and it holds a device
tree only because no firmware was loaded into it. Boot with `-bios` or `-pflash` and that
address is firmware. Hardcoding `0x0` works on exactly this command line and nowhere else, which
is the same disease as hardcoding the RAM size.

It is worth knowing anyway, for two reasons. It is a second copy of the same blob at a low
address, so a parser that confuses a blob *offset* with an absolute *address* will read valid
tokens out of it and appear to half-work - a memorably confusing bug. And it is a free way to
check a parser against a blob you did not place yourself.

---

## 4. Two ways to get the blob into RAM

### Path A - load the file yourself, keep everything else

QEMU's generic loader will drop any file at any address, with no opinion about what it is:

```
-device loader,file=virt.dtb,addr=0x47000000,force-raw=on
```

Verified:

```
B: -kernel kernel.elf -device loader,...   ->  data found at location: 0x47000000
```

Nothing about the boot changes. `linker.ld` untouched, `kernel.s` untouched, ELF still loaded at
`0x40000000`, `lldb` still finds symbols. The kernel reads the address from a constant instead
of from `x0`.

The cost is that `virt.dtb` is a file on disk, dumped from some earlier run. It does not follow
`-m`. Pass `-m 256M` and the blob still claims 128 MiB, which quietly breaks the one thing this
skill is supposed to prove. That is fixable inside the Makefile - dump the DTB and boot in the
same recipe with the same flags, so the file is regenerated every time:

```
qemu-system-aarch64 -M virt,dumpdtb=virt.dtb -cpu cortex-a72 -m $(MEM) -display none
```

`dumpdtb` writes the file and exits immediately, so it is a cheap first step rather than a
second machine.

Pick the load address with care. `0x47000000` is 112 MiB into RAM: clear of the kernel image and
its stack, and fine at `-m 128M`. At `-m 64M` it is off the end of RAM and the load silently
does nothing.

### Path B - boot the way a real arm64 kernel boots

Make the image something QEMU will treat as a Linux kernel, and it does the whole job for you:
generates a fresh DTB from the live machine configuration, places it, and passes the pointer.

That means two changes. First, a raw binary instead of an ELF (`llvm-objcopy -O binary`).
Second, a 64-byte **arm64 Image header** at the very front of the image, which is the header
every arm64 Linux kernel starts with:

```
offset  size  field
0x00    u32   code0        first instruction, must branch over the header
0x04    u32   code1
0x08    u64   text_offset  where to load, as an offset from RAM base
0x10    u64   image_size   total size, or 0 for "unknown"
0x18    u64   flags
0x20    3x u64 reserved
0x38    u32   magic        0x644d5241, the bytes "ARM\x64"
0x3c    u32   reserved
```

All little-endian, unlike the DTB. `code0` is a real instruction that has to jump past the other
60 bytes, because the CPU starts executing at offset 0.

Measured placement, on this QEMU, with the magic valid and `image_size` non-zero:

| `text_offset`  | where the kernel actually starts                          |
| -------------- | --------------------------------------------------------- |
| `0x1000`       | `0x4000_1000`                                             |
| `0x100000`     | `0x4010_0000`                                             |
| `0x200000`     | `0x4020_0000`                                             |
| `0x300000`     | `0x4030_0000`                                             |
| `0`            | `0x4020_0000` (treated as "no preference", 2 MiB aligned) |
| no valid magic | `0x4008_0000` (the legacy default)                        |

So the entry is `0x4000_0000 + text_offset`, honoured exactly. It cannot be zero, because QEMU
puts its own bootloader stub at `0x4000_0000`. `linker.ld` has to move to match whatever you
pick.

Then `x0` arrives as promised:

```
text_offset=0x1000 ->  pc = 0x40001000   x0 = 0x44000000
```

And the DTB address tracks the machine, which is the whole point:

| `-m`   | DTB lands at  |
| ------ | ------------- |
| `128M` | `0x4400_0000` |
| `256M` | `0x4800_0000` |

The rule is `RAM base + min(ram_size / 2, 128 MiB)`. Hardcoding it would work today and break on
the next `-m`, which is precisely the habit this skill exists to break.

The blob really is generated live. Read it back out of RAM at `-m 256M` and decode it:

```
memory@40000000 {
        reg = <0x00 0x40000000 0x00 0x10000000>;
        device_type = "memory";
};
```

`0x10000000` is 256 MiB. The machine is describing itself.

### Which one

**Do A first, then B.** Not because A is better - B is the real thing, and B is what makes this
skill's title honest. But A gets a genuine device tree in front of your parser in about two
minutes, with zero risk to a boot path that currently works. B changes the linker script, the
entry code, and the build, and every mistake in it looks identical from outside: a machine that
starts and prints nothing.

Debug one new thing at a time. Write the parser against a blob you loaded by hand, get it
printing the memory node, and only then move the boot. When you do, the parser does not change -
a pointer that came from `x0` and a pointer that came from a constant are the same pointer.

---

## 5. The layout: one header, three blocks

A DTB is a header followed by three independent blocks, each found by an offset in the header.
Everything is big-endian. The header is ten `u32`s:

| offset | field               | this machine                      |
| ------ | ------------------- | --------------------------------- |
| 0x00   | `magic`             | `0xd00dfeed`                      |
| 0x04   | `totalsize`         | `0x00100000` (QEMU pads to 1 MiB) |
| 0x08   | `off_dt_struct`     | `0x00000040`                      |
| 0x0c   | `off_dt_strings`    | `0x00001bc8`                      |
| 0x10   | `off_mem_rsvmap`    | `0x00000030`                      |
| 0x14   | `version`           | `17`                              |
| 0x18   | `last_comp_version` | `16`                              |
| 0x1c   | `boot_cpuid_phys`   | `0`                               |
| 0x20   | `size_dt_strings`   | `0x000001ce`                      |
| 0x24   | `size_dt_struct`    | `0x00001b88`                      |

Two things to check before trusting anything else. `magic` must be `0xd00dfeed` - if you read
`0xedfe0dd0` you forgot to byte-swap, which is the single most common first bug here. And
`last_comp_version` must be `<= 17`, which is the blob promising it is readable by a version-17
parser.

`totalsize` being 1 MiB while the content ends around `0x1d96` is QEMU leaving room to add nodes.
Do not use `totalsize` as "how much to read".

The three blocks:

**Memory reservation block** at `off_mem_rsvmap`. Pairs of big-endian `u64` (address, size),
terminated by a pair of zeros. These are regions the kernel must not use. On this machine it is
empty - the first pair is already zeros:

```
mem_rsvmap: 00000000000000000000000000000000
```

Empty, but read it anyway. Ignoring a reservation means allocating over something the firmware
is still using, and that failure shows up hours later as corruption.

**Structure block** at `off_dt_struct`. The tree itself, as tokens. Section 6.

**Strings block** at `off_dt_strings`. Every property _name_ in the tree, once, as
null-terminated strings back to back. Properties refer to their name by an offset into here.
`compatible` appears 40-odd times in this tree and is stored once.

---

## 6. The struct block, token by token

The structure block is a sequence of 32-bit big-endian tokens. Five exist:

| token            | value | followed by                                                          |
| ---------------- | ----- | -------------------------------------------------------------------- |
| `FDT_BEGIN_NODE` | `0x1` | the node's name, null-terminated, padded to a 4-byte boundary        |
| `FDT_END_NODE`   | `0x2` | nothing                                                              |
| `FDT_PROP`       | `0x3` | `len` (u32), `nameoff` (u32), then `len` bytes of value, padded to 4 |
| `FDT_NOP`        | `0x4` | nothing - skip it                                                    |
| `FDT_END`        | `0x9` | nothing, the block is over                                           |

The whole parser is that table plus a cursor. Depth is `BEGIN_NODE` minus `END_NODE`. The root
node's name is the empty string, so it is four bytes of token and then a single padding word.

`FDT_NOP` exists so tools can delete a node or property in place without rewriting the file.
Every walker must skip them. QEMU's blob has none today, which means forgetting to handle them
costs nothing now and breaks on the first blob from anywhere else.

**Padding is where parsers go wrong.** After a name or a value, advance to the next multiple of 4. `(p + 3) & !3`. A property whose value is 7 bytes is followed by 1 byte of padding; miss it
and every token after that point is garbage, usually presenting as impossible node names and an
infinite loop.

Here is the real thing. Offsets are relative to `off_dt_struct`, taken from the live blob:

```
0x0000 BEGIN_NODE '/'
0x0008   PROP len=4  nameoff=0xb9 'interrupt-parent' = 00008002
0x0018   PROP len=0  nameoff=0x2c 'dma-coherent'     =
0x0024   PROP len=17 nameoff=0x26 'model'            = 6c696e75782c64756d6d792d7669727400
0x0044   PROP len=4  nameoff=0x1a '#size-cells'      = 00000002
0x0054   PROP len=4  nameoff=0xb  '#address-cells'   = 00000002
0x0064   PROP len=17 nameoff=0x0  'compatible'       = 6c696e75782c64756d6d792d7669727400
0x0084   BEGIN_NODE 'psci'
0x0090     PROP len=4  nameoff=0x1c6 'migrate'       = c4000005
...
0x0110   END_NODE
0x0114   BEGIN_NODE 'memory@40000000'
0x0128     PROP len=16 nameoff=0xa6 'reg'            = 00000000400000000000000010000000
0x0144     PROP len=7  nameoff=0x9a 'device_type'    = 6d656d6f727900
0x0158   END_NODE
0x015c   BEGIN_NODE 'platform-bus@c000000'
```

Things to notice in those bytes.

`dma-coherent` has `len=0`. A property with no value is a **boolean**: present means true,
absent means false. Not a bug, not an empty string.

`model` is 17 bytes for a 16-character string. Strings in properties include their null
terminator.

`compatible` at `0x0064` is 17 bytes here but 35 bytes on the `psci` node, holding three strings
back to back with a null after each. `compatible` is always a _list_, even when it has one
entry, and you match against any element.

`device_type = "memory"` is how you identify the memory node without trusting its name. Older
trees and non-`virt` machines can name it differently, and there can be more than one.

The `memory` node's `reg` is 16 bytes: `00000000 40000000 00000000 10000000`. Four cells. Base
`0x0000000040000000`, size `0x0000000010000000`. That blob was taken at `-m 256M`.

Reaching that one property from the start of the block means stepping through 0x114 bytes of
tokens you do not care about. There is no index, no table of contents, no way to seek. That is
the trade the flattened format makes: trivial to parse, linear to search. For a boot-time parser
that reads a handful of values once, it is the right trade.

---

## 7. Cells, and why `reg` has no fixed size

`reg = <0x00 0x40000000 0x00 0x10000000>` is not "two 64-bit numbers". It is four 32-bit cells,
and the split into (address, size) comes from the **parent** node:

```
/ {
        #size-cells = <0x02>;
        #address-cells = <0x02>;
```

Two cells of address, two of size. So the 16 bytes are `[addr_hi, addr_lo, size_hi, size_lo]`.

Read those two properties from the parent, not from the node you are looking at. `/memory` has
no `#address-cells` of its own. Look at `platform-bus@c000000` in the dump above and you will
see it declares `1` and `1` - so _its_ children's `reg` properties are 8 bytes, not 16, with the
same syntax and a different meaning. A parser that assumes 16 will read one child's address and
the next child's length and produce a number that looks plausible and is wrong.

The default when a parent does not say is 2 for address and 1 for size. Do not rely on it here;
the root says what it wants.

**Read cell by cell.** Four `u32` big-endian reads, combined with shifts:

```
addr = (cell0 << 32) | cell1
```

Not one 64-bit read. Two reasons. The cells are independent by definition, and the language does
not promise the value is 8-byte aligned. The second reason bites hard on this machine: the MMU
is off, so all memory is Device memory, and Device memory forbids unaligned access. An 8-byte
load from a 4-aligned address raises an Alignment fault with ESR DFSC `0x21`. That signature is
already in `CLAUDE.md` because you have hit it once before.

Node names and string values have no alignment guarantee at all - read those a byte at a time.

Cell counts are also why "how big is an address" has no answer in this format. A 32-bit board
uses 1 cell; this one uses 2. The tree carries its own word size.

---

## 8. What to build

Four stages. Each one runs and prints something before the next one starts.

### Stage 1 - prove you can read the header

Take a pointer, validate it, print what is inside.

```rust
pub struct Dtb { base: usize }

impl Dtb {
    pub unsafe fn from_ptr(ptr: usize) -> Option<Dtb>;   // None unless magic and version are sane
    pub fn total_size(&self) -> u32;
}
```

`unsafe` because you are promising a valid blob lives there. Nothing else in the type needs to
be unsafe once that promise is made.

Print magic, version and totalsize over the UART and compare against the table in section 5.
Small, but it separates "my pointer is wrong" from "my parser is wrong", and those two failures
look the same from a dead machine.

You will want a big-endian `u32` read next to `mem::read_32`. It is one `swap_bytes` call, or
`u32::from_be_bytes`. Keep the byte order at the edge, in one function, and let everything above
it work in native numbers.

### Stage 2 - walk the whole tree and print it

The stage with the best payoff for the effort. A loop over the tokens that prints node names
indented by depth and property names underneath. A `dtc` you wrote, running on the machine it is
describing.

```rust
pub fn for_each_node(&self, f: &mut impl FnMut(usize, &str));
```

A callback, not an iterator - there is no allocator, and an iterator over this needs the cursor
to live somewhere. A callback puts it on the stack. An iterator is nicer to use and you can
build one later once you know the walk is correct.

Do not skip this stage and jump to "find the memory node". The full dump is what tells you your
padding and token handling are right, because a single alignment mistake turns the output into
garbage immediately and visibly. Compare it against `dtc -I dtb -O dts virt.dtb` line for line.

It is also just worth seeing. Every device on the machine, listed by the machine.

### Stage 3 - the memory node

The actual acceptance criterion.

```rust
pub fn memory(&self) -> Option<(usize, usize)>;   // (base, size)
```

Find the node whose `device_type` is `"memory"`, read the root's `#address-cells` and
`#size-cells`, decode `reg`. Print it. Change `-m` and watch it change.

A tree may list several memory nodes, and `reg` may hold several pairs. Handling only the first
is fine for now - say so in a comment so it is a decision and not an oversight.

### Stage 4 - not yet

Finding devices by `compatible`, so `UART_BASE` comes from the tree instead of `board.rs`.
Reading `interrupts` to get INTID 33 rather than knowing it. Both are natural and both need the
`interrupt-map` and `interrupt-parent` machinery to be done properly, which is a lot of tree
walking for facts you already have correct.

Leave `board.rs` alone in this skill. The memory map is the fact that actually changes at
runtime, and it is the one the allocator needs.

---

## 9. Bring-up order

1. Get a blob into RAM by Path A and put its address in a constant. Confirm with `make mem
ADDR=0x47000000 N=4 FMT=xw` that the first word is `0xd00dfeed`, before writing any Rust.
2. Big-endian `u32` read. One function.
3. Header parse and validation. Print it. Compare with section 5.
4. Memory reservation block. Walk to the terminating zero pair. Expect zero entries.
5. Token walk with the node names only. Print with indentation. This is where padding bugs live.
6. Add properties to the walk. Names from the strings block. Full dump. Diff against `dtc`.
7. `#address-cells` and `#size-cells` from the root.
8. Find `/memory`, decode `reg`, print base and size.
9. Vary `-m`, confirm the numbers follow. Path A needs the Makefile to regenerate the dtb here
   or this step lies to you.
10. Switch to Path B. Image header, linker base, objcopy, `x0` saved in `kernel.s` before it is
    clobbered. The parser does not change.

Steps 1-9 are a normal afternoon. Step 10 is a boot change and deserves its own sitting.

Saving `x0` in step 10 is two instructions at the top of `_start`, before the stack exists.
Anywhere that survives `.bss` zeroing works - a callee-saved register you then pass to `kmain`
as an argument is the least surprising, since `kmain` already takes none and adding one is free.

---

## 10. Proving it works

**The blob is where you think.** `make mem ADDR=<addr> N=4 FMT=xw` shows `0xd00dfeed`. If it
shows `0xedfe0dd0` you are looking at the right memory with the wrong byte order, which is
progress.

**The dump matches `dtc`.** Run `dtc -I dtb -O dts virt.dtb` on the host and compare against
what your kernel printed. Node names, order, property names. This is the strongest check
available and it costs one diff.

**The size follows `-m`.** The real test:

| `-m`   | expected size |
| ------ | ------------- |
| `64M`  | `0x0400_0000` |
| `128M` | `0x0800_0000` |
| `256M` | `0x1000_0000` |
| `512M` | `0x2000_0000` |

Base stays `0x4000_0000` in every case. If your printed size is constant across those runs you
are reading a stale file, not the machine.

**Cell counts are honoured.** A cheap negative test: pretend `#address-cells` is 1 and watch the
base become `0x00000000` and the size become `0x40000000`. Seeing the specific way it goes wrong
is worth more than seeing it go right, because that is the shape the bug will have when it is
real.

**Reading it twice gives the same answer.** The DTB is read-only data in normal RAM. If a second
parse differs, something is writing over it - which is section 12.

---

## 11. When nothing happens

| symptom                                                | almost certainly                                                                                                                                                                              |
| ------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| magic reads `0xedfe0dd0`                               | no byte swap. The whole blob is big-endian.                                                                                                                                                   |
| magic reads `0x00000000`                               | nothing loaded there. Path A address outside RAM, or you are on the ELF path with no blob at all - re-read section 3.                                                                         |
| `x0` is 0                                              | the ELF path. QEMU never set it. Not a bug in your code.                                                                                                                                      |
| `x0` looks like a stack address                        | `kernel.s` clobbered it before you saved it. First instruction.                                                                                                                               |
| node names are garbage after the first few             | padding. Round up to 4 after both names and values.                                                                                                                                           |
| walker never terminates                                | same cause, or `FDT_END` not handled, or `FDT_NOP` not skipped. Bound the loop by `size_dt_struct` so a bad blob stops instead of hanging.                                                    |
| property names wrong but structure right               | `nameoff` is an offset into the strings block, not into the struct block or the file.                                                                                                         |
| `PC=0x200`                                             | a fault before the vector table, per `CLAUDE.md`. Should not happen here - your vectors are installed long before this code runs. If you see it, this code is running earlier than you think. |
| data abort, ESR DFSC `0x21`                            | unaligned 8-byte read with the MMU off. Read cells as `u32`.                                                                                                                                  |
| size is right at `-m 128M` and unchanged at `-m 256M`  | stale `virt.dtb` on disk. Path A, without regenerating.                                                                                                                                       |
| kernel silently does nothing after switching to Path B | link address and `text_offset` disagree. `0x4000_0000 + text_offset` is where it lands, exactly.                                                                                              |

The general move for all of these: dump the bytes with `make mem` and read them yourself. The
blob is 8 KB of data sitting at a known address, and everything in it is checkable by hand
against `dtc` output. There is no state, no timing, and no hardware to blame. It is the most
debuggable thing in the project so far.

---

## 12. The blob is in RAM you are about to allocate

At `-m 128M` the DTB sits at `0x4400_0000`, 64 MiB into RAM. The `/memory` node it contains
reports all 128 MiB as usable - including the range the blob itself occupies. Nothing marks it.
The memory reservation block is empty.

So the next skill, a bump allocator handing out memory from the end of `.bss` upward, will
eventually hand out the device tree. Not immediately, and not visibly. It will hand out RAM for
a while and then start returning bytes that something else considers meaningful, and the failure
will look like corruption with no cause.

Two ways out, both fine. Copy the few values you need into statics during boot and never touch
the blob again, which is what this skill needs and costs nothing. Or record the blob's range and
have the allocator skip it, which is what you need the moment anything wants to re-read the tree
later.

Worth writing down now, while the reason is fresh, because the symptom will appear one skill
later and look unrelated.

---

## 13. Deliberately left out

**`phandle` and `interrupt-parent`.** A phandle is a number that acts as a pointer to another
node - that `0x8002` on the root is the GIC. Following them is how you learn which controller a
device's interrupt goes to. You already know: there is one GIC and everything goes to it.

**`interrupt-map`.** How PCI devices swizzle interrupt lines onto controller inputs. Real, ugly,
and irrelevant until there is PCI.

**`ranges`.** Address translation between a bus and its parent. `virt` uses flat identity
mappings for the devices you care about, so translating is a no-op. It stops being a no-op the
first time a device is behind a bridge.

**`/chosen`.** Where the bootloader puts the kernel command line and the initrd location. QEMU
fills it in when you pass `-append`. Nothing to parse until there is something to configure.

**`/aliases`.** Short names for common nodes, mostly a convenience for bootloaders.

**Overlays, and `libfdt`'s write side.** Modifying a tree. Not a thing a kernel does at boot.

**CPU nodes.** `/cpus` describes every core, and it is exactly what the MANY CORES side quest
needs to know which cores to start. Left for that skill.

---

## 14. Done when

- The kernel prints the RAM base and size, read from the tree, and they follow `-m` across at
  least three values.
- The full tree dump matches `dtc` output.
- The parser walks the tokens rather than searching for byte patterns, and skips `FDT_NOP`.
- Cell counts come from the parent node, not from a constant.
- Nothing reads a `u64` out of the blob directly.
- You can say out loud why `reg` has no fixed size, and why the DTB is big-endian on a
  little-endian machine.

The last one has an answer worth knowing. The format was fixed by Open Firmware on
big-endian PowerPC and IBM hardware in the 1990s, and the tree outlived the machines. Every
arm64 board today byte-swaps for a decision made about a CPU nobody in this project has
touched. Formats are more permanent than hardware.

---

## Optional reading

- **Devicetree Specification v0.4**, devicetree-specification-v0.4.pdf on devicetree.org.
  Chapter 5 is the flattened format - header, tokens, alignment. About twelve pages and it is
  the actual normative text for everything in sections 5 to 7 here.
- **`hw/arm/boot.c`** in the QEMU source. Ground truth for section 3: which images get a DTB,
  where it is placed, what lands in `x0`.
- **`Documentation/arch/arm64/booting.rst`** in the Linux source. The arm64 Image header and the
  state the kernel is entered in. Short, and it is the contract Path B implements.
- **`libfdt`** in dtc's source tree, particularly `fdt_ro.c`. A complete read-only parser in C,
  under 1000 lines, doing exactly what you are about to do. Worth reading after yours works, not
  before.
- `virt.dts` in this repo. The machine, in text.
