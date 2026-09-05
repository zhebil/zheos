# AGENTS.md

Guidance for Claude Code when working in this repository.

## What this project is

`zheos` - a learning project. Bare-metal, low-level programming on QEMU the way Ben Eater does
it on breadboards: understand the CPU, memory map, and I/O devices at the level of "I write
this byte to this address and something physically happens".

Long-term goal: a very simple kernel from scratch. Not a product. No deadline. The point is
understanding, not shipping.

Target is `aarch64-unknown-none` on QEMU's `virt` machine, chosen because `virt` has flat
memory, no BIOS, and no legacy modes - `-kernel` loads an ELF and the CPU starts at its entry
point. The Ben Eater feel is preserved: the UART is a PL011 at `0x0900_0000`, and storing a
byte there prints a character.

## The framing: a survival skill tree

The project is run as a survival game. The machine starts with a CPU, some RAM, and a few
devices at fixed addresses - no OS, no library, no `print`, no allocator. Every ability it
ever has, the user builds. Each issue in beads is a **skill**; its dependencies are real
technical prerequisites, not arbitrary ordering. `docs/roadmap.md` is the map, and it is not
authoritative - beads is.

Use this framing when talking about progress. It is not decoration: it is why "one step at a
time, and do not run ahead" is a rule rather than a preference. A skill is not unlocked until
the user can explain it out loud.

**Roadmap and current state live in beads, not in this file.**

```sh
bd ready            # skills unlockable right now
bd list             # the whole tree
bd show <id>        # what a skill is, plus notes from when it was built
bd blocked          # what is still locked, and by what
```

Do not duplicate any of it here; it will go stale. Titles are prefixed by tier
(`T0 ·`, `T1 ·`, `SIDE ·`) and labelled `tier-N` / `side-quest`.

If a diagram of the tree is ever wanted, generate it on demand from `bd list --json` - the
dependency graph and the tier labels are all it needs. Do not commit one; a checked-in picture
of a moving graph is guaranteed to be wrong.

## Working agreement (IMPORTANT - overrides default behavior)

1. **Never write code unless the user explicitly asks for it.** This is the core rule. The user
   writes the code; that is the whole point of the project. Allowed without asking: config
   files, build scripts, linker scripts if requested, docs, commit messages, notes.
2. **Simple words, and be short.** Default to the shortest explanation that actually answers the
   question. No jargon dumps, no background nobody asked for, no restating what they just said.
   When a new term appears (MMIO, ELF, linker script, exception level, trap), define it in one
   plain sentence before using it. Go long only when asked, or when the short answer would be
   wrong.
3. **Explain fully, do not quiz.** Answer the whole question. Do not end replies with questions
   back at the user and do not withhold part of an explanation to make them work it out. Short
   means no padding, not a partial answer.
4. **Links are an optional appendix.** Put primary sources at the end, marked as optional
   reading. Never make reading a prerequisite for the next step.
5. Reviewing, debugging, and explaining *their* code is always welcome - that is not "writing
   code".
6. Do not introduce a crate or a macro that hides hardware detail unless they ask for it.
   Assembly only where unavoidable (the instructions before a stack exists); Rust everywhere
   else.
7. **No tests, and no test-driven development.** Decided 2026-08-27, after the host unit tests
   that existed were deleted. Do not write `#[cfg(test)]` modules, do not add a `cargo test`
   target, do not propose writing a test first, and do not put a "Testing it" section in a
   guide. The kernel is verified by booting it and reading the output - that is what the
   "How you will know it worked" section of every guide is for. If a guide needs to argue that
   something is correct, argue it as a boot-time observable.

## Build

Cargo, edition 2024, no dependencies. The `Makefile` is the front end so QEMU targets stay one
command.

```
Cargo.toml           panic = "abort", debug symbols on in release
.cargo/config.toml   pins target = aarch64-unknown-none
build.rs             passes -Tlinker.ld, plus rerun-if-changed so edits rebuild
linker.ld            layout; ENTRY(_start), plants __bss_start/__bss_end/__stack_top
src/kernel.s         stack, .bss zeroing, calls kmain. Pulled in by global_asm!(include_str!)
src/main.rs          kmain, panic handler, bit_mask
src/uart.rs          PL011 driver
```

`make kernel.elf` runs cargo and copies the output to `kernel.elf` at the repo root, which is
the path the debugger and every QEMU target use. There is no separate assembler step.

Toolchain: rustc/cargo 1.97, `qemu-system-aarch64` 11.0.3, `clang`, `make`, `lldb`,
`llvm-tools`. **No `gdb`** - `lldb` talks to QEMU's stub via `gdb-remote localhost:1234`, but
expect to translate tutorials that assume gdb.

## Commands

```sh
make run                          # build + boot, serial here. Ctrl-A X quits, Ctrl-A C = monitor
make debug                        # frozen at instruction 0, gdb stub on :1234
make regs                         # boot, dump PC/SP/X0-X3, exit
make mem ADDR=0x09000018 N=4 FMT=xw   # dump physical memory or device registers
make feed INPUT='abc'             # pipe scripted keystrokes to the guest's serial input
make test-bss                     # poison .bss, boot, verify it was zeroed (with a guard word)
make lint                         # clippy -D warnings + rustfmt --check; the pre-commit gate
make dis / sections / syms        # llvm-objdump / readobj / nm on kernel.elf
make trace                        # -d int,in_asm with a size cap
make kill                         # kill every stray qemu-system-aarch64
```

Monitor commands: `info mtree -f` (flat memory map), `info registers`, `info qtree`,
`xp /16xb <addr>`, `system_reset`.

Regenerate the device tree:

```sh
qemu-system-aarch64 -M virt,dumpdtb=virt.dtb -cpu cortex-a72 -nographic
dtc -I dtb -O dts virt.dtb -o virt.dts
```

### Housekeeping - ALWAYS kill QEMU after a test

The kernel ends in an infinite busy loop. QEMU cannot tell that from real work, so **every
abandoned instance pins a full CPU core forever**, and instances whose terminal closes get
reparented to launchd and survive invisibly. Four accumulated once and ate four cores.

- Terminate every ad-hoc `qemu-system-aarch64` run. Never leave one across turns. Check with
  `make kill`.
- Prefer the non-interactive targets (`make regs`, `make mem`, `make feed`) - they pipe `quit`
  to the monitor and exit on their own.
- Tracing writes to disk with no built-in limit. A forgotten `-d in_asm,int` run once grew to
  **174 GB** in three hours. `make trace` caps it with `ulimit -f`; any hand-rolled
  `-d ... -D file` run needs the same.

## The `virt` memory map

Verified with `info mtree -f` on `-M virt -cpu cortex-a72 -m 128M`. Full device tree in
`virt.dts`.

| Address range | What it is |
|---|---|
| `0x00000000-0x07FFFFFF` | pflash banks 0 and 1, where UEFI would live. Unused with `-kernel`. |
| `0x08000000` | `gic_dist` - interrupt controller, distributor half (GICv2) |
| `0x08010000` | `gic_cpu` - interrupt controller, per-CPU half |
| **`0x09000000`** | **`pl011` - the UART** |
| `0x09010000` | `pl031` - real time clock |
| `0x09020000` | `fw_cfg` - QEMU's channel for passing config to the guest |
| `0x09030000` | `pl061` - GPIO controller (closest thing to Ben's 6522) |
| `0x0A000000-0x0A003FFF` | 32 `virtio-mmio` slots, 0x200 apart |
| **`0x40000000-0x47FFFFFF`** | **`mach-virt.ram` - 128 MiB of RAM. Code and data go here.** |
| `0x4010000000+` | PCIe config / MMIO windows |

Everything else is unmapped; touching it raises a data abort. RAM base is always `0x40000000`
and `-m` changes only the size. Devices sit *below* RAM, the opposite of Ben's layout.

## Debugging signatures

- **`PC=0x200`** means a fault was taken *before* `install_vectors()` ran. `0x200` is the
  synchronous-exception slot's offset, fixed by the architecture; QEMU leaves `VBAR_EL1` at 0, so
  the slot resolves into empty flash and the CPU fault-loops forever. From outside it looks like
  a hang. Check with `make regs`. After the vectors are installed a fault prints a report
  instead, so seeing this again means the install was skipped or ran too late.
- **MMU off means all memory is Device memory**, which forbids unaligned access. An 8-byte load
  from a 4-byte-aligned address raises an Alignment fault (ESR DFSC `0x21`). Disappears once the
  MMU is on, so it is a phase-specific trap.
- **QEMU is more lenient than the hardware.** It ignores the UART enable bit and the baud rate
  entirely, so "it still prints" is never evidence that init is correct. Read the registers back
  with `make mem` instead.
- `-d int` logs every exception with its ESR; `-d in_asm` logs translated blocks.

## Primary sources

- **ARM PrimeCell UART (PL011) TRM**, ARM DDI 0183, on developer.arm.com. Chapter 3
  "Programmer's Model" has the register layout, bit fields, and reset values.
- **`hw/char/pl011.c`** in the QEMU source. Ground truth for what is *actually* emulated, as
  opposed to what the TRM requires. The gap between the two is where "works in QEMU, dead on
  hardware" bugs come from.
- `virt.dts` - the machine's own description of its memory map. Gives device base addresses, and
  `apb-pclk` gives UARTCLK (24 MHz).
- QEMU `virt` docs: <https://www.qemu.org/docs/master/system/arm/virt.html>
- ARM Architecture Reference Manual (ARMv8-A) - the CPU itself. Read sections on demand.
- OSDev wiki - good concepts, mostly x86-flavored, treat with care.

Use the `find-docs` skill rather than recalling API details from memory.

## Note on the block below

`bd init` generated it and rewrites it, so do not edit inside the markers. Three corrections: there
**is** a git remote now (`origin`, on the `github.com-personal` host alias), so `git push` applies;
there is no Dolt remote, so ignore `bd dolt push`; and where the session-close checklist says to run
"Tests, linters, builds", there are no tests - the gate is `make lint`, and booting it.

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **Clean up** - Clear stashes
5. **Verify** - All changes committed
6. **Hand off** - Provide context for next session

<!-- END BEADS INTEGRATION -->
