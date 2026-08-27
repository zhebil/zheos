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

The work is small. The whole implementation is under sixty lines, and most of it is forwarding to
SLAB, which already does the real job. Everything below is already built by then: SLAB cuts pages
into objects, FRAMES owns the pages, and `Bump` bootstrapped FRAMES. What makes it worth a guide is that the trait's
signature does not fit what you have, in three separate ways, and each of the three has a
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
`#[panic_handler]` at `src/main.rs:23`. That string is compiled into the `alloc` library that
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
`src/main.rs:22`. Nothing in your code calls that function; `core` does, by name, at link time.
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
a type, not a pointer. `Bump::alloc` already takes one, so this half is done. `align()` is always
a non-zero power of two, which is exactly what `align_up` in `src/bump.rs:104` requires.

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
**R**e**q**uest. `cpu::without_interrupts` in `src/cpu.rs:24` saves and restores all four.

## 5. How a `push` gets to your allocator

The diagram of this descent is [`diagrams/heap.tldx.jsx`](diagrams/heap.tldx.jsx) - open it with
`tldx serve docs/diagrams/heap.tldx.jsx`.

Seven steps, in order. The first four were verified from the assembly of a ten-element `Vec<u32>`;
the last three are the layers this project builds:

1. `v.push(i)` sees the vector is full and calls `alloc::raw_vec::RawVec::grow_one`.
2. `grow_one` computes a new capacity, builds a `Layout` for it, and calls `__rust_realloc` with
   the old pointer, the old size, the alignment, and the new size.
3. `__rust_realloc` is the shim the attribute generated. It rebuilds the old `Layout` and calls
   `GlobalAlloc::realloc` on your static.
4. Your `realloc` - the default one, unless you override it - calls your `alloc` with the new
   layout, copies the old bytes over, and calls your `dealloc` on the old pointer.
5. Your `alloc` takes the lock on the `static`, calls into SLAB with the `Layout`, and turns the
   `Option<NonNull<u8>>` it gets back into a raw pointer.
6. SLAB rounds the layout to a size class and pops a free slot, or asks FRAMES for a page if no
   partial slab has room. A request too large for any class goes straight to FRAMES.

If step 5 comes back empty, the pointer is null, and step 2 sees the null and jumps to
`raw_vec::handle_error`, which is the panic described in section 2.

Step 5 is the whole skill, and by this point it is thin: SLAB and FRAMES do the allocating, and
this layer is the adapter between Rust's expectations and theirs. The conversion at the end of it
is smaller than it sounds:
`Option<NonNull<u8>>` and `*mut u8` are the same bits in memory, because `NonNull` cannot be zero
and the compiler uses zero to mean `None`. Turning one into the other is one method call and
compiles to nothing.

## 6. The three things that do not fit

### The trait wants `&self`, `Bump` wants `&mut self`

```rust
unsafe fn alloc(&self, layout: Layout) -> *mut u8;
```

Shared reference. It has to be: the allocator is a `static`, and a `static` can never be borrowed
mutably. But `Bump::alloc` is `&mut self` and must be - it moves `next`.

The answer is `UnsafeCell<T>`, the one type in Rust that legally hands out a `*mut T` from a
`&self`. Everything else with interior mutability, from `Cell` to `Mutex`, is built on it.

You already have this exact pattern in the repo. `src/irq.rs:15` is a struct wrapping an
`UnsafeCell`, with an `unsafe impl Sync` under a comment explaining why the overlap it permits
cannot happen. Read that comment before you write yours - the shape of the argument is the same,
but the reason is not, and yours is section 6c.

`Sync` is required because a `static` is reachable from every core, so the compiler demands proof
that sharing it is sound. `Bump` is four `usize`-shaped fields and an array of `Region`, so
nothing inside it objects; the `UnsafeCell` is what makes the wrapper not `Sync`, and the
`unsafe impl` is you asserting the thing the wrapper does about it.

### A `static` is built at compile time, the allocator is built at boot

`static HEAP: Heap = ...` is baked into the image. But FRAMES needs the device tree and a `Bump`,
which only exist once `kmain` is running.

The tempting shape is a static holding "an allocator, or nothing yet", and it is the wrong one.
The better shape is a static that is **always** there, born empty, given its memory later:

- An empty allocator already refuses everything. Its arena is zero pages, so the bounds check
  fails on the first request without a single extra line. `Option` adds a state and a branch to
  express something the data already expresses.
- `const fn new()` producing that empty state is easy - `Region::EMPTY` is already a const and
  `Region` is `Copy`, so the fixed arrays inside are const-constructible.
- `init` then fills in the arena rather than installing a value. Refuse if it has already been
  called, which is one compare, and you get back the double-init detection that `Option` would
  have given you.

An allocation before `init` therefore panics with `memory allocation of N bytes failed`. That is a
good failure: it names itself, at the line that did it, instead of writing over address zero.

The cost is real and worth seeing before you commit. If the static owns the allocator from birth,
`kmain` no longer has a local to lend out as `&mut`. Anything that needs one borrows through the
static:

```
heap::with(|frames| { ... })
```

Signatures of the things being called do not change. And the lock guard from LOCK is already
closure-shaped, so the two compose rather than fight.

### Interrupts are already live

`irq::unmask()` runs at `src/main.rs:92`, well before any of this. From that instruction onwards,
the timer handler and the serial handler can run between any two instructions of any function,
including between the read of `next` and the write of `next`.

Nothing allocates from a handler today. The moment one does - a handler pushing a byte into an
owned buffer, say - an allocation can begin inside an allocation that is halfway done.

The tool already exists, because LOCK built it: `SpinLock<T>` masks interrupts, then takes the
lock, and its guard releases and restores on drop. Neither half alone is enough - masking does
nothing about another core, and a bare lock deadlocks against your own handler - which is the
whole argument in the LOCK guide, section 3.

So the allocator's `static` is a `SpinLock<Slab>`, and `alloc` and `dealloc` each take the guard
for the length of one call.

The cost is worth naming: interrupts are masked for the length of one slab allocation, which is a
class lookup and a free-list pop. Tens of nanoseconds. The path that is not is the one where SLAB
has to go to FRAMES for a new page, which can split blocks down several orders - still bounded,
but worth measuring rather than assuming once it is running.

## 7. `dealloc`, for real

By the time this skill runs, freeing works. SLAB puts the slot back on its free list, and when the
last object in a page is freed, SLAB hands the page back to FRAMES, which merges it with its buddy
and back up the orders. Nothing leaks.

That is worth stating plainly because the shortcut was tempting and was rejected on purpose. A
`GlobalAlloc` over `Bump` with an empty `dealloc` would have worked, in the sense that `Vec` and
`Box` would have compiled and run. It would also have leaked every byte it ever handed out, and it
would have leaked invisibly - `zhemon` allocating a few hundred times would never have shown it.
The first thing to notice would have been SCHED, spawning and exiting tasks in a loop, dying after
an hour for no visible reason. Traps that hide for three skills are the expensive kind.

The two methods you do not have to write:

**`realloc`** has a default body that allocates the new block, copies, and frees the old one. It is
correct, and it is what a growing `Vec` uses. Overriding it is a real optimisation once SLAB
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
live. With real freeing underneath, the first two blocks go straight back to their slab as the
vector grows, so the memory actually in use afterwards is 64 bytes and not 112.

Both numbers matter, and they measure different things. Bytes requested is a property of
`RawVec` and will be 112 whatever you do. Bytes still held is a property of your allocator, and it
is 64 only if `dealloc` really works. Print both.

Then drop the vector and print again. Back to zero is the number that says the whole stack -
`GlobalAlloc`, SLAB, FRAMES - closed the loop.

## 9. What you are building

One module, `src/heap.rs`. Nothing else in the tree changes except four lines in `main.rs`.

A type - call it `Heap` - wrapping the SLAB allocator in the `SpinLock<T>` from LOCK, with:

- a `const fn new()` producing the empty state, so a `static` can be built from it
- `fn init(&self, bump: &mut Bump, arena: Region)` building FRAMES and SLAB into it
- `fn free_bytes(&self) -> usize`, forwarding down to FRAMES, for the observable in section 13
- `unsafe impl GlobalAlloc for Heap`, with `alloc` and `dealloc`, each taking the lock guard for
  the length of one call

Note what is **not** in that list. There is no `unsafe impl Sync` and no `UnsafeCell`, because
`SpinLock<T>` already carries both and already carries the argument for them. That is the payoff
for having built LOCK first: this skill inherits a safety proof instead of writing a new one.

And, outside the type:

- `static HEAP: Heap = Heap::new();` carrying `#[global_allocator]`.
- `extern crate alloc;` in `main.rs`. The 2024 edition removed the need for `extern crate` for
  ordinary dependencies, but `alloc` is not an ordinary dependency - it is not in `Cargo.toml` and
  the compiler will not link it unless you name it.

The hard part is not any of those signatures. It is that `alloc` must never, on any path, allocate
- not a log line, not a formatted error, not a `Vec` for bookkeeping. Anything that allocates
inside the allocator takes a lock the current call already holds, and LOCK section 3 says what
that is: a permanent stop. Read your own `alloc` body once looking only for that.

## 10. Where it goes in `kmain`

The order is forced by what each step needs from the one before it. Current `kmain`, from
`src/main.rs:100`, with the new steps folded in:

1. `Bump::discover` - the arena and the reservations exist
2. `Table::new(&mut bump)` and two `identity_map` calls - the tables exist
3. `mmu::enable(&mut table)` - translation is on
4. **`heap::init(&mut bump, board.memory)`** - FRAMES takes its metadata array from `Bump`, then
   the free ranges, then SLAB sits on top. `Vec` starts working here.

Step 4 has to be after step 1, because FRAMES needs `Bump` and needs to know what `Bump` reserved.
There is no reason to put it before step 3, and one reason not to: a fault while building the
allocator is easier to read once the tables are live.

`Bump` is borrowed, not consumed, and it keeps whatever is left. Nothing else will ask it for
memory, but it stays as the record of what was reserved.

One thing to check rather than assume: the memory the heap hands out has to be mapped. Step 2 maps
all of `board.memory` with `Descriptor::NORMAL_BLOCK`, and the arena is a range inside
`board.memory`, so every address the heap can return is covered. If you ever shrink that mapping,
this is what breaks first, and it breaks as a data abort on a write to a `Vec` rather than
anywhere near the mapping code.

## 11. When nothing happens

| symptom                                                                                  | almost certainly                                                                                                                                                                                                               |
| ---------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| link fails, undefined symbol ending `__rust_no_alloc_shim_is_unstable_v2`                | `extern crate alloc` is there but `#[global_allocator]` is not, or it is behind a `cfg` that is off in this build. The marker symbol exists to make exactly this a link error instead of a mystery.                            |
| link fails, undefined `__rust_alloc`                                                     | same cause. These two always appear together.                                                                                                                                                                                  |
| `memory allocation of N bytes failed` at the first `push` on the machine                 | `alloc` returned null. Either `heap::init` never ran, or it ran after the allocation, or memory really is exhausted. Print `free_bytes()` to tell which.                                                                       |
| the machine stops dead on the first allocation, no output                                | `alloc` allocated. A `println!`, a formatted error, or any bookkeeping inside the allocator takes the lock the current call already holds. Section 9.                                                                          |
| `free_bytes()` drops and never comes back                                                | `dealloc` is reaching the wrong layer - freeing to FRAMES what SLAB handed out, or the reverse. The `Layout` decides which, and it has to be the same decision `alloc` made.                                                   |
| `Vec` contents are garbage after it grows                                                | an overridden `realloc` that returned the same pointer for a size that changed class, so the copy never happened. Only skip the copy when the class is genuinely unchanged.                                                    |
| data abort on the first write into a `Vec`, any alignment                                | the address is outside what `identity_map` covered. Section 10.                                                                                                                                                                |
| the machine stops dead the first time a handler allocates                                | recursion into the lock, as above, or a lock taken without masking. LOCK section 3.                                                                                                                                            |
| everything works, `.text` grew by several kilobytes                                      | expected, and not from the allocator. `Box` and `Vec` pull in `raw_vec`, its overflow checks and its error path.                                                                                                               |
| clippy complains about a `&mut` derived from a `&self`                                   | it is right to ask. Answer it in the safety comment rather than silencing it.                                                                                                                                                  |

## 12. How you will know it worked

Push ten numbers into a `Vec<u32>` in `kmain`, print the length, print the sum, and print
`heap::free_bytes()` before, during, and after the vector is dropped.

The output that means it worked:

- the length is 10 and the sum is what you computed on paper, which proves the memory survived two
  reallocations and two copies
- while the vector is alive, free memory is down by the size of one slab page, not by 112 bytes.
  Do not expect the request size here - SLAB took a whole page and will hand the rest of it to the
  next allocation, which is the layer doing its job
- **after the vector is dropped, free memory is back to exactly where it started.** Not close.
  Exactly. This is the number that says the whole stack closed the loop, and it is the first time
  in this project that memory has ever come back
- `make run` still reaches the monitor prompt afterwards, which proves nothing was written over

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
- `src/irq.rs` and `src/cpu.rs` in this repo. Both patterns in section 6 are already there.
