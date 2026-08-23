import { Doc, Frame, Box, Group, Edge, Sticky, flow } from "tldsl";

const Layer = ({ id, name, color, children }) => (
  <Frame id={id} name={name} layout="row" gap="40" pad="34" color={color}>
    {children}
  </Frame>
);

export default function Diagram() {
  return (
    <Doc layout="row" gap="140" align="start">
      <Group id="stack" layout="col" gap="46" align="start">
        <Layer id="l4" name="L4 - Applications">
          <Box id="zhemon" label="zhemon" color="orange" />
          <Box id="console" label="console" color="orange" />
        </Layer>

        <Layer id="l3" name="L3 - Services">
          <Box id="irq" label="irq" color="green" />
          <Box id="swtimer" label="timer" color="green" />
          <Box id="input" label="input" color="green" />
          <Box id="fmt" label="print!/println!" color="green" />
        </Layer>

        <Layer id="l2" name="L2 - Drivers">
          <Box id="uart" label={"uart\nPL011"} color="blue" />
          <Box id="gic" label={"gic\nGICv2"} color="blue" />
          <Box id="future" label={"rtc, virtio\nlater"} color="light-blue" dash="dashed" />
        </Layer>

        <Layer id="l1" name="L1 - Board">
          <Box id="boardt" label="Board" color="violet" />
          <Box id="dtb" label="dtb parser" color="violet" />
          <Box id="earlycon" label={"earlycon\nconst"} color="light-violet" dash="dashed" />
        </Layer>

        <Layer id="l0" name="L0 - Arch (aarch64)">
          <Box id="boot" label={"boot.s\nvectors.s"} color="grey" />
          <Box id="cpuz" label="cpu" color="grey" />
          <Box id="exc" label="exception" color="grey" />
          <Box id="mmio" label="mmio" color="grey" />
        </Layer>

        <Frame id="hw" name="Hardware - QEMU virt" layout="row" gap="40" pad="34" color="red">
          <Box id="core" label="Cortex-A72" color="red" fill="none" />
          <Box id="mmiodev" label={"MMIO\n0x0900_0000+"} color="red" fill="none" />
          <Box id="ram" label={"RAM\n0x4000_0000+"} color="red" fill="none" />
        </Frame>
      </Group>

      <Group id="side" layout="col" gap="70">
        <Frame id="bootcol" name="boot order" layout="col" gap="30" pad="34">
          <Box id="b1" label={"1  _start\nstack, .bss"} color="grey" size="s" />
          <Box id="b2" label={"2  dtb::parse"} color="violet" size="s" />
          <Box id="b3" label={"3  Board::from(dtb)"} color="violet" size="s" />
          <Box id="b4" label={"4  uart.init(board)"} color="blue" size="s" />
          <Box id="b5" label={"5  install_vectors"} color="grey" size="s" />
          <Box id="b6" label={"6  gic.init(board)"} color="blue" size="s" />
          <Box id="b7" label={"7  timer.init(board)"} color="green" size="s" />
          <Box id="b8" label={"8  heap.init(board.ram)"} color="green" size="s" dash="dashed" />
          <Box id="b9" label={"9  zhemon"} color="orange" size="s" />
        </Frame>

        <Frame id="rulecol" name="the one rule" layout="col" gap="26" pad="34">
          <Box id="rule" label={"deps point DOWN only"} color="black" fill="none" />
          <Box
            id="viol"
            label={"violation today:\nexception (L0)\ncalls println! (L3)"}
            color="red"
            fill="none"
            size="s"
          />
        </Frame>
      </Group>

      <Edge from="l0" to="hw" label="volatile ld/st, msr/mrs" />
      <Edge from="dtb" to="boardt" label="fills" />
      {flow("b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9")}

      <Sticky on="boardt">
        Runtime struct, not consts. Bases and IRQ ids come from the DTB.
      </Sticky>
      <Sticky on="earlycon">
        The only hardcoded address left: UART base, so a failed parse can still print.
      </Sticky>
    </Doc>
  );
}
