import { Frame, Box, Group, Text, Edge } from "tldx";

const W = "420";

const PAGES = [
  ["200", "free  order 0", "prev = 0x400CA000\nnext = usize::MAX", "green"],
  ["201", "used  order 0", "in use\ncaller's bytes", "red"],
  ["202", "free  order 0", "prev = 0x400CB000\nnext = 0x400C8000", "green"],
  ["203", "free  order 0", "prev = 0x400CD000\nnext = 0x400CA000", "green"],
  ["204", "used  order 0", "in use\ncaller's bytes", "red"],
  ["205", "free  order 0", "prev = usize::MAX\nnext = 0x400CB000", "green"],
  ["206", "used  order 0", "in use\ncaller's bytes", "red"],
  ["207", "used  order 0", "in use\ncaller's bytes", "red"],
];

export function ListBuddy({ ns }) {
  return (
    <Frame
      id={`${ns}-buddy`}
      name="A. buddy free list, order 0 - head in the Frames struct, links inside the free pages"
      layout="col"
      gap="40"
      pad="50"
      align="start"
    >
      <Group id={`${ns}-headrow`} layout="row" gap="60">
        <Box id={`${ns}-head`} label={"Frames.lists.heads[0]\n= page 205"} color="black" font="mono" size="s" w="560" h="150" />
        <Box id={`${ns}-cap`} label={"one head per order, 11 of them, in .bss.\nThis is the only part of the list that is not in RAM the allocator manages."} color="grey" font="mono" size="s" fill="none" w="1400" h="150" />
      </Group>

      <Text size="s">metadata table at 0x4000_0000 - one 8-byte row per page. It says free or used, and nothing about the list.</Text>

      <Group id={`${ns}-table`} layout="row" gap="0">
        {PAGES.map((p) => (
          <Box id={`${ns}-t${p[0]}`} label={`row ${p[0]}\n${p[1]}`} color={p[3]} w={W} h="150" font="mono" size="s" fill="semi" />
        ))}
      </Group>

      <Text size="s">RAM - the pages themselves. A free page's first 16 bytes ARE the two link words.</Text>

      <Group id={`${ns}-mem`} layout="row" gap="0">
        {PAGES.map((p) => (
          <Box id={`${ns}-m${p[0]}`} label={`page ${p[0]}\n${p[2]}`} color={p[3]} w={W} h="230" font="mono" size="s" fill="semi" />
        ))}
      </Group>

      <Box
        id={`${ns}-note`}
        label={"The arrows sit on the lower strip because that is literally where the words are: page.to_addr() + 0 and + 8. The order is newest-first, not address order - push puts each freed block at the head."}
        color="violet"
        font="mono"
        size="s"
        fill="none"
        w="2600"
        h="160"
      />

      <Edge from={`${ns}-head`} to={`${ns}-m205`} color="green" size="s" fromSide="bottom" toSide="top" dash="dashed" />
      <Edge from={`${ns}-m205`} to={`${ns}-m203`} color="green" size="s" fromSide="top" toSide="top" bend="200" />
      <Edge from={`${ns}-m203`} to={`${ns}-m202`} color="green" size="s" fromSide="top" toSide="top" bend="140" />
      <Edge from={`${ns}-m202`} to={`${ns}-m200`} color="green" size="s" fromSide="top" toSide="top" bend="200" />
    </Frame>
  );
}
