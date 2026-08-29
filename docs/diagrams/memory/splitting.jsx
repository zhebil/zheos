import { Frame, Box, Group, Text, Edge } from "tldx";

const Cell = ({ id, label, color, w, h }) => (
  <Box id={id} label={label} color={color} w={w} h={h || "150"} font="mono" size="s" fill="semi" />
);

const ClassRow = ({ ns, id, name, cells }) => (
  <Frame id={`${ns}-${id}`} name={name} layout="row" gap="0" pad="26">
    {cells.map((c, i) => (
      <Cell id={`${ns}-${id}-${i}`} label={c[0]} color={c[1]} w={c[2]} h="140" />
    ))}
  </Frame>
);

const c64 = [
  ["slot 0\n64 B", "red", "300"],
  ["slot 1\n64 B", "green", "300"],
  ["slot 2\n64 B", "red", "300"],
  ["slot 3\n64 B", "green", "300"],
  ["slot 4\n64 B", "red", "300"],
  ["slot 5\n64 B", "green", "300"],
  ["slot 6\n64 B", "red", "300"],
  ["57 more\nsame size", "grey", "300"],
];

const c512 = [
  ["slot 0\n512 B", "red", "300"],
  ["slot 1", "green", "300"],
  ["slot 2", "red", "300"],
  ["slot 3", "red", "300"],
  ["slot 4", "green", "300"],
  ["slot 5", "red", "300"],
  ["slot 6", "green", "300"],
  ["slot 7", "red", "300"],
];

const c2048 = [
  ["slot 0\n2048 B", "red", "1200"],
  ["slot 1\n2048 B", "green", "1200"],
];

const c8 = [
  ["0", "red", "300"],
  ["1", "green", "300"],
  ["2", "red", "300"],
  ["3", "red", "300"],
  ["4", "green", "300"],
  ["5", "red", "300"],
  ["6", "green", "300"],
  ["505 more\n8 B each", "grey", "300"],
];

export function Splitting({ ns }) {
  return (
    <Frame
      id={`${ns}-split`}
      name="Splitting: address space, then RAM, then one page, then one slot"
      layout="col"
      gap="90"
      pad="50"
    >
      <Frame id={`${ns}-l1`} name="1. the physical address space splits into devices and RAM. Nothing else is memory." layout="row" gap="0" pad="30">
        <Cell id={`${ns}-dev`} label={"0x0000_0000 .. 0x3FFF_FFFF\n\ndevices and holes\nflash, GIC, UART, virtio\n\nno pages, no metadata"} color="blue" w="1000" h="290" />
        <Cell id={`${ns}-ram`} label={"0x4000_0000 .. 0x47FF_FFFF\n\nRAM - the arena\n128 MiB\n\nthis is the only part\nFRAMES manages"} color="green" w="1000" h="290" />
        <Cell id={`${ns}-hi`} label={"0x4800_0000 and up\n\nunmapped, then PCIe\n\nno pages, no metadata"} color="grey" w="900" h="290" />
      </Frame>

      <Frame id={`${ns}-l2`} name="2. RAM splits into 32768 pages of 4096 bytes. FRAMES hands these out whole." layout="row" gap="0" pad="30">
        <Cell id={`${ns}-pg0`} label={"page 0\n0x4000_0000"} color="red" w="330" />
        <Cell id={`${ns}-pg1`} label={"page 1"} color="red" w="330" />
        <Cell id={`${ns}-pgd1`} label={"..."} color="grey" w="240" />
        <Cell id={`${ns}-pg146`} label={"page 146"} color="red" w="330" />
        <Cell id={`${ns}-pg147`} label={"page 147\n0x4009_3000"} color="orange" w="380" />
        <Cell id={`${ns}-pg148`} label={"page 148"} color="green" w="330" />
        <Cell id={`${ns}-pgd2`} label={"..."} color="grey" w="240" />
        <Cell id={`${ns}-pglast`} label={"page 32767\n0x47FF_F000"} color="green" w="380" />
      </Frame>

      <Text size="l">3. one page splits into slots. Which slots, and how many, is the class - it is a choice, not a property of the page. Cells are drawn equal width to stay readable - only the class 512 and 2048 rows are to scale.</Text>

      <ClassRow ns={ns} id="k64" name="class 64 - 4096 / 64 = 64 slots, 0 bytes wasted. This is page 147." cells={c64} />
      <ClassRow ns={ns} id="k8" name="class 8 - the same page cut differently: 4096 / 8 = 512 slots, 0 bytes wasted" cells={c8} />
      <ClassRow ns={ns} id="k512" name="class 512 - 4096 / 512 = 8 slots, 0 bytes wasted" cells={c512} />
      <ClassRow ns={ns} id="k2048" name="class 2048 - 4096 / 2048 = 2 slots. Above this the request goes straight to FRAMES." cells={c2048} />

      <Frame id={`${ns}-l4`} name="4. one slot splits into bytes - and who owns them depends only on whether it is free" layout="row" gap="90" pad="40">
        <Cell
          id={`${ns}-free`}
          label={"free slot, 64 bytes\n\n+0  u16  next free slot index\n+2  62 bytes, whatever was\n    left there last time\n\nowned by the allocator"}
          color="green"
          w="800"
          h="330"
        />
        <Cell
          id={`${ns}-used`}
          label={"in-use slot, 64 bytes\n\n+0  all 64 bytes are\n    the caller's\n\nthe allocator may not\ntouch one byte of it"}
          color="red"
          w="800"
          h="330"
        />
        <Cell
          id={`${ns}-why`}
          label={"That is the whole trick.\n\nThe free list costs no extra\nmemory, because it only ever\nuses bytes nobody owns."}
          color="violet"
          w="800"
          h="330"
        />
      </Frame>

      <Edge from={`${ns}-ram`} to={`${ns}-pg147`} label={"RAM cuts into pages"} size="s" fromSide="bottom" toSide="top" />
      <Edge from={`${ns}-pg147`} to={`${ns}-k64`} label={"page 147 cuts into 64 slots"} size="s" fromSide="bottom" toSide="top" />
    </Frame>
  );
}
