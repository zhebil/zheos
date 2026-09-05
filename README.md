# ZheOS

A bare-metal learning project. Rust on an emulated ARM64 machine, starting from nothing.

## What this is

ZheOS exists to teach me things - low-level code, Rust without a standard library, the
algorithms real operating systems use. And because it is fun to make a machine do something.
That is the whole purpose.

**This is not a real operating system and it never will be.** No roadmap to a release, no
users, no deadline.

I think the best way to understand something is to rebuild it from scratch. Reading about a
memory allocation teaches you theory, writing one really makes you understand it.

## The AI rule

This is an educational project, so it comes with a rule I hold myself to:

> **AI does not write code in this repo. I do.**

I use AI heavily - to explain things I do not understand, to ask for clarifications, to argue
with, to review what I wrote and tell me what the compiler actually did with it. It reads specs
with me and drafts docs. It does not write the kernel. Handing the interesting part to a model
would defeat the only reason this repo exists.

That rule is written into the repo's own agent instructions, in
[`AGENTS.md`](AGENTS.md), so any assistant working here inherits it.

This is a hobby. Hobbycoding.

## The survival game

I treat the project as a survival game.

You wake up on a machine with a CPU, some RAM, and a handful of devices wired to fixed
addresses. Nothing else. No operating system, no library, no `print`, no allocator, no concept
of a file. Every ability this machine will ever have, you have to build.

That is not just a metaphor. It is literally the starting state of `qemu-system-aarch64 -M virt
-kernel`.

So every capability is a **skill** on a tree, and a skill's prerequisites are real technical
prerequisites, not arbitrary ordering: you cannot write a monitor before you can read a
character, and you cannot read a character before there is a stack to hold the function that
reads it. One skill at a time, and a skill is not unlocked until I can explain it.

The map is [`ROADMAP.md`](ROADMAP.md). The authoritative tree lives in beads (see
[Tools](#tools)).

## The machine

|         |                                                                                              |
| ------- | -------------------------------------------------------------------------------------------- |
| Target  | `aarch64-unknown-none-softfloat`                                                             |
| Machine | QEMU `virt`, `-cpu cortex-a72`, 128 MiB                                                      |
| Entry   | `-kernel` loads the ELF and the CPU starts at `_start`. No BIOS, no bootloader, no firmware. |
| RAM     | `0x4000_0000`, 128 MiB                                                                       |
| Console | PL011 UART at `0x0900_0000` - store a byte there and a character appears                     |

The full memory map, straight from the machine's own device tree, is in [`virt.dts`](virt.dts)
and summarised in [`AGENTS.md`](AGENTS.md).

The softfloat variant of the target matters: plain `aarch64-unknown-none` enables NEON, and
the CPU traps every FP/SIMD instruction at reset until `CPACR_EL1.FPEN` is set.

## Getting started

### Prerequisites

- **Rust** 1.97 or newer (edition 2024), plus the bare-metal target:
  ```sh
  rustup target add aarch64-unknown-none-softfloat
  ```
- **QEMU** with ARM support - `qemu-system-aarch64` 11.x
  ```sh
  brew install qemu          # macOS
  sudo apt install qemu-system-arm   # Debian/Ubuntu
  ```
- **`make`**
- **`llvm-tools`** (optional, for the disassembly and symbol targets):
  ```sh
  rustup component add llvm-tools
  ```

No crates. No `std`, no `alloc`, no dependencies at all - that is the point.

> The `Makefile` looks up the LLVM binaries under an `aarch64-apple-darwin` sysroot, so on a
> non-Apple-Silicon host you need to adjust the `BIN` variable at the top. Everything except
> `dis`, `asm`, `sections`, `syms` and `test-bss` works regardless.

### Run it

```sh
make run
```

**To quit QEMU:** `Ctrl-A` then `X`. `Ctrl-A` then `C` drops into the QEMU monitor.

> ⚠️ The kernel ends in a busy loop, which QEMU cannot tell from real work, so an abandoned
> instance pins a CPU core forever. `make kill` cleans up strays.

### Everything else

```sh
make debug        # boot frozen at instruction 0, gdb stub on :1234
make regs         # boot, dump PC/SP/X0-X3, exit
make mem ADDR=0x09000018 N=4 FMT=xw    # dump physical memory or a device register
make feed INPUT='40000000\r'           # pipe scripted keystrokes to the guest
make test-bss     # poison .bss, boot, verify it was zeroed
make dis          # disassemble
make asm          # disassembly with the Rust source interleaved, into kernel.asm
make sections     # section headers
make syms         # symbols, sorted by address
make trace        # -d int,in_asm, with a size cap so it cannot eat the disk
make kill         # kill every stray qemu-system-aarch64
make clean
```

## Tools

Two tools outside the kernel earn their place here.

**[tldx](https://github.com/zhebil/tldx)** - architecture diagrams written as JSX and rendered
with tldraw. Built by a genuinely ingenious developer (me). It exists because diagrams are the
fastest way to understand something, and because a AI can create them fast and sometimes good.
The `.tldx.jsx` sources live in `docs/diagrams/`.

**[beads](https://github.com/steveyegge/beads)** - a git-backed issue tracker built for coding
agents. Here it is used as a local, plain-text task tracker for a human.
It holds the skill tree, and it is the authoritative one - `ROADMAP.md` is just the map on the
wall.

```sh
bd ready            # skills unlockable right now
bd list             # the whole tree
bd show <id>        # what a skill is, plus notes from when it was built
bd blocked          # what is still locked, and by what
```

## Licence

MIT. See [LICENSE](LICENSE).
