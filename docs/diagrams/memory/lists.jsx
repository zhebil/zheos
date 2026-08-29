import { Frame, Box, Group, Text } from "tldx";

const QUESTIONS = ["head lives in", "links live in", "unit stored", "end marker", "what is on it", "why the links go there"];

const ANSWERS = {
  buddy: [
    "Frames.lists.heads[order]\n11 slots, one per order,\nin the Frames struct",
    "the free page's own\nfirst 16 bytes:\n[prev, next]",
    "absolute address\npfn.to_addr()",
    "usize::MAX",
    "whole free blocks of\nexactly 2^order pages",
    "a free page belongs to\nnobody, so all 4096 of\nits bytes are spare",
  ],
  partial: [
    "Cache.heads[class_idx]\n9 slots, one per class,\nin the Cache struct",
    "the metadata entry:\nnext_partial, prev_partial\n19 bits each",
    "arena index\npfn - 0x40000",
    "0x7FFFF = (1 << 19) - 1",
    "slab pages of this class\nthat still have\na free slot",
    "a slab page is full of\ncaller data - there is\nnowhere spare inside it",
  ],
  slots: [
    "free_head in the\nmetadata entry\n10 bits",
    "the free slot's own\nfirst 2 bytes:\nnext slot index",
    "slot index\nwithin this one page",
    "1023",
    "free slots inside\none page",
    "a free slot belongs to\nnobody; an in-use slot\nbelongs to the caller",
  ],
};

const SpecColumn = ({ ns, id, name, color, rows, labels }) => (
  <Frame id={`${ns}-${id}`} name={name} layout="col" gap="0" pad="24">
    {rows.map((text, i) => (
      <Box
        id={`${ns}-${id}-${i}`}
        label={labels ? `${QUESTIONS[i]}\n\n${text}` : text}
        color={color}
        w="520"
        h="220"
        font="mono"
        size="s"
        fill="semi"
      />
    ))}
  </Frame>
);

export function Lists({ ns }) {
  return (
    <Frame
      id={`${ns}-lists`}
      name="8. The three lists side by side"
      layout="col"
      gap="90"
      pad="50"
    >
      <Group id={`${ns}-spec`} layout="row" gap="40" align="start">
        <SpecColumn ns={ns} id="qa" name="the question" color="grey" rows={QUESTIONS} />
        <SpecColumn ns={ns} id="ba" name="A. buddy free lists" color="blue" rows={ANSWERS.buddy} />
        <SpecColumn ns={ns} id="pa" name="B. cache partial lists" color="orange" rows={ANSWERS.partial} />
        <SpecColumn ns={ns} id="sa" name="C. free-slot chain" color="green" rows={ANSWERS.slots} />
      </Group>

    </Frame>
  );
}
