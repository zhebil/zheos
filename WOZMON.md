# WOZMON - specification

A monitor: a small program that lets a person at the serial terminal look at any location in
the machine's memory, change it, and run code that lives there. It is a port of the monitor
Steve Wozniak wrote for the Apple I in 256 bytes.

This document describes only what the user sees and types. It says nothing about how it is
built.

Anything in this document marked **DECISION** is a choice made for this port where the original
was either different or unspecified. Everything else follows the Apple I monitor.

---

## 1. Vocabulary

**Address** - a number naming one location in memory, written in hex, up to 16 digits.
`40000000`, `9000000`, `4000abcd` are addresses.

**Byte** - the contents of one location, always written as exactly two hex digits: `00` through
`FF`.

**Hex digit** - one of `0`-`9`, `A`-`F`.

**Current address** - the monitor remembers the last address it looked at or wrote to. Several
commands continue from there rather than naming a new address.

---

## 2. Starting up

On start the monitor prints a banner, then a prompt on its own line. The prompt is a single
backslash:

```
\
```

The prompt is printed before every line of input, and again after every command line finishes,
however it finished.

**DECISION** - the current address at startup is `0`. Nothing has been examined yet, so any
command that continues from the current address starts at `0`.

---

## 3. Typing a line

The monitor reads one whole line before doing anything. Nothing happens as you type except
that the characters appear on screen.

- **Enter** ends the line and runs it.
- **Backspace** removes the last character typed and erases it from the screen. Backspace on an
  empty line does nothing at all - it must not erase the prompt.
- **Escape** abandons the line. The monitor prints a fresh prompt and forgets what was typed.
- Letters may be typed in upper or lower case and mean the same thing.
- Spaces separate things. Any number of spaces may appear between anything, and leading or
  trailing spaces are ignored.
- An empty line - just Enter - does nothing and prints a fresh prompt.

**DECISION** - a line holds at most 128 characters. Once it is full, further characters are
ignored and do not appear on screen. The line is not truncated or discarded; you can still
backspace and press Enter.

---

## 4. The four commands

### 4.1 Examine one location

Type an address on its own.

```
\40000000
40000000: 1F
```

The monitor prints the address, a colon, a space, and the byte at that address. The current
address becomes the address you named.

### 4.2 Examine a range

Type a start address, a dot, and an end address.

```
\40000000.4000000F
40000000: 1F 20 03 D5 00 00 80 D2
40000008: E0 03 00 91 1F 20 03 D5
```

Every byte from the start address to the end address is printed, inclusive of both ends.

**Layout of the output**

- Eight bytes per row.
- Each row begins with the address of its own first byte, then a colon and a space.
- Bytes within a row are separated by a single space.
- A row ends when the next address would be a multiple of eight. This means the **first** row
  may be short, so that every following row starts at a round address and the columns line up.

```
\40000005.40000012
40000005: 03 D5 00
40000008: 00 80 D2 E0 03 00 91 1F
40000010: 20 03 D5
```

If the end address is below the start address, nothing is printed. It is not an error.

The current address becomes the end address.

### 4.3 Examine continuing from the current address

Type a dot and an end address, with no start address.

```
\40000000
40000000: 1F
\.4000000F
40000001: 20 03 D5 00 00 80 D2
40000008: E0 03 00 91 1F 20 03 D5
```

The range begins at the location **after** the current address and runs to the end address you
named. Same layout rules as above.

### 4.4 Store bytes

Type an address, a colon, and one or more bytes.

```
\40010000: DE AD BE EF
```

The first byte is written at the address named, and each following byte at the next address up.
Nothing is printed on success. The current address becomes the address of the **last** byte
written.

Each value must be exactly one or two hex digits. `5` and `05` both mean the same byte. Three
or more digits in a row is an error - see section 6.

### 4.5 Store continuing from the current address

Type a colon and bytes, with no address.

```
\40010000: DE AD
\: BE EF
```

The bytes are written starting at the location after the current address. In the example above,
the four bytes land at `40010000` through `40010003`, exactly as if written in one command.

### 4.6 Run

Type an address followed by `R`.

```
\40010000 R
```

The monitor hands control to the code at that address and does not come back on its own. If
that code returns, the monitor carries on with the next command on the line, or prints a fresh
prompt.

**DECISION** - `R` may also be typed with no address, in which case the code at the current
address is run.

---

## 5. Several commands on one line

A line may hold any number of commands, one after another, separated by spaces. They are
carried out left to right, and each one sees the current address left by the one before it.

```
\40010000: 20 00 80 D2 C0 03 5F D6 40010000 R
```

That stores eight bytes and then runs them.

```
\40000000 .40000020
```

That examines one byte and then a range continuing from it.

---

## 6. When the line is wrong

**DECISION** - the monitor checks the whole line before carrying out any part of it. If
anything in it is wrong, **nothing at all happens** - no memory is read, no memory is written,
nothing is run - and the monitor says what was wrong and prints a fresh prompt.

The message names the problem and the position in the line where it was found, counting from 1.

```
\40000000 Q
? unexpected character at 10
\4000000000000000000
? address too long at 1
\40010000: DEAD
? byte too long at 11
\40010000:
? expected a byte at 10
\.
? expected an address at 2
```

The position is where the offending thing starts. For something missing off the end of the
line, it is one past the last character typed.

The five things that can be wrong:

| message | what causes it |
|---|---|
| `? unexpected character at N` | a character that is not a hex digit, `.`, `:`, `R`, or a space |
| `? address too long at N` | more than 16 hex digits run together where an address is expected |
| `? byte too long at N` | more than 2 hex digits run together where a byte is expected |
| `? expected a byte at N` | a colon with no bytes after it |
| `? expected an address at N` | a dot with no address after it |

This differs from the Apple I, which carried out each command as it read it and so could half
execute a bad line. Checking first is safer on a machine where a bad address can stop everything
(section 7).

---

## 7. What the monitor does not protect you from

The monitor does exactly what it is told. There is no notion of an address being allowed or
forbidden.

- Naming an address the machine does not have will **stop the machine dead**. It will not print
  an error, it will not return to the prompt, and the only way out is to restart it. This is
  expected and is not a fault in the monitor.
- Storing bytes over the monitor's own code, or over the memory it is using to work, will break
  it in ways that cannot be predicted.
- Running an address that does not contain sensible code will do something arbitrary, most
  likely stopping the machine.

This is the point of the tool, not a shortcoming of it.

---

## 8. Worked session

Everything the user types is shown after a `\` prompt; everything else is the machine.

```
ZheOS monitor
\40000000
40000000: 1F
\.4000001F
40000001: 20 03 D5 00 00 80 D2
40000008: E0 03 00 91 1F 20 03 D5
40000010: 00 00 80 D2 E0 03 00 91
40000018: 1F 20 03 D5 00 00 80 D2
\40010000: 48 49
\: 0A 00
\40010000.40010003
40010000: 48 49 0A 00
\40010002
40010002: 0A
\: 21 00
\40010000.40010004
40010000: 48 49 0A 21 00
\40020000: 20 00 80 D2 C0 03 5F D6 40020000 R
\Q
? unexpected character at 1
\
```

The last store-and-run writes eight bytes that happen to be two instructions, then hands
control to them. They do nothing visible and return, so the monitor carries on and prints a
prompt. Everything before it was plain data, which is why it was only ever looked at, never
run.

---

## 9. Done when

1. A single address prints the right byte, checked against a value you can confirm another way.
2. A range spanning several rows prints with correct row addresses, eight bytes to a row, the
   first row short when the start address is not a multiple of eight, and no bytes missing or
   repeated at the row joins.
3. `.END` on its own continues from where the last command stopped, starting one past it.
4. Bytes written with `:` read back identically with a range examine.
5. `:` on its own continues writing from where the last command stopped.
6. Bytes typed in by hand and then run with `R` produce their visible effect, and control comes
   back to the prompt afterwards.
7. Several commands on one line all happen, in order, each continuing from the last.
8. Each of the five error messages can be produced, and in every case the memory is provably
   unchanged.
9. Backspace at an empty prompt does not eat the prompt.
10. A line of 128 characters does not misbehave, and the 129th character typed is ignored.

---

## 10. Differences from the Apple I original, in one place

| | Apple I | here |
|---|---|---|
| address width | 4 hex digits | up to 16 |
| backspace key | `_` | the Backspace key |
| bad input | the line is carried out up to the mistake | nothing on the line is carried out |
| error report | silence, a fresh prompt | a message naming the problem and its position |
| `R` with no address | not available | runs the current address |
| line length | 127 characters | 128 |
| letter case | upper only | either, on input |
