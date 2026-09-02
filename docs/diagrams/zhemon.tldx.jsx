// Zhemon - how one typed line becomes commands and then memory traffic.
// Positions are pinned because the layout was arranged by hand on the canvas.
import { Doc, Box, Text, Edge } from "tldx";

const Step = (id, label, x, y, w, h, color) => (
  <Box id={id} label={label} x={x} y={y} w={w} h={h} color={color} fill="solid" font="sans" size="s" />
);

const Choice = (id, label, x, y) => (
  <Box id={id} label={label} x={x} y={y} w="250" h="150" geo="diamond" color="violet" fill="solid" font="sans" size="s" />
);

export default function Zhemon() {
  return (
    <Doc title="Zhemon - parsing and execution" layout="free">
      <Text id="note" x="274" y="0" maxW="549" color="black" font="sans" size="s">
        The same loop runs twice per line: pass 1 parses and executes nothing, pass 2 executes. A bad line never half-runs.
      </Text>

      {Step("line", "one typed line, e.g.   40000000: 1F 20 03 D5  R", "409", "91", "280", "82", "orange")}
      {Step("skip", "skip spaces", "409", "243", "280", "82", "grey")}

      {Choice("q_end", "end of\nthe line?", "427", "432")}
      {Step("finished", "line finished", "858", "482", "137", "57", "orange")}

      {Choice("q_hex", "hex digit\nnext?", "422", "705")}
      {Choice("q_store", "in storing\nmode?", "830", "703")}
      {Step("read_byte", "read two hex digits - a byte", "1322", "751", "259", "57", "yellow")}
      {Step("colon_mode", "switch into storing mode -\ndigits are bytes from now on", "0", "761", "269", "82", "blue")}

      {Choice("q_char", "which\ncharacter?", "420", "959")}
      {Step("read_addr", "read up to 16 hex digits - an address", "816", "1048", "280", "82", "grey")}

      {Choice("q_instr", "is next char instruction?", "837", "1234")}
      {Step("dot_range", "read the address \nprint current..end", "94", "1269", "266", "86", "blue")}
      {Step("r_jump", "jump to the current address", "438", "1271", "211", "82", "blue")}

      {Step("cmd_range", "ExamineContinuing ", "98", "1613", "268", "107", "light-green")}
      {Step("cmd_run", "Run", "415", "1610", "268", "107", "light-green")}
      {Step("cmd_set", "SetAddress - save address in memory", "712", "1608", "268", "107", "light-green")}
      {Step("cmd_one", "ExamineOne - print the byte at addr", "1006", "1609", "268", "107", "light-green")}
      {Step("cmd_store", "StoreContinuing - write the byte at address", "1318", "1609", "268", "107", "light-green")}

      <Edge from="line" to="skip" color="black" font="sans" size="s" fromSide="bottom" toSide="top" />
      <Edge from="skip" to="q_end" color="black" font="sans" size="s" toSide="top" />
      <Edge from="q_end" to="finished" label="yes" color="orange" font="sans" size="s" fromSide="right" toSide="left" />
      <Edge from="q_end" to="q_hex" label="no" color="black" font="sans" size="s" />

      <Edge from="q_hex" to="q_store" label="yes" color="black" font="sans" size="s" />
      <Edge from="q_store" to="read_byte" label="yes" color="yellow" font="sans" size="s" fromSide="right" toSide="left" />
      <Edge from="q_store" to="read_addr" label="no" color="black" font="sans" size="s" />
      <Edge from="read_byte" to="cmd_store" color="yellow" font="sans" size="s" fromSide="bottom" />

      <Edge from="q_hex" to="q_char" label="no" color="blue" font="sans" size="s" />
      <Edge from="q_char" to="colon_mode" label=":" color="blue" font="sans" size="s" bend="-2" toSide="0.74,0.5" />
      <Edge from="colon_mode" to="skip" color="blue" dash="dashed" font="sans" size="s" bend="143" fromSide="top" />
      <Edge from="q_char" to="dot_range" label="." color="blue" font="sans" size="s" bend="35" />
      <Edge from="q_char" to="r_jump" label="R" color="blue" font="sans" size="s" bend="-80" />
      <Edge from="dot_range" to="cmd_range" color="blue" font="sans" size="s" bend="-8" />
      <Edge from="r_jump" to="cmd_run" color="blue" font="sans" size="s" bend="1" fromSide="bottom" />

      <Edge from="read_addr" to="q_instr" color="black" font="sans" size="s" />
      <Edge from="q_instr" to="cmd_set" label="yes" color="black" font="sans" size="s" />
      <Edge from="q_instr" to="cmd_one" label="no" color="black" font="sans" size="s" />
    </Doc>
  );
}
