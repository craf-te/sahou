// tsc-only fixture (negative): a wrong node name, unknown connection, payload type mismatch, and a
// non-participating direction all become type errors the moment they are written.
import { connect } from "../../../../examples/demo/runtime/gen/sahou.gen.mjs";

export async function bad(): Promise<void> {
  const ghost = await connect("d", { node: "ghost" }); // unknown node name
  void ghost;
  const visuals = await connect("d", { node: "visuals" });
  await visuals.subscribe("nope", () => {}); // unknown connection
  await visuals.subscribe("touch", (p) => {
    const s: string = p.x; // type mismatch: x is number
    void s;
  });
  const sensor = await connect("d", { node: "sensor" });
  await sensor.subscribe("touch", () => {}); // non-participating direction: sensor has no subscribe
}
