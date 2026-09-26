import { Doc, Col, Row, Frame, Box, Text, Edge } from "tldx";

const run = (id, label, color) => (
  <Box id={id} label={label} color={color} fill="semi" w="230" h="130" size="m" />
);

const tick = (id) => <Box id={id} label="tick" color="orange" fill="solid" w="90" h="130" size="s" />;

const step = (id, label) => <Box id={id} label={label} color="orange" w="640" size="m" />;

export default function Diagram() {
  return (
    <Doc title="Sched: the timer takes turns for the tasks" layout="col" gap="80">
      <Text size="xl" maxW="1700">
        Preemption: the tasks never give up the CPU. A timer interrupts whoever is running, 100
        times a second, and the kernel hands the CPU to the next task in line.
      </Text>

      <Frame id="timeline" name="one CPU, time going right" layout="row" gap="30" pad="30" align="center">
        {run("a1", "A runs\n10 ms", "blue")}
        {tick("k1")}
        {run("b1", "B runs\n10 ms", "green")}
        {tick("k2")}
        {run("z1", "zhemon runs\n10 ms", "violet")}
        {tick("k3")}
        {run("a2", "A continues\nexactly where\nit stopped", "blue")}
      </Frame>

      <Row id="below" gap="140" align="start">
        <Col id="steps" gap="22" align="start">
          <Text size="l" maxW="640">what the kernel does at every tick</Text>
          {step("s1", "1   the timer interrupts the running task")}
          {step("s2", "2   save that task's registers on its own stack")}
          {step("s3", "3   pick the next task in the table")}
          {step("s4", "4   load that task's saved registers, let it continue")}
          <Text size="m" maxW="640">
            Nothing is lost: a paused task's whole state waits on its own stack until its turn
            comes back.
          </Text>
        </Col>

        <Col id="table" gap="22" align="center">
          <Text size="l">the task table - round robin</Text>
          <Col id="ring" gap="110" align="center">
            <Box id="ta" label="A" color="blue" fill="semi" w="160" h="90" size="l" />
            <Row id="ring-bottom" gap="220" align="center">
              <Box id="tz" label="zhemon" color="violet" fill="semi" w="160" h="90" size="m" />
              <Box id="tb" label="B" color="green" fill="semi" w="160" h="90" size="l" />
            </Row>
          </Col>
          <Text size="m" maxW="620">
            Each tick moves to the next task, and after the last one it wraps around to the first.
            A and B are busy loops that never call the scheduler - the timer is the only reason they
            take turns.
          </Text>
        </Col>
      </Row>

      <Edge from="a1" to="k1" />
      <Edge from="k1" to="b1" />
      <Edge from="b1" to="k2" />
      <Edge from="k2" to="z1" />
      <Edge from="z1" to="k3" />
      <Edge from="k3" to="a2" />

      <Edge from="s1" to="s2" color="orange" />
      <Edge from="s2" to="s3" color="orange" />
      <Edge from="s3" to="s4" color="orange" />

      <Edge from="ta" to="tb" fromSide="right" toSide="top" label="tick" />
      <Edge from="tb" to="tz" label="tick" />
      <Edge from="tz" to="ta" fromSide="top" toSide="left" label="tick" />
    </Doc>
  );
}
