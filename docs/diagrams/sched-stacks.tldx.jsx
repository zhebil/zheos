import { Doc, Col, Row, Frame, Box, Text, Edge } from "tldx";

const slot = (id, label, color, fill) => (
  <Box id={id} label={label} color={color} fill={fill} w="360" h="130" size="m" />
);

const step = (id, label, color) => <Box id={id} label={label} color={color} w="520" size="m" />;

export default function Diagram() {
  return (
    <Doc title="Sched: one tick, two stacks" layout="col" gap="70">
      <Text size="xl" maxW="1700">
        One tick puts task A down and picks task B up. Each task keeps its registers on its own
        stack, so the only thing the kernel has to change is sp - which stack the CPU is standing on.
      </Text>

      <Row id="board" gap="110" align="start">
        <Col id="a-side" gap="24" align="center">
          <Text size="l">A's stack - being put down</Text>
          <Frame id="a-stack" name="A's stack" layout="col" gap="10" pad="20" align="stretch">
            <Text size="s">top of the stack</Text>
            {slot("a-own", "A's own work\n(its function calls)", "blue", "none")}
            {slot("a-all", "all of A's registers\n= where A was\npushed in step 2", "orange", "semi")}
            {slot("a-hnd", "the handler's place\n= how to get back out\npushed in step 4", "light-blue", "semi")}
            <Box id="a-sp" label="sp during steps 1-4" color="blue" fill="solid" w="360" h="70" size="s" />
          </Frame>
          <Box id="a-rec" label={"tasks[A]\nsaved sp"} color="blue" w="240" h="110" size="m" />
        </Col>

        <Col id="steps" gap="22" align="center">
          <Text size="l">in this order</Text>
          {step("s1", "1  tick arrives while A runs", "blue")}
          {step("s2", "2  save all of A's registers\n   onto A's stack", "blue")}
          {step("s3", "3  tell the GIC: done, next tick allowed", "blue")}
          {step("s4", "4  switch: note the handler's place,\n   store sp into tasks[A]", "blue")}
          {step("s5", "5  load sp from tasks[B]\n   the CPU is now on B's stack", "black")}
          {step("s6", "6  take the handler's place off B's stack,\n   leave the handler", "green")}
          {step("s7", "7  restore all of B's registers,\n   B continues where it was", "green")}
        </Col>

        <Col id="b-side" gap="24" align="center">
          <Text size="l">B's stack - being picked up</Text>
          <Frame id="b-stack" name="B's stack" layout="col" gap="10" pad="20" align="stretch">
            <Text size="s">top of the stack</Text>
            {slot("b-own", "B's own work\n(its function calls)", "green", "none")}
            {slot("b-all", "all of B's registers\n= where B was\ntaken off in step 7", "orange", "semi")}
            {slot("b-hnd", "the handler's place\n= how to get back out\ntaken off in step 6", "light-blue", "semi")}
            <Box id="b-sp" label="sp from step 5 on" color="green" fill="solid" w="360" h="70" size="s" />
          </Frame>
          <Box id="b-rec" label={"tasks[B]\nsaved sp"} color="green" w="240" h="110" size="m" />
          <Text size="s" maxW="400">
            Both frames were left here by an earlier tick, when B was the one put down.
          </Text>
        </Col>
      </Row>

      <Edge from="s1" to="s2" />
      <Edge from="s2" to="s3" />
      <Edge from="s3" to="s4" />
      <Edge from="s4" to="s5" />
      <Edge from="s5" to="s6" />
      <Edge from="s6" to="s7" />

      <Edge from="s2" to="a-all" fromSide="left" toSide="right" color="orange" label="push" />
      <Edge from="s4" to="a-hnd" fromSide="left" toSide="right" color="light-blue" label="push" />
      <Edge from="a-sp" to="a-rec" fromSide="bottom" toSide="top" color="blue" label="step 4" />
      <Edge from="b-rec" to="b-sp" fromSide="top" toSide="bottom" color="green" label="step 5" />
    </Doc>
  );
}
