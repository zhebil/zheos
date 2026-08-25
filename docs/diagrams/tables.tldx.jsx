import { Doc, Frame, Group, Box, Text, Edge, Edges } from "tldx";

const Slot = ({ id, label, color, w }) => (
  <Box id={id} label={label} color={color} fill="semi" font="mono" size="s" w={w} h="180" />
);

export default function Diagram() {
  return (
    <Doc title="Page tables - the tree" layout="col" gap="90" align="start">
      <Text size="l" color="black" maxW="900">Two tables map the whole machine</Text>

      <Frame id="legend" name="A slot is 8 bytes and says one of three things" layout="row" gap="40" pad="34">
        <Slot id="k-block" w="330" color="green"
          label={"BLOCK  (bottom bits 01)\n\nhere is the answer for\nthis whole chunk. stop."} />
        <Slot id="k-table" w="330" color="blue"
          label={"TABLE  (bottom bits 11)\n\ngo one level down and\nuse the next 9 bits."} />
        <Slot id="k-zero" w="330" color="grey"
          label={"ZERO\n\nnothing mapped here.\nany access faults."} />
      </Frame>

      <Group id="tree" layout="col" gap="110" align="start">
        <Frame id="l1" name="LEVEL 1 table - 512 slots x 8 B = 4 KiB - one slot covers 1 GiB" layout="row" gap="40" pad="34">
          <Slot id="l1s0" w="330" color="green"
            label={"slot 0\n0x0000_0000..0x4000_0000\n\nBLOCK -> all the devices\n(they span 0x0800_0000\nto 0x0A00_4000)"} />
          <Slot id="l1s1" w="330" color="blue"
            label={"slot 1\n0x4000_0000..0x8000_0000\n\nTABLE -> 1 GiB is too\ncoarse for 128 MiB of RAM"} />
          <Slot id="l1rest" w="330" color="grey"
            label={"slots 2..511\n\nZERO\n510 x 8 B = 4 KiB unused"} />
        </Frame>

        <Frame id="l2" name="LEVEL 2 table - 512 slots x 8 B = 4 KiB - one slot covers 2 MiB" layout="row" gap="40" pad="34">
          <Slot id="l2used" w="510" color="green"
            label={"slots 0..63\n0x4000_0000..0x4800_0000\n\n64 BLOCKS x 2 MiB = 128 MiB\nNormal memory, the real RAM"} />
          <Slot id="l2rest" w="510" color="grey"
            label={"slots 64..511\n0x4800_0000 and up\n\nZERO - RAM that does not exist.\na stray pointer here now faults."} />
        </Frame>
      </Group>

      <Frame id="cost" name="Why the tree is smaller, even with 958 unused slots" layout="row" gap="40" pad="34">
        <Slot id="c-flat" w="380" color="red"
          label={"ONE FLAT TABLE\n\n2^39 / 4 KiB = 134,217,728\nentries x 8 B\n\n= 1 GiB, always"} />
        <Slot id="c-tree" w="380" color="green"
          label={"THIS TREE\n\n2 tables x 4 KiB\n\n= 8 KiB"} />
        <Slot id="c-why" w="380" color="yellow"
          label={"ONE zero at level 1 says\n'nothing in this whole GiB'.\n\nThe flat table needs 2 MiB\nof zeros to say the same."} />
      </Frame>

      <Text size="m" color="black" maxW="1250">
        Blocks compress uniformity, zeros compress emptiness. A flat table can express neither,
        so it pays for the whole address space instead of for the memory you actually have.
      </Text>

      <Edge from="l1s1" to="l2" fromSide="bottom" toSide="top" color="blue" label="holds the address of" />
      <Edges color="grey" dash="dashed">{`
        l1s0 -> k-block
        l1s1 -> k-table
        l1rest -> k-zero
      `}</Edges>
    </Doc>
  );
}
