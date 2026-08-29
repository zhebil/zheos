import { Doc, Text } from "tldx";

import { AddressSpace } from "./memory/address-space.jsx";
import { Splitting } from "./memory/splitting.jsx";
import { Arena } from "./memory/arena.jsx";
import { Units } from "./memory/units.jsx";
import { ListBuddy } from "./memory/list-buddy.jsx";
import { ListPartial } from "./memory/list-partial.jsx";
import { ListSlots } from "./memory/list-slots.jsx";
import { Lists } from "./memory/lists.jsx";

export default function Diagram() {
  return (
    <Doc title="Memory: addresses, Pfns and the three linked lists" layout="col" gap="180">
      <Text size="xl">Where a byte lives, what number names it, and which list it is on</Text>

      <AddressSpace ns="as" />
      <Splitting ns="sp" />
      <Arena ns="ar" />
      <Units ns="un" />
      <ListBuddy ns="lb" />
      <ListPartial ns="lp" />
      <ListSlots ns="lt" />
      <Lists ns="ls" />
    </Doc>
  );
}
