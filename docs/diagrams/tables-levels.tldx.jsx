import { Doc, Group, Box, Text, Edges } from "tldx";

// A real tree: only the branch that needs finer grain grows deeper.
// Slots and the span each one covers - nothing else.
const LevelBox = ({ id, label }) => (
  <Box id={id} w="330" h="140" color="blue" fill="semi" font="mono" size="s" label={label} />
);

const SlotBox = ({ id, color, dash, label }) => (
  <Box id={id} w="290" h="150" color={color} dash={dash} fill="semi" font="mono" size="s" label={label} />
);

export default function Diagram() {
  return (
    <Doc title="Page tables: the tree" layout="col" gap="56">
      <Text size="l" maxW="1300">
        A slot either answers or points deeper. Only the branch that needs finer
        grain grows another level.
      </Text>

      <Group id="tree" layout="col" gap="70" align="center">
        <Box id="ttbr" w="330" h="80" color="black" fill="none" font="mono" size="s"
          label={"TTBR0_EL1\nthe root"} />

        <LevelBox id="l1" label={"LEVEL 1\n512 slots\n1 GiB per slot"} />

        <Group id="l1_kids" layout="row" gap="80" align="start">
          <SlotBox id="l1_dev" color="violet"
            label={"slot 0\nBLOCK\n\n1 GiB\ndevices"} />

          <Group id="ram_branch" layout="col" gap="70" align="center">
            <SlotBox id="l1_ram" color="blue"
              label={"slot 1\nTABLE\n\n1 GiB\ngo deeper"} />

            <LevelBox id="l2" label={"LEVEL 2\n512 slots\n2 MiB per slot"} />

            <Group id="l2_kids" layout="row" gap="80" align="start">
              <SlotBox id="l2_ram" color="green"
                label={"slots 0-63\nBLOCK\n\n128 MiB\nRAM"} />

              <Group id="fine_branch" layout="col" gap="70" align="center">
                <SlotBox id="l2_fine" color="blue" dash="dashed"
                  label={"one slot\nTABLE\n\n2 MiB\ngo deeper"} />

                <LevelBox id="l3" label={"LEVEL 3\n512 slots\n4 KiB per slot"} />

                <Group id="l3_kids" layout="row" gap="80" align="start">
                  <SlotBox id="l3_page" color="green" dash="dashed"
                    label={"one slot\nPAGE\n\n4 KiB\nRAM"} />
                  <SlotBox id="l3_zero" color="red" dash="dashed"
                    label={"one slot\nZERO\n\n4 KiB\nunmapped"} />
                </Group>
              </Group>

              <SlotBox id="l2_zero" color="red"
                label={"slots 64-511\nZERO\n\n896 MiB\nunmapped"} />
            </Group>
          </Group>

          <SlotBox id="l1_zero" color="red"
            label={"slots 2-511\nZERO\n\n510 GiB\nunmapped"} />
        </Group>
      </Group>

      <Text size="s" color="grey" maxW="1300">
        BLOCK and ZERO are leaves - the walk stops there. TABLE is the only slot
        with children. Dashed is level 3: the code supports it, boot does not
        need it.
      </Text>

      <Edges>{`
        ttbr -> l1
        l1 -> l1_dev
        l1 -> l1_ram
        l1 -> l1_zero
        l1_ram -> l2
        l2 -> l2_ram
        l2 -> l2_fine
        l2 -> l2_zero
        l2_fine -> l3
        l3 -> l3_page
        l3 -> l3_zero
      `}</Edges>
    </Doc>
  );
}
