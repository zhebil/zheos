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
0000000040000000: 40
```

The monitor prints the address, a colon, a space, and the byte at that address. The current
address becomes the address you named.

**DECISION** - addresses are always printed as 16 hex digits, padded with leading zeros, however
few digits you typed. Every line of output is then the same width and the columns line up.

### 4.2 Examine a range

Type a start address, a dot, and an end address.

```
\40000000.4000000F
0000000040000000: 40 01 00 58 1f 00 00 91
0000000040000008: 40 01 00 58 61 01 00 58
```

Every byte from the start address to the end address is printed, inclusive of both ends.

**Layout of the output**

- Eight bytes per row.
- Each row begins with the address of its own first byte, then a colon.
- Every byte is printed as a space and two hex digits, so each byte occupies three columns.
- A row ends when the next address would be a multiple of eight. This means the **first** row
  may be short, so that every following row starts at a round address.
- A short first row is padded on the left with blanks, three columns per byte it is missing, so
  a byte's column tells you its offset within the row no matter which row it is on.

```
\40000005.40000012
0000000040000005:                00 00 91
0000000040000008: 40 01 00 58 61 01 00 58
0000000040000010: 1f 00 01
```

If the end address is below the start address, nothing is printed. It is not an error.

The current address becomes the end address.

### 4.3 Examine continuing from the current address

Type a dot and an end address, with no start address.

```
\40000000
0000000040000000: 40
\.4000000F
0000000040000001:    01 00 58 1f 00 00 91
0000000040000008: 40 01 00 58 61 01 00 58
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
\40000000 40000008 40000010
```

That examines three single bytes.

Spaces carry no meaning, so `40000000 .40000020` and `40000000.40000020` are the same thing: one
range, not an examine followed by a range.

---

## 6. When the line is wrong

**DECISION** - the monitor checks the whole line before carrying out any part of it. If
anything in it is wrong, **nothing at all happens** - no memory is read, no memory is written,
nothing is run - and the monitor says what was wrong and prints a fresh prompt.

**DECISION** - the report is two lines. First a caret on its own line, in the column of the
character that is wrong, lined up under the line you just typed. Then the word `Error:` and what
was wrong. Pointing beats counting: nobody should have to count columns to find their own typo.

```
\40000000 Q
          ^
Error: Unexpected character
\4000000000000000000
                 ^
Error: Address too long
\40010000: DEAD
             ^
Error: Byte too long
\40010000:
         ^
Error: Expected a byte
\.
 ^
Error: Expected an address
```

The caret sits under the first character that cannot be accepted. For something missing rather
than wrong - a colon with no bytes, a dot with no address - it sits under the command character
that wanted it.

The five things that can be wrong:

| message | where the caret points | what causes it |
|---|---|---|
| `Unexpected character` | the character itself | anything that is not a hex digit, `.`, `:`, `R` or a space |
| `Address too long` | the 17th digit | more than 16 hex digits run together where an address is expected |
| `Byte too long` | the 3rd digit | more than 2 hex digits run together where a byte is expected |
| `Expected a byte` | the colon | a colon with no bytes after it |
| `Expected an address` | the dot | a dot with no address after it |

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
Hello, ZheOS!
Type 'exit' to shutdown the system
----------------------------------
\40000000
0000000040000000: 40
\.4000001F
0000000040000001:    01 00 58 1f 00 00 91
0000000040000008: 40 01 00 58 61 01 00 58
0000000040000010: 1f 00 01 eb 62 00 00 54
0000000040000018: 1f 84 00 f8 fd ff ff 17
\40010000: 48 49
\: 0A 00
\40010000.40010003
0000000040010000: 48 49 0a 00
\40010002
0000000040010002: 0a
\: 21 00
\40010000.40010004
0000000040010000: 48 49 0a 21 00
\40020000: 20 00 80 D2 C0 03 5F D6 40020000 R
\Q
 ^
Error: Unexpected character
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
   first row short and left-padded when the start address is not a multiple of eight, and no
   bytes missing or repeated at the row joins.
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
| error report | silence, a fresh prompt | a caret under the offending character and a message |
| `R` with no address | not available | runs the current address |
| line length | 127 characters | 128 |
| letter case | upper only | either, on input |
