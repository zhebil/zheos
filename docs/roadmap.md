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
bd show <id>        # what a skill is, plus notes from when it was built
bd blocked          # what is still locked, and by what
```

This file is the map you look at to remember why you are here. `bd ready` is the thing you
actually act on. If the two ever disagree, beads is right.

**Beads is finer-grained than this file.** A row in a table here is often several issues:
"Wozmon" is six separate skills, one per command. That is deliberate - each one should be a
single session's work, small enough to finish and understand. Start a session with `bd ready`
and you get something buildable, not a project.

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
| **FLUSH** | Wait for `BUSY` to clear before halting | Your last message actually leaves the wire instead of dying in the FIFO. |
| **RXERR** | Decode the framing, parity, break and overrun bits on receive | You find out when input was corrupted or dropped, instead of guessing. |
| **LANGUAGE** | `impl core::fmt::Write` | `write!` with hex, padding and alignment, from `core`, with no allocator. |
| **PANIC SPEAKS** | A panic handler that prints its message and location | A panic stops being indistinguishable from a hang. |

Two survival utilities also sit here, unlocked by FOOTING rather than by the UART:

| Skill | What it really is | What it unlocks |
|---|---|---|
| **IDLE** | `wfi` instead of `b .` in the park loop | The machine stops pinning a whole host CPU core while doing nothing. |
| **SHUTDOWN** | PSCI `SYSTEM_OFF` via `HVC` | The kernel can end. Every scripted run terminates on its own instead of needing a kill. |

EARS is the pivotal one in this tier. Everything before it is a program that talks at you.
After it, the machine can be asked questions.

## Tier 2 - Workshop

| Skill | What it really is | What it unlocks |
|---|---|---|
| **LINE** | Read a line with echo and backspace into a fixed buffer | Editable input. Everything above it assumes you can type a command and fix a typo. |
| **HEX IN** | Parse hex text into a `u64` | Half of Wozmon. `core::fmt` already gives you the printing half. |
| **PEEK** | Print the byte at an address | The machine becomes inspectable from its own console. |
| **SCAN** | Print a range, in rows, each prefixed by its address | Reading structures, not single bytes. |
| **POKE** | Write bytes into memory from typed hex | The console can now *change* the machine, not just watch it. |
| **LEAP** | Cast an address to a function pointer and call it | Type machine code in hex and run it. This is why Wozmon mattered on the Apple I. |
| **WORKBENCH** | Tie them together into Wozmon's real grammar | `ADDR`, `ADDR.ADDR`, `ADDR: XX XX`, `ADDR R`. |

This is the first tier that exists to make *other* tiers easier. Once WORKBENCH is up, every
later tier gets an interactive debugger that runs on the target itself, with no host tooling.
Ben Eater ports this to his 6502 for exactly the same reason.

## Tier 3 - Reflexes

| Skill | What it really is | What it unlocks |
|---|---|---|
| **VECTORS** | The exception vector table, installed in `VBAR_EL1` | Faults report their `ESR` and faulting address instead of hanging at `PC=0x200`. |
| **GIC** | Distributor and CPU interface brought up | A device raising an interrupt actually reaches your handler. |
| **TIMER** | The generic timer, reloaded on each tick | The first thing that happens without the program asking. |
| **UART IRQ** | Receive interrupt draining the FIFO into a ring buffer | Input stops requiring the CPU's full attention. |
| **REFLEX** | All four together | The machine reacts instead of polling. |

This is where the CPU stops being a fast calculator and starts being something a kernel can be
built on. Nothing above Tier 3 is possible without it: preemption, real I/O, and privilege
boundaries all ride on the exception mechanism.

## Tier 4 - Territory

| Skill | What it really is | What it unlocks |
|---|---|---|
| **DTB** | Parse the device tree QEMU leaves a pointer to in `x0` | The machine describes its own RAM instead of you hardcoding `0x40000000`. |
| **BUMP** | The simplest allocator: a pointer that only moves forward | Memory you can hand out. Enough to build page tables with. Demoted by FRAMES to what is left of it: the memory map. |
| **TABLES** | Build aarch64 translation tables | Virtual addresses exist, even if nothing uses them yet. |
| **MMU ON** | `TTBR0_EL1`, `TCR_EL1`, `MAIR_EL1`, then the M bit | Real memory management. The instruction after the enable is fetched through translation, which makes it the most delicate moment in the project. |
| **LOCK** | A spin lock that masks interrupts first | One primitive that closes both hazards: your own interrupt handler, and another core. Everything shared after this is protected rather than argued about. |
| **FRAMES** | A buddy page allocator | The first allocator that gives memory back. It bootstraps itself, so there is no memblock layer here - what survives of `Bump` is the reserved-region list, which is a data structure and not an allocator. |
| **SLAB** | Size classes cut out of a page | Small allocations stop wasting 99 percent of a page. This is what `kmalloc` is. |
| **HEAP** | A `GlobalAlloc` over SLAB | `Box`, `Vec`, `String`, with real freeing underneath. It stops feeling like assembly with extra steps. |
| **LOCKDOWN** | Per-region permissions and a guard page | `.text` stops being writable and the stack stops being executable. Without it there is no privilege boundary to drop to. |

Turning the MMU on also quietly removes the alignment restriction that bites while it is off,
because memory stops being Device memory. Two milestones in one.

Two allocator layers, not one and not three. One structure cannot do both jobs: buddy metadata is
proportional to *all* memory, so tracking 32-byte granularity across 128 mebibytes would cost 4
mebibytes of bookkeeping instead of 32 kibibytes, while slab metadata is proportional only to the
pages currently cut into objects. The layering is what lets the dense bookkeeping exist only where
it is needed.

Linux has a third, memblock, and this project does not, because the reasons memblock exists are
all absent on `virt`: one memory bank rather than several with node affinity, one set of
reservations known at once rather than arriving in stages, no memory hotplug. FRAMES places its
own metadata and the bump allocator becomes ten lines inside its constructor.

## Tier 5 - Civilization

| Skill | What it really is | What it unlocks |
|---|---|---|
| **SWITCH** | Save one context, restore another, return into different code | Two things can take turns. Assembly, unavoidably. |
| **SCHED** | A task table and a timer handler that picks the next one | Preemption: tasks are switched without ever agreeing to it. |
| **WAIT** | A state per task, wait queues, exit, an idle task | Tasks sleep until something happens and end for good. zhemon waits for a key without using the CPU. |
| **MANY HANDS** | All of it together | More than one thing running. The first structure that genuinely deserves the word kernel. |

Tier 5 is complete. MANY CORES and TWO WORLDS used to sit here; they moved to Tier 6, after the
kernel has a shape.

## The final milestone: DOOM

Every tier from here leads to one goal: **unmodified Linux programs, ending with Doom**, built
against musl and running on zheos in the QEMU window. That goal decides three things up front:

- **System calls are Linux's**, numbers and all, from the very first one (TWO WORLDS). Every call
  added is a step towards running real binaries.
- **Processes are made with `fork` and `execve`**, not a home-grown `spawn`, because that is what
  Linux programs, and shells above all, do.
- **Devices are exposed the Linux way**: a terminal with termios, `/dev/fb0` for the screen,
  `/dev/input/event0` for the keyboard. Doom's backend is written against those, so it also runs
  on Linux unchanged.

## Tier 6 - Citizens

*Programs the kernel does not trust.*

| Skill | What it really is | What it unlocks |
|---|---|---|
| **SHAPE** | `arch/`, `drivers/`, `platform/`, `mm/`, `kernel/`, `lib/`, `tty/` | A kernel with a skeleton. Everything after this adds a folder rather than a file next to `main.rs`. Guide: `docs/architecture.md`. |
| **TWO WORLDS** | EL0, `svc`, Linux's syscall ABI | A privilege boundary. The moment this stops being a program and becomes an operating system. |
| **HIGH GROUND** | The kernel moves to the upper half, `TTBR1_EL1` | The whole lower half is free for programs, where Linux binaries expect to be loaded. |
| **REDRAW** | Change a mapping while the MMU is running | Break-before-make and TLB invalidation: tables that change after boot. |
| **ADDRESS SPACES** | A translation table per process, switched with the task | Programs that cannot see each other or the kernel. |
| **LOADER** | Parse a static ELF, build Linux's initial stack | The first unmodified Linux binary runs. |
| **MEMORY MAP** | `brk`, `mmap`, page faults | Programs that allocate. A bad pointer kills the program, not the kernel. |
| **FORK** | `fork`, `execve`, `wait4`, a process tree | Processes that make processes. |
| **MANY CORES** | Wake the other processors with PSCI | Four cores instead of one. Every `unsafe impl Sync` argument has to be true rather than plausible. Best done after LOADER, once the shape of a task has settled. |

## Tier 7 - Archives

*Files, and a real Unix shell.*

| Skill | What it really is | What it unlocks |
|---|---|---|
| **RAMDISK** | A cpio archive loaded next to the kernel | Files that arrive with the kernel, with no disk or network. |
| **FILES** | A VFS and per-process file descriptors | `open`, `read`, `getdents64`, `execve` by path. |
| **PIPES** | A bounded buffer with two ends | `a \| b`. |
| **TERMINAL** | Linux's tty: canonical and raw mode, termios | A shell can edit its own line. |
| **SIGNALS** | Handlers, delivery on the way back to EL0, `rt_sigreturn` | Ctrl-C, and a parent told its child died. |
| **CLOCKS** | `clock_gettime`, `nanosleep`, a timer list | Time for programs, and many sleepers each waking on time. |
| **BUSYBOX** | Static busybox as process 1 | Milestone: a real Unix shell with `ls`, `cat`, `grep` and pipes. |

## Tier 8 - Senses

*Devices beyond the UART.*

| Skill | What it really is | What it unlocks |
|---|---|---|
| **VIRTIO** | The virtio-mmio transport and its rings, first device virtio-rng | Every virtual device QEMU offers: storage, network, input, random numbers. |
| **PIXELS** | QEMU's `ramfb`, configured through `fw_cfg` | A screen. |
| **SCREEN DEVICE** | `/dev/fb0`, its ioctls, `mmap` into a program | Programs draw. |
| **KEYS** | virtio-input as `/dev/input/event0` | Key presses *and* releases, which a game needs and a serial line cannot give. |

## Tier 9 - DOOM

| Skill | What it really is | What it unlocks |
|---|---|---|
| **DOOM** | doomgeneric, static against musl, with a backend on Linux APIs and the shareware WAD in the ramdisk | E1M1, playable. |

## Side quests

Optional branches. None of them are on DOOM's path, all of them teach something the main line
does not.

| Skill | Needs | Why bother |
|---|---|---|
| **TIMEKEEPER** | LANGUAGE | A second device, deliberately easy. Proves the driver pattern transfers to a chip you have never seen, from its manual alone. Also gives CLOCKS a `CLOCK_REALTIME`. |
| **HANDS** | SIGNAL | The PL061 GPIO controller, the closest thing here to Ben's 6522 parallel port. |
| **STORAGE** | VIRTIO | A real disk through virtio-blk. |
| **DISK FS** | STORAGE, FILES | ext2, Linux's own filesystem, mounted into the VFS. |
| **WIRE** | VIRTIO | virtio-net, Ethernet, ARP, IPv4: answer a ping. |
| **SOCKETS** | WIRE, FILES | UDP for programs, sockets as file descriptors. |
| **TCP** | SOCKETS, CLOCKS | Reliable connections: retransmits, connect, listen, accept. |
| **THREADS** | FORK, MEMORY MAP | Tasks that share an address space, and `futex`. |

## Where things stand

Run `bd ready`. That is the answer, always, and it is the only place this is written down.

Want to see the tree as a picture? Generate it from `bd list --json` when you want one. The
dependency graph and the `tier-N` labels are enough to build a mermaid diagram from scratch,
and a generated one is never out of date the way a committed one would be.
