import { Doc, Frame, Group, Box, Edges, Text } from "tldx";

// green = unlocked, orange = training now, grey dashed = locked
const Skill = ({ id, name, note, color }) => (
  <Box
    id={id}
    label={`${name}\n${note}`}
    color={color}
    fill="semi"
    dash={color === "grey" ? "dashed" : "solid"}
    font="mono"
    size="s"
    w="180"
    maxW="155"
  />
);

const Branch = ({ id, children }) => (
  <Group id={id} layout="row" gap="40" align="center">
    {children}
  </Group>
);

export default function Diagram() {
  return (
    <Doc title="zheos skill tree" layout="col" gap="60" align="center">
      <Frame id="legend" name="LEGEND" layout="row" gap="26" pad="20">
        <Skill id="lg-done" name="UNLOCKED" note="built it, get it" color="green" />
        <Skill id="lg-now" name="TRAINING" note="on it right now" color="orange" />
        <Skill id="lg-lock" name="LOCKED" note="deps not met" color="grey" />
        <Text size="s" color="grey">
          arrow = hard prerequisite
        </Text>
      </Frame>

      <Group id="tree" layout="row" gap="46" align="center">
        <Skill id="firststeps" name="FIRST STEPS" note="qemu + asm hello" color="green" />
        <Skill id="ground" name="GROUND" note="linker, stack, .bss" color="orange" />
        <Skill id="rustcore" name="RUST CORE" note="no_std, kmain" color="grey" />

        <Group id="branches" layout="col" gap="60" align="start">
          <Branch id="io">
            <Skill id="uart" name="UART" note="PL011 driver" color="grey" />
            <Skill id="print" name="PRINT" note="fmt::Write, print!" color="grey" />
            <Skill id="wozmon" name="WOZMON" note="a live monitor" color="grey" />
          </Branch>

          <Branch id="mem">
            <Skill id="dtb" name="DEVICE TREE" note="the machine's map" color="grey" />
            <Skill id="memblock" name="MEMBLOCK" note="free RAM, bump" color="grey" />
            <Skill id="tables" name="TABLES" note="translation tables" color="grey" />
            <Skill id="mmu" name="MMU" note="virtual memory on" color="grey" />
            <Skill id="pages" name="PAGES" note="buddy, real free" color="grey" />
            <Skill id="slab" name="SLAB" note="size classes" color="grey" />
            <Skill id="heap" name="HEAP" note="alloc: Vec, Box" color="grey" />
          </Branch>

          <Branch id="irq">
            <Skill id="interrupts" name="INTERRUPTS" note="vectors + GIC" color="grey" />
            <Skill id="timer" name="TIMER" note="a periodic tick" color="grey" />
          </Branch>
        </Group>

        <Skill id="tasks" name="TASKS" note="switch + scheduler" color="grey" />
        <Skill id="userspace" name="USERSPACE" note="EL0 + syscalls" color="grey" />
      </Group>

      <Edges color="grey">{`
        firststeps -> ground -> rustcore
        rustcore -> uart -> print -> wozmon
        rustcore -> interrupts -> timer
        rustcore -> dtb -> memblock -> tables -> mmu -> pages -> slab -> heap
        pages -> tasks
        timer -> tasks
        tasks -> userspace
      `}</Edges>
    </Doc>
  );
}
