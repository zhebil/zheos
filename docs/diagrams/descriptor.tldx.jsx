import { Doc, Frame, Group, Box, Text } from "tldx";

// Field positions come straight from src/mmu/descriptor.rs `mod bits`.
// The two decoded values are Descriptor::NORMAL_BLOCK and ::DEVICE_BLOCK
// as packed by to_u64().

const H = "84";

const FieldRow = ({ id, color, name, meaning }) => (
  <Group id={`${id}_row`} layout="row" gap="10" pad="0" align="start">
    <Box id={`${id}_b`} w="230" h={H} color={color} fill="semi" font="mono" size="s" label={name} />
    <Box id={`${id}_d`} w="1050" h={H} color="grey" fill="none" size="s" maxW="1020"
      textAlign="start" label={meaning} />
  </Group>
);

const Example = ({ id, name, color, hex, lines }) => (
  <Frame id={id} name={name} layout="col" gap="10" pad="18">
    <Box id={`${id}_hex`} w="600" h="70" color={color} fill="semi" font="mono" size="m" label={hex} />
    <Box id={`${id}_bits`} w="600" h="225" fill="none" color="black" font="mono" size="s"
      textAlign="start" label={lines} />
  </Frame>
);

export default function Diagram() {
  return (
    <Doc title="One descriptor: 64 bits" layout="col" gap="34">
      <Text size="l" maxW="1400">
        Every slot in every table is one 64-bit number.
      </Text>

      <Frame id="fields" name="Low bit first, the order they are packed in" layout="col" gap="8" pad="18">
        <FieldRow id="kind" color="blue" name={"KIND\nbits 1:0"}
          meaning={"00 nothing mapped. 01 a block - the answer. 11 a table, or a page at level 3."} />
        <FieldRow id="attr" color="violet" name={"AttrIndex\nbits 4:2"}
          meaning={"Slot in MAIR_EL1. 0 = Normal RAM, cached. 1 = Device, no cache, no reordering."} />
        <FieldRow id="ns" color="grey" name={"NS\nbit 5"}
          meaning={"Secure world or not. Always 0 - there is only one world so far."} />
        <FieldRow id="ap" color="red" name={"AP\nbits 7:6"}
          meaning={"Read/write rights. 00 kernel RW, 10 kernel RO. Odd values also let user code in."} />
        <FieldRow id="sh" color="grey" name={"SH\nbits 9:8"}
          meaning={"How far this memory stays coherent. 11 = inner shareable, what other cores need."} />
        <FieldRow id="af" color="grey" name={"AF\nbit 10"}
          meaning={"0 makes the first touch fault so an OS can track use. 1 means: do not tell me."} />
        <FieldRow id="ng" color="grey" name={"nG\nbit 11"}
          meaning={"0 = a global mapping, so the TLB may keep it across an address space switch."} />
        <FieldRow id="addr" color="green" name={"ADDRESS\nbits 47:12"}
          meaning={"The physical address, 36 bits. Its low 12 bits are always zero, so it needs no shift."} />
        <FieldRow id="cont" color="grey" name={"CONT\nbit 52"}
          meaning={"Hint: one of 16 neighbours, the TLB may fold them into one. Never set here."} />
        <FieldRow id="pxn" color="red" name={"PXN\nbit 53"}
          meaning={"The kernel may not execute from here. 1 on devices, 0 on RAM."} />
        <FieldRow id="uxn" color="red" name={"UXN\nbit 54"}
          meaning={"User code may not execute from here. 1 everywhere - nothing runs at EL0 yet."} />
        <FieldRow id="rest" color="grey" name={"unused\n63:55, 51:48"}
          meaning={"Reserved or ignored at this stage. Written as zero."} />
      </Frame>

      <Group id="examples" layout="row" gap="40" align="start">
        <Example
          id="ram_ex"
          name="RAM: one 2 MiB block at level 2"
          color="green"
          hex={"0x0040_0000_4000_0701"}
          lines={
            "KIND      01     a block, answer now\n" +
            "AttrIndex 0      Normal memory, cached\n" +
            "AP        00     kernel read-write\n" +
            "SH        11     inner shareable\n" +
            "AF        1      no fault on first touch\n" +
            "ADDRESS   0x4000_0000\n" +
            "PXN       0      kernel may execute\n" +
            "UXN       1      user may not"
          }
        />
        <Example
          id="dev_ex"
          name="Devices: one 1 GiB block at level 1"
          color="violet"
          hex={"0x0060_0000_0000_0405"}
          lines={
            "KIND      01     a block, answer now\n" +
            "AttrIndex 1      Device memory, no cache\n" +
            "AP        00     kernel read-write\n" +
            "SH        00     non shareable\n" +
            "AF        1      no fault on first touch\n" +
            "ADDRESS   0x0000_0000\n" +
            "PXN       1      kernel may NOT execute\n" +
            "UXN       1      user may not either"
          }
        />
        <Box id="note" w="480" h="310" fill="none" color="grey" size="s" maxW="450" textAlign="start"
          label={
            "These two differ in three places only: memory type, shareability, and one execute bit.\n\n" +
            "A table descriptor uses the same bits, but only KIND and ADDRESS mean anything - the permissions are ignored until the walk reaches a leaf."
          } />
      </Group>
    </Doc>
  );
}
