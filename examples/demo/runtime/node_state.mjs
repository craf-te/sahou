// Responds to get_state as the demo contract's archive (a demo of the 4 query boundaries; also aggregates touch).
// Run: cd examples/demo/runtime && node --experimental-wasm-modules node_state.mjs
// Uses the generated whole-descriptor typed connect (gen/sahou.gen.mjs): `answer`/`subscribe` and the
// request/response payloads are typed for node "archive". It re-exports the real connect (runtime-identical).
import { fileURLToPath } from "node:url";
import { connect } from "./gen/sahou.gen.mjs";

const desc = fileURLToPath(new URL("./gen/descriptor.json", import.meta.url));
const node = await connect(desc, { node: "archive" });
console.log("[node_state] connected as archive (responds to get_state, aggregates touch)");
let count = 0;
await node.subscribe("touch", () => {
  count += 1;
});
await node.answer("get_state", (req) => {
  console.log("[node_state] get_state", req);
  return { level: count % 10 }; // matches the response slot (level: int)
});
process.on("SIGINT", async () => {
  await node.close();
  process.exit(0);
});
