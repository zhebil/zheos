import { Doc, Frame, Box, Group, Text, Edge } from "tldx";

const folder = (id, label, color) => (
  <Box id={id} label={label} color={color} fill="semi" w="460" size="m" textAlign="start" />
);

export default function Diagram() {
  return (
    <Doc title="zheos architecture" layout="col" gap="60" align="center">
      <Text size="l" w="1400">
        Two sides. A port to another board or CPU replaces the right side and keeps the left.
      </Text>

      <Group id="sides" layout="row" gap="240" align="start">
        <Frame id="generic" name="generic - knows no hardware" layout="col" gap="34" pad="44" color="green">
          {folder("zhemon", "zhemon/\nthe monitor; later the first user programs", "orange")}
          {folder("tty", "tty/\ninput, line editing, print!", "orange")}
          {folder("kernel", "kernel/\ntask, sched + wait, irq, time, power, panic", "green")}
          {folder("mm", "mm/\nregions, frames, slab, heap", "green")}
          {folder("lib", "lib/\nlock, ring_buffer, mmio", "grey")}
        </Frame>

        <Frame id="hardware" name="hardware-facing - replaced by a port" layout="col" gap="34" pad="44" color="blue">
          {folder("arch", "arch/aarch64/\nboot.s, vectors.s, switch.s, cpu,\nexception, context, mmu, timer, psci", "violet")}
          {folder("drivers", "drivers/\npl011, gic", "blue")}
          {folder("platform", "platform/\ndtb, board: what is where", "light-blue")}
        </Frame>
      </Group>

      <Box id="machine" label={"QEMU virt\nCortex-A72, PL011, GICv2, RAM at 0x4000_0000"} color="red" fill="none" w="1000" />

      <Edge from="generic" to="hardware" label="calls through each folder's pub surface" fromSide="0.98,0.3" toSide="0.02,0.3" />
      <Edge from="hardware" to="generic" label={"calls back into named entry points:\nkmain, handle_interrupt, task_exit, push_character"} fromSide="0.02,0.75" toSide="0.98,0.75" />
      <Edge from="hardware" to="machine" label="msr / mrs, MMIO loads and stores, hvc" />
    </Doc>
  );
}
