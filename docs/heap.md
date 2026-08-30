# HEAP - one trait, and `Box` and `Vec` appear

## 1. What this is

`Box`, `Vec` and `String` are not built into the Rust language. They live in a crate called
`alloc`, which ships with the compiler and works without an operating system - on the one
condition that somebody tells it how to get memory. You are somebody.

There is no hardware in this skill at all. No registers, no memory-mapped device, no barriers,
nothing to look up in a manual. It is a **software convention**: the `alloc` crate declares a
trait, you implement it, and you mark one `static` with an attribute so the compiler knows which
implementation to wire in. Every skill before this one was about a machine. This one is about an
agreement between you and a library.

The work is small, because `Heap` already exists. `src/heap.rs` owns `Pages`, `Frames` and
`Cache`, and already has the two methods the trait needs:

```
Heap::alloc_layout(&mut self, Layout) -> Option<usize>
Heap::free_layout(&mut self, usize, Layout) -> Option<()>
```

So the allocating is done. What is left is roughly forty lines, and all of it is plumbing: the
trait's signature does not fit what you have, in three separate ways, and each of the three has a
different answer.

## 2. What you get, and what it costs

You get `Box<T>`, `Vec<T>`, `String`, `BTreeMap` and every other owning container in `alloc`.
`zhemon` stops living inside a fixed `[u8; N]`, and a device list stops being an array with a
length beside it.

The cost, stated plainly because it is the one thing about this skill you would rather find out
now than later: **`alloc` panics when it runs out of memory.** Not returns an error - panics. The
path is fixed inside the crate and you cannot intercept it. Your repo rule that the kernel never
panics does not survive this skill in its current form.

Concretely: `Vec::push` calls `alloc::raw_vec::handle_error`, which calls `handle_alloc_error`,
which panics with the message `memory allocation of N bytes failed`, which lands in your
`#[panic_handler]` at `src/main.rs:20`. That string is compiled into the `alloc` library that
ships with your toolchain; it is not something you write or can replace.

Two ways to live with it:

- `Vec::try_reserve(n)` returns `Result` and never panics. It is stable, it compiles for
  `aarch64-unknown-none`, and if you reserve up front then no later push can fail. This is what
  the Linux kernel's Rust code does.
- Accept it. Out of memory here is not recoverable anyway, and a message beats a `None` nobody
  checks.

Decide before you write anything: it changes how every caller is written, not how the allocator
is written.

## 3. There is nothing to touch

Every skill so far had a "how do you reach this hardware" answer: a memory-mapped address you
store to, like the serial port at `0x0900_0000`, or a system register reached with `msr` and
`mrs`. This one has neither. Nothing here is observable with `make mem`, because none of it is
at an address you chose.

What replaces it is a **lang item**: a symbol the compiler and the standard library agree to call
by a fixed name. You have used one already - `#[panic_handler]` on `panic_handler` in
`src/main.rs:20`. Nothing in your code calls that function; `core` does, by name, at link time.
`#[global_allocator]` works the same way.

Here is what the attribute actually does, read out of the compiled object rather than the
documentation. Compiling a crate with a `#[global_allocator]` static and running `llvm-nm` on the
result shows five symbols defined that appear nowhere in the source:

```
T __rust_alloc
T __rust_dealloc
T __rust_realloc
T __rust_alloc_zeroed
T __rust_no_alloc_shim_is_unstable_v2
```

The first four are one-line wrappers the attribute generated: each takes a size and an alignment,
rebuilds a `Layout`, and calls the matching method on your static. `alloc` calls `__rust_alloc`
and has never heard of your type. The fifth is a marker with no body whose only job is to fail
the link when it is missing. The same `llvm-nm` on the shipped `alloc` library shows all five as
`U`, undefined, waiting for you.

That is the entire mechanism: an attribute generates four functions with agreed names, and a
library compiled years before your kernel calls them.

## 4. Reading the names

**`GlobalAlloc`** - **Global** because there is exactly one per program, chosen at link time, not
passed as an argument; **Alloc**ator. The trait is in `core::alloc`, so you do not need the
`alloc` crate to implement it, only to benefit from it.

**`Layout`** - a request, described as two numbers: `size()` in bytes and `align()` in bytes. Not
a type, not a pointer. `Heap::alloc_layout` already takes one and already does everything with
it: `slab::class_of` at `src/slab/mod.rs:16` picks a size class whose size is a multiple of
`align()`, and when no class fits, `alloc_layout` turns both numbers into whole pages with
`div_ceil(PAGE_SIZE)` and takes the larger. Nothing in this skill re-does that work.

**`alloc` / `dealloc` / `realloc` / `alloc_zeroed`** - the four methods. Only the first two are
yours to write; the trait provides the other two with default bodies, described in section 7.

**Shim** - a function that exists only to translate between two calling conventions. The four
`__rust_*` symbols above are shims: the `alloc` crate wants to call a plain function with two
integers, your trait wants a method on a value, and the shim bridges them.

**Lang item** - a definition the compiler looks up by name rather than by import. `#[panic_handler]`
and `#[global_allocator]` both produce one.

**ZST**, **Z**ero-**S**ized **T**ype - a type with no bytes, like `()`. `Vec<()>` never allocates
and `Box<()>` uses a dangling but correctly aligned pointer, so a zero-size request never reaches
you. You do not have to handle it.

**Interior mutability** - changing something through a shared reference (`&self`) rather than an
exclusive one (`&mut self`). It is the subject of section 6.

**DAIF** - the four interrupt mask bits in the processor's state word: **D**ebug, **A**bort,
**I**RQ, **F**IQ. `IRQ` is Interrupt **R**e**q**uest and `FIQ` is **F**ast Interrupt
**R**e**q**uest. `cpu::without_interrupts` in `src/cpu.rs:24` saves and restores all four, and
`SpinLock` uses the same pair of `cpu::stop_interrupts` and `cpu::restore_interrupts` directly.

## 5. How a `push` gets to your allocator

The diagram of this descent is [`diagrams/heap.tldx.jsx`](diagrams/heap.tldx.jsx) - open it with
`tldx serve docs/diagrams/heap.tldx.jsx`.

Seven steps, in order. The first four were verified from the assembly of a ten-element `Vec<u32>`;
the last three are the layers this project has already built:

1. `v.push(i)` sees the vector is full and calls `alloc::raw_vec::RawVec::grow_one`.
2. `grow_one` computes a new capacity, builds a `Layout` for it, and calls `__rust_realloc` with
   the old pointer, the old size, the alignment, and the new size.
3. `__rust_realloc` is the shim the attribute generated. It rebuilds the old `Layout` and calls
   `GlobalAlloc::realloc` on your static.
4. Your `realloc` - the default one, unless you override it - calls your `alloc` with the new
   layout, copies the old bytes over, and calls your `dealloc` on the old pointer.
5. Your `alloc` takes the lock on the `static`, calls `Heap::alloc_layout`, and turns the
   `Option<usize>` it gets back into a raw pointer.
6. `alloc_layout` asks `class_of` for a size class. If there is one, `Cache` pops a free slot,
   asking `Frames` for a fresh page only when no partial slab of that class has room. If there is
   not - the request is over 2048 bytes, or wants an alignment no class satisfies - it goes
   straight to `Frames` as an order.
7. `Frames` splits a buddy block down to that order and hands back a `Pfn`, which becomes an
   address.

If step 5 comes back empty, the pointer is null, and step 2 sees the null and jumps to
`raw_vec::handle_error`, which is the panic described in section 2.

Step 5 is the whole skill, and by this point it is thin. The conversion at the end of it is a
two-arm `match`: `Some(address)` becomes `address as *mut u8`, and `None` becomes
`core::ptr::null_mut()`. It is a real branch, unlike the free conversion an `Option<NonNull<u8>>`
would have given you - `NonNull` cannot be zero so the compiler reuses zero for `None`, and a
`usize` can be zero so it cannot. One branch on a path that already did a class lookup and a list
pop does not matter, but do not describe it to yourself as free.

## 6. The three things that do not fit

### The trait wants `&self`, `Heap` wants `&mut self`

```rust
unsafe fn alloc(&self, layout: Layout) -> *mut u8;
```

Shared reference. It has to be: the allocator is a `static`, and a `static` can never be borrowed
mutably. But `Heap::alloc_layout` is `&mut self` and must be - it moves free-list heads and writes
into the page table.

The type that bridges those is `UnsafeCell<T>`, the one type in Rust that legally hands out a
`*mut T` from a `&self`. Everything else with interior mutability, from `Cell` to `Mutex`, is
built on it - **and so is your own `SpinLock`**, at `src/lock.rs:14`. So you do not write an
`UnsafeCell` in this skill, and you do not write an `unsafe impl Sync` either: `SpinLock<T>`
already has both, with the `T: Send` bound at `src/lock.rs:87` carrying the argument. That is the
payoff for having built LOCK first. This skill inherits a safety proof instead of writing a new
one.

`src/irq.rs:17` is the same pattern written out by hand, with the `unsafe impl Sync` at line 23 and
a comment arguing for it. Read it as the thing you are *not* doing here.

### A `static` is built at compile time, the allocator is built at boot

`static ALLOCATOR: ... = ...;` is baked into the image. But `Pages` has to find its own storage in
the `MemoryMap`, and `Frames` has to be seeded from the map's unreserved runs, and neither of
those exists until `kmain` is running.

The tempting shape is a static holding "an allocator, or nothing yet", and it is the wrong one.
The better shape is a static that is **always** there, born empty, given its memory later:

- An empty allocator already refuses everything. `Pages::empty()` has `len` zero, so `Frames` has
  no free lists to pop from and the very first request fails without a single extra line. `Option`
  adds a state and a branch to express something the data already expresses.
- The `const fn` chain is short: `FreeLists::empty` at `src/frames/lists.rs:18` and
  `Region::EMPTY` are const already, so you need `Pages::empty()`, `Frames::empty()`, `const` on
  the existing `Cache::new`, and `Heap::empty()` calling all three.
- `init` then fills in the fields rather than installing a value. It is today's `Heap::new` with
  its `Option<Self>` turned into `&mut self` and `Option<()>`, and one guard on the front:
  `self.pages.len() != 0` means it has already run. That one compare buys back the double-init
  detection `Option` would have given you.

An allocation before `init` therefore panics with `memory allocation of N bytes failed`. That is a
good failure: it names itself, at the line that did it, instead of writing over address zero.

The cost is real and worth seeing before you commit. Today `kmain` owns a `let mut heap` and lends
it out as `&mut`. Once the static owns it, there is no local to lend, and everything that wants
one borrows through the static:

```
heap::with(|heap| ...)
```

`Table::new`, `identity_map` and `map_range` keep their `&mut Heap` parameters unchanged - only
their call sites move. `Table::new(&mut heap)` becomes `heap::with(Table::new)`, and the two
`identity_map` calls wrap the same way.

**One hazard, and it is the deadlock from LOCK.** The closure passed to `with` holds the guard for
its whole body, and `identity_map` allocates page after page as it walks. That is fine, and only
because those allocations go through the `&mut Heap` it was handed, not back through the static.
The moment anything inside a `with` closure allocates through `Box`, `Vec` or the static, it takes
a lock the same call already holds, and the machine stops.

### Interrupts are already live

`irq::unmask()` runs at `src/main.rs:93`, well before any of this. From that instruction onwards,
the timer handler and the serial handler can run between any two instructions of any function,
including between the read of a free-list head and the write of it.

Nothing allocates from a handler today. The moment one does - a handler pushing a byte into an
owned buffer, say - an allocation can begin inside an allocation that is halfway done.

The tool already exists, because LOCK built it: `SpinLock<T>` masks interrupts, then takes the
lock, and its guard releases and restores on drop. Neither half alone is enough - masking does
nothing about another core, and a bare lock deadlocks against your own handler - which is the
whole argument in the LOCK guide, section 3.

So the allocator's `static` is a `SpinLock<Heap>`, and `alloc` and `dealloc` each take the guard
for the length of one call.

The cost is worth naming: interrupts are masked for the length of one slab allocation, which is a
class lookup and a free-list pop. Tens of nanoseconds. The path that is not is the one where
`Cache` has to go to `Frames` for a new page, which can split blocks down as many as ten orders -
still bounded, but worth measuring rather than assuming once it is running.

## 7. `dealloc`, for real

By the time this skill runs, freeing works. `Slab` puts the slot back on its free list, `Cache`
returns the page to `Frames` when the last object in it is freed, and `Frames` merges it with its
buddy and back up the orders. Nothing leaks.

That is worth stating plainly because the shortcut was tempting and was rejected on purpose. A
`GlobalAlloc` over a bump allocator with an empty `dealloc` would have compiled and run, and would
have leaked every byte it ever handed out, invisibly, until SCHED spawned tasks in a loop and died
after an hour for no visible reason. Traps that hide for three skills are the expensive kind.

The two methods you do not have to write:

**`realloc`** has a default body that allocates the new block, copies, and frees the old one. It is
correct, and it is what a growing `Vec` uses. Overriding it is a real optimisation now that SLAB
exists - if the new size lands in the same class, nothing has to move at all, and `realloc` can
return the same pointer - but it is an optimisation, not a correctness fix, and it should be
measured rather than assumed.

**`alloc_zeroed`** has a default that calls your `alloc` and writes zeros. SLAB cannot do better;
the memory it hands out is whatever the last owner left, so the zeroing has to happen either way.

## 8. The numbers, and where they come from

`Vec` growth is not one byte at a time. Measured, not recalled - a `Vec<u32>` pushed ten times:

| after push | capacity | bytes requested |
| ---------- | -------- | --------------- |
| 1          | 4        | 16              |
| 5          | 8        | 32              |
| 9          | 16       | 64              |

Three allocations, not ten. The first capacity is 4 rather than 1 because `RawVec` has a minimum
non-zero capacity, chosen by element size: 8 elements for a one-byte element, 4 for anything up to
1024 bytes, 1 above that. `u32` is 4 bytes, so 4 elements, so `4 x 4 = 16` bytes. After that it
doubles, which is why the requests are 16, 32, 64 and not 16, 24, 32.

So ten pushes of a `u32` request **112 bytes** in total - `16 + 32 + 64` - and hold 64 of them
live. With real freeing underneath, the first two blocks go straight back to their slabs as the
vector grows, so the memory actually in use afterwards is 64 bytes and not 112.

Both numbers matter, and they measure different things. Bytes requested is a property of
`RawVec` and will be 112 whatever you do. Bytes still held is a property of your allocator, and it
is 64 only if `dealloc` really works. Print both.

Then drop the vector and print again. Back to zero is the number that says the whole stack -
`GlobalAlloc`, `Heap`, `Cache`, `Frames` - closed the loop.

## 9. What you are building

One file changes structurally, `src/heap.rs`, and it already exists - so this is an addition to it
rather than a new module. `Pages`, `Frames` and `Cache` each gain one `const fn`, and `main.rs`
changes at every place that touches the heap.

At the top of `src/heap.rs`:

- `static ALLOCATOR: Allocator = Allocator::new();` carrying `#[global_allocator]`.
- a newtype wrapping the lock - `struct Allocator(SpinLock<Heap>)` - with a `const fn new()`
  building it from `Heap::empty()`. It costs nothing at runtime and gives the trait a type of its
  own to hang on.
- `unsafe impl GlobalAlloc for Allocator`, with `alloc` and `dealloc`, each taking the guard for
  the length of one call and forwarding to `alloc_layout` and `free_layout`.
- `pub fn init(map: &mut MemoryMap) -> Option<()>` and `pub fn with<R>(f: impl FnOnce(&mut Heap)
  -> R) -> R`, the two doors into the static. `with` is what replaces `kmain`'s local.

Inside `impl Heap`:

- `const fn empty()`, and `fn init(&mut self, map: &mut MemoryMap) -> Option<()>` replacing today's
  `new`, guarded as section 6 describes.

Elsewhere:

- `Pages::empty()`, `Frames::empty()`, and `const` on `Cache::new`.
- `extern crate alloc;` in `main.rs`. The 2024 edition removed the need for `extern crate` for
  ordinary dependencies, but `alloc` is not an ordinary dependency - it is not in `Cargo.toml` and
  the compiler will not link it unless you name it.

Note what is **not** in that list. No `UnsafeCell`, no `unsafe impl Sync`, no
`#[alloc_error_handler]` - the compiler has shipped a default one since 1.68 and the attribute is
still unstable, so adding it will not even build.

The hard part is not any of those signatures. It is that `alloc` must never, on any path, allocate
- not a log line, not a formatted error, not a `Vec` for bookkeeping. Anything that allocates
inside the allocator takes a lock the current call already holds, and LOCK section 3 says what
that is: a permanent stop. Read your own `alloc` body once looking only for that, and then read
every `with` closure in `main.rs` looking for the same thing.

## 10. Where it goes in `kmain`

The order is forced by what each step needs from the one before it, and it has changed since
FRAMES: **the heap now comes before the page tables**, because `Table::new` gets its page from the
heap. Current `kmain`, from `src/main.rs:99`, with the new steps folded in:

1. `MemoryMap::new(board.memory)` and the two `map.reserve` calls for the image and the device
   tree - the arena and the reservations exist.
2. **`heap::init(&mut map)`** - `Pages` finds its own storage and reserves it, `Frames` seeds
   itself from what is left, `Cache` starts empty. `Vec` starts working here.
3. `heap::with(Table::new)` and two `identity_map` calls - the tables exist, built out of pages
   the heap handed over.
4. `mmu::enable(&mut table)` - translation is on.

Step 2 has to be after step 1 because it needs the map, and before step 3 because step 3 allocates.
There is no longer a choice about putting it after the MMU: that ordering was possible when tables
came from a bump allocator, and it is not now.

One thing to check rather than assume: the memory the heap hands out has to be mapped. Step 3 maps
all of `board.memory` with `Descriptor::NORMAL_BLOCK`, and the arena is a range inside
`board.memory`, so every address the heap can return is covered. If you ever shrink that mapping,
this is what breaks first, and it breaks as a data abort on a write to a `Vec` rather than
anywhere near the mapping code.

There is a second trap in the same window. Between step 2 and step 4 the MMU is off, and with the
MMU off every access is Device memory, which forbids unaligned access. A `Vec<u32>` is fine, since
`Heap` hands out class-aligned addresses, but a type whose 8-byte field lands on a 4-byte offset
faults with an Alignment fault: **ESR**, **E**xception **S**yndrome **R**egister, reporting
**DFSC** - **D**ata **F**ault **S**tatus **C**ode - `0x21`.

## 11. When nothing happens

| symptom                                                                                  | almost certainly                                                                                                                                                                                                               |
| ---------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| link fails, undefined symbol ending `__rust_no_alloc_shim_is_unstable_v2`                | `extern crate alloc` is there but `#[global_allocator]` is not, or it is behind a `cfg` that is off in this build. The marker symbol exists to make exactly this a link error instead of a mystery.                            |
| link fails, undefined `__rust_alloc`                                                     | same cause. These two always appear together.                                                                                                                                                                                  |
| `error: use of unstable library feature 'alloc_error_handler'`                            | you added `#[alloc_error_handler]`. Delete it. The default has shipped since 1.68 and it is what produces the message in the next row.                                                                                          |
| `memory allocation of N bytes failed` at the first `push` on the machine                 | `alloc` returned null. Either `heap::init` never ran, or it ran after the allocation, or memory really is exhausted. Print the heap's `Display` to tell which.                                                                 |
| the machine stops dead on the first allocation, no output                                | `alloc` allocated. A `println!`, a formatted error, or any bookkeeping inside the allocator takes the lock the current call already holds. Section 9.                                                                          |
| the machine stops dead inside `identity_map`                                             | something in that `with` closure allocated through the static instead of through the `&mut Heap` it was handed. Same lock, same stop. Section 6.                                                                               |
| free pages drop and never come back                                                      | `dealloc` is reaching the wrong layer - freeing to `Frames` what `Cache` handed out, or the reverse. `class_of` decides which, and it has to make the same decision in `free_layout` that it made in `alloc_layout`.           |
| `Vec` contents are garbage after it grows                                                | an overridden `realloc` that returned the same pointer for a size that changed class, so the copy never happened. Only skip the copy when the class is genuinely unchanged.                                                    |
| data abort on the first write into a `Vec`, any alignment                                | the address is outside what `identity_map` covered. Section 10.                                                                                                                                                                |
| data abort, ESR DFSC `0x21`, before `mmu::enable`                                        | an unaligned access with the MMU still off, so all memory is Device memory. Section 10.                                                                                                                                        |
| the machine stops dead the first time a handler allocates                                | recursion into the lock, as above, or a lock taken without masking. LOCK section 3.                                                                                                                                            |
| everything works, `.text` grew by several kilobytes                                      | expected, and not from the allocator. `Box` and `Vec` pull in `raw_vec`, its overflow checks and its error path.                                                                                                               |
| `heap::init` returns `None` on a second call                                             | the guard doing its job. Something calls it twice; find the second caller rather than removing the guard.                                                                                                                     |

## 12. How you will know it worked

Push ten numbers into a `Vec<u32>` in `kmain`, print the length, the capacity and the sum, and
print the heap's `Display` before the vector exists, while it is alive, and after it is dropped.

The output that means it worked, as it actually came out on this machine:

```
heap: 32430 of 32768 pages free
vec: len 10 cap 16 sum 45
heap: 32429 of 32768 pages free with the vec live
heap: 32430 of 32768 pages free
```

Read it line by line:

- length 10, capacity 16, sum 45. The capacity is the third row of section 8's table, which proves
  the memory survived two reallocations and two copies. The sum proves the copies copied.
- while the vector is alive, free memory is down by **one page**, not by 112 bytes and not by 64.
  `Cache` took a whole page for the 64-byte class and will hand the other 63 slots to the next
  allocation. That is the layer doing its job, and seeing the request size here instead would mean
  it was not.
- **after the vector is dropped, free memory is back to exactly where it started.** Not close.
  Exactly. This is the number that says the whole stack closed the loop, and it is the first time
  in this project that memory has ever come back.
- `make run` still reaches the monitor prompt afterwards, which proves nothing was written over.

The absolute 32430 moves whenever the image, the device tree or the metadata table changes size.
The two deltas are the observable, not it.

Then take one number away from it. Set the arena smaller than the vector needs and boot again: the
machine prints `memory allocation of N bytes failed` through your own panic handler, with a line
number in `raw_vec`. Recognising that message on sight is worth more later than the successful run
is now.

---

## Optional reading

- `core::alloc::GlobalAlloc` in the standard library documentation. The safety contract on each
  method is short and is the actual specification of what you are promising.
- `library/alloc/src/raw_vec/mod.rs` in the Rust source. `grow_amortized` is the doubling rule and
  `MIN_NON_ZERO_CAP` is the table in section 8.
- The Rust Reference, "Runtime" chapter, on `#[global_allocator]` and the `alloc` crate.
- `src/lock.rs` and `src/cpu.rs` in this repo. The `UnsafeCell`, the `unsafe impl Sync` and the
  DAIF save-and-restore in section 6 are all already there, written in LOCK.
