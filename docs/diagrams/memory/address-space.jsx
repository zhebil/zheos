import { Frame, Box, Group, Text } from "tldx";

const Band = ({ id, label, color, h }) => (
  <Box id={id} label={label} color={color} w="820" h={h} font="mono" size="s" fill="semi" />
);

export function AddressSpace({ ns }) {
  return (
    <Frame
      id={`${ns}-space`}
      name="1. Physical address space - one flat line from 0. Devices below RAM, RAM above."
      layout="row"
      gap="90"
      pad="50"
    >
      <Group id={`${ns}-map`} layout="col" gap="0">
        <Band id={`${ns}-hi`} label={"0x40_1000_0000 +\nPCIe config and MMIO windows"} color="grey" h="90" />
        <Band
          id={`${ns}-ram`}
          label={
            "0x4800_0000   end of RAM, exclusive\n\n" +
            "mach-virt.ram   128 MiB   32768 pages\n\n" +
            "this whole span is THE ARENA:\n" +
            "every page FRAMES has an entry for\n\n" +
            "0x4000_0000   RAM base"
          }
          color="green"
          h="330"
        />
        <Band id={`${ns}-gap`} label={"0x0A00_4000 .. 0x3FFF_FFFF\nunmapped - touching it is a data abort"} color="grey" h="90" />
        <Band id={`${ns}-virtio`} label={"0x0A00_0000   32 virtio-mmio slots"} color="blue" h="70" />
        <Band id={`${ns}-uart`} label={"0x0900_0000   PL011 UART"} color="blue" h="70" />
        <Band id={`${ns}-gic`} label={"0x0800_0000   GIC distributor and CPU interface"} color="blue" h="70" />
        <Band id={`${ns}-flash`} label={"0x0000_0000   pflash, unused with -kernel"} color="grey" h="90" />
      </Group>

      <Group id={`${ns}-side`} layout="col" gap="36">
        <Text size="l">Pfn has exactly one meaning</Text>
        <Box
          id={`${ns}-def`}
          label={"Pfn(a) = a / 4096\n\ncounted from address 0.\nNever from the arena.\nNever from anything else.\nThere is no second convention."}
          color="violet"
          font="mono"
          size="s"
          w="720"
          h="300"
        />
        <Box
          id={`${ns}-ex`}
          label={
            "0x0900_0000 -> Pfn 0x09000 =  36864\n" +
            "0x4000_0000 -> Pfn 0x40000 = 262144\n" +
            "0x4009_3000 -> Pfn 0x40093 = 262291\n" +
            "0x4800_0000 -> Pfn 0x48000 = 294912"
          }
          color="light-blue"
          font="mono"
          size="s"
          w="720"
          h="240"
        />
        <Box
          id={`${ns}-note`}
          label={
            "The UART page HAS a Pfn.\n" +
            "Nothing stops you building it.\n\n" +
            "What it does not have is a\n" +
            "metadata entry - the table only\n" +
            "covers the arena. That is the\n" +
            "distinction you are hunting for,\n" +
            "and it is not a Pfn distinction."
          }
          color="orange"
          font="mono"
          size="s"
          w="720"
          h="440"
        />
      </Group>
    </Frame>
  );
}
