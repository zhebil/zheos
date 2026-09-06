import { Frame, Box } from "tldx";

export const KIND = {
  exec: { color: "green", label: "Executable" },
  ro: { color: "blue", label: "Read only" },
  rw: { color: "orange", label: "Writable" },
  free: { color: "grey", label: "Free RAM" },
  none: { color: "red", label: "Not mapped" },
};

const Swatch = ({ id, kind, what, how }) => (
  <Box
    id={id}
    label={`${KIND[kind].label}\n${what}\n${how}`}
    color={KIND[kind].color}
    fill={kind === "none" ? "none" : "solid"}
    dash={kind === "none" ? "dotted" : "draw"}
    size="s"
    w="290"
    h="150"
  />
);

export function Kinds({ ns }) {
  return (
    <Frame id={`${ns}-kinds`} name="Four kinds of memory, and one hole" layout="row" gap="30" pad="34">
      <Swatch id={`${ns}-k-exec`} kind="exec" what="the code, the vector table" how="read it, run it, never write it" />
      <Swatch id={`${ns}-k-ro`} kind="ro" what="constants, the device tree" how="read it, nothing else" />
      <Swatch id={`${ns}-k-rw`} kind="rw" what="stack, data, bss" how="read and write, never run" />
      <Swatch id={`${ns}-k-free`} kind="free" what="everything left over" how="read and write" />
      <Swatch id={`${ns}-k-none`} kind="none" what="the guard page" how="touch it and the CPU faults" />
    </Frame>
  );
}
