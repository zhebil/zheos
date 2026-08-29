import { Frame, Box, Group, Text, Edge } from "tldx";

const W = "460";

const PAGES = [
  ["200", "slab  class 3\nin_use 64  FULL\nprev -    next -", "64 objects\nevery byte spoken for", "grey"],
  ["201", "slab  class 3\nin_use 12\nprev 203  next 0x7FFFF", "12 objects live\n52 slots free", "orange"],
  ["202", "buddy free\norder 0", "on the buddy list,\nnot a slab at all", "green"],
  ["203", "slab  class 3\nin_use 60\nprev 205  next 201", "60 objects live\n4 slots free", "orange"],
  ["204", "slab  class 6\nin_use 3\nprev -    next -", "class 512, so it is on\nheads[6], not heads[3]", "blue"],
  ["205", "slab  class 3\nin_use 1\nprev 0x7FFFF  next 203", "1 object live\n63 slots free", "orange"],
  ["206", "buddy used\norder 2", "part of a 4-page block\nsomebody holds", "red"],
  ["207", "slab  class 3\nin_use 64  FULL\nprev -    next -", "64 objects\nevery byte spoken for", "grey"],
];

export function ListPartial({ ns }) {
  return (
    <Frame
      id={`${ns}-partial`}
      name="B. cache partial list, class 64 - head in the Cache struct, links inside the metadata table"
      layout="col"
      gap="40"
      pad="50"
      align="start"
    >
      <Group id={`${ns}-headrow`} layout="row" gap="60">
        <Box id={`${ns}-head`} label={"Cache.heads[3]\n= page 205"} color="black" font="mono" size="s" w="560" h="150" />
        <Box id={`${ns}-cap`} label={"one head per class, 9 of them, in .bss. Only pages of THIS class that still have a free slot are on it."} color="grey" font="mono" size="s" fill="none" w="1500" h="150" />
      </Group>

      <Text size="s">metadata table - next_partial and prev_partial are two 19-bit fields of the same u64. The links live here.</Text>

      <Group id={`${ns}-table`} layout="row" gap="0">
        {PAGES.map((p) => (
          <Box id={`${ns}-t${p[0]}`} label={`row ${p[0]}\n${p[1]}`} color={p[3]} w={W} h="260" font="mono" size="s" fill="semi" />
        ))}
      </Group>

      <Text size="s">RAM - and this is why the links cannot live down here.</Text>

      <Group id={`${ns}-mem`} layout="row" gap="0">
        {PAGES.map((p) => (
          <Box id={`${ns}-m${p[0]}`} label={`page ${p[0]}\n${p[2]}`} color={p[3]} w={W} h="200" font="mono" size="s" fill="semi" />
        ))}
      </Group>

      <Group id={`${ns}-notes`} layout="row" gap="60">
        <Box id={`${ns}-n1`} label={"Full slabs come OFF the list.\n205 -> 203 -> 201 skips 200 and 207\nbecause they have no free slot to give."} color="grey" font="mono" size="s" fill="none" w="1000" h="180" />
        <Box id={`${ns}-n2`} label={"204 is a slab with free slots too, but of\nclass 512. It is on heads[6]. Nine lists,\nnine separate chains through the same table."} color="blue" font="mono" size="s" fill="none" w="1100" h="180" />
        <Box id={`${ns}-n3`} label={"The stored number is the arena index -\n205, not Pfn 0x400CD. 19 bits."} color="violet" font="mono" size="s" fill="none" w="900" h="180" />
      </Group>

      <Edge from={`${ns}-head`} to={`${ns}-t205`} color="orange" size="s" fromSide="bottom" toSide="top" dash="dashed" />
      <Edge from={`${ns}-t205`} to={`${ns}-t203`} color="orange" size="s" fromSide="top" toSide="top" bend="200" />
      <Edge from={`${ns}-t203`} to={`${ns}-t201`} color="orange" size="s" fromSide="top" toSide="top" bend="200" />
    </Frame>
  );
}
