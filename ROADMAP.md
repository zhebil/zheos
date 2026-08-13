# zheos - the skill tree

## The premise

You wake up on a machine with a CPU, some RAM, and a handful of devices wired to fixed
addresses. Nothing else. No operating system, no library, no `print`, no allocator, no
concept of a file. Every ability this machine will ever have, you have to build.

That is not a metaphor for the project. That is literally the starting state of
`qemu-system-aarch64 -M virt -kernel`.

So the project is run as a survival game. Each skill is a real, working capability that the
machine did not have before. Skills have prerequisites, because they genuinely do: you cannot
write a debugger console before you can read a character, and you cannot read a character
before there is a stack to hold the function that reads it. Nothing here is arbitrary
gamification - the dependency graph is the actual technical dependency graph.

## How it is tracked

The tree lives in **beads**, not in this file.

```sh
bd ready            # skills unlockable right now - nothing is blocking them
bd list             # the whole tree
bd show <id>        # what a skill is, how it will be tested, and notes from building it
bd blocked          # what is still locked, and by what
```

This file is the map you look at to remember why you are here. `bd ready` is the thing you
actually act on. If the two ever disagree, beads is right.

Rules of the game:

- **One skill at a time.** Do not run ahead. A skill is not unlocked until it is understood
  well enough to explain out loud.
- **Prove it, do not assume it.** QEMU is more forgiving than real hardware. "It still prints"
  is not proof. Reading the registers back is proof.
- **The user writes the code.** Claude explains, reviews, and debugs. Building the thing is the
  entire point of playing.

## Tier 0 - Bare Rock

*You have a CPU and nothing else. Every one of these is a precondition for having a program at
all.*

| Skill | What it really is | What it unlocks |
|---|---|---|
| **OBSERVE** | QEMU machine models, the monitor, `info mtree`, `info registers` | You can see the machine's state. Without this, every failure is a silent hang. |
| **FIRST BREATH** | A few instructions in an ELF, executing under the debugger | Proof that your bytes become the CPU's next instruction. |
| **SPARK** | Store a byte to `0x0900_0000` and a character appears | Output. The single most valuable thing to have early. |
| **GROUND** | A linker script: where `.text`, `.data`, `.bss` and the stack land | Control over the address space. |
| **FOOTING** | A stack, `.bss` zeroed, jump into `#![no_std]` Rust | Function calls and local variables exist. You can stop writing assembly. |

Tier 0 is complete. The machine can run a Rust program and shout one string into the void.

## Tier 1 - Tools

*Turn "poke an address and hope" into a device you actually control.*

| Skill | What it really is | What it unlocks |
|---|---|---|
| **CALIBRATE** | The PL011 init sequence: disable, flush, baud divisors, 8N1, enable | The device works to spec, not by QEMU's leniency. Real hardware would print nothing before this. |
| **VOICE** | `putc` that checks the transmit FIFO before writing | Output that does not silently drop bytes. |
| **EARS** | `getc`: poll the receive flag, read the data register | **Input.** The machine stops being a one-way broadcast and becomes interactive. |
| **LANGUAGE** | `impl core::fmt::Write` | `write!` with hex, padding and alignment, from `core`, with no allocator. |

EARS is the pivotal one in this tier. Everything before it is a program that talks at you.
After it, the machine can be asked questions.

## Tier 2 - Workshop

| Skill | What it really is | What it unlocks |
|---|---|---|
| **WORKBENCH** | Wozmon - Wozniak's 256-byte Apple I monitor | Read memory, write memory, jump to an address, all from the serial line. |

This is the first skill that exists to make *other* skills easier. Once WORKBENCH is up, every
later tier gets an interactive debugger that runs on the target itself, with no host tooling.
Ben Eater ports this to his 6502 for exactly the same reason.

## Tier 3 - Reflexes

| Skill | What it really is | What it unlocks |
|---|---|---|
| **REFLEX** | Vector table in `VBAR_EL1`, the GIC, timer interrupts | The machine can react to things instead of only polling. Faults report themselves instead of hanging at `PC=0x200`. |

This is where the CPU stops being a fast calculator and starts being something a kernel can be
built on. Nothing above Tier 3 is possible without it: preemption, real I/O, and privilege
boundaries all ride on the exception mechanism.

## Tier 4 - Territory

| Skill | What it really is | What it unlocks |
|---|---|---|
| **TERRITORY** | Read the memory map from the device tree, a bump allocator, then the MMU | Memory stops being one hardcoded constant and becomes a resource you manage. |

Turning the MMU on also quietly removes the alignment restriction that bites while it is off,
because memory stops being Device memory. Two milestones in one.

## Tier 5 - Civilization

| Skill | What it really is | What it unlocks |
|---|---|---|
| **MANY HANDS** | Tasks, context switching on the timer interrupt | More than one thing running. The first structure that genuinely deserves the word kernel. |

Deliberately last. Everything below it is a prerequisite, and attempting it early is the
classic way to build something that half works and cannot be debugged.

## Side quests

Optional branches. None of them are on the critical path, all of them teach something the main
line does not.

| Skill | Needs | Why bother |
|---|---|---|
| **TIMEKEEPER** | LANGUAGE | A second device, deliberately easy. Proves the driver pattern transfers to a chip you have never seen, from its manual alone. |
| **HANDS** | SIGNAL | The PL061 GPIO controller, the closest thing here to Ben's 6522 parallel port. |
| **STORAGE** | TERRITORY | virtio-mmio and a real disk. The first device that is not just registers: it needs shared memory rings. |
| **MANY CORES** | REFLEX | Wake the other CPUs with PSCI. Instantly every shared thing needs a lock, including the UART. |
| **TWO WORLDS** | REFLEX | Drop to EL0 and add syscalls. The moment there is a privilege boundary, this stops being a program and becomes an operating system. |

## Where things stand

Run `bd ready`. That is the answer, always, and it is the only place this is written down.

The visual version of this tree is in [SKILLTREE.md](SKILLTREE.md).
