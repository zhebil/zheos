import { Doc, Frame, Box, Group, Text, Edges, Edge, Sticky } from "tldx";

const Slot = ({ id, label, color, w }) => (
  <Box id={id} label={label} color={color} w={w} size="s" />
);

export default function Diagram() {
  return (
    <Doc title="SLAB" layout="col" gap="110">
      <Text size="l">SLAB - one page, one size class, and the free list lives in the free slots</Text>

      <Frame
        id="page"
        name="one 4 KiB page as a slab of the 64-byte class - 4096 / 64 = 64 slots, nothing wasted"
        layout="col"
        gap="30"
      >
        <Group id="strip" layout="row" gap="0">
          <Slot id="p0" label={"0\nin use"} color="red" w="230" />
          <Slot id="p1" label={"1\nnext = 3"} color="green" w="230" />
          <Slot id="p2" label={"2\nin use"} color="red" w="230" />
          <Slot id="p3" label={"3\nnext = 6"} color="green" w="230" />
          <Slot id="p4" label={"4\nin use"} color="red" w="230" />
          <Slot id="p5" label={"5\nin use"} color="red" w="230" />
          <Slot id="p6" label={"6\nnext = none"} color="green" w="230" />
          <Slot id="prest" label={"... 57 more"} color="grey" w="460" />
        </Group>
        <Group id="headrow" layout="row" gap="40">
          <Box id="fhead" label={"free_head = 1"} color="blue" size="s" />
          <Text size="s">a free slot's first 8 bytes ARE the next pointer - the list costs no memory</Text>
        </Group>
      </Frame>

      <Edges color="green" size="s">{`
        fhead -> p1
        p1 -> p3
        p3 -> p6
      `}</Edges>

      <Text size="l">Where the bookkeeping lives - and why it is not in the page</Text>

      <Frame
        id="oob"
        name="decision: slab metadata sits in the FRAMES per-page table, indexed by PFN"
        layout="col"
        gap="40"
      >
        <Group id="lookup" layout="row" gap="90">
          <Box id="ptr" label={"free(ptr)\n0x4020_1080"} color="violet" />
          <Box id="mask" label={"ptr & !0xFFF\n= page base"} color="blue" />
          <Box id="pfn" label={"pfn =\n(base - RAM) >> 12"} color="blue" />
          <Box id="entry" label={"table[pfn]\n8 bytes"} color="orange" />
        </Group>

        <Group id="union" layout="row" gap="120">
          <Frame id="uview" name="one entry, two meanings - a page is never both" layout="col" gap="34">
            <Box
              id="buddyview"
              label={"buddy page (5 bits)\nis_slab: 1   free: 1   order: 4"}
              color="light-blue"
              w="820"
              h="150"
            />
            <Box
              id="slabview"
              label={"slab page (63 bits of a u64)\nis_slab: 1   class: 4   free_head: 10\nin_use: 10   next: 19   prev: 19"}
              color="orange"
              w="820"
              h="230"
            />
          </Frame>
          <Frame id="cost" name="what it costs" layout="col" gap="18">
            <Box id="c1" label={"32768 pages on 128 MiB"} color="grey" fill="none" size="s" />
            <Box id="c2" label={"1 byte/page today = 32 KiB"} color="grey" fill="none" size="s" />
            <Box id="c3" label={"8 bytes/page = 256 KiB"} color="orange" fill="none" size="s" />
            <Box id="c4" label={"0.2% of RAM, paid always"} color="orange" fill="none" size="s" />
            <Box id="c5" label={"19-bit links cap the arena\nat 2 GiB - checked at boot"} color="grey" fill="none" size="s" />
          </Frame>
        </Group>
      </Frame>

      <Edges size="s">{`
        ptr -> mask -> pfn -> entry
      `}</Edges>

      <Text size="l">Why out of band - a header inside the page costs a whole object</Text>

      <Frame id="waste" name="slots per page, and the bytes that end up unusable" layout="row" gap="120">
        <Frame id="inpage" name="16-byte header IN the page - 4080 left, not a power of two" layout="col" gap="16">
          <Box id="i64" label={"class 64:  63 slots, 48 wasted"} color="red" fill="none" size="s" />
          <Box id="i128" label={"class 128:  31 slots, 112 wasted"} color="red" fill="none" size="s" />
          <Box id="i256" label={"class 256:  15 slots, 240 wasted"} color="red" fill="none" size="s" />
          <Box id="i1024" label={"class 1024:  3 slots, 1008 wasted"} color="red" size="s" />
          <Box id="iwhy" label={"the header pushes out\none whole object, every time"} color="red" fill="none" size="s" />
        </Frame>
        <Frame id="outpage" name="metadata OUT of the page - the whole 4096 is slots" layout="col" gap="16">
          <Box id="o64" label={"class 64:  64 slots, 0 wasted"} color="green" fill="none" size="s" />
          <Box id="o128" label={"class 128:  32 slots, 0 wasted"} color="green" fill="none" size="s" />
          <Box id="o256" label={"class 256:  16 slots, 0 wasted"} color="green" fill="none" size="s" />
          <Box id="o1024" label={"class 1024:  4 slots, 0 wasted"} color="green" size="s" />
          <Box id="owhy" label={"only 96 and 192 have a tail,\n64 bytes each"} color="green" fill="none" size="s" />
        </Frame>
      </Frame>

      <Text size="l">The three states, and the list that makes freeing worth doing</Text>

      <Frame id="states" name="a cache is one size class - it walks its partial list first" layout="row" gap="150" align="start">
        <Box id="cache" label={"cache[64]\npartial ->"} color="blue" />
        <Group id="col1" layout="col" gap="130">
          <Box id="sp1" label={"partial\n12 / 64 used"} color="yellow" w="400" h="170" />
          <Box id="sempty" label={"empty\n0 / 64 used\ngoes back to FRAMES"} color="green" w="400" h="170" />
        </Group>
        <Group id="col2" layout="col" gap="130">
          <Box id="sp2" label={"partial\n61 / 64 used"} color="yellow" w="400" h="170" />
          <Box id="sfull" label={"full\n64 / 64 used\noff the list"} color="red" w="400" h="170" />
        </Group>
      </Frame>

      <Edges size="s">{`
        cache -> sp1
        sp1 -> sp2: next / prev
        sp1 -> sempty: in_use hits 0
      `}</Edges>
      <Edge from="sp2" to="sfull" label="last slot taken" size="s" fromSide="bottom-left" toSide="top-left" />
      <Edge from="sfull" to="sp2" label="an object freed" size="s" fromSide="top-right" toSide="bottom-right" />

      <Sticky on="sempty">
        Returning empty slabs is the whole reason free has to find its slab. Skip it and the kernel
        slowly turns all of RAM into empty 64-byte slots.
      </Sticky>

      <Frame id="above" name="above the largest class there is no slab at all" layout="row" gap="60">
        <Box id="big" label={"request > 2048"} color="violet" />
        <Box id="rounds" label={"round up to whole pages"} color="blue" />
        <Box id="frames" label={"FRAMES, matching order"} color="light-blue" />
      </Frame>

      <Edges size="s">{`
        big -> rounds -> frames
      `}</Edges>
    </Doc>
  );
}
