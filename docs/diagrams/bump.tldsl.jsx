import { Doc, Frame, Box, Group, Edge, Sticky } from "tldsl";

const Slab = ({ id, label, h, color }) => (
  <Box id={id} label={label} w="300" h={h} color={color} fill="semi" font="mono" size="s" />
);

const Note = ({ id, head, body, w }) => (
  <Box id={id} label={`${head}\n\n${body}`} maxW={w} color="grey" fill="none" size="s" />
);

export default function Diagram() {
  return (
    <Doc layout="col" gap="110" align="start">
      <Box id="title" label="BUMP - giving the other 127.9 MiB a name" fill="none" color="black" size="l" />

      <Group id="top" layout="row" gap="150" align="start">
        <Frame id="ram" name="RAM at -m 128M, as the machine has it" layout="col" gap="0" pad="36">
          <Slab id="img" h="110" color="red"
            label={"image + stack\n0x4000_0000 .. 0x4000_d390"} />
          <Slab id="free1" h="200" color="light-green"
            label={"free\n~112 MiB"} />
          <Slab id="dtb" h="110" color="red"
            label={"device tree blob\n0x4700_0000 .. 0x4710_0000"} />
          <Slab id="free2" h="70" color="light-green"
            label={"free\n~15 MiB"} />
        </Frame>

        <Frame id="model" name="What Bump holds - memblock, minus what virt cannot use" layout="col" gap="28" pad="36">
          <Box id="arena" font="mono" size="s" color="blue" maxW="340"
            label={"next  = memory.base\nend   = memory.base + memory.size"} />
          <Note id="arenanote" w="340" head="the arena is ALL of RAM"
            body="/memory in the device tree says so. Nothing is carved off the ends." />
          <Box id="res" font="mono" size="s" color="violet" maxW="340"
            label={"reserved: [Region; 8]\n  [0] image     from linker symbols\n  [1] dtb blob  from its own totalsize"} />
          <Note id="resnote" w="340" head="taken RAM is an entry in a list"
            body="not a smaller arena. alloc searches memory minus reserved - which is the whole of memblock's idea." />
        </Frame>
      </Group>

      <Frame id="why" name="Why this shape and not a simpler one" layout="row" gap="46" pad="36">
        <Note id="w1" w="300" head="1 - the tree lies about itself"
          body="/memory reports all 128 MiB usable, including the megabyte the blob is sitting in. The reservation block is empty. Nothing marks it." />
        <Note id="w2" w="300" head="2 - boundaries hide bugs"
          body="Start the arena at __stack_top and the skip loop never runs until 112 MiB are gone. Reserve the image instead and it runs on allocation #1." />
        <Note id="w3" w="300" head="3 - page tables come next"
          body="A table entry stores a PHYSICAL address, so page tables must be built out of physical RAM. Only this allocator has any. That is why BUMP comes before TABLES." />
      </Frame>

      <Frame id="cut" name="What memblock has that this does not" layout="row" gap="40" pad="36">
        <Note id="c1" w="220" head="a memory LIST"
          body="virt has one /memory node with one reg pair. A list of one is not a list." />
        <Note id="c2" w="270" head="growable arrays"
          body="memblock_double_array allocates its own metadata out of itself. Two entries of eight." />
        <Note id="c3" w="220" head="sorted + merged"
          body="Sorting buys an early exit from a loop that runs twice." />
        <Note id="c4" w="220" head="top-down first-fit"
          body="Exists to keep low RAM free for devices with narrow DMA. There is no DMA." />
        <Note id="c5" w="220" head="free()"
          body="The one place this is genuinely simpler, not just smaller. zheos-9ka gets it." />
      </Frame>

      <Edge from="img" to="res" color="violet" label="reserve(image())" />
      <Edge from="dtb" to="res" color="violet" label="reserve(dtb.region())" />
      <Edge from="ram" to="arena" color="light-green" label="Bump::new(board.memory)" />

      <Sticky on="dtb">QEMU pads virt.dtb to 1 MiB and writes the padded length into totalsize. Linux reserves the same megabyte.</Sticky>
    </Doc>
  );
}
