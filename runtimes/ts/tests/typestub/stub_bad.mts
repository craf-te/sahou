// tsc-only fixture (negative): type mismatches / unknown connections / non-participating directions become type errors.
import { typedNode } from "../../../../examples/demo/runtime/gen/visuals/sahou_stub.mjs";

export async function use(nodeObj: unknown): Promise<void> {
  const node = typedNode(nodeObj);
  await node.subscribe("touch", (p) => {
    const s: string = p.x; // type mismatch: x is number
    void s;
  });
  await node.subscribe("ghost", () => {}); // unknown connection name
  // a non-participating direction does not exist in the type: visuals has no publish
  await node.publish("touch", { x: 0.5, phase: "move", meta: { ts: 0 } });
}
