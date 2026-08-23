import { Doc, Frame, Box, Group, Edge, Sticky } from "tldsl";

const Step = ({ id, label, color }) => (
  <Box id={id} label={label} font="mono" size="s" color={color || "blue"} maxW="300" />
);

const Ask = ({ id, label }) => (
  <Box id={id} label={label} geo="diamond" w="310" h="170" color="orange" size="s" />
);

export default function Diagram() {
  return (
    <Doc layout="col" gap="100" align="start">
      <Box id="title" label="Bump::alloc - the memblock gap-walk, forward only" fill="none" color="black" size="l" />

      <Frame id="chart" name="alloc(&mut self, layout: Layout) -> Option<NonNull<u8>>" layout="col" gap="60" pad="50">
        <Step id="enter" color="green" label={"alloc(layout)"} />

        <Group id="loop" layout="col" gap="55">
          <Step id="align" label={"start  = align_up(next, layout.align())\nfinish = start + layout.size()"} />
          <Ask id="hits" label={"[start, finish)\nhits a\nreservation?"} />
        </Group>

        <Step id="jump" color="violet" label={"next = that reservation's\nbase + size"} />

        <Ask id="past" label={"finish past\nthe end?"} />

        <Step id="none" color="red" label={"return None\n\nnext UNCHANGED"} />

        <Step id="commit" color="green" label={"next = finish\nreturn start"} />
      </Frame>

      <Frame id="traps" name="The three things that go wrong here" layout="row" gap="46" pad="36">
        <Box id="t1" size="s" color="grey" fill="none" maxW="300"
          label={"re-align after jumping\n\nThe yes-branch goes back to align_up, not to the reservation's end. A reservation can end anywhere; the caller asked for 4096."} />
        <Box id="t2" size="s" color="grey" fill="none" maxW="300"
          label={"do not move next on failure\n\nThe natural way is to bump then check, which loses the alignment padding and every skipped reservation on each failed call. Locals, check, then store."} />
        <Box id="t3" size="s" color="grey" fill="none" maxW="300"
          label={"bound the retry loop\n\nIt terminates unsorted: an intersecting reservation ends past start, so next strictly increases. Cap it at reserved_len + 1 anyway."} />
      </Frame>

      <Edge from="enter" to="align" />
      <Edge from="align" to="hits" />
      <Edge from="hits" to="jump" label="yes" color="violet" />
      <Edge from="jump" to="align" label="retry" color="violet" dash="dashed" />
      <Edge from="hits" to="past" label="no" />
      <Edge from="past" to="none" label="yes" color="red" />
      <Edge from="past" to="commit" label="no" color="green" />

      <Sticky on="hits">This is what memblock's for_each_free_mem_range does, walking two lists at once. Yours walks one list and a pointer.</Sticky>
      <Sticky on="jump">The very first allocation takes this branch - it starts at RAM base, hits the image reservation, and jumps clear of it.</Sticky>
    </Doc>
  );
}
