import { Frame, Box, Group } from "tldx";

const ROWS = [
  ["0 .. 63", "0x40000 .. 0x4003F", "0x4000_0000", "FRAMES metadata table, 64 pages - reserved", "red"],
  ["64 .. 127", "0x40040 .. 0x4007F", "0x4004_0000", "free", "green"],
  ["128 .. 146", "0x40080 .. 0x40092", "0x4008_0000", "kernel image + stack, 19 pages - reserved", "red"],
  ["147", "0x40093", "0x4009_3000", "the page alloc(0) handed back at boot", "orange"],
  ["148 .. 16383", "0x40094 .. 0x43FFF", "0x4009_4000", "free", "green"],
  ["16384 .. 16639", "0x44000 .. 0x440FF", "0x4400_0000", "device tree blob, 256 pages - reserved", "red"],
  ["16640 .. 32767", "0x44100 .. 0x47FFF", "0x4410_0000", "free", "green"],
];

const line = (a, b, c, d) => a.padEnd(18) + b.padEnd(23) + c.padEnd(16) + d;

export function Arena({ ns }) {
  return (
    <Frame
      id={`${ns}-arena`}
      name="2. The same 32768 pages, read in three units. Every number below was printed at boot on -m 128M."
      layout="col"
      gap="50"
      pad="50"
    >
      <Group id={`${ns}-table`} layout="col" gap="0">
        <Box
          id={`${ns}-head`}
          label={line("arena index", "Pfn (absolute)", "start address", "what lives there")}
          color="black"
          w="1500"
          h="70"
          font="mono"
          size="s"
          fill="none"
          textAlign="start"
        />
        {ROWS.map((row, i) => (
          <Box
            id={`${ns}-r${i}`}
            label={line(row[0], row[1], row[2], row[3])}
            color={row[4]}
            w="1500"
            h="70"
            font="mono"
            size="s"
            fill="semi"
            textAlign="start"
          />
        ))}
      </Group>

      <Group id={`${ns}-facts`} layout="row" gap="60">
        <Box
          id={`${ns}-rule`}
          label={"arena index\n  = Pfn - Pfn(arena.base)\n  = Pfn - 0x40000"}
          color="violet"
          font="mono"
          size="s"
          w="660"
          h="250"
        />
        <Box
          id={`${ns}-entry`}
          label={"Metadata::entry(pfn) =\n  0x4000_0000 + 8 * (pfn - 0x40000)\n\n8 bytes per page. One u64."}
          color="light-blue"
          font="mono"
          size="s"
          w="660"
          h="250"
        />
        <Box
          id={`${ns}-sum`}
          label={"32768 - 64 - 19 - 256 = 32429\n\nwhich is what 'frames:' prints"}
          color="green"
          font="mono"
          size="s"
          w="660"
          h="250"
        />
      </Group>

      <Group id={`${ns}-warn`} layout="col" gap="24">
        <Box
          id={`${ns}-w1`}
          label={"An arena index is a ROW NUMBER in the metadata table. It is not a Pfn."}
          color="orange"
          font="mono"
          size="s"
          fill="none"
        />
        <Box
          id={`${ns}-w2`}
          label={"to_addr() on one gives a device address. Index 147 becomes 0x0009_3000, which is pflash."}
          color="red"
          font="mono"
          size="s"
          fill="none"
        />
        <Box
          id={`${ns}-w3`}
          label={"It exists so the 19-bit slab links stay honest. As an index, 19 bits reach 2 GiB of arena wherever RAM sits. As an absolute Pfn they would stop dead at 0x8000_0000."}
          color="grey"
          font="mono"
          size="s"
          fill="none"
        />
      </Group>
    </Frame>
  );
}
