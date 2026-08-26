import { Doc, Frame, Group, Box, Text, Edges } from "tldx";

const Step = ({ id, label }) => (
  <Box id={id} label={label} color="violet" fill="semi" font="mono" size="s" w="420" h="120" />
);

const Barrier = ({ id, label }) => (
  <Box id={id} label={label} color="blue" fill="semi" font="mono" size="s" w="420" h="90" />
);

const Live = ({ id, label }) => (
  <Box id={id} label={label} color="green" fill="semi" font="mono" size="s" w="420" h="120" />
);

const Note = ({ id, children }) => (
  <Text id={id} size="s" color="grey" maxW="560">{children}</Text>
);

const Row = ({ id, step, note }) => (
  <Group id={id} layout="row" gap="48" align="center">
    {step}
    {note}
  </Group>
);

export default function Diagram() {
  return (
    <Doc title="MMU init sequence" layout="col" gap="56" align="start">
      <Text size="l" color="black" maxW="1200">
        Turning the MMU on: seven writes, in this order
      </Text>
      <Text size="s" color="grey" maxW="1200">
        Every box on the left is one instruction. These are system registers, not memory - you reach
        them with msr / mrs by name, never with a store to an address.
      </Text>

      <Frame id="seq" name="EL1, MMU still off" layout="col" gap="40" pad="48">
        <Row id="r0"
          step={<Step id="s0" label={"table.base()\nalready built"} />}
          note={<Note id="n0">The tree from TABLES. Root is one 4 KiB page, 512 slots, identity mapping devices and RAM to themselves.</Note>} />

        <Row id="r1"
          step={<Barrier id="s1" label={"dsb ishst"} />}
          note={<Note id="n1">Data Synchronization Barrier. Waits until every store you already made - the whole table - is visible to the other observers in the inner shareable domain, including the hardware table walker.</Note>} />

        <Row id="r2"
          step={<Barrier id="s2" label={"tlbi vmalle1"} />}
          note={<Note id="n2">TLB Invalidate, all entries for EL1. The address cache starts empty, so nothing stale from firmware can answer a lookup.</Note>} />

        <Row id="r3"
          step={<Barrier id="s3" label={"dsb ish"} />}
          note={<Note id="n3">Wait for that invalidate to actually finish everywhere.</Note>} />

        <Row id="r4"
          step={<Step id="s4" label={"msr mair_el1, x\n0x0000_0000_0000_00FF"} />}
          note={<Note id="n4">Eight memory-type slots, one byte each. Slot 0 = 0xFF = Normal, cached. Slot 1 = 0x00 = Device, no caching, no reordering. AttrIndx in every descriptor picks a slot.</Note>} />

        <Row id="r5"
          step={<Step id="s5" label={"msr tcr_el1, x\n0x0000_0002_8080_3519"} />}
          note={<Note id="n5">Shape of the address space: 39-bit VA, 4 KiB granule, walk starts at level 1, TTBR1 half disabled, tables themselves cached and inner shareable.</Note>} />

        <Row id="r6"
          step={<Step id="s6" label={"msr ttbr0_el1, x\ntable.base()"} />}
          note={<Note id="n6">Physical address of the root table. Bits 47:1. The CPU now knows where to start walking - it just is not walking yet.</Note>} />

        <Row id="r7"
          step={<Barrier id="s7" label={"isb"} />}
          note={<Note id="n7">Instruction Synchronization Barrier. Throws away everything the CPU fetched or decoded ahead, so the next instruction sees the new MAIR / TCR / TTBR0.</Note>} />

        <Row id="r8"
          step={<Live id="s8" label={"mrs x0, sctlr_el1\nmov x1, #0x1005\norr x0, x0, x1\nmsr sctlr_el1, x0"} />}
          note={<Note id="n8">Read-modify-write, never a bare store: SCTLR_EL1 holds bits you did not set and must not clear. M = MMU on. C = data cache on. I = instruction cache on.</Note>} />

        <Row id="r9"
          step={<Barrier id="s9" label={"isb"} />}
          note={<Note id="n9">The instruction after this one is the first one fetched through the MMU.</Note>} />
      </Frame>

      <Frame id="after" name="What changed" layout="row" gap="60" pad="40">
        <Box id="a1" label={"every address\nis now virtual"} color="green" fill="semi" size="s" w="300" h="110" />
        <Box id="a2" label={"identity map means\nPC is the same number\non both sides"} color="green" fill="semi" size="s" w="300" h="110" />
        <Box id="a3" label={"unaligned loads\nwork in RAM"} color="green" fill="semi" size="s" w="300" h="110" />
        <Box id="a4" label={"a bad address faults\ninstead of hitting\nthe bus"} color="green" fill="semi" size="s" w="300" h="110" />
      </Frame>

      <Edges color="black">{`
        s0 -> s1 -> s2 -> s3 -> s4 -> s5 -> s6 -> s7 -> s8 -> s9
      `}</Edges>
    </Doc>
  );
}
