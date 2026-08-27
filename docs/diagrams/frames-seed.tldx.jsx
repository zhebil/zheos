import { Doc, Frame, Box, Group, Text, Edges, Edge } from "tldx";

export default function Diagram() {
  return (
    <Doc layout="row" gap="120">
      <Frame id="walk" name="Seeding the free lists - one pass over the arena" layout="col" gap="52">
        <Box id="start" label={"metadata zeroed\nall 32768 pages read as used"} geo="ellipse" color="grey" w="420" h="150" />
        <Box id="run" label={"take the next free run\np = first page, n = pages in it"} color="blue" w="420" h="110" />
        <Box id="empty" label="n == 0 ?" geo="diamond" color="yellow" w="330" h="180" />
        <Box id="order" label={"order = min(\n  MAX_ORDER,\n  p.trailing_zeros(),\n  n.ilog2() )"} color="violet" w="420" h="180" font="mono" size="s" />
        <Box id="take" label="take 2^order pages at p" color="green" w="420" h="90" />
        <Box id="mark" label={"metadata[p] = free, order\nonly the head byte matters"} color="green" w="420" h="110" />
        <Box id="push" label={"push p onto list[order]\nthe link lives inside the page"} color="green" w="420" h="110" />
        <Box id="adv" label={"p += 2^order\nn -= 2^order"} color="blue" w="420" h="110" />
        <Box id="more" label="another free run ?" geo="diamond" color="yellow" w="380" h="200" />
        <Box id="done" label={"done\nfree_pages() == arena pages\nminus reserved"} geo="ellipse" color="grey" w="440" h="180" />
      </Frame>

      <Group id="side" layout="col" gap="80">
        <Frame id="caps" name="The three caps, and why each exists" layout="col" gap="22">
          <Box id="cap_max" label={"MAX_ORDER = 10\npolicy. Largest block is 4 MiB."} color="violet" w="460" h="110" size="s" />
          <Box id="cap_align" label={"p.trailing_zeros()\nalignment. An order-k block must\nstart at a multiple of 2^k."} color="violet" w="460" h="140" size="s" />
          <Box id="cap_room" label={"n.ilog2()\nroom. The biggest power of two\nthat still fits in the run."} color="violet" w="460" h="140" size="s" />
        </Frame>

        <Frame id="ex" name="Run two of this kernel: p = 144, n = 32624" layout="col" gap="14">
          <Box id="e1" label={"p 144 = 0b10010000  tz 4  -> order 4, take 16"} color="light-blue" font="mono" size="s" w="560" h="66" />
          <Box id="e2" label={"p 160 = 0b10100000  tz 5  -> order 5, take 32"} color="light-blue" font="mono" size="s" w="560" h="66" />
          <Box id="e3" label={"p 192 = 0b11000000  tz 6  -> order 6, take 64"} color="light-blue" font="mono" size="s" w="560" h="66" />
          <Box id="e4" label={"p 256               tz 8  -> order 8, take 256"} color="light-blue" font="mono" size="s" w="560" h="66" />
          <Box id="e5" label={"p 512               tz 9  -> order 9, take 512"} color="light-blue" font="mono" size="s" w="560" h="66" />
          <Box id="e6" label={"p 1024+             tz 10+ -> order 10, x 31"} color="light-blue" font="mono" size="s" w="560" h="66" />
          <Text id="exnote" maxW="560">No order 7 in this run. The staircase climbs as fast as alignment allows, it does not step one at a time.</Text>
        </Frame>

        <Frame id="why" name="Why start from all-used" layout="col" gap="16">
          <Box id="w1" label={"all used, then free what is available\na page you miss is leaked - low free_pages() at boot"} color="light-green" size="s" w="560" h="110" />
          <Box id="w2" label={"all free, then mark what is used\na page you miss is handed out twice - corruption, much later"} color="light-red" size="s" w="560" h="110" />
        </Frame>

        <Frame id="trap" name="The two that get forgotten" layout="col" gap="16">
          <Box id="t1" label={"The metadata array is not in the caller's\nreserved slice. Skip it too, or FRAMES\nhands out its own bookkeeping."} color="orange" size="s" w="560" h="140" />
          <Box id="t2" label={"Never push everything as order 0.\n32000 single pages, ten empty lists, and no\norder-5 block until merging rebuilds\nthe structure you just threw away."} color="orange" size="s" w="560" h="170" />
        </Frame>
      </Group>

      <Edges>{`
        start -> run
        run -> empty
        empty -> order: no
        order -> take -> mark -> push -> adv
        empty -> more: yes
        more -> done: no
      `}</Edges>
      <Edge from="adv" to="empty" fromSide="left" toSide="left" label="loop" />
      <Edge from="more" to="run" fromSide="right" toSide="right" label="yes" />
    </Doc>
  );
}
