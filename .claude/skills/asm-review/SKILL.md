---
name: asm-review
description: Use when the user asks what their Rust compiled to, whether code is optimized, "check the asm", "review my diff for codegen", or after they finish writing a chunk of kernel code. Reads the diff, maps it to the generated aarch64 assembly, and explains what the compiler did with it and what it actually costs.
---

Review the user's recent Rust against the assembly it produced. The goal is habit, not audit: they
should end up predicting the codegen before running this.

## Gather

```sh
make asm                          # kernel.asm, source interleaved
git diff                          # or `git diff HEAD~1` if they just committed
make sections                     # .text size
$BIN/llvm-nm -n kernel.elf        # which functions still exist as symbols
```

`$BIN` is `$(rustc --print sysroot)/lib/rustlib/aarch64-apple-darwin/bin`.

Only read the parts of `kernel.asm` covering changed code. Compare `.text` against the previous
build when the diff is non-trivial.

## What to check

**Did the abstraction survive?** Any Rust function still present in `llvm-nm` output was not
inlined. In a kernel this size that is usually unintended. A `bl` to a mangled name, or
`sub sp, sp, #N` in a function with no locals, means a real call and a real frame.

**Did the constants vanish?** `const fn`, `const` items and compile-time math should leave no
arithmetic at all. Seeing `udiv`, `mul`, or shifts on something you believed was constant means
it is runtime.

**Loops.** `[u8; N]` has its length in the type and unrolls, with the data baked into immediates
and nothing in `.rodata`. `&[u8]` does not: real counter, real pointer, and the bytes land in
memory. Both are fine; know which one was written.

**Bounds checks.** `buf[i]` with a runtime `i` emits a compare and a branch to a panic block.
Iterating instead emits nothing. Worth flagging in ring buffers and line editors, where it is
easy to write dozens by accident.

**Panic machinery.** A `bl` into anything with `panic` or `fmt` in the name pulls in kilobytes.
An unexplained `.text` jump is almost always this, or `memcpy`.

**Volatile.** Ordering is preserved between volatile accesses and nothing else moves across them,
which is the point for MMIO. It also means a redundant volatile read is a real extra bus
transaction. Reading one status register twice in a function is the common case: read once into a
local, test bits on the local.

**Address constants.** A repeated `mov wN, #<large constant>` before each store means LLVM
rematerialized a cheap constant instead of holding it in a register. It happens because a const
base plus a const offset folds into one bare integer in the front end, destroying the base/offset
relationship before the backend sees it. Changing the field from `usize` to `*mut u32` does not
fix it; only a base that is genuinely unknown at compile time does. Usually not worth fixing.

**Alignment, while the MMU is off.** All memory is Device memory, so unaligned access faults.
Any multi-byte access to a computed address is a hazard until Tier 4.

## Prove it, do not assert it

Never make a claim about codegen without compiling it. To test a variant without touching the
kernel, write a standalone file in `/tmp` and compile it directly:

```sh
rustc --target aarch64-unknown-none --crate-type lib -O --emit asm -o v.s v.rs
```

It needs `#![no_std]` and a `#[panic_handler]`, nothing else. No linker script, no build system.
Compile the user's version and the alternative and diff them. A negative result (the "better"
version generates identical code) is one of the more useful things to hand back.

To study one function in isolation, `#[inline(never)]` keeps it as a real symbol. Remove it after.

## Sort every finding into one of three buckets

1. **Real waste, worth changing.** Say what to change and what it saves.
2. **Real cost, inherent and correct.** Name it so it is not mistaken for waste later. A spin loop
   on a UART flag is not slow code.
3. **A curiosity, not worth fixing.** Explain the mechanism, then say plainly to leave it alone.

Most findings are 2 and 3. Do not invent bucket 1 work to look useful. "This is already about as
tight as it gets, here is why" is a valid and common answer, and saying it is what teaches the
user where the real costs are.

Weigh cost against context: an instruction issued in the shadow of a slow uncached device read is
free, whatever the instruction count says.

## Rules

Same working agreement as the rest of the repo:

- Do not write the fix. Explain what is happening and what the options cost. The user writes it.
- Plain words. Define a term the first time it appears (rematerialization, inlining, addressing
  mode) in one sentence.
- Explain fully, do not quiz. No questions back at the end.
- QEMU is more forgiving than hardware. Correct output is never evidence of correct code.

## Output

Per changed function: what it became, then the bucket. Short. If nothing is worth changing, say so
in a line and spend the space on why the compiler made the choice it made.
