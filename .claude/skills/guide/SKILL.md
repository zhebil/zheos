---
name: guide
description: Write a technical guide in docs/ for a zheos skill - the kind the user reads and then writes the code from. Use when asked to write, rewrite, or fix a guide, or when starting a new skill that needs one. Also the checklist to run against a guide before calling it done.
---

Write the guide the user can build from without asking a follow-up question.

That is the acceptance test, in their words: *"Create now comprehencive guide on timer so i could understant it and actually write everything i need from this guide."* Not "comprehensive" - the one guide they asked to be comprehensive, `gic.md`, is the one they later graded *"a bit overcomplicated for me"*.

## Calibration, from their own ranking

| Guide | Verdict | Lesson |
|---|---|---|
| `dtb.md` | *"last guide on btd was good"* | Opens from something they already have running. |
| `bump.md` | *"Follow same pattern as you used for bump"* | Named as the template for step granularity. |
| `mmu.md` | The post-`/bro` rewrite | Shortest file, most register detail. Copy its section order. |
| `gic.md` | *"a bit overcomplicated for me"* | Complete and unreadable. |
| `wozmon.md` | *"no technical difficulty of understanding hardware and registers"* | Readable and too shallow. |
| `tables.md` v1 | `/bro`, rewritten from scratch | 588 lines that never said whether page tables are hardware or software. |

Aim between `wozmon.md` and `gic.md`. `mmu.md` is where that lands.

## Why guides fail here

Not laziness about rules - `/bro` listed most of these rules in August and they were broken three more times. They fail because the writer knows the material and orders it by their own mental map. The reader hits a name before it means anything, and every later section lands on sand.

So this skill is half rules and half a **checklist you run against the finished file**. The checklist is the part that actually works.

## Before you write

Answer these three in one sentence each, in your own head. If any is shaky, the guide will be too.

1. **What category is this thing?** Hardware format, software convention, or a choice we made? This is the single most repeated complaint. `tables.md` v1 spent 588 lines on page tables without saying the format is fixed by the CPU and the contents are ours.
2. **What does the reader already have working that this connects to?** Start there. `dtb.md` opens from `board.rs`, which they wrote, and names what breaks in it.
3. **What is the one observable that proves it works?** If you cannot name it now, you do not understand the skill well enough to teach it.

## Section order

Follow `mmu.md`. It is the only guide that got through without a rewrite.

1. **What this is and why**, in plain words, before any mechanism. Two or three sentences. Include the category answer from above.
2. **How you touch this hardware at all.** Memory-mapped at an address, or a system register reached with `msr`/`mrs`? `mmu.md` §3 answers this *before naming a single register*, contrasts it against the MMIO they already own in `uart.rs`, and says outright that `make mem` cannot see a system register. `tables.md` introduces four system registers and contains not one occurrence of `msr`, `mrs`, or "system register".
3. **Reading the names.** A section whose whole job is spelling out letters. See the abbreviation rule below.
4. **The mechanism**, one idea per section, in the order the hardware does it.
5. **The constants**, every field derived. See the magic-number rule below.
6. **What you are building** - the shape and the interface, not the code.
7. **How you will know it worked** - concrete observables and failure signatures. This is the last section. `mmu.md` §12 is the model.

**No "Testing it" section.** The project has no tests and no test harness (`CLAUDE.md` working agreement, rule 7). Everything such a section would say belongs in section 7 as something the machine prints at boot. A correctness property worth stating is worth stating as an observable.

Sources go after that, marked optional. Nothing before them may depend on reading them.

## The four rules that keep getting broken

### Expand every abbreviation, character by character

The ask is stronger than a gloss: *"each register name is abbr, show me what each chracter means, it makes things easier to understant."*

`mmu.md:71-76` is the standard - **M**emory **A**ttribute **I**ndirection **R**egister, **E**xception **L**evel 1. Every letter mapped to its word.

Half-expansion is a failure of its own. `timer.md:123` bolds **C**ompare **VAL**ue and then never says what `CNT`, `PCT`, `FRQ` or `CTL` stand for.

A front-loaded Vocabulary section does not discharge this. All three older guides have one, and all three then use a completely different set of acronyms in the body that are never expanded anywhere. Put the glossary before first use, then check the whole file mechanically (below).

If a name genuinely has no expansion, say so - `mmu.md:91`: *"ARM never expands the E and the P"*. Honest beats silent.

### Derive every constant, and say what else it could have been

Three things per field: where the value came from, what the bits mean, **and the other legal encodings**.

`mmu.md:143` is the standard: *"`00` non-cacheable, **`01` Write-Back read+write-allocate**, `10` Write-Through, `11` Write-Back read-allocate only"*.

**Partial derivation is worse than none.** `T0SZ = 25` was explained and `0b01` was not, and the verdict was that the whole register looked arbitrary: *"each value for TCR_EL1 is magic number."* Explaining one field of a register and skipping the rest is the specific failure.

Never write that a value speaks for itself. `MAIR_EL1 = 0xFF` was called self-evident and drew *"what FF means in that case? what else it enables?"*

Bit-masking idioms count as constants. `slot & SLOT_MASK`, `(va >> 30) & 0x1FF`, `(1 << width) - 1` each produced an "explain this syntax" reply. If a line is the core operation of the skill, it does not get shipped as a magic line.

`tables.md:346-364` derives `0x0060_0000_0000_0405` digit by digit. That technique is correct and appears exactly once in 16,000 words. Use it for every constant.

### If it has a shape, draw it - and look at the render

`docs/diagrams/` already holds five diagrams. **No guide links to any of them.** `tables.tldx.jsx` and `mmu-init.tldx.jsx` were built for these exact topics and are orphaned. Link the diagram from the guide.

Anything with a shape gets one: a tree, a pointer chain, a walk, an ordered init sequence, a column layout. Prose loses to a picture for all of these, and the request has always come from the user rather than being offered - twice appended to a `/bro` complaint.

Use the `tldsl` skill. Then **Read the exported PNG before saying it is done**. Both diagrams that shipped unchecked were wrong: a tree that did not branch (*"bro this is not a tree"*), then an arrow into an error node (*"for god sake, your read the root goes into error"*). A clean `tldsl check` is not a finished diagram.

### Do not write the code

Rule 1 of `CLAUDE.md`, and the guides break it worst exactly where the lesson is.

`bump.md:242-256` hands over the `unsafe extern` block and the `&raw const` line complete - the technique the guide itself calls *"the one new Rust technique in the whole skill"*. `timer.md` ships the whole module in five pieces and prices it at *"roughly 60 lines of Rust"*. `tables.md:513-525` hands over a module skeleton hedged as *"since you asked for guidance and not code"*, which concedes the point.

Describe the shape, the interface, and the hard part. A type name and what it has to hold is guidance. A function body is not.

The hard part is usually the one that gets skipped. `tables.md` taught descriptor format and never showed that a child table's physical address goes in the parent's descriptor, so the user reinvented `Vec` and `Box` to solve a problem that did not exist: *"but how do i link tables? ... I don't understand it's shape."*

## Ship checklist

Run this against the file. Do not skip it - this is the part `/bro` was missing.

```sh
F=docs/<guide>.md

grep -oE '\b[A-Z][A-Z0-9_]{2,}\b' $F | sort -u      # every acronym - is each one expanded?
grep -nE '0x[0-9A-Fa-f]+|0b[01]+' $F                # every constant - is each one derived?
grep -n '—' $F                                       # em dashes: must be zero
grep -nE 'diagram|\.jsx' $F                          # does it link the diagram it needs?
grep -nE 'msr|mrs|system register' $F                # if it has *_EL1 registers, this must be non-empty
wc -w $F                                             # over ~3500, cut rather than add
```

Then read the file and confirm:

- [ ] The first three sentences say what the thing is and what category it belongs to.
- [ ] No section has two ideas in it.
- [ ] Nothing is enumerated only to be dismissed. Cut the row, or say the others exist in one line and move on. `timer.md:130` spends four lines on registers it tells you to "mentally delete"; `tables.md:108` lists level 0 and then says it never exists.
- [ ] There is no "Deliberately left out" section. It is padding about padding - 40 lines in `bump.md`, 23 in `dtb.md`, 18 in `gic.md`.
- [ ] The last section is an observable, not a checklist and not a summary. Three of four guides end on "Done when", and two then close with *"You can say out loud..."* - that is a self-quiz, which rule 3 bans.
- [ ] Every number the guide tells the reader to expect was actually observed, not recalled. `dtb.md:546` says step 1 will print `0xd00dfeed`; the same guide's own transcript at `:143` and its failure table at `:569` both say it prints `0xedfe0dd0`. `gic.md` shipped with three wrong register values, found only when the user ordered an update.
- [ ] Terms mean the same thing they mean in the neighbouring guides. `tables.md` calls eight bytes a "slot"; `mmu.md` calls the same thing a "descriptor" and reuses "slot" for a MAIR byte. The two also disagree on the MAIR device value - `0x04` against `0x00`.

## Working rules while the guide is in use

- **A new task does not imply a new markdown file.** The user pre-empts this now: *"do not create md file for it"*, *"I want in response - message"*. Write a guide when asked for a guide.
- **"Do not update guide, just answer"** is a standing preference, stated twice. When they ask a question about a guide mid-skill, answer in conversation.
- **No explanatory comments in their source.** Standing rule: *"i deleted some of your comments, do not put them back, and do not add new comments."* Explanation belongs in the guide.
- **Steps: not so fine they need no thought, not so coarse the reader cannot tell if they are on track.** Both errors happened in one session - *"step one was a bit too granular"*, then *"you instructions are too shalow, and i am not sure i am in right dirrection"*. Every step needs a mid-step signal, not just an end state. `bump.md` granularity is the reference.
- **Say the whole thing the first time.** Two separate failures here: a recommendation with no mechanism (*"I'd take the 540-byte change"* - how?), and knowing something is wrong and waiting to be asked. The second is worse and drew *"why you hanven't mentioned it before... what you haven't told me except that?"*
- **A review bullet that packs a claim, a proof and a recommendation into one clause will come back as "explain this."** It happened five times. Give the before and after.
- **A guide goes stale the moment the code moves.** After a refactor that touches a documented interface, check the guide in the same sitting.
