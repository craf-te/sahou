// tsc-only fixture (positive): the generated stub's facade / interface passes clean. Not executed.
import { SCHEMA_HASHES, typedNode, type Touch } from "../../../../examples/demo/runtime/gen/visuals/sahou_stub.mjs";

export async function use(nodeObj: unknown): Promise<void> {
  const node = typedNode(nodeObj);
  await node.subscribe("touch", (p) => {
    const x: number = p.x; // the handler argument is inferred as Touch
    const phase: "down" | "move" | "up" = p.phase;
    const source: string | undefined = p.meta.source; // required:false → optional
    void x;
    void phase;
    void source;
  });
  await node.subscribe("points", (p) => {
    const first: number = p.pts[0][0];
    void first;
  });
  const t: Touch = { x: 0.5, phase: "move", meta: { ts: 0 } };
  const h: string = SCHEMA_HASHES["touch"];
  void t;
  void h;
  await node.close();
}
