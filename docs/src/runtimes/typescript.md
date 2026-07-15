# TypeScript (Node & browser)

The TypeScript runtime is [`sahou`](https://www.npmjs.com/package/sahou) on npm.
It has **two entry points** — `sahou` for Node.js and `sahou/browser` for the
browser. Validation, envelopes, and handshake verdicts are delegated to the
bundled Rust core (as WebAssembly), so diagnostics are byte-identical with Python
and TouchDesigner.

## Install

```bash
npm install sahou
```

## Usage (Node)

On Node, the runtime auto-spawns a [`sahou link`](../networking-and-deployment.md)
if none is running.

```ts
import { connect } from "sahou";

const node = await connect("gen/descriptor.json", { node: "visuals" });

await node.subscribe("touch", (p) => { /* ... */ }, {
  onReject: (code, detail) => { /* a boundary NO lands here, not in the handler */ },
});
await node.publish("cue", { /* ... */ });          // send-boundary NO → SahouRejected
const res = await node.queryConfirmed("get_state", { /* ... */ }, { retries: 3 });
await node.answer("get_state", async (req) => ({ level: 3 }));
await node.close();
```

## Usage (browser)

The browser cannot spawn a link; when one is not reachable, `connect` returns a
NO with the startup steps. See
[Networking & deployment](../networking-and-deployment.md) for the browser path
(a link on the LAN, or a BFF in front).

```ts
import { connect } from "sahou/browser";
const node = await connect(descriptorJson, { node: "visuals" });
```

## Typed connect (opt-in)

The base `connect` is untyped (`node`/`conn` are strings, payloads are
`unknown`). Generate a whole-descriptor typed layer and import its `connect` for
node-name, connection-name, and payload completion:

```bash
sahou gen schema.sahou.yaml --out-dir gen --lang ts                  # node target (default)
sahou gen schema.sahou.yaml --out-dir gen --lang ts --target browser
```

```ts
import { connect } from "./gen/sahou.gen.mjs";   // browser: generate with --target browser
const node = await connect("gen/descriptor.json", { node: "visuals" }); // node name completes
await node.subscribe("touch", (p) => { p.phase; });  // conn completes; p is typed
```

`sahou.gen.mjs` re-exports the real `connect` at zero runtime cost; `.d.mts`
supplies the types. The engine behaves identically without it, and `sahou check`
catches drift. A per-node stub (`--node <name>` → `typedNode()`) is also
available.

## Environment variables

- `SAHOU_LINK_CMD` — the executable to spawn as the link (default: `sahou` on
  PATH).
- `SAHOU_LINK_ARGS` — extra arguments when spawning (e.g. `--no-multicast --grace 4`).

See [`runtimes/typescript/README.md`](https://github.com/craf-te/sahou/blob/main/runtimes/typescript/README.md)
for development details.
