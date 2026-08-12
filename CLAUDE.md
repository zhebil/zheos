# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this project is

`zheos` - a learning project. The user wants to do bare-metal, low-level programming on a
virtual machine (QEMU) the way Ben Eater does it on breadboards: understand the CPU, memory
map, and I/O devices at the level of "I write this byte to this address and something
physically happens".

Long-term goal: write a very simple kernel from scratch. Not a product. No deadline. The point
is understanding, not shipping.

Current state: milestones 1-4 done. `kernel.s` sets up a stack, zeroes `.bss`, and prints a
string via the PL011 at `0x09000000`. `linker.ld` controls layout, `make test-bss` verifies the
zeroing with a poison-and-guard test. Milestone 5 in progress: handing off from assembly to
`#![no_std]` Rust in `main.rs`.

Build is `make`-driven, not cargo. `rustc --emit=obj` produces `main.o`, which is linked
alongside `kernel.o` by `rust-lld`. No cargo until there is a reason for one.

## Working agreement (IMPORTANT - overrides default behavior)

1. **Never write code unless the user explicitly asks for it.** This is the core rule. The user
   writes the code; that is the whole point of the project. Allowed without asking: config
   files, build scripts, linker scripts if requested, docs, commit messages, notes.
2. **Explain first, and explain in simple words.** No jargon dumps. When a new term appears
   (MMIO, ELF, linker script, exception level, trap), define it in one plain sentence before
   using it.
3. **Explain fully, do not quiz.** Give the whole answer in plain language. Do not end replies
   with questions back at the user and do not withhold part of an explanation to make them
   work it out. They will ask when they want more.
4. **Links are an optional appendix.** Put primary sources at the end, marked as optional
   reading for the parts they found interesting. Never make reading a prerequisite for the
   next step.
5. Reviewing, debugging, and explaining *their* code is always welcome - that is not "writing
   code".

## Language plan

Rust is the target language. Assembly only where it is unavoidable (the first instructions
after reset, before a stack exists). The plan is to write the few lines of assembly needed by
hand so the user understands what the CPU does with no runtime, then move to `#![no_std]` Rust
as soon as there is a valid stack.

The user wants to understand every line. Do not introduce a crate or a macro that hides
hardware detail unless they ask for it.

## Platform decision (proposed, not yet confirmed by the user)

Host is an Apple Silicon Mac. Recommended target: **`aarch64-unknown-none` on QEMU's `virt`
machine.**

Why this and not x86:
- QEMU `virt` is a machine invented for virtualization: flat memory, no BIOS, no legacy modes.
  `-kernel` loads an ELF and the CPU starts at its entry point. Nothing else runs first.
- x86 boots through 16-bit real mode -> 32-bit protected mode -> 64-bit long mode, plus BIOS or
  UEFI, plus a 512-byte boot sector. That is a lot of historical accident to learn before the
  first character appears on screen.
- `aarch64-unknown-none` is a built-in Rust target - no custom target JSON, no nightly needed
  for the basics.
- Host is already arm64, so what the user learns matches the machine on their desk.

The Ben Eater feel is preserved: the UART on `virt` is a PL011 at physical address
`0x0900_0000`. Storing a byte there prints a character. That is a memory-mapped register, same
idea as wiring a chip to an address decoder.

If the user prefers x86_64 (to learn how a real PC boots) that is a legitimate different
project - re-plan rather than mixing the two.

## Toolchain

Verified present: `rustc`/`cargo` 1.89 (stable + nightly, host `aarch64-apple-darwin`),
`clang`, `make`, `lldb`.

Missing, needed before anything runs:

```sh
brew install qemu                        # provides qemu-system-aarch64
rustup target add aarch64-unknown-none   # bare-metal Rust target, no OS, no libc
```

Notes:
- There is **no `gdb`** on this machine, only `lldb`. For QEMU's remote debug stub
  (`-s -S`, GDB protocol on :1234), either `brew install gdb` / `aarch64-elf-gdb`, or connect
  with `lldb` via `gdb-remote localhost:1234`. `lldb` works but is less commonly documented for
  this; expect to translate tutorials.
- `cargo-binutils` + `llvm-tools` give `cargo objdump` / `cargo nm` / `cargo size`, which are
  the main way to inspect what was actually produced. Install when the user wants to look at
  their own binary.
- No `nasm` and no cross `gcc` - not needed for the aarch64 plan, Rust's own assembler
  (`global_asm!` / `.s` files via the LLVM toolchain) covers it.

## The `virt` memory map (verified, QEMU 11.0.3, `-M virt -cpu cortex-a72 -m 128M`)

Dumped with `info mtree -f`. Full device tree is in `virt.dts` (regenerate: see commands below).

| Address range | What it is |
|---|---|
| `0x00000000-0x03FFFFFF` | `virt.flash0` - pflash, where UEFI/`-bios` would live. Unused with `-kernel`. |
| `0x04000000-0x07FFFFFF` | `virt.flash1` - second flash bank. Unused. |
| `0x08000000` | `gic_dist` - interrupt controller, distributor half (GICv2) |
| `0x08010000` | `gic_cpu` - interrupt controller, per-CPU half |
| **`0x09000000`** | **`pl011` - the UART. Store a byte here and a character appears.** |
| `0x09010000` | `pl031` - real time clock |
| `0x09020000` | `fw_cfg` - QEMU's channel for passing config to the guest |
| `0x09030000` | `pl061` - GPIO controller (closest thing to Ben's 6522 parallel port) |
| `0x0A000000-0x0A003FFF` | 32 `virtio-mmio` slots, 0x200 apart - disk, net, etc. |
| `0x3EFF0000` | PCIe I/O window |
| **`0x40000000-0x47FFFFFF`** | **`mach-virt.ram` - 128 MiB of actual RAM. Code and data go here.** |
| `0x4010000000+` | PCIe config / MMIO windows |

Everything not listed is unmapped. Touching it raises a data abort.

RAM base is always `0x40000000`; `-m` changes only the size. Devices sit *below* RAM here, the
opposite of Ben's layout, which does not matter but surprises people once.

## Commands (verified working)

Assembly phase uses the `Makefile` (`make run`, `make debug`, `make regs`, `make clean`).
It assembles `kernel.s` with clang and links with `rust-lld`, both of which are already
installed - no Homebrew packages needed for the build. Verified working end to end.

```sh
make run      # build + boot, serial on this terminal, Ctrl-A X to quit
make debug    # same but frozen at instruction 0, gdb stub on :1234
make regs     # build + boot + dump CPU registers non-interactively + exit

# the two commands make run wraps, for reference:
clang --target=aarch64-unknown-none -c kernel.s -o kernel.o
$(rustc --print sysroot)/lib/rustlib/aarch64-apple-darwin/bin/rust-lld \
  -flavor gnu -Ttext=0x40000000 -e _start kernel.o -o kernel.elf

# regenerate the device tree (the machine describing its own memory map)
qemu-system-aarch64 -M virt,dumpdtb=virt.dtb -cpu cortex-a72 -nographic
dtc -I dtb -O dts virt.dtb -o virt.dts

# run a monitor command without an interactive session (useful for grepping)
printf 'info mtree -f\nquit\n' | qemu-system-aarch64 -M virt -cpu cortex-a72 -m 128M \
  -display none -serial null -monitor stdio
```

In `-nographic`, `Ctrl-A C` toggles between the guest serial console and the QEMU monitor,
`Ctrl-A X` quits.

### Housekeeping - ALWAYS kill QEMU after a test

The kernel ends in `b loop`, an infinite busy loop. QEMU cannot tell that from real work, so
**every abandoned instance pins a full CPU core forever**. Instances started from a terminal
that later closes get reparented to launchd (PPID 1) and survive invisibly. Four of them
accumulated once and ate four cores.

Rules:
- Every ad-hoc `qemu-system-aarch64 ...` run must be terminated when the test is done -
  `Ctrl-A X` interactively, or `kill <pid>` for anything launched in the background.
- Never leave a QEMU running across turns. Check with `make kill` (kills every
  `qemu-system-aarch64` and reports what is left).
- Tracing (`-d`) writes to disk with no built-in limit. A forgotten `-d in_asm,int` run grew
  `/tmp/broken.log` to **174 GB** in under three hours. `make trace` caps it via `ulimit -f`
  (`LOGCAP`, 512-byte blocks, default ~200 MB); QEMU dies with SIGXFSZ rather than filling the
  disk. Any hand-rolled `-d ... -D file` run needs the same `ulimit -f` in front of it.
- Prefer the non-interactive targets (`make regs`, `make mem`, `make feed`) for scripted checks -
  they pipe `quit` to the monitor and exit on their own.

Useful monitor commands: `info mtree -f` (flat memory map), `info registers`, `info qtree`
(device hierarchy), `xp /16xb <addr>` (dump physical memory), `system_reset`.

Binary inspection (`rustup component add llvm-tools` is installed). Tools live in
`$(rustc --print sysroot)/lib/rustlib/aarch64-apple-darwin/bin/`:
`llvm-objdump -d kernel.elf`, `llvm-readobj --section-headers`, `llvm-nm`, `llvm-size`.

## Debugging signatures

- **`PC=0x200`** means a fault was taken with no handler installed. `VBAR_EL1` is 0 at reset,
  `0x200` is the synchronous-exception slot, and it points into empty flash, so the CPU
  fault-loops forever. From outside it just looks like a hang. Check with `make regs`.
- `-d int -D /tmp/x.log` logs every exception with its ESR. `-d in_asm` logs translated blocks.
- **MMU off means all memory is Device memory**, which forbids unaligned access. An 8-byte load
  from a 4-byte-aligned address raises an Alignment fault (ESR DFSC `0x21`). This restriction
  disappears once the MMU is on, so it is a phase-specific trap.

## Toolchain gaps hit so far

- No `gdb`. `lldb` can talk to QEMU's stub via `gdb-remote localhost:1234` but expect to
  translate tutorials that assume gdb.

## Roadmap

Each step should end with the user able to explain what happened. Do not run ahead.

1. QEMU basics - what a machine model is, boot an existing kernel, learn the monitor
   (`Ctrl-A C`, `info registers`, `info mtree`).
2. Smallest possible thing that runs: a few instructions in an ELF, observed under the
   debugger. Nothing printed yet.
3. Print a character by storing a byte to the UART address. This is the "hello world" of
   bare metal.
4. Linker script - understand where code, data, bss and the stack land, and why the entry
   address must match what QEMU expects.
5. Set up a stack, zero `.bss`, jump into Rust. First `#![no_std]` Rust that runs.
6. A real UART driver (init, status register polling, read input as well as write).
7. **Wozmon** - the user's chosen first real project. A port of Steve Wozniak's 256-byte Apple I
   monitor (the one Ben Eater ports to his 6502): read a hex address over serial and print the
   byte, print a range, write bytes into memory, jump to an address and run it. Needs nothing
   but the UART. Becomes the interactive debugging console for every later step.
8. Exceptions and interrupts - the vector table, timer interrupt, the GIC.
9. Memory: physical memory map from the device tree, a bump allocator, then paging (MMU).
10. Only then: tasks/scheduling, i.e. the first thing that resembles a kernel.

## Where to look (primary sources)

- QEMU `virt` machine docs: <https://www.qemu.org/docs/master/system/arm/virt.html> - which
  devices exist and where.
- The generated device tree (`dumpdtb` above, read with `dtc -I dtb -O dts`) - the machine's
  own description of its memory map. Better than any tutorial for "what address is what".
- **ARM PrimeCell UART (PL011) Technical Reference Manual**, ARM DDI 0183, on developer.arm.com.
  The authoritative register layout. Chapter 3 "Programmer's Model" is the one that matters.
- **QEMU's own model**: `hw/char/pl011.c` in the qemu source tree on GitHub. Ground truth for
  what is *actually* emulated, as opposed to what the TRM requires. The two differ, and the
  differences are exactly where "works in QEMU, dead on hardware" bugs come from.
- `virt.dts` gives the base address and `apb-pclk` gives UARTCLK (24 MHz on this board).
- ARM Architecture Reference Manual (ARMv8-A) - the CPU itself. Huge; read sections on demand,
  never front to back.
- `rust-osdev/embedded-rust` ecosystem, `cortex-a` crate source - useful to *read* for how
  registers are accessed, not to depend on.
- OSDev wiki - good concepts, mostly x86-flavored, treat with care.

Use the `find-docs` skill rather than recalling API details from memory.
