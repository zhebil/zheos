import { Frame, Box, Text } from "tldx";

export function Elsewhere({ ns }) {
  return (
    <Frame id={`${ns}-rest`} name="The rest of the map" layout="col" gap="34" pad="28">
      <Box id={`${ns}-free`} label={"free RAM\n0x40000000 - 0x48000000\nrw-, minus the image and the DTB"} color="grey" font="mono" size="s" w="320" h="110" />
      <Box id={`${ns}-dtb`} label={"device tree\n0x44000000\nr--"} color="blue" font="mono" size="s" w="300" h="95" />
      <Box id={`${ns}-mmio`} label={"devices (UART, GIC, ...)\n0x00000000 - 0x40000000\nrw-, Device memory"} color="violet" font="mono" size="s" w="320" h="120" />
      <Text size="s" maxW="320">Everything unclaimed is filled in automatically, so a new region never means splitting this by hand</Text>
    </Frame>
  );
}
