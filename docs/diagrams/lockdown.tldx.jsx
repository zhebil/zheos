import { Doc, Row, Text, Edge } from "tldx";

import { Kinds } from "./lockdown/kinds.jsx";
import { Ram, Image } from "./lockdown/map.jsx";

export default function Diagram() {
  return (
    <Doc title="Lockdown: colour is the permission" layout="col" gap="60">
      <Text size="xl">Every byte used to be readable, writable and executable. Now the colour is the rule.</Text>
      <Kinds ns="lk" />
      <Row id="maps" gap="170" align="start">
        <Ram ns="lk" />
        <Image ns="lk" />
      </Row>
      <Edge from="lk-r-img" to="lk-img" fromSide="right" toSide="top-left" dash="dashed" label="zoom" />
    </Doc>
  );
}
