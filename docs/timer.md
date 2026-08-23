# TIMER - the machine's own heartbeat

The GIC made it possible for something to speak first. The UART used that to speak when *you*
typed. This is the first thing in the machine that speaks when nobody did anything at all.

Everything above Tier 3 needs it. A scheduler is "swap tasks on a tick". A timeout is "give up
after N ticks". `sleep` is "stop the CPU until the tick I care about". None of them exist without
a heartbeat, and the heartbeat is one small device that is not on the bus at all.

Sections 1 and 3-5 are hardware and will not change. Section 6 is the design decision - what to
actually build. Sections 7-9 are how to build it, 10-11 are how to prove it and what the failures
look like.

---

## 1. Vocabulary

**Counter** - a number that goes up on its own, forever, at a fixed rate. Nothing starts it and
nothing can stop it. Reading it tells you *how much time has passed*. This is a clock in the
"stopwatch" sense, not the "what time is it in London" sense.

**Comparator** - a register holding a number, and a piece of logic that shouts when the counter
reaches it. This is the alarm. Set it, and when the counter gets there the timer raises an
interrupt.

**Tick** - one firing of that alarm, plus the handler re-arming it for the next one. Two different
things get called ticks and it will confuse you: the hardware counter increments 62.5 million times
a second, and your kernel's heartbeat fires maybe 100 times a second. Both are "ticks". Pick
different names in the code and stay strict about it.

**Frequency** - how many counter increments happen per second. On this machine 62,500,000. On real
hardware it is usually different. You read it, you never hardcode it.

**Deadline** - a value of the counter in the future. "At counter value 42,000,000, wake me."

**Monotonic** - only ever goes up, never jumps back, never gets adjusted. The generic timer counter
is monotonic. That is why it is the right thing to measure durations with and the wrong thing to
print as a date.

**Drift** - the gap between where a repeating alarm *should* land and where it actually lands,
accumulated over many repetitions. Section 9. It is the one real engineering decision in this task.

**PPI** - Private Peripheral Interrupt, from GIC.md section 5. An interrupt from a device that
exists once per core rather than once per machine. The timer is inside the CPU, so each core has
its own, so it is a PPI.

---

## 2. Where you are now

Four of the five GIC gates are already open and the fifth is `irq::unmask()` at the end of `kmain`.
The pieces you will reuse, unchanged:

**`irq::register(intid, handler)`** fills a handler slot and calls `gic::enable(intid)`. It already
does the right thing for a PPI: `gic::enable` skips the `ITARGETSR` write for anything below INTID
32, because SGIs and PPIs are always delivered to the local core. You do not need to touch
`src/gic.rs` at all for this task.

**`irq::handle_interrupt`** acknowledges, dispatches by INTID, and calls `interrupt.end()`
unconditionally. Your handler does not do the EOI - it is done for you, after your handler returns.

**`src/vectors.s`** routes the IRQ slot at `0x280` into that. Nothing to add.

**`src/board.rs`** holds the machine's facts. `TIMER_INTID` belongs there next to `UART_INTID`.

**`src/cpu.rs`** already has the exact pattern for reading and writing a system register with
`asm!` - `read_daif` at `src/cpu.rs:29` is `mrs`, `restore_daif` at `src/cpu.rs:46` is `msr`. Every
timer register is reached the same way. Copy that shape, including the note about why `nomem` is
absent.

The one thing that will change underneath you: `input::getc` parks in `wfi` waiting for a key.
Once the timer is live, that `wfi` wakes 100 times a second whether you typed or not. The loop
re-checks the buffer, finds nothing, and parks again - so nothing breaks, but the CPU is no longer
truly idle between keystrokes. That is normal and every real kernel does it.

---

## 3. The two clocks, and why this device is different

Every device so far lived at an address. The UART is at `0x0900_0000`; you store a byte there and a
character appears. The GIC is at `0x0800_0000`. You found both in `virt.dts` under `reg = <...>`.

Look at the timer's node:

```
timer {
	interrupts = <0x01 0x0d 0x104   0x01 0x0e 0x104   0x01 0x0b 0x104   0x01 0x0a 0x104>;
	always-on;
	compatible = "arm,armv8-timer", "arm,armv7-timer";
};
```

**There is no `reg`.** No address. It is not on the bus, it is not memory-mapped, and `mem::read_32`
cannot reach it. The generic timer is part of the CPU core itself, and you talk to it with **system
registers** - the `mrs` / `msr` instructions, the same way you read `DAIF` or write `VBAR_EL1`.

That is the whole reason this task feels different from the UART. There is no base address to get
wrong, no `info mtree` to check, and no `make mem` to peek with. The state is inside the core.

Inside that core-local timer there are two independent halves, and conflating them is the most
common way to get confused here:

| | What it is | Register | Needs an interrupt? |
|---|---|---|---|
| **Counter** | Free-running, 64-bit, always counting, cannot be stopped or written | `CNTPCT_EL0` | No |
| **Comparator** | One deadline, plus enable/mask bits, raises an IRQ when reached | `CNTP_CVAL_EL0`, `CNTP_CTL_EL0` | Yes |

The counter alone gives you `delay()` and "how long did that take", with no GIC involvement
whatsoever - read it, subtract, spin. The comparator is what gives you a *heartbeat*, and it is the
only half that needs everything from the GIC task.

The counter is running right now, in the kernel you already have. It has been running since the
machine powered on.

---

## 4. The registers

Six of them matter. All are read or written with `mrs` / `msr`.

| Register | Width | Access | What it is |
|---|---|---|---|
| `CNTFRQ_EL0` | 32 | read | Counter increments per second. **62,500,000 on this machine.** |
| `CNTPCT_EL0` | 64 | read only | The counter. Cannot be written or reset, by design. |
| `CNTP_CVAL_EL0` | 64 | read/write | **C**ompare **VAL**ue. The deadline, as an absolute counter value. |
| `CNTP_TVAL_EL0` | 32 signed | read/write | **T**imer **VAL**ue. A convenience view of the same deadline, as a countdown. |
| `CNTP_CTL_EL0` | 32 | read/write | Three bits. Section 4.3. |
| `CNTKCTL_EL1` | 32 | read/write | Whether EL0 may touch any of the above. You run at EL1, so ignore it. |

The `P` in `CNTP_*` is **physical**. There is also a virtual timer, `CNTV_*`, with the identical
layout. It exists so a hypervisor can hand a guest a timer whose counter it can offset. You have no
hypervisor. Use the physical one, and mentally delete every `CNTV_*` and `CNTHP_*` you see in the
manual.

### 4.1 `CNTFRQ_EL0` - read it, never assume it

Two traps here, and both have bitten people in this exact repo's neighbourhood:

**It is not the UART's clock.** `virt.dts` gives `apb-pclk` a `clock-frequency` of `0x16e3600` =
24 MHz, and `src/uart.rs:102` correctly uses that as `UARTCLK`. The timer runs at 62.5 MHz. They
are unrelated clocks on unrelated devices. Reusing the UART's 24 MHz constant here gives you a tick
2.6 times too fast, and it will look almost plausible.

**It is not architecturally guaranteed.** `CNTFRQ_EL0` is a *software-written* register on real
hardware - firmware is supposed to fill it in before Linux boots, and firmware that forgets leaves
it zero. QEMU fills it in honestly. Read it once at init, and assert it is non-zero; a divide by a
zero frequency is a much worse first symptom than a clear panic.

Verified value, read out of a running QEMU over the gdb stub:

```
CNTFRQ_EL0 = 0x0000000003b9aca0     = 62,500,000
```

### 4.2 `CVAL` and `TVAL` are one deadline with two views

This is the part of the manual that reads like a riddle. It is simpler than it sounds.

There is exactly **one** deadline, stored as a 64-bit absolute counter value in `CVAL`. `TVAL` is
not a second register with its own storage - it is arithmetic performed on your behalf:

```
writing TVAL = n     is defined as    CVAL := CNTPCT_EL0 + n     ("wake me n ticks from now")
reading TVAL         is defined as    CVAL - CNTPCT_EL0          ("how many ticks left", may be negative)
```

So `TVAL` is the countdown view and `CVAL` is the alarm-clock view. Writing either one arms the
timer.

`TVAL` is 32-bit **signed**, which caps a single `TVAL`-armed interval at 2^31-1 counter ticks =
about 34 seconds on this machine. Once the deadline passes, `TVAL` keeps counting down through
zero into negative numbers, which is how you can tell how *late* you were.

Start with `TVAL`. It is the easier one to reason about. Section 9 is about when to graduate to
`CVAL`.

### 4.3 `CNTP_CTL_EL0` - the three bits

| Bit | Name | Direction | Meaning |
|---|---|---|---|
| 0 | `ENABLE` | you write | 0 = timer off entirely. 1 = comparing. |
| 1 | `IMASK` | you write | 1 = **mask** the output. Condition still tracked, no interrupt raised. |
| 2 | `ISTATUS` | read only | 1 = the deadline has passed *right now*. |

The interrupt line is asserted when, and only when:

```
ISTATUS == 1  &&  ENABLE == 1  &&  IMASK == 0
```

So `ENABLE = 1, IMASK = 0` is the value you write - numerically `1`. Note that `IMASK` is
inverted relative to how you would name it: you write **0** to let interrupts through.

`ISTATUS` is the useful debugging bit. It is live, computed from `CNTPCT >= CVAL` continuously, and
it is the difference between "my timer never fired" and "my timer fired and the GIC ate it".

### 4.4 The one thing that makes this device dangerous

**`ISTATUS` is a level, not an event.** The timer output is a level-sensitive interrupt (GIC.md
section 1): the timer holds the wire high for as long as the deadline is in the past, and the only
way to lower it is to move the deadline into the future, or to set `IMASK`.

The GIC does not clear it for you. `interrupt.end()` does not clear it. Acknowledging the interrupt
does not clear it.

If your handler does not re-arm, then the instant `irq::handle_interrupt` returns and `eret` runs,
the wire is *still high*, the GIC re-pends the same interrupt, and you take it again. Forever. Your
tick counter races upward at millions per second and the main loop never executes another
instruction. It looks exactly like a hang.

That is the failure mode of this task. Everything else is a typo.

---

## 5. The interrupt number, and the gates

From `virt.dts`, the timer node lists four interrupts. In device tree encoding, kind `1` = PPI, and
INTID = number + 16:

| dts entry | Which timer | INTID |
|---|---|---|
| `<1 13 0x104>` | Secure physical (EL3) | 29 |
| `<1 14 0x104>` | **Physical, EL1 - yours** | **30** |
| `<1 11 0x104>` | Virtual | 27 |
| `<1 10 0x104>` | Hypervisor physical (EL2) | 26 |

**INTID 30.** Put it in `src/board.rs`.

The five gates from GIC.md, restated for this device. Four of them you already know how to open:

1. **The device's own enable** - `CNTP_CTL_EL0` bits `ENABLE=1, IMASK=0`. New, and yours.
2. **Distributor: INTID 30 enabled** - `irq::register(30, ...)` does this via `gic::enable`.
3. **Distributor: routing** - not applicable. PPIs are always local; `gic::enable` correctly skips
   `ITARGETSR` below 32.
4. **CPU interface enabled and priority passes** - already done by `gic::init`.
5. **`PSTATE.I` clear** - already done by `irq::unmask()`.

Plus one that the UART did not have: **a deadline must actually be set.** A timer with `ENABLE=1`
and `CVAL` still at its reset value of 0 has a deadline infinitely in the past, so it fires
immediately and permanently. Arm first, enable second.

---

## 6. What to build

This is the design question. The hardware gives you two capabilities - "how much time has passed"
and "wake me later" - and a scheduler five tiers away will want a lot more than that. Build the two,
not the five.

Three stages. Stage 1 is the task. Stage 2 is worth doing while the manual is open. Stage 3 is a
trap.

### Stage 1 - the heartbeat

```rust
// src/timer.rs
pub fn init(hz: u32)          // read CNTFRQ, compute the interval, register INTID 30,
                              // arm, enable. Call before irq::unmask().
pub fn ticks() -> u64         // heartbeats since init. Bumped by the handler, nothing else.
```

plus a private `fn handle_interrupt(intid: u32)` matching the `fn(u32)` signature `irq::register`
wants, exactly like `uart::handle_interrupt` at `src/uart.rs:242`.

That is the whole acceptance criterion: a counter that only the handler touches, visibly rising.
`hz = 100` is the conventional starting point - fast enough that 10 ms of latency is invisible to a
human, slow enough that the handler's cost is noise. Old Unix used exactly 100. Do not go to 1000
until something needs it; at 1 kHz you are paying for an exception entry every millisecond forever.

**Storage for the counter.** `AtomicU64` with `Ordering::Relaxed`, a plain `static`. Not the
`UnsafeCell` + `unsafe impl Sync` dance that `HandlerTable` and `InputBuffer` needed - those exist
because a ring buffer has four fields that must move together. One counter incremented in one place
does not. `main.rs` already uses `AtomicBool` this way for `ALREADY_PANICKED`.

Relaxed is genuinely enough: you need the value to be a single indivisible 64-bit access and you
need the compiler to actually re-read it each time round a loop. Relaxed gives you both. It does
not give you ordering against *other* variables, and you do not have any yet.

### Stage 2 - the counter half

```rust
pub fn now_us() -> u64        // CNTPCT_EL0 converted to microseconds
pub fn delay_us(us: u64)      // busy-wait on now_us. No interrupts involved.
pub fn sleep_ms(ms: u64)      // wfi until ticks() reaches the deadline
```

`now_us` and `delay_us` are three lines each and do not use the comparator, the GIC, or the handler
at all. They are worth building now for two reasons: they let you *measure* whether the heartbeat is
actually running at the rate you asked for, and `delay_us` is the only timing primitive that works
before interrupts are up - which is exactly what a device driver's reset sequence needs. Every
hardware datasheet you read from here on will contain a sentence like "wait 5 µs after deasserting
reset".

`sleep_ms` is the one that shows off the tick. Compute a target tick count, then loop: `wfi`, check
`ticks()`, repeat. The `wfi` is the point - the CPU is genuinely stopped between heartbeats instead
of spinning, and the timer is what wakes it. Note the resolution: `sleep_ms` can only be as precise
as your tick, so at 100 Hz `sleep_ms(1)` sleeps up to 10 ms. That is not a bug to fix, it is what
a tick-based sleep is. Say so in one comment and move on.

**Unit conversion, and the overflow in it.** The obvious `counter * 1_000_000 / freq` overflows u64
after about 84 hours of uptime, which is exactly the kind of bug that shows up once, in production,
at 3am, and never reproduces. Split the division instead:

```
whole     = counter / freq          // seconds
remainder = counter % freq          // leftover ticks, always < freq
us        = whole * 1_000_000 + remainder * 1_000_000 / freq
```

No overflow until the counter itself wraps, which at 62.5 MHz is about 9,300 years.

**One instruction you will not guess.** Put an `isb` immediately before reading `CNTPCT_EL0`. The
counter read is allowed to be reordered relative to surrounding instructions, so without the
barrier a tight `delay_us` loop can read a value the core fetched earlier. `isb` costs nothing here
and is what the ARM manual's own example does.

### Stage 3 - what not to build

Do not build any of this yet:

- **A callback registry / timer wheel / list of pending timers.** One tick and one counter covers
  every use you have. Sorted timeout lists arrive with SCHED, when there is something to schedule.
- **One-shot timers, `set_timeout(ms, callback)`.** Same reason. Speculative.
- **Wall-clock time, dates, a `struct Time`.** That is the PL031 RTC - a completely different
  device, already filed as `zheos-kho`. This device cannot tell you the date and never could.
- **Calibrating the tick against anything.** `CNTFRQ_EL0` is the calibration.
- **A generic `Clock` trait with the physical and virtual timers behind it.** One implementation,
  one caller.

Roughly 60 lines of Rust for stages 1 and 2 together, of which about 15 are `asm!` wrappers.

---

## 7. Bring-up order

`kmain` currently reads:

```
uart().init()
exception::install_vectors()
gic::init()
irq::register(UART_INTID, uart::handle_interrupt)
uart().enable_interrupt()
irq::unmask()
```

The timer slots in as one more registration, and the same rule applies as for the UART: everything
that can produce an interrupt is registered **before** `irq::unmask()`, so no interrupt can arrive
before there is somewhere for it to go.

Inside `timer::init`, the order also matters:

```
1. read CNTFRQ_EL0, assert it is non-zero
2. compute interval = freq / hz, assert it is non-zero and fits in 31 bits
3. irq::register(TIMER_INTID, handle_interrupt)     GIC side ready first
4. write CNTP_TVAL_EL0 = interval                   arm: set a deadline in the future
5. write CNTP_CTL_EL0 = ENABLE                      then, and only then, enable
```

Steps 4 and 5 in that order because of the reset value: `CVAL` is 0, meaning a deadline in the
distant past, so enabling before arming asserts the interrupt line immediately. Harmless in
practice, but it means your first tick is not one interval long, and "the first measurement is
always wrong" is a horrible property to debug later.

Registering before arming (3 before 4) is the same discipline as the UART: the handler exists
before the source can fire.

The handler itself is two operations, in this order:

```
1. re-arm     write CNTP_TVAL_EL0 = interval    lowers the interrupt line
2. count      TICKS.fetch_add(1, Relaxed)
```

Re-arm first. `irq::handle_interrupt` will EOI the moment your handler returns, and the line must
already be low by then. Nothing else belongs in there - no printing, no work. A handler that takes
longer than one interval is a machine that does nothing but handle timer interrupts.

`interval` needs to be reachable from both `init` and the handler. A `static AtomicU64` written once
in `init` is fine and needs no unsafe. A `const` computed from a hardcoded frequency is not fine -
that is trap 4.1 again.

---

## 8. The life of one tick

Worth having straight before you debug it.

1. `CNTPCT_EL0` increments. It has been doing this since power-on and never stops.
2. It reaches `CVAL`. The timer sets `ISTATUS = 1`.
3. `ENABLE` is 1 and `IMASK` is 0, so the timer raises its output line, and **holds it**.
4. The GIC distributor sees PPI 30 asserted, and it is enabled, so it marks it pending.
5. The CPU interface compares its priority against the running priority and the PMR, passes, and
   signals IRQ to the core.
6. `PSTATE.I` is clear, so the core takes the exception: masks IRQ, saves the return address, jumps
   to `VBAR_EL1 + 0x280`.
7. `irq_entry` in `vectors.s` saves x0-x30 and calls `handle_interrupt`.
8. `gic::acknowledge` reads `IAR`, gets 30, marks it active.
9. Your handler runs. It writes `CNTP_TVAL_EL0`, which sets `CVAL = CNTPCT + interval`, a value in
   the future, so `ISTATUS` drops to 0 and **the line goes low**. Then it bumps the counter.
10. `interrupt.end()` writes `EOIR`, deactivating it. The running priority drops.
11. `eret`. `PSTATE.I` is restored to clear, and the interrupted code resumes on the exact
    instruction it was on.

Step 9 is the one that is unique to this device. Delete it and the loop 3→10→3 runs at the speed of
exception entry.

---

## 9. Drift, and the `TVAL` → `CVAL` upgrade

Both of these arm the timer. They are not equivalent:

```
TVAL = interval           CVAL := CNTPCT_EL0 + interval      relative to now
CVAL = CVAL + interval                                       relative to the last deadline
```

`TVAL` measures from the moment the handler executes, which is some microseconds after the deadline
actually passed - exception entry, register saves, the GIC read. Call that latency L. Each tick you
lose L, and the loss accumulates: after an hour at 100 Hz that is 360,000 × L. If L is 2 µs your
clock is 0.7 seconds slow per hour, and it never catches up because every interval starts late.
Worse, L is not constant - it depends on what the interrupted code was doing.

`CVAL += interval` measures from the previous *deadline*, not from now, so the latency of one tick
does not shift the next one. Jitter stays, drift disappears. It is one extra read and an add.

Build it with `TVAL` first, because a wrong `TVAL` is easier to see than a wrong `CVAL` and you want
the thing ticking before you optimise it. Then switch. The switch is worth doing in this project
specifically because you can *measure* it: with stage 2 built, print `now_us()` and `ticks()`
together after a few minutes and compare `ticks * 10_000` against `now_us`. `TVAL` diverges. `CVAL`
does not.

One caveat that arrives with `CVAL`: if the machine is ever stopped longer than one interval - a
debugger breakpoint, a very long handler - the new deadline can land in the past, and the timer
fires again immediately, repeatedly, until it catches up. Linux handles this by looping the add
until the deadline is genuinely in the future. You do not need that yet; know it exists so it does
not surprise you at a breakpoint.

---

## 10. Proving it works

**The counter is running before you write a line of driver.** `CNTPCT_EL0` has been incrementing
since power-on in every kernel you have already built. The first thing to write is `now_us`, print
it twice a second apart by hand, and confirm it moved by roughly a million. That validates your
`mrs` wrapper and your unit conversion with the comparator, the GIC and the handler all still out
of the picture. Debug one thing at a time.

**Look at the timer from outside with lldb.** QEMU's gdb stub exposes the timer system registers,
which `make regs` cannot show you. This is the highest-value debugging tool for this task:

```sh
make debug                                  # in one terminal, frozen at instruction 0
lldb kernel.elf                             # in another
(lldb) gdb-remote localhost:1234
(lldb) register read CNTFRQ_EL0 CNTP_CTL_EL0 CNTP_CVAL_EL0
```

At reset you will see exactly this - confirmed on this machine:

```
CNTFRQ_EL0    = 0x0000000003b9aca0     62,500,000
CNTP_CTL_EL0  = 0x0000000000000000     disabled, unmasked, deadline not reached
CNTP_CVAL_EL0 = 0x0000000000000000     no deadline set
```

Let it run, interrupt it, and read again. After `init` you want `CNTP_CTL_EL0 = 1`
(`ENABLE=1, IMASK=0, ISTATUS=0`) and a large non-zero `CVAL`. If you catch `CNTP_CTL_EL0 = 5`
repeatedly, that is `ISTATUS=1` stuck high - the storm from section 4.4, and you now have proof
rather than a guess. `CNTP_TVAL_EL0` and `CNTPCT_EL0` are not exposed by the stub; read `CVAL`
instead.

**Rate check by hand.** Print `ticks()` from the monitor, wait a measured 10 seconds, print it
again. At 100 Hz the difference should be about 1000. If it is about 2600, you used 24 MHz. If it is
about 385, you divided the wrong way round.

**Rate check against the other clock.** Once stage 2 exists, this is the honest test:
`ticks() * (1_000_000 / hz)` against `now_us()`. Two independent halves of the same device, one
interrupt-driven and one not, should agree. They will not agree exactly - see drift - and the size
of the disagreement is the measurement, not the failure.

**Automate it.** `make feed INPUT='...'` already pipes scripted keystrokes and captures output. A
zhemon command that prints ticks, run twice, is enough to catch a regression later without a human
watching.

**Break it on purpose.** Three deliberate breakages, each of which teaches one thing:

| Break | What you should see |
|---|---|
| Comment out the re-arm in the handler | Machine goes deaf. Typing does nothing. `ticks()` is enormous. This is the storm. |
| Set `IMASK` in `CNTP_CTL_EL0` | Nothing ticks, but lldb shows `ISTATUS` cycling to 1. Proves the timer works and the *interrupt* is blocked. |
| Skip `irq::register`, arm the timer anyway | `No handler for IRQ 30` - no, actually nothing at all, because `gic::enable` never ran. Then arm it *and* call `gic::enable(30)` directly to see the message. |

The last one is worth doing precisely because it is confusing: two different mistakes produce two
different silences, and `irq::register` bundles them.

---

## 11. When nothing happens

The failure table. As with the GIC, most wrong things produce identical silence.

| Symptom | Likely cause |
|---|---|
| Nothing ticks, machine otherwise fine | One of the five gates. Read `CNTP_CTL_EL0` with lldb first - it separates "device off" from "GIC ate it" in one step. |
| Nothing ticks, `CNTP_CTL_EL0` reads 0 | `init` never ran, or you wrote `CTL` before `TVAL` and something re-cleared it. |
| Nothing ticks, `CNTP_CTL_EL0` reads 2 | You set `IMASK`. Bit 1 is inverted from how it reads in English - write 0 to allow. |
| Nothing ticks, `CTL` reads 1, `CVAL` non-zero and sensible | The timer is fine. Gate 2 or 5 - `irq::register` not called, or called after `irq::unmask()`. |
| Machine hangs, keyboard dead, `ticks()` enormous if you can read it | The storm. Handler is not re-arming, or re-arming *after* something that returns early. Section 4.4. |
| Machine hangs immediately at boot, before "Hello" | `ENABLE` set before any deadline was written, so it fired at once and the storm started before `kmain` finished. |
| Ticks about 2.6× too fast | You used the UART's 24 MHz `apb-pclk` instead of `CNTFRQ_EL0`'s 62.5 MHz. |
| Ticks wildly too fast, near the exception-entry rate | Interval computed as 0 - integer division with `hz` bigger than the frequency, or an operand order slip. Assert the interval is non-zero in `init`. |
| Divide-by-zero panic in `init` | `CNTFRQ_EL0` read as 0. On QEMU that means you read the wrong register name. |
| `sleep_ms` never returns | The deadline compare is wrong, or you are reading the counter through a plain `static mut` the compiler hoisted out of the loop. `AtomicU64` with `Relaxed` prevents the hoist. |
| `sleep_ms` returns instantly | Deadline computed in the wrong units - milliseconds compared against ticks. |
| Panic report with `EC = 0x18` | Trapped `MSR`/`MRS`. On this machine it means a typo in the register name; the "trapped to EL2" version of this cannot happen here because QEMU boots you at EL1 with the physical timer already permitted. |
| Everything works but time drifts slow | Expected with `TVAL`. Section 9. |

`make trace` (`-d int`) logs every exception taken, with its ESR. For a storm it will fill the log
cap almost instantly, which is itself the diagnosis - and is exactly why `make trace` has a
`ulimit -f` on it.

---

## 12. Deliberately left out

Named here so their absence is a decision rather than an oversight:

- **The virtual timer `CNTV_*`.** Identical interface, exists for hypervisors. Nothing to learn from
  building it twice.
- **`CNTKCTL_EL1` and EL0 access.** There is no EL0 yet. When USERLAND arrives, this register is how
  you decide whether user code may read the clock without a syscall - a genuinely interesting
  decision, at that point.
- **The other three timer interrupts (26, 27, 29).** Other exception levels' timers. Not yours.
- **The memory-mapped generic timer (`CNTBase`).** A separate, optional, actually-on-the-bus version
  of the same architecture, for peripherals that need a timer without a CPU. QEMU's `virt` does not
  have it.
- **Tickless operation.** Modern kernels stop the periodic tick when idle and program a one-shot for
  the next actual deadline, to save power. It requires the timer wheel that section 6 says not to
  build. Correct destination, wrong decade.
- **Interrupt priorities.** Every INTID currently sits at priority 0, so the timer cannot preempt the
  UART handler and vice versa. Fine while both handlers are microseconds long.

---

## 13. Done when

- `ticks()` rises on its own, with the main loop doing nothing, and typing still works.
- You can state what `CNTP_CTL_EL0` currently holds and why, without reading this file.
- You can explain what happens if the handler does not re-arm, and you have seen it happen.
- The tick rate is derived from `CNTFRQ_EL0`, and no 24 MHz constant appears in `src/timer.rs`.
- `sleep_ms` parks the core in `wfi` rather than spinning.
- `board.rs` holds `TIMER_INTID`; `gic.rs` is unchanged.

Then `bd close zheos-ub4`, which unblocks REFLEX and, five tiers up, SCHED.

---

## Optional reading

- **ARM Architecture Reference Manual (ARMv8-A)**, chapter D11 "The Generic Timer in AArch64". The
  register descriptions are D11.2; the `CVAL`/`TVAL` relationship is stated there as the two
  equations in section 4.2 above.
- **`target/arm/helper.c`** in the QEMU source, the `gt_*` functions. Ground truth for what is
  emulated. `GTIMER_HZ` is where 62,500,000 comes from.
- **`Documentation/devicetree/bindings/timer/arm,arch_timer.yaml`** in the Linux source, for the
  interrupt ordering in the `timer` node - secure, non-secure, virtual, hypervisor.
- **`arch/arm64/kernel/`** `arch_timer` code, if you ever want to see what the drift handling and
  tickless machinery actually cost in lines.
