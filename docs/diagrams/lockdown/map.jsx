import { Frame, Box, Text } from "tldx";
import { KIND } from "./kinds.jsx";

const Block = ({ id, kind, label, w, h }) => (
  <Box
    id={id}
    label={label}
    color={KIND[kind].color}
    fill={kind === "none" ? "none" : "solid"}
    dash={kind === "none" ? "dotted" : "draw"}
    font="mono"
    size="s"
    w={w}
    h={h}
  />
);

export function Ram({ ns }) {
  return (
    <Frame id={`${ns}-ram`} name="All of memory" layout="col" gap="0" pad="34">
      <Block id={`${ns}-r-free2`} kind="free" label={"free RAM"} w="330" h="150" />
      <Block id={`${ns}-r-dtb`} kind="ro" label={"device tree\n0x44000000"} w="330" h="90" />
      <Block id={`${ns}-r-free1`} kind="free" label={"free RAM"} w="330" h="150" />
      <Box id={`${ns}-r-img`} label={"kernel image\n0x40080000"} color="black" fill="none" dash="dashed" font="mono" size="s" w="330" h="90" />
      <Block id={`${ns}-r-free0`} kind="free" label={"free RAM, page tables\n0x40000000"} w="330" h="110" />
      <Block id={`${ns}-r-dev`} kind="rw" label={"devices\n0x00000000"} w="330" h="110" />
    </Frame>
  );
}

export function Image({ ns }) {
  return (
    <Frame id={`${ns}-img`} name="The kernel image, zoomed in" layout="col" gap="0" pad="34">
      <Block id={`${ns}-i-stack`} kind="rw" label={"stack\n32 KiB"} w="360" h="170" />
      <Text size="s" color="red">grows down, into the guard</Text>
      <Block id={`${ns}-i-guard`} kind="none" label={"GUARD PAGE\n4 KiB"} w="360" h="90" />
      <Block id={`${ns}-i-bss`} kind="rw" label={".data + .bss\n4 KiB"} w="360" h="90" />
      <Block id={`${ns}-i-rodata`} kind="ro" label={".rodata\n12 KiB"} w="360" h="110" />
      <Block id={`${ns}-i-text`} kind="exec" label={".text + .vectors\n36 KiB"} w="360" h="190" />
    </Frame>
  );
}
