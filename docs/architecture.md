# SHAPE - the kernel gets a skeleton

## 1. What this is and why

Today `src/` is 23 modules side by side under `main.rs`, plus three assembly files. That was the
right call while every skill added one file. It stops being right now, because everything ahead -
programs, files, devices, Doom - adds whole subsystems, and each one needs an obvious place to go.

SHAPE moves the existing code into Linux's top-level layout, small: `arch/`, `drivers/`,
`platform/`, `mm/`, `kernel/`, `lib/`, `tty/`, with `zhemon/` on top. The category is **a choice
we made**: no hardware is involved and no behaviour changes. The proof is that the machine boots
and prints exactly what it printed before.

`docs/diagrams/architecture.tldx.jsx` shows the result: a generic side that knows no hardware, a
hardware-facing side that a port to another board or CPU would replace, and the two ways they talk.

## 2. What you touch: Rust's module system

No registers this time. The tool is the module system, and five of its rules do all the work:

- **A folder is a module.** `src/mm/mod.rs` is the module `mm`. `main.rs` declares `mod mm;`, and
  `mm/mod.rs` declares its own children (`mod frames;`). A child is found next to its parent:
  `mm/frames/mod.rs` or `mm/frames.rs`.
- **A module is private unless declared `pub`.** `mod frames;` inside `mm` means nothing outside
  `mm` can name `mm::frames`. The compiler refuses with error `E0603`, "module is private".
- **`pub use` re-exports.** `mm/mod.rs` can say `pub use frames::Frames;`, and the rest of the
  kernel writes `mm::Frames` without ever seeing `frames`. That pair - private `mod`, public
  `pub use` - is how a folder gets a surface the compiler enforces.
- **A child sees its parent's private items.** `kernel/sched/wait.rs`, a child of `sched`, can
  touch the private `SCHED` static in `kernel/sched/mod.rs`. That is why the wait queue can move
  out of `sched.rs` without making the scheduler's internals public.
- **Paths in macros are relative to the file they sit in.** `global_asm!(include_str!("boot.s"))`
  looks for `boot.s` next to the `.rs` file containing it. `#[macro_export]` macros always land at
  the crate root (`crate::println!`), wherever they are defined, and must name what they call with
  a full `$crate::...` path, so `print.rs`'s `$crate::uart::uart()` changes when the UART moves.

## 3. Reading the names

- **arch** - **arch**itecture, meaning the CPU's instruction set family: aarch64, x86-64, RISC-V.
  Everything that would change if the CPU changed.
- **mm** - **m**emory **m**anagement. Linux's name for the folder with the allocators and page tables.
- **tty** - **T**ele**TY**pewriter. Unix's name for a terminal, from the printing terminals the
  first Unix ran on. It is where keyboard input, line editing and console output live.
- **lib** - **lib**rary: small helpers with no opinion about hardware or about the kernel.
- **HAL** - **H**ardware **A**bstraction **L**ayer: a set of traits that hides which hardware is
  underneath. Not built here; section 4 says why.
- **PL011** is ARM's part number for its UART, from its PrimeCell peripheral family. ARM does not
  expand it further. **GIC** - **G**eneric **I**nterrupt **C**ontroller. **DTB** - **D**evice
  **T**ree **B**lob. **PSCI** - **P**ower **S**tate **C**oordination **I**nterface.
- **`cfg`** - **c**on**f**i**g**uration: `#[cfg(target_arch = "aarch64")]` compiles an item only
  when building for that CPU.

## 4. The three rules

The folders matter less than these. Each one can be checked by a command or by the compiler.

**Rule 1 - only `arch/` and `drivers/` know hardware.** Outside them, no file contains `asm!`,
`global_asm!`, a system register name, or a device register offset. Generic code that needs the CPU
calls `arch::...`; code that needs a device calls its driver. This is the line a port would follow:
a Raspberry Pi port adds to `drivers/` and `platform/`, an x86-64 port adds `arch/x86_64/`, and
`mm/`, `kernel/`, `tty/` and `zhemon/` move over unchanged.

`platform/` sits on the hardware side but holds no instructions: it is *what is where* - the device
tree parser and `Board`, which turns the tree into addresses and interrupt numbers.

**Rule 2 - each folder has one door.** A folder's `mod.rs` declares its files private and `pub use`s
what the rest of the kernel may touch. `kernel/` uses `mm::Heap`, never `mm::frames::lists`. With
the files private, breaking this is a compile error rather than a code review comment.

**Rule 3 - `kmain` reads as the boot order.** One line per step, in the order the machine needs
them. A subsystem prints its own report from its own `init`; `kmain` does not reach into it to do
the printing.

**What is deliberately not here: traits.** With one CPU and one board, a `trait Cpu` would have
exactly one implementation and add only indirection. The folder boundary *is* the abstraction for
now. The day a second architecture exists, its differences decide what the traits look like - a
guess made today would be wrong in ways nobody can see yet.

## 5. Calls across the line: down through doors, up through entry points

The old sketch of this diagram said dependencies point down only. A kernel cannot keep that rule,
and Linux does not try: the hardware side has to call *up* whenever the hardware starts something.
An interrupt arrives in `vectors.s`, and something generic has to handle it.

So the rule is about *how* the hardware side calls up: only through a short list of **entry
points**, functions the generic side provides for exactly that purpose. Today there are five:

| entry point | lives in | called by | why |
| --- | --- | --- | --- |
| `kmain` | `main.rs` | `arch/aarch64/boot.s` | boot is over, Rust takes over |
| `handle_interrupt` | `kernel/irq.rs` | `arch/aarch64/vectors.s` | an IRQ arrived |
| `handle_exception` | `arch/aarch64/exception.rs` | `vectors.s` | a fault; stays in `arch`, because decoding ESR is ARM knowledge |
| `task_exit` | `kernel/sched` | `arch/aarch64/switch.s` (the trampoline) | a task's function returned |
| `push_character` | `tty/input.rs` | `drivers/pl011.rs` | a byte came off the wire; Linux's serial drivers push into the tty layer the same way |

Two calls cross the line in the wrong direction and stay, on purpose:

- **Fault and panic reports write straight to the UART.** They run when the kernel may be broken,
  so they take no lock and go through no layer that could be the thing that broke.
- **`kernel/irq.rs` talks to `drivers/gic.rs` directly.** There is one interrupt controller. Linux
  has a registration interface (`irq_chip`) because it has hundreds; ours waits for a second one.

## 6. The split that is not just a move: `context.rs`, `timer.rs`, `cpu.rs`

Most of SHAPE is moving files and fixing `use` lines. Three files hold two things each and have to
be cut:

**`context.rs`** is half ARM and half kernel. `SwitchFrame` (which registers `switch.s` saves, in
which order), `Context`, `switch`, `trampoline`, and the code in `Task::new` that writes a new
task's first frame are all ARM. `Task`, `State` and `ContextStack` are not. The cut: `arch` offers
one way to make the context of a task that has never run - given the top of its stack and its entry
function, it writes the frame and returns the `Context` - and `kernel/task.rs` calls it. After the
cut, `Task::new` never mentions `x19` or `x30`.

**`timer.rs`** mixes the generic timer's registers with the kernel's idea of time. The registers
(`CNTFRQ_EL0`, `CNTPCT_EL0`, `CNTP_CVAL_EL0`, `CNTP_TVAL_EL0`, `CNTP_CTL_EL0`) and "arm the next
deadline" go to `arch/aarch64/timer.rs`. `HZ`, the tick count, `sleep`, and the interrupt handler
itself go to `kernel/time.rs`; the handler asks `arch` to re-arm, then counts the tick and calls
`sched::tick`. That way `arch` never calls up for the timer at all.

**`cpu.rs`** is three modules already: the generic part (`wfi`, the stack pointer, masking
interrupts) becomes `arch/aarch64/cpu.rs`; `cpu::generic_timer` goes to `arch/aarch64/timer.rs`;
`cpu::mmu` goes to `arch/aarch64/mmu/` next to the code that writes those registers.

One small fix rides along: `lib/lock.rs` gives up by calling `println!` and `crate::halt`, which
reaches from `lib` up into the kernel. Make it `panic!` instead. The panic handler already prints
and parks, and it writes to the UART without a lock, so a stuck lock cannot stop its own report.

## 7. The map

Every file, and where it goes.

| today | goes to |
| --- | --- |
| `kernel.s` | `arch/aarch64/boot.s` (its `global_asm!` moves from `main.rs` to `arch/aarch64/mod.rs`) |
| `vectors.s`, `exception.rs` | `arch/aarch64/` |
| `switch.s`, the ARM half of `context.rs` | `arch/aarch64/switch.s`, `arch/aarch64/context.rs` |
| `cpu.rs` | split three ways, section 6 |
| `mmu/` | `arch/aarch64/mmu/`, plus the register accessors from `cpu::mmu` |
| `psci.rs` | `arch/aarch64/psci.rs` - it is an `hvc` instruction |
| `uart.rs` | `drivers/pl011.rs` |
| `gic.rs` | `drivers/gic.rs` |
| `dtb/`, `board.rs` | `platform/` |
| `memory/` | `mm/`: `region.rs`, `pfn.rs`, `pages.rs`, `gaps.rs`, `map.rs`, and `image.rs` for the linker symbols in `memory/mod.rs` |
| `frames/`, `slab/`, `heap.rs` | `mm/` |
| the kernel half of `context.rs` | `kernel/task.rs` |
| `sched.rs` | `kernel/sched/mod.rs`, with `WaitQueue` in `kernel/sched/wait.rs` |
| `irq.rs` | `kernel/irq.rs` |
| the generic half of `timer.rs` | `kernel/time.rs` |
| `halt`, `shutdown` from `main.rs` | `kernel/power.rs` |
| the panic handler from `main.rs` | `kernel/panic.rs` |
| `lock.rs`, `ring_buffer.rs`, `mmio.rs` | `lib/` |
| `input.rs`, `console.rs`, `print.rs` | `tty/` |
| `zhemon.rs`, `zhemon/` | `zhemon/mod.rs` and its two files |
| `print_letters` from `main.rs` | `demo.rs`, until the first user program replaces it |

`arch/mod.rs` holds two lines: the `aarch64` module, and a `pub use` of everything in it, both
under `#[cfg(target_arch = "aarch64")]`. Generic code writes `arch::cpu::wait_for_interrupt()` and
never says `aarch64`. A second architecture would be two more lines.

`mmio.rs` lands in `lib/`, not `drivers/`, because it is only volatile loads and stores, and
zhemon uses it to examine raw memory - which is the monitor's job, not a driver's.

## 8. Where everything ahead goes

So each later skill adds a folder or a file rather than a new top-level module:

| skill | lands in |
| --- | --- |
| TWO WORLDS | `arch/aarch64/` for the drop to EL0 and the `svc` entry; `kernel/syscall.rs` for the table |
| HIGH GROUND, REDRAW | `arch/aarch64/mmu/` |
| ADDRESS SPACES, MEMORY MAP | `mm/` for the address space and `brk`/`mmap`; `arch/aarch64/mmu/` for the tables and ASIDs |
| LOADER | `fs/binfmt_elf.rs` - Linux keeps program loaders with the filesystem code |
| FORK | `kernel/fork.rs`, `kernel/exit.rs` |
| MANY CORES | `arch/aarch64/smp.rs`, and per-core state in `kernel/` |
| RAMDISK, FILES, PIPES | `fs/` |
| TERMINAL | `tty/` |
| SIGNALS | `kernel/signal.rs`, with the signal frame in `arch/aarch64/signal.rs` |
| CLOCKS | `kernel/time.rs` |
| VIRTIO, KEYS | `drivers/virtio/` |
| PIXELS, SCREEN DEVICE | `drivers/firmware/fw_cfg.rs`, `drivers/video/` |
| STORAGE, DISK FS | `drivers/virtio/`, `block/`, `fs/ext2/` |
| WIRE, SOCKETS, TCP | `drivers/virtio/` for the card, `net/` for the protocols |

`net/` and `fs/` follow rule 1 the same way `kernel/` does: a TCP state machine or an ext2 inode
knows nothing about hardware. Only the virtio driver underneath does.

## 9. What you are building

In this order. Each step ends the same way: `make lint` is clean, the machine boots, and the
transcript matches (section 10). Stop at any step and the tree still works.

**Step 0 - the baseline.** Before moving anything, take the transcript you will compare against:

```sh
make feed INPUT='ps\rexit' > /tmp/before.txt
```

**Step 1 - `lib/`.** Move `lock.rs`, `ring_buffer.rs` and `mmio.rs`. This is the small one, for
learning the mechanics: a `lib/mod.rs`, the `mod` lines out of `main.rs`, every `crate::lock` path
updated. Then the `panic!` fix in the lock (section 6).
*Check:* the transcript matches.

**Step 2 - `arch/aarch64/`, the plain moves.** `boot.s`, `vectors.s` with `exception.rs`,
`psci.rs`, and `cpu.rs` whole - its `generic_timer` and `mmu` modules ride along for one step.
`main.rs` loses its `global_asm!`.
*Check:* the transcript matches, and `grep -rlE 'asm!' src | grep -v '^src/arch/'` lists only
`src/context.rs`, the one file left to cut.

**Step 3 - the MMU and the timer.** Split `cpu.rs` three ways and cut `timer.rs` in two
(section 6); `mmu/` joins its register accessors in `arch/aarch64/mmu/`.
*Check:* the transcript matches, and `grep -rnE 'CNT[A-Z_]*_EL0|_EL1' src | grep -v '^src/arch/'`
finds no system register outside `arch/`. One hit is expected: a doc comment in `memory/mod.rs`
that says the vector table is what `VBAR_EL1` points at. Naming a register in prose is not using it.

**Step 4 - `kernel/`.** Cut `context.rs` (section 6). Move `sched.rs` into `kernel/sched/`, with
the wait queue in its own `wait.rs`. Add `kernel/irq.rs`, `kernel/power.rs`, `kernel/panic.rs`.
*Check:* the transcript matches, and the grep from step 2 prints nothing.

**Step 5 - the rest.** `drivers/`, `platform/`, `mm/`, `tty/`, `zhemon/`, `demo.rs`. Mechanical:
moves and `use` lines, and `print.rs`'s `$crate` path.
*Check:* the transcript matches, and `ls src` shows `main.rs`, `demo.rs` and folders, nothing else.

**Step 6 - `kmain`.** Make it the boot order (rule 3). The memory printing moves into the `init`
of the subsystem that owns it, the kernel mapping plan into `arch::mmu`, and the demo spawns into
`demo.rs`. Deleting a debug print you no longer want is allowed - it shows in the diff as a deletion
and nothing else.
*Check:* the transcript matches apart from deliberate deletions, and `kmain` fits on one screen.

**Step 7 - close the doors.** In every folder's `mod.rs`, children become private `mod` and the
surface becomes `pub use`. Fix what the compiler names.
*Check:* add a line anywhere outside `mm/` that names `crate::mm::frames` directly. The build fails
with `E0603`. Remove the line.

## 10. How you will know it worked

**The transcript.** After every step:

```sh
make feed INPUT='ps\rexit' > /tmp/after.txt
norm() { grep -v -e '^\\[AB]' -e '^cargo' -e 'Compiling' -e 'Finished' "$1" | sed -E 's/[0-9a-fA-F]+/N/g'; }
diff <(norm /tmp/before.txt) <(norm /tmp/after.txt) && echo SAME
```

`norm` drops the cargo lines and the A/B letters line, whose interleaving changes on every boot, and
turns every number into `N`. Moving code changes sizes and addresses - the `text:` size, the page
table's address, a tick count - but never which lines print or in what order. Two boots of the same
tree, before any move, already give `SAME`; a moved tree must too.

**The greps.** At the end:

```sh
grep -rlE 'asm!' src | grep -v '^src/arch/'          # nothing
find src -name '*.s' -not -path 'src/arch/*'           # nothing
ls src                                                 # main.rs, demo.rs, folders
```

**The compiler.** Step 7's `E0603` is rule 2 enforced by `rustc` rather than by memory.

| symptom | almost certainly |
| --- | --- |
| `error: couldn't read src/.../kernel.s` | `include_str!` resolves next to the file that calls it; the `.s` moved and the macro did not, or the other way round |
| an error inside a `println!` expansion that names `uart` | `print.rs` still says `$crate::uart::uart()`; `#[macro_export]` macros expand where they are used, so the path must be the full new one |
| `undefined symbol: task_exit` or `handle_interrupt` at link time | an entry point lost its `#[unsafe(no_mangle)]`, or its module stopped being compiled because a `mod` line went missing |
| boots, then `PC=0x200` in `make regs` | `install_vectors` no longer runs first - `kmain` was reordered past it |
| transcript differs by a whole line | a print moved into an `init` that now runs in a different order; rule 3 is about keeping that order |

---

## Optional reading

- The Rust Reference, "Modules" and "Visibility and privacy" - the exact rules section 2 uses.
- Linux's top-level `arch/`, `drivers/`, `mm/`, `kernel/`, `fs/`, `net/`, `lib/` directories:
  any file in this guide has a counterpart there worth comparing against.
- Linux `arch/arm64/kernel/entry.S`: the real version of section 5's entry points.
