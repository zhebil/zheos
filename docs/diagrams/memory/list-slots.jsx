import { Frame, Box, Group, Text, Edge } from "tldx";

const SLOTS = [
  ["0", "0x4009_3000", "in use", "red"],
  ["1", "0x4009_3040", "in use", "red"],
  ["2", "0x4009_3080", "in use", "red"],
  ["3", "0x4009_30C0", "in use", "red"],
  ["4", "0x4009_3100", "in use", "red"],
  ["5", "0x4009_3140", "free\nnext = 9", "green"],
  ["6", "0x4009_3180", "in use", "red"],
  ["7", "0x4009_31C0", "in use", "red"],
  ["8", "0x4009_3200", "in use", "red"],
  ["9", "0x4009_3240", "free\nnext = 12", "green"],
  ["10", "0x4009_3280", "in use", "red"],
  ["11", "0x4009_32C0", "in use", "red"],
  ["12", "0x4009_3300", "free\nnext = 1023", "green"],
];

export function ListSlots({ ns }) {
  return (
    <Frame
      id={`${ns}-slots`}
      name="C. free-slot chain inside one page - head in the metadata row, links inside the free slots"
      layout="col"
      gap="40"
      pad="50"
      align="start"
    >
      <Group id={`${ns}-headrow`} layout="row" gap="60">
        <Box
          id={`${ns}-head`}
          label={"metadata row 147\n\nslab  class 3  free_head 5\nin_use 61  prev -  next -"}
          color="black"
          font="mono"
          size="s"
          w="620"
          h="200"
        />
        <Box
          id={`${ns}-cap`}
          label={"free_head is 10 bits, so it can name any slot of any class: the worst case is class 8, 512 slots.\n1023 is the end marker, and it is safe because no class ever has 1023 slots."}
          color="grey"
          font="mono"
          size="s"
          fill="none"
          w="1900"
          h="200"
        />
      </Group>

      <Text size="s">page 147 at 0x4009_3000, class 64: 4096 / 64 = 64 slots, none wasted</Text>

      <Group id={`${ns}-strip`} layout="row" gap="0">
        {SLOTS.map((s) => (
          <Box id={`${ns}-s${s[0]}`} label={`${s[0]}\n${s[1]}\n${s[2]}`} color={s[3]} w="290" h="220" font="mono" size="s" fill="semi" />
        ))}
        <Box id={`${ns}-srest`} label={"slots 13 .. 63\n51 more, same size"} color="grey" w="360" h="220" font="mono" size="s" fill="semi" />
      </Group>

      <Group id={`${ns}-notes`} layout="row" gap="60">
        <Box id={`${ns}-n1`} label={"The number in a free slot is a SLOT INDEX,\nnot an address and not a Pfn. Two bytes.\nalloc turns it back into an address with\npage.to_addr() + slot * 64."} color="violet" font="mono" size="s" fill="none" w="1200" h="220" />
        <Box id={`${ns}-n2`} label={"A red slot holds the caller's bytes.\nThe allocator never reads or writes one -\nwhich is exactly why the chain can only\nrun through the green ones."} color="red" font="mono" size="s" fill="none" w="1200" h="220" />
        <Box id={`${ns}-n3`} label={"in_use 61 and a 3-long chain are the same\nfact twice: 64 - 61 = 3. If they ever\ndisagree, something wrote past a slot."} color="grey" font="mono" size="s" fill="none" w="1200" h="220" />
      </Group>

      <Edge from={`${ns}-head`} to={`${ns}-s5`} color="green" size="s" fromSide="bottom" toSide="top" dash="dashed" />
      <Edge from={`${ns}-s5`} to={`${ns}-s9`} color="green" size="s" fromSide="top" toSide="top" bend="-170" />
      <Edge from={`${ns}-s9`} to={`${ns}-s12`} color="green" size="s" fromSide="top" toSide="top" bend="-170" />
    </Frame>
  );
}
