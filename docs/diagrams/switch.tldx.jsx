import { Doc, Col, Row, Frame, Box, Text, Edge } from "tldx";

const FP = [
  ["f3", "d14   d15      sp+144"],
  ["f2", "d12   d13      sp+128"],
  ["f1", "d10   d11      sp+112"],
  ["f0", "d8    d9       sp+96"],
];

const GP = [
  ["a", "x27   x28      sp+64"],
  ["b", "x25   x26      sp+48"],
  ["c", "x23   x24      sp+32"],
  ["d", "x21   x22      sp+16"],
  ["e", "x19   x20      sp+0"],
];

function Stack({ ns, caption, name, top, topColor, slotDash, slotColor, note }) {
  const slot = (k, label, color) => (
    <Box
      id={`${ns}-${k}`}
      label={label}
      w="330"
      font="mono"
      size="s"
      dash={slotDash}
      color={color}
    />
  );

  return (
    <Col id={`${ns}-side`} gap="20" align="center">
      <Text size="l" maxW="350">{caption}</Text>
      <Frame id={`${ns}-stack`} name={name} layout="col" gap="6" pad="18" align="stretch">
        <Text size="s">higher addresses</Text>
        {FP.map(([k, label]) => slot(k, label, "light-violet"))}
        <Box id={`${ns}-x30`} label={top} color={topColor} w="330" font="mono" size="s" />
        {GP.map(([k, label]) => slot(k, label, slotColor))}
        <Box id={`${ns}-sp`} label="sp" color="blue" w="330" font="mono" size="s" fill="semi" />
      </Frame>
      <Box id={`${ns}-rec`} label={"Task\ncontext.sp"} color="blue" w="220" font="mono" size="s" />
      <Text size="s" maxW="350">{note}</Text>
    </Col>
  );
}

export default function Diagram() {
  return (
    <Doc title="Switch: one ret, two stacks" layout="col" gap="60">
      <Text size="xl" maxW="1500">
        The frame you pop is not the frame you pushed. 160 bytes on each stack, and the ret at the
        end reads a return address that no call ever put there.
      </Text>

      <Row id="board" gap="130" align="start">
        <Stack
          ns="ping"
          caption="ping - has run before"
          name="ping stack"
          top={"x29   x30 = back inside ping()"}
          topColor="green"
          slotDash="solid"
          slotColor="black"
          note={"Pushed by switch itself, last time ping was put down. x30 is the instruction after its own call."}
        />

        <Col id="mid" gap="26" align="center">
          <Text size="l" maxW="360">switch(from, to)</Text>
          <Box id="s1" label={"1  stp x19..x30, d8..d15\n   10 pairs, 160 bytes"} w="360" font="mono" size="s" />
          <Box id="s2" label={"2  store sp in from.sp"} color="blue" w="360" font="mono" size="s" />
          <Box id="s3" label={"3  load sp from to.sp"} color="blue" w="360" font="mono" size="s" />
          <Box id="s4" label={"4  ldp x19..x30, d8..d15\n   ret"} color="red" w="360" font="mono" size="s" />
          <Box id="entry" label={"fn pong() -> !"} color="red" w="360" font="mono" size="s" fill="semi" />
          <Text size="s" maxW="380">Between step 2 and step 3 the stack pointer belongs to nobody. That is the switch.</Text>
          <Box
            id="fpen"
            label={"in kernel.s, before bl kmain:\nCPACR_EL1.FPEN = 0b11  (3 << 20)"}
            color="light-violet"
            w="440"
            font="mono"
            size="s"
          />
          <Text size="s" maxW="380">Without it the stp d8 in step 1 traps. d0-d7 and d16-d31 are caller-saved and never appear here.</Text>
        </Col>

        <Stack
          ns="pong"
          caption="pong - never run, faked by hand"
          name="pong stack"
          top={"x29   x30 = pong entry point"}
          topColor="red"
          slotDash="dashed"
          slotColor="grey"
          note={"Nineteen zeros and one address, written by the task builder. Nothing to restore, so it only has to look restorable."}
        />
      </Row>

      <Edge from="ping-rec" to="ping-sp" fromSide="top" toSide="bottom" label="holds the lowest slot" />
      <Edge from="pong-rec" to="pong-sp" fromSide="top" toSide="bottom" label="holds the lowest slot" />
      <Edge from="s2" to="ping-rec" fromSide="left" toSide="right" color="blue" label="outgoing sp saved" />
      <Edge from="pong-rec" to="s3" fromSide="left" toSide="right" color="blue" label="incoming sp loaded" />
      <Edge from="pong-x30" to="s4" fromSide="left" toSide="right" color="red" label="ret reads this slot" />
      <Edge from="s4" to="entry" color="red" />
    </Doc>
  );
}
