import { Frame, Box, Text } from "tldx";

export function Legend({ ns }) {
  return (
    <Frame id={`${ns}-leg`} name="How a descriptor says it" layout="col" gap="26" pad="28">
      <Box id={`${ns}-r`} label={"r  read\nAP = KernelReadOnly or KernelReadWrite"} color="black" fill="none" font="mono" size="s" w="440" h="95" />
      <Box id={`${ns}-w`} label={"w  write\nAP = KernelReadWrite"} color="black" fill="none" font="mono" size="s" w="440" h="80" />
      <Box id={`${ns}-x`} label={"x  execute\nPXN = false. It is eXecute Never,\nso false is what grants it"} color="black" fill="none" font="mono" size="s" w="440" h="120" />
      <Box id={`${ns}-wxn`} label={"SCTLR_EL1.WXN\nwritable implies execute-never,\nin hardware, whatever the table says"} color="red" fill="none" font="mono" size="s" w="440" h="110" />
      <Text size="s" maxW="440">No descriptor at all beats every permission: the walk fails first, so the guard page raises a translation fault, not a permission one.</Text>
    </Frame>
  );
}
