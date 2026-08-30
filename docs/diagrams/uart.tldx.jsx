import { Doc, Frame, Group, Box, Edge, Edges, Text } from "tldx";

const Reg = ({ id, name, note, color = "blue" }) => (
  <Box id={id} label={`${name}\n${note}`} color={color} fill="semi" font="mono" size="s" w="190" maxW="165" />
);

const Part = ({ id, name, note, color }) => (
  <Box id={id} label={`${name}\n${note}`} color={color} fill="semi" font="mono" size="s" w="200" maxW="175" />
);

const Slot = ({ id, ch, full }) => (
  <Box
    id={id}
    label={ch}
    color={full ? "violet" : "grey"}
    fill={full ? "semi" : "none"}
    dash={full ? "solid" : "dotted"}
    font="mono"
    w="56"
    h="70"
  />
);

const Bit = ({ id, v, name, color }) => (
  <Box id={id} label={`${v}\n${name}`} color={color} fill="semi" font="mono" size="s" w="96" h="118" />
);

const queued = ["H", "e", "l", "l", "o"];
// 'H' = 0x48 = 0b0100_1000, least significant bit goes out first
const bits = [0, 0, 0, 1, 0, 0, 1, 0];

export default function Diagram() {
  return (
    <Doc title="UART from scratch" layout="col" gap="130" align="center">
      {/* ---------- 1. the chip ---------- */}
      <Frame id="sec1" name="1 - THE CHIP" layout="col" gap="40" pad="34" align="center">
        <Text size="l">UART: I/O interface</Text>

        <Group id="world" layout="row" gap="110" align="center">
          <Box id="code" label={"MY CODE\nkmain()"} color="green" fill="semi" font="mono" w="160" />

          <Frame id="chip" name="PL011 UART @ 0x0900_0000" layout="col" gap="44" pad="30">
            <Frame id="tx" name="OUTBOUND (TX)" layout="row" gap="44" pad="22">
              <Reg id="drw" name="DR (write)" note="I drop a byte in" color="orange" />
              <Part id="txfifo" name="TX FIFO" note="16-byte queue" color="violet" />
              <Part id="txshift" name="shift register" note="byte -> bits" color="violet" />
            </Frame>

            <Frame id="rx" name="INBOUND (RX)" layout="row" gap="44" pad="22">
              <Reg id="drr" name="DR (read)" note="I take a byte out" color="orange" />
              <Part id="rxfifo" name="RX FIFO" note="16-byte queue" color="light-blue" />
              <Part id="rxshift" name="collector" note="bits -> byte" color="light-blue" />
            </Frame>

          </Frame>

          <Box id="term" label={"MY TERMINAL\nQEMU wires it\nto the other end"} color="green" fill="semi" font="mono" w="190" />
        </Group>

      </Frame>

      {/* ---------- 2. the queue ---------- */}
      <Frame id="sec2" name="2 - THE QUEUE" layout="col" gap="90" pad="34" align="center">
        <Text size="l">The TX queue</Text>

        <Group id="line" layout="row" gap="46" align="center">
          <Box id="head" label={"HEAD\nthe chip pulls\nfrom here"} color="violet" fill="semi" font="mono" size="s" w="200" />

          <Group id="queue" layout="row" gap="14" align="center">
            {Array.from({ length: 16 }, (_, i) => (
              <Slot id={`s${i}`} ch={queued[i] ?? ""} full={i < queued.length} />
            ))}
          </Group>

          <Box id="tail" label={"TAIL\nI push here,\nthrough DR"} color="orange" fill="semi" font="mono" size="s" w="200" />
        </Group>


        <Frame id="putc" name="my whole putc()" layout="row" gap="70" pad="30">
          <Box id="check" label={"FR.TXFF == 1?\n(queue full?)"} geo="diamond" color="blue" fill="semi" font="mono" size="s" w="360" h="220" />
          <Box id="write" label={"write the byte to DR\nand move on"} color="green" fill="semi" font="mono" size="s" w="240" />
        </Frame>
      </Frame>

      {/* ---------- 3. the wire ---------- */}
      <Frame id="sec3" name="3 - THE WIRE" layout="col" gap="40" pad="34" align="center">
        <Text size="l">On the wire: one byte</Text>

        <Group id="byte" layout="row" gap="34" align="center">
          <Box id="ch" label={"'H'"} color="orange" fill="semi" font="mono" size="l" w="120" h="120" />
          <Text size="m" font="mono">
            = 0x48 = 0b0100_1000
          </Text>
        </Group>

        <Frame id="frame" name="what the chip drives onto the wire, left to right" layout="row" gap="10" pad="26">
          <Bit id="start" v="0" name="START" color="red" />
          {bits.map((b, i) => (
            <Bit id={`b${i}`} v={String(b)} name={`bit ${i}`} color="blue" />
          ))}
          <Bit id="stop" v="1" name="STOP" color="green" />
        </Frame>

        <Text size="s" color="grey">
          least significant bit goes out first
        </Text>

        <Group id="notes" layout="row" gap="40" align="start">
          <Box id="n-brd" label={"IBRD / FBRD\nhow long one bit lasts\n115200 bit/s -> 8.7 us"} color="grey" fill="semi" font="mono" size="s" w="320" />
          <Box id="n-lcr" label={"LCR_H\nhow many data bits sit\nbetween START and STOP\n8 = exactly one byte"} color="grey" fill="semi" font="mono" size="s" w="320" />
          <Box id="n-why" label={"receiving end counts backwards:\nsaw START -> count 8 bits\n-> reassemble the byte"} color="grey" fill="semi" font="mono" size="s" w="320" />
        </Group>

      </Frame>

      {/* ---------- edges ---------- */}
      <Edges size="s" font="mono">{`
        drw -> txfifo: to the tail
        txfifo -> txshift: from the head, one at a time
        rxshift -> rxfifo: to the tail
        rxfifo -> drr: waits until I read it
      `}</Edges>

      <Edge from="code" to="drw" label="putc('H')" fromSide="right" toSide="left" size="s" font="mono" color="green" />
      <Edge from="drr" to="code" label="getc()" fromSide="left" toSide="right" size="s" font="mono" color="green" />
      <Edge from="txshift" to="term" label="one bit at a time" fromSide="right" toSide="left" size="s" font="mono" color="violet" />
      <Edge from="term" to="rxshift" label="I pressed a key" fromSide="bottom" toSide="right" size="s" font="mono" color="light-blue" />

      <Edge from="s0" to="head" fromSide="left" toSide="right" color="violet" />
      <Edge from="tail" to="s5" fromSide="top" toSide="top" color="orange" bend="90" />

      <Edges size="s" font="mono">{`
        check -> check: full - I spin and wait
        check -> write: room
      `}</Edges>

      <Edge from="ch" to="start" label="shift register, bit by bit" fromSide="bottom" toSide="top" size="s" font="mono" color="orange" />
    </Doc>
  );
}
