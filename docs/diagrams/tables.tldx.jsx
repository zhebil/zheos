import { Doc, Frame, Group, Box, Text, Edges } from "tldx";

const Node = ({ id, label }) => (
  <Box id={id} label={label} color="violet" fill="semi" font="mono" size="s" w="330" h="130" />
);

const Answer = ({ id, label }) => (
  <Box id={id} label={label} color="green" fill="semi" font="mono" size="s" w="330" h="185" />
);

const Fault = ({ id, label }) => (
  <Box id={id} label={label} color="red" fill="none" font="mono" size="s" w="330" h="185" />
);

export default function Diagram() {
  return (
    <Doc title="Page tables - the tree" layout="col" gap="60" align="start">
      <Text size="l" color="black" maxW="1200">The tree, walked for one address: 0x4008_1234</Text>
      <Text size="s" color="grey" maxW="1200">
        Only a TABLE slot has children. A BLOCK slot and a ZERO slot are both leaves - the walk ends there.
      </Text>

      <Group id="board" layout="row" gap="150" align="start">
        <Group id="tree" layout="col" gap="90" align="center">
          <Box id="ttbr" label={"TTBR0_EL1\nholds the root table's address"} color="black"
            fill="none" font="mono" size="s" w="400" h="100" />

          <Node id="l1" label={"LEVEL 1 TABLE\n512 slots x 8 B = 4 KiB\none slot covers 1 GiB"} />

          <Group id="l1kids" layout="row" gap="90" align="start">
            <Answer id="s0"
              label={"slot 0  -  BLOCK\n0x0000_0000..0x4000_0000\n\nleaf: here is the answer\nall the devices\nDevice memory"} />

            <Group id="b1" layout="col" gap="90" align="center">
              <Box id="s1" label={"slot 1  -  TABLE\n0x4000_0000..0x8000_0000\n\n1 GiB is too coarse\nfor 128 MiB of RAM"}
                color="blue" fill="semi" font="mono" size="s" w="330" h="185" />

              <Node id="l2" label={"LEVEL 2 TABLE\n512 slots x 8 B = 4 KiB\none slot covers 2 MiB"} />

              <Group id="l2kids" layout="row" gap="90" align="start">
                <Answer id="s1a"
                  label={"slots 0..63  -  BLOCK\n0x4000_0000..0x4800_0000\n\nleaf: here is the answer\n64 x 2 MiB = 128 MiB\nNormal memory, real RAM"} />
                <Fault id="s1b"
                  label={"slots 64..511  -  ZERO\n0x4800_0000 and up\n\nleaf: nothing mapped\nRAM that does not exist\nany access faults"} />
              </Group>
            </Group>

            <Fault id="s2"
              label={"slots 2..511  -  ZERO\n0x8000_0000 and up\n\nleaf: nothing mapped\nno table underneath -\nit was never allocated"} />
          </Group>
        </Group>

        <Group id="side" layout="col" gap="50" align="start">
          <Frame id="split" name="How the address picks the path" layout="col" gap="26" pad="30">
            <Box id="sp1" color="blue" fill="none" font="mono" size="s" w="470" h="250"
              label={"0x4008_1234\n\nbits 38..30 = 1     -> slot 1 of level 1\nbits 29..21 = 0     -> slot 0 of level 2\nbits 11..0  = 0x234   carried through"} />
            <Box id="sp2" color="green" fill="none" font="mono" size="s" w="470" h="215"
              label={"slot 0 of level 2 is a BLOCK,\nso the walk stops there.\n\nanswer = 0x4000_0000 + 0x8_1234\n       = 0x4008_1234"} />
          </Frame>

          <Frame id="cost" name="Why branching beats one flat table" layout="col" gap="26" pad="30">
            <Box id="c1" color="red" fill="none" font="mono" size="s" w="470" h="145"
              label={"ONE FLAT TABLE\n2^39 / 4 KiB = 134,217,728\nentries x 8 B = 1 GiB, always"} />
            <Box id="c2" color="green" fill="none" font="mono" size="s" w="470" h="115"
              label={"THIS TREE\n2 tables x 4 KiB = 8 KiB"} />
            <Box id="c3" color="yellow" fill="none" font="mono" size="s" w="470" h="250"
              label={"Every ZERO leaf costs 8 bytes and\nhas nothing under it.\n\nOne of them says 'nothing in this\nwhole GiB'. The flat table needs\n2 MiB of zeros to say the same."} />
          </Frame>
        </Group>
      </Group>

      <Edges color="red" size="l">{`
        ttbr -> l1: read the root
        l1 -> s1: bits 38..30 = 1
        s1 -> l2: a TABLE, so go down
        l2 -> s1a: bits 29..21 = 0, and it is a BLOCK - stop
      `}</Edges>
      <Edges color="grey">{`
        l1 -> s0
        l1 -> s2
        l2 -> s1b
      `}</Edges>
    </Doc>
  );
}
