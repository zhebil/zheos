import { Frame, Box, Group, Edges, Edge } from "tldx";

export function Units({ ns }) {
  return (
    <Frame
      id={`${ns}-units`}
      name="3. Four units, and every conversion the code actually uses"
      layout="col"
      gap="300"
      pad="50"
    >
      <Group id={`${ns}-chain`} layout="row" gap="260">
        <Box id={`${ns}-addr`} label={"address\nusize\n\n0x4009_3140"} color="blue" font="mono" size="s" w="320" h="200" />
        <Box id={`${ns}-pfn`} label={"Pfn\n\n0x40093\n= 262291"} color="violet" font="mono" size="s" w="320" h="200" />
        <Box id={`${ns}-idx`} label={"arena index\nu32, 19 bits\n\n147"} color="orange" font="mono" size="s" w="320" h="200" />
        <Box id={`${ns}-slot`} label={"slot index\nu16, 10 bits\n\n5"} color="green" font="mono" size="s" w="320" h="200" />
      </Group>

      <Frame id={`${ns}-mask`} name="address & !0xFFF - what the mask does, bit by bit" layout="row" gap="90" pad="40">
        <Box
          id={`${ns}-bits`}
          label={
            "                            low 16 bits\n" +
            "address    0x4009_3140   0011 0001 0100 0000\n" +
            "!0xFFF     ..FFFF_F000   1111 0000 0000 0000\n" +
            "AND        ------------  -------------------\n" +
            "page base  0x4009_3000   0011 0000 0000 0000\n" +
            "\n" +
            "thrown away   0x140 = 320   the offset in the page\n" +
            "slot          320 / 64  =  5"
          }
          color="black"
          font="mono"
          size="s"
          fill="none"
          w="900"
          h="300"
        />
        <Group id={`${ns}-masknotes`} layout="col" gap="34">
          <Box id={`${ns}-m1`} label={"0xFFF = 4095 = every offset a\n4096-byte page can have.\n!0xFFF flips those 12 bits off\nand keeps everything above."} color="light-blue" font="mono" size="s" w="560" h="230" />
          <Box id={`${ns}-m2`} label={"So a & !0xFFF is exactly\n(a / 4096) * 4096:\nround down to the start of\nthe page that a lives in."} color="light-blue" font="mono" size="s" w="560" h="230" />
          <Box id={`${ns}-m3`} label={"In Cache::free the mask is dead.\nfrom_addr_down divides by 4096\nand drops those bits anyway, so\nPfn::from_addr_down(address)\ngives the same Pfn."} color="red" font="mono" size="s" w="560" h="280" />
        </Group>
      </Frame>

      <Group id={`${ns}-legend`} layout="row" gap="70">
        <Box id={`${ns}-l1`} label={"blue: what a caller holds,\nand the only thing the CPU\ncan actually load from"} color="blue" fill="none" size="s" />
        <Box id={`${ns}-l2`} label={"violet: what FRAMES speaks.\nAbsolute, from address 0."} color="violet" fill="none" size="s" />
        <Box id={`${ns}-l3`} label={"orange: what fits in a metadata\nfield. Never leaves the table."} color="orange" fill="none" size="s" />
        <Box id={`${ns}-l4`} label={"green: meaningless without\none page and one class\nto go with it"} color="green" fill="none" size="s" />
      </Group>

      <Edges font="mono" size="s">{`
        ${ns}-addr -> ${ns}-pfn: Pfn::from_addr_down(a) = a / 4096
        ${ns}-pfn -> ${ns}-idx: pfn.index_from(base) = pfn - 0x40000
      `}</Edges>
      <Edge from={`${ns}-pfn`} to={`${ns}-addr`} label={"to_addr() = pfn * 4096"} font="mono" size="s" bend="-110" />
      <Edge from={`${ns}-idx`} to={`${ns}-pfn`} label={"Pfn::from_addr_up(arena_base).offset(i)"} font="mono" size="s" bend="-110" />
      <Edge from={`${ns}-slot`} to={`${ns}-addr`} label={"page.to_addr() + slot * CLASSES[class]"} font="mono" size="s" bend="-210" />
    </Frame>
  );
}
