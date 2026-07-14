// Subscribes to touch as the demo contract's visuals (auto-spawns link if it isn't running).
// Run: cd examples/demo/runtime && npm install && node --experimental-wasm-modules node_sub.mjs
// Uses the generated whole-descriptor typed connect (gen/sahou.gen.mjs), so `node` completes to a node
// name and `subscribe`/payloads are typed in an editor. It re-exports the real connect (runtime-identical).
import { fileURLToPath } from "node:url";
import { connect } from "./gen/sahou.gen.mjs";

const desc = fileURLToPath(new URL("./gen/descriptor.json", import.meta.url));
const node = await connect(desc, { node: "visuals" });
console.log("[node_sub] connected as visuals (link is auto-spawned, self-terminates once everyone leaves)");
node.onReject((conn, diags) => console.warn(`[node_sub] boundary NO on ${conn}:`, diags));
await node.subscribe("touch", (p) => console.log("[node_sub] touch", p));
process.on("SIGINT", async () => {
  await node.close();
  process.exit(0);
});
