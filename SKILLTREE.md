# Skill tree

High-level view. The authoritative version is `bd list` / `bd ready`; this is the picture.

Most nodes here expand into several atomic skills in beads - `WORKBENCH` alone is six, one per
Wozmon command. Those are deliberately kept out of the diagram so it stays readable.

Legend: **solid** = unlocked, **bold outline** = in progress, **dashed** = locked.

```mermaid
graph TD
    subgraph T0["Tier 0 - Bare Rock"]
        OBSERVE["OBSERVE<br/><i>QEMU, the monitor</i>"]
        BREATH["FIRST BREATH<br/><i>instructions in an ELF</i>"]
        SPARK["SPARK<br/><i>a byte to the UART</i>"]
        GROUND["GROUND<br/><i>linker script</i>"]
        FOOTING["FOOTING<br/><i>stack, .bss, into Rust</i>"]
    end

    subgraph T1["Tier 1 - Tools"]
        CALIBRATE["CALIBRATE<br/><i>UART init</i>"]
        VOICE["VOICE<br/><i>putc, flow control</i>"]
        EARS["EARS<br/><i>getc, input</i>"]
        LANGUAGE["LANGUAGE<br/><i>core::fmt::Write</i>"]
        SIGNAL["SIGNAL<br/><i>the PL011 driver</i>"]
    end

    subgraph T2["Tier 2 - Workshop"]
        WORKBENCH["WORKBENCH<br/><i>Wozmon</i>"]
    end

    subgraph T3["Tier 3 - Reflexes"]
        REFLEX["REFLEX<br/><i>vectors, GIC, timer</i>"]
    end

    subgraph T4["Tier 4 - Territory"]
        TERRITORY["TERRITORY<br/><i>device tree, allocator, MMU</i>"]
    end

    subgraph T5["Tier 5 - Civilization"]
        HANDS2["MANY HANDS<br/><i>tasks, scheduling</i>"]
    end

    subgraph SIDE["Side quests"]
        TIMEKEEPER["TIMEKEEPER<br/><i>PL031 RTC</i>"]
        GPIO["HANDS<br/><i>PL061 GPIO</i>"]
        STORAGE["STORAGE<br/><i>virtio block device</i>"]
        CORES["MANY CORES<br/><i>PSCI, SMP</i>"]
        WORLDS["TWO WORLDS<br/><i>EL0, syscalls</i>"]
    end

    OBSERVE --> BREATH --> SPARK --> GROUND --> FOOTING
    FOOTING --> CALIBRATE
    CALIBRATE --> VOICE
    CALIBRATE --> EARS
    VOICE --> LANGUAGE
    EARS --> SIGNAL
    LANGUAGE --> SIGNAL
    SIGNAL --> WORKBENCH --> REFLEX --> TERRITORY --> HANDS2

    LANGUAGE -.-> TIMEKEEPER
    SIGNAL -.-> GPIO
    TERRITORY -.-> STORAGE
    REFLEX -.-> CORES
    REFLEX -.-> WORLDS

    classDef done fill:#1f6f3f,stroke:#2ea060,color:#fff
    classDef active fill:#8a5a00,stroke:#ffb300,stroke-width:3px,color:#fff
    classDef locked fill:#2b2b33,stroke:#555,color:#aaa,stroke-dasharray:4 3

    class OBSERVE,BREATH,SPARK,GROUND,FOOTING,CALIBRATE,VOICE done
    class SIGNAL active
    class EARS,LANGUAGE locked
    class WORKBENCH,REFLEX,TERRITORY,HANDS2 locked
    class TIMEKEEPER,GPIO,STORAGE,CORES,WORLDS locked
```

## Reading it

The arrows are real prerequisites, not a suggested order. `EARS` needs `CALIBRATE` because a
receive path on an unconfigured UART reads garbage. `WORKBENCH` needs the whole of `SIGNAL`
because Wozmon is nothing but reading hex in and printing hex out. `MANY CORES` needs `REFLEX`
because waking a second CPU without an exception mechanism gives you two things fighting over
one UART and no way to see it happen.

Solid arrows are the main line. Dashed arrows lead to side quests, which are optional and
teach something the main line skips.

## Keeping it current

The colours are hand-maintained and will drift. `bd ready` and `bd blocked` never do. When a
skill is closed in beads, move it to the `done` class here; when the next one is claimed, move
it to `active`.
