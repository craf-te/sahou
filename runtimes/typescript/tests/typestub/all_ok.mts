// tsc-only fixture (positive): the whole-descriptor typed connect gives node-name completion, per-node
// connection completion, and payload inference from a single import. Not executed.
import { connect, type Touch } from "../../../../examples/demo/runtime/gen/sahou.gen.mjs";

export async function useVisuals(): Promise<void> {
  // node name is completed to the descriptor's sahou nodes; returns the VisualsNode facade
  const node = await connect("gen/descriptor.json", { node: "visuals" });
  await node.subscribe("touch", (p) => {
    const x: number = p.x; // handler argument inferred as Touch
    const phase: "down" | "move" | "up" = p.phase;
    const source: string | undefined = p.meta.source; // required:false → optional
    void x;
    void phase;
    void source;
  });
  await node.subscribe("debug_tap", (p) => {
    void p; // typing:any → unknown
  });
  await node.close();
}

export async function useSensor(): Promise<void> {
  const node = await connect("gen/descriptor.json", { node: "sensor" });
  const t: Touch = { x: 0.5, phase: "move", meta: { ts: 0 } };
  await node.publish("touch", t);
  const resp = await node.queryConfirmed("get_state", { sel: "levels" });
  const level: number = resp.level; // response typed as GetStateResponse
  void level;
  await node.close();
}
