import { Doc, Frame, Box, Grid, Group, Edge, Sticky } from "tldsl";

const H = ({ id, label }) => <Box id={id} label={label} fill="none" color="grey" size="s" />;

export default function Diagram() {
  return (
    <Doc layout="col" gap="120" align="start">
      <Frame id="ladder" name="From a number to a byte - four steps, one instruction" layout="col" gap="30" pad="40">
        <Grid id="lgrid" cols="3" gap="34">
          <H id="h1" label="you write" />
          <H id="h2" label="what it is" />
          <H id="h3" label="what the CPU does" />

          <Box id="r1a" label={"let base: usize\n= 0x4700_0000;"} font="mono" size="s" color="grey" />
          <Box id="r1b" label={"a number.\nmeans nothing yet"} size="s" color="grey" />
          <Box id="r1c" label="nothing" size="s" color="light-green" />

          <Box id="r2a" label={"let p = base\n  as *const [u8; 40];"} font="mono" size="s" color="violet" />
          <Box id="r2b" label={"same number.\ncompiler now knows\nwhat lives there"} size="s" color="violet" />
          <Box id="r2c" label="nothing" size="s" color="light-green" />

          <Box id="r3a" label={"let h = unsafe { &*p };"} font="mono" size="s" color="blue" />
          <Box id="r3b" label={"still the same number.\nnow lifetime-checked,\nnon-null, no aliasing"} size="s" color="blue" />
          <Box id="r3c" label="nothing" size="s" color="light-green" />

          <Box id="r4a" label={"let b = h[4];"} font="mono" size="s" color="red" />
          <Box id="r4b" label={"the actual byte"} size="s" color="red" />
          <Box id="r4c" label={"ldrb w0, [x1, #4]\n\nreal bus read"} font="mono" size="s" color="red" />
        </Grid>
      </Frame>

      <Frame id="stars" name="* and & mean different things depending on where they sit" layout="row" gap="90" pad="40" align="start">
        <Frame id="intype" name="in a TYPE - part of the name" layout="col" gap="26" pad="30">
          <Box id="t1" label={"*const u32"} font="mono" color="violet" />
          <Box id="t1d" label={"\"raw pointer to a u32\""} size="s" fill="none" color="grey" />
          <Box id="t2" label={"&[u8; 40]"} font="mono" color="blue" />
          <Box id="t2d" label={"\"reference to 40 bytes\""} size="s" fill="none" color="grey" />
          <Box id="tnote" label={"a noun.\nno code, ever."} size="s" color="light-green" />
        </Frame>

        <Frame id="inexpr" name="in an EXPRESSION - an operation" layout="col" gap="26" pad="30">
          <Box id="e1" label={"*p"} font="mono" color="violet" />
          <Box id="e1d" label={"\"go to that address\""} size="s" fill="none" color="grey" />
          <Box id="e2" label={"&x"} font="mono" color="blue" />
          <Box id="e2d" label={"\"take the address of x\""} size="s" fill="none" color="grey" />
          <Box id="enote" label={"a verb.\nmaybe code."} size="s" color="orange" />
        </Frame>

        <Frame id="combo" name={"why &*p is not a no-op"} layout="col" gap="26" pad="30">
          <Box id="c1" label={"&  *  p"} font="mono" color="black" />
          <Box
            id="c2"
            label={"right to left:\n*p  names the place\n&   takes its address"}
            size="s"
            fill="none"
            color="grey"
          />
          <Box
            id="c3"
            label={"nothing is loaded.\na downgrade:\nraw pointer to reference"}
            size="s"
            color="light-green"
          />
        </Frame>
      </Frame>

      <Frame id="vol" name="volatile - the question that is also an event" layout="row" gap="90" pad="40">
        <Frame id="plain" name="plain read - a question" layout="col" gap="26" pad="30">
          <Box id="p1" label={"let x = *p;\nlet y = *p;"} font="mono" size="s" color="green" />
          <Box id="p2" label={"asking twice gives\nthe same answer"} size="s" fill="none" color="grey" />
          <Box id="p3" label={"so LLVM may read once,\nreorder it, or skip it"} size="s" color="green" />
          <Box id="p4" label={"the DTB: written once\nby QEMU, never again"} size="s" color="light-green" />
        </Frame>

        <Frame id="volat" name="volatile read - an event" layout="col" gap="26" pad="30">
          <Box id="v1" label={"let x = read_volatile(p);\nlet y = read_volatile(p);"} font="mono" size="s" color="red" />
          <Box id="v2" label={"asking twice is not\nthe same as asking once"} size="s" fill="none" color="grey" />
          <Box id="v3" label={"so LLVM must emit both,\nin this order, exactly once each"} size="s" color="red" />
          <Box
            id="v4"
            label={"the UART data register:\nreading POPS the FIFO"}
            size="s"
            color="orange"
          />
        </Frame>
      </Frame>

      <Edge from="r4a" to="r4c" label="the only line that touches memory" color="red" />

      <Sticky on="v4">Fold two reads into one here and you lose a keystroke. That is why uart.rs is volatile.</Sticky>
    </Doc>
  );
}
