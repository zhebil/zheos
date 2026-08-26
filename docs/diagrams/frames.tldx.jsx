import { Doc, Frame, Box, Group, Text, Edges, Edge } from "tldx";

const Blk = ({ id, label, color, w }) => <Box id={id} label={label} color={color} w={w} />;

export default function Diagram() {
  return (
    <Doc layout="col" gap="90">
      <Text size="l">Allocating order 0 when only an order-3 block is free</Text>

      <Frame id="split" name="split - recurse up until a list is non-empty, then halve on the way down" layout="col" gap="24">
        <Group id="r3" layout="row" gap="0">
          <Blk id="s3" label={"8..15"} color="green" w="1600" />
        </Group>
        <Group id="r2" layout="row" gap="0">
          <Blk id="s2a" label={"8..11"} color="orange" w="800" />
          <Blk id="s2b" label={"12..15"} color="green" w="800" />
        </Group>
        <Group id="r1" layout="row" gap="0">
          <Blk id="s1a" label={"8..9"} color="orange" w="400" />
          <Blk id="s1b" label={"10..11"} color="green" w="400" />
          <Blk id="s1pad" label={" "} color="grey" w="800" />
        </Group>
        <Group id="r0" layout="row" gap="0">
          <Blk id="s0a" label={"8"} color="red" w="200" />
          <Blk id="s0b" label={"9"} color="green" w="200" />
          <Blk id="s0pad" label={" "} color="grey" w="1200" />
        </Group>
        <Group id="legend" layout="row" gap="60">
          <Box id="lg1" label={"red: handed to the caller"} color="red" fill="none" size="s" />
          <Box id="lg2" label={"green: pushed onto the free list for its order"} color="green" fill="none" size="s" />
          <Box id="lg3" label={"orange: split further"} color="orange" fill="none" size="s" />
          <Box id="lg4" label={"grey: untouched"} color="grey" fill="none" size="s" />
        </Group>
      </Frame>

      <Text size="l">Freeing page 8 again - merge while the buddy is free AND the same order</Text>

      <Frame id="merge" name="buddy_pfn = pfn ^ (1 << order)   -   one exclusive-or, no search" layout="col" gap="34">
        <Box id="m0" label={"free 8, order 0\nbuddy = 8 ^ 1 = 9\nfree, same order -> merge"} color="violet" />
        <Box id="m1" label={"now 8, order 1\nbuddy = 8 ^ 2 = 10\nfree, same order -> merge"} color="violet" />
        <Box id="m2" label={"now 8, order 2\nbuddy = 8 ^ 4 = 12\nfree, same order -> merge"} color="violet" />
        <Box id="m3" label={"8, order 3\nback where it started"} color="green" />
        <Box id="stop" label={"stop: buddy in use,\ndifferent order,\nor order 10 reached"} color="red" fill="none" />
      </Frame>

      <Frame id="why" name="why the same-order check is not optional" layout="col" gap="26">
        <Box id="w1" label={"a split buddy is not a free\nblock of order n - it is a region\nwith some pages in use"} color="red" />
        <Box id="w2" label={"merging with it hands out memory somebody is holding"} color="red" fill="none" />
      </Frame>

      <Edges>{`
        m0 -> m1 -> m2 -> m3
      `}</Edges>
      <Edge from="w1" to="w2" />
    </Doc>
  );
}
