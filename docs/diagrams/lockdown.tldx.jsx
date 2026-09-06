import { Doc, Row, Text } from "tldx";

import { ImageMap } from "./lockdown/image-map.jsx";
import { Elsewhere } from "./lockdown/elsewhere.jsx";
import { Legend } from "./lockdown/legend.jsx";

export default function Diagram() {
  return (
    <Doc title="Lockdown: what each part of memory is allowed to do" layout="col" gap="90">
      <Text size="xl">Every byte was rw-x. Now each region gets only what it needs.</Text>
      <Row id="top" gap="110" align="start">
        <ImageMap ns="im" />
        <Elsewhere ns="el" />
        <Legend ns="lg" />
      </Row>
    </Doc>
  );
}
