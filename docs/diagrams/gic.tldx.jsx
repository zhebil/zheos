// GIC - how a byte arriving on the wire becomes a call to a Rust function.
//
// Left to right is the hardware path: the device raises a line, the shared
// distributor decides which core hears it, that core's own interface signals
// the CPU. Top to bottom inside CORE 0 is the software path, and the two
// arrows going back to the interface are the only things the handler is
// obliged to tell the GIC: "got it" and "done".
import { Doc, Frame, Group, Box, Text, Edges, Edge, Sticky } from "tldx";

const Node = ({ id, label, color, dash }) => (
  <Box
    id={id}
    label={label}
    color={color}
    fill="solid"
    dash={dash}
    font="sans"
    size="s"
    maxW="235"
  />
);

const Later = ({ id, label }) => (
  <Box
    id={id}
    label={label}
    color="grey"
    fill="semi"
    dash="dashed"
    font="sans"
    size="s"
    maxW="130"
  />
);

export default function Gic() {
  return (
    <Doc title="GIC - from a device line to a handler" layout="col" gap="70">
      <Group id="board" layout="row" gap="110" align="center">
        <Frame id="devices" name="DEVICES" layout="col" gap="34" pad="26">
          <Node id="uart" label={"UART - PL011\nintid 33"} color="orange" />
          <Later id="timer" label={"generic timer\nintid 30\n(next post)"} />
        </Frame>

        <Frame id="gic" name="GIC v2" layout="col" gap="46" pad="26">
          <Node
            id="gicd"
            label={"Distributor - GICD\n0x0800_0000\none for the whole machine"}
            color="violet"
          />
          <Group id="ifaces" layout="row" gap="22" align="start">
            <Later id="gicc1" label={"GICC\ncore 1"} />
            <Later id="gicc2" label={"GICC\ncore 2"} />
            <Node
              id="gicc0"
              label={"CPU interface - GICC\n0x0801_0000\ncore 0"}
              color="blue"
            />
          </Group>
        </Frame>

        <Frame id="core" name="CORE 0" layout="col" gap="26" pad="26">
          <Node
            id="entry"
            label={"irq_entry\nsave x0-x30"}
            color="light-blue"
          />
          <Node
            id="ack"
            label={"gic::acknowledge()\nread IAR -> intid"}
            color="green"
          />
          <Node id="lookup" label={"HANDLERS[intid]"} color="light-green" />
          <Node
            id="run"
            label={"uart::handle_interrupt\npush the byte into the ring buffer"}
            color="light-green"
          />
          <Node
            id="eoi"
            label={"interrupt.end()\nwrite EOIR"}
            color="green"
          />
          <Node
            id="eret"
            label={"eret - the interrupted code carries on"}
            color="light-blue"
          />
        </Frame>
      </Group>

      <Text id="legend" maxW="900" color="grey" font="sans" size="s">
        Dashed grey is hardware that exists but nothing drives yet. The
        distributor is shared; every core gets its own interface, which is why
        a shared interrupt has to be routed to one of them on purpose.
      </Text>

      <Edges color="black" font="sans" size="s">{`
        uart -> gicd: 1. a byte arrives, the line goes high
        gicd -> gicc0: 2. routed by ITARGETSR
        entry -> ack -> lookup -> run -> eoi -> eret
      `}</Edges>

      <Edge
        from="gicc0"
        to="entry"
        label="3. IRQ - the CPU drops everything"
        color="black"
        font="sans"
        size="s"
        fromSide="top-right"
        toSide="left"
        bend="-140"
      />
      <Edge
        from="ack"
        to="gicc0"
        label="4. got it - who was that?"
        color="green"
        font="sans"
        size="s"
        fromSide="left"
        toSide="right"
        bend="60"
      />
      <Edge
        from="eoi"
        to="gicc0"
        label="5. done with it"
        color="green"
        font="sans"
        size="s"
        fromSide="left"
        toSide="bottom"
        bend="-90"
      />

      <Sticky on="ack" color="red">
        Skip this and the line stays high - the handler is re-entered forever.
      </Sticky>
      <Sticky on="eoi" color="red">
        Skip this and the running priority never drops. The GIC delivers
        nothing again, silently.
      </Sticky>
    </Doc>
  );
}
