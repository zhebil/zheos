import { Frame, Box, Text } from "tldx";

const Seg = ({ id, name, range, size, perms, color, h, dash, fill }) => (
  <Box
    id={id}
    label={`${name}\n${range}\n${size}  ${perms}`}
    color={color}
    dash={dash}
    fill={fill}
    font="mono"
    size="s"
    w="430"
    h={h}
  />
);

export function ImageMap({ ns }) {
  return (
    <Frame id={`${ns}-img`} name="Kernel image, high addresses on top" layout="col" gap="0" pad="28">
      <Seg id={`${ns}-stack`} name="stack" range="0x4008e000 - 0x40096000" size="32 KiB" perms="rw-" color="orange" h="150" />
      <Text size="s" color="red">the stack grows down, into the guard</Text>
      <Seg id={`${ns}-guard`} name="GUARD PAGE" range="0x4008d000 - 0x4008e000" size=" 4 KiB" perms="not mapped" color="red" dash="dotted" fill="none" h="90" />
      <Seg id={`${ns}-data`} name=".data + .bss" range="0x4008c000 - 0x4008d000" size=" 4 KiB" perms="rw-" color="orange" h="90" />
      <Seg id={`${ns}-rodata`} name=".rodata" range="0x40089000 - 0x4008c000" size="12 KiB" perms="r--" color="blue" h="90" />
      <Seg id={`${ns}-text`} name=".text + .vectors" range="0x40080000 - 0x40089000" size="36 KiB" perms="r-x" color="green" h="170" />
    </Frame>
  );
}
