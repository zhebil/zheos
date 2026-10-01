import { Doc, Col, Row, Frame, Box, Text, Edge } from "tldx";

const state = (id, label, color) => (
  <Box id={id} label={label} color={color} fill="semi" w="360" h="130" size="l" />
);

const step = (id, label, color) => <Box id={id} label={label} color={color} w="560" h="110" size="m" />;

export default function Diagram() {
  return (
    <Doc title="Wait: tasks that sleep and tasks that end" layout="col" gap="20">
      <Text size="l" w="2000">
        The scheduler only picks Ready tasks. A task leaves the CPU by changing its own state.
      </Text>

      <Row id="board" gap="160" align="start">
        <Col id="states" gap="30" align="center">
          <Text size="l" w="600">the life of a task</Text>
          <Frame id="machine" name="states" layout="col" gap="120" pad="40" align="center">
            <Row id="top" gap="260" align="center">
              {state("ready", "Ready\ncan be picked", "green")}
              {state("blocked", "Blocked\nasleep, skipped", "orange")}
            </Row>
            <Row id="bottom" gap="260" align="center">
              {state("dead", "Dead\nskipped, stack still held", "red")}
              {state("gone", "slot empty\nstack freed", "grey")}
            </Row>
          </Frame>
          <Box
            id="idle"
            label={"idle task\npicked only when nobody is Ready\nloops on wfi, never sleeps, never ends"}
            color="violet"
            fill="semi"
            w="520"
            h="140"
            size="m"
          />
        </Col>

        <Col id="order" gap="22" align="center">
          <Text size="l" w="600">going to sleep, in this order</Text>
          <Box id="mask" label="interrupts masked for all three steps" color="grey" w="560" h="60" size="s" />
          {step("s1", "1  prepare: mark me Blocked,\n   put me on the wait queue", "orange")}
          {step("s2", "2  check the buffer:\n   a byte there? mark me Ready, take it", "blue")}
          {step("s3", "3  nothing there: schedule(),\n   another task runs until I am woken", "black")}
          <Text size="m" w="540">
            A byte that lands before step 2 is found by step 2. One that lands after it finds me on
            the queue and wakes me. Check first and prepare second, and a byte in between is lost.
          </Text>
        </Col>
      </Row>

      <Edge from="ready" to="blocked" fromSide="top" toSide="top" label="getc finds no byte" />
      <Edge from="blocked" to="ready" label="UART interrupt: wake all" />
      <Edge from="ready" to="dead" label="its function returns" />
      <Edge from="dead" to="gone" label="next schedule() reaps it" />

      <Edge from="s1" to="s2" />
      <Edge from="s2" to="s3" />
      <Edge from="s3" to="s1" fromSide="left" toSide="left" label="woken" />
    </Doc>
  );
}
