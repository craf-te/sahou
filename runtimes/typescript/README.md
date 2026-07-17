# sahou

The TypeScript runtime for Sahou (two entry points: node / browser). Validation, envelopes, and handshake
verdicts are all delegated to the bundled Rust core (wasm), so diagnostics are **byte-identical** with Python / TD.

## Usage

```ts
// Node (auto-spawns a link if none is running)
import { connect } from "sahou";
const node = await connect("descriptor.json", { node: "visuals" });
// (The path is up to you. To use the output of `sahou gen --out-dir gen`, pass "gen/descriptor.json".)

// browser (cannot spawn; when not connected, returns a NO with startup steps)
import { connect } from "sahou/browser";
const node = await connect(descriptorJson, { node: "visuals" });

await node.subscribe("touch", (p) => { ... }, { onReject: (c, d) => { ... } });
await node.publish("cue", { ... });                     // send-boundary NO → SahouRejected
const res = await node.queryConfirmed("get_state", { ... }, { retries: 3 });
await node.answer("get_state", async (req) => ({ level: 3 }));
await node.close();
```

## Typed connect (opt-in, generated)

The base `connect` above is untyped (`node`/`conn` are strings, payloads are `unknown`). To get editor
completion for **node names**, per-node **connection names**, and **payload types** from the contract, generate
a whole-descriptor typed layer and import its `connect` instead:

```sh
sahou gen schema.sahou.yaml --out-dir gen --lang ts            # node target (default)
sahou gen schema.sahou.yaml --out-dir gen --lang ts --target browser
```

```ts
import { connect } from "./gen/sahou.gen.mjs";           // browser: generate with --target browser
const node = await connect("gen/descriptor.json", { node: "visuals" }); // node name completes
await node.subscribe("touch", (p) => { p.phase; });      // conn completes; p is typed (Touch)
```

`sahou.gen.mjs` re-exports the real `connect` (zero runtime cost); `sahou.gen.d.mts` supplies the types (one
`connect` overload per node → the correct facade). The engine behaves identically without it. `sahou check`
detects stub↔IR drift. A per-node stub (`sahou gen --lang ts --node <name>` → `typedNode()`) is also available.

## Vitals (node self-report)

Each connected node declares, by default, a liveliness token and a small self-report
queryable at `<namespace>/@sahou/vitals/<node>` — its identity, schema generation
(per-connection hashes), runtime versions, uptime, and cached handshake verdicts.
The `sahou doctor --lan` roll call uses these. Both entries declare vitals; the browser
entry reports no zenoh library version (not discoverable in a browser — omitted, not faked).

```ts
const node = await connect("descriptor.json", { node: "sensor", vitals: false }); // opt out
```

**Exposure note, honestly:** the transport carries no authentication or encryption, so
*any peer on the same LAN can read a node's vitals* — just as it can already read the
full contract from the contract queryable. Vitals report only state the engine holds
anyway (versions, hashes, verdicts); if that is still too much for your network, opt
out with `vitals: false`.

## Environment variables
- `SAHOU_LINK_CMD`: the executable to spawn as the link (default: `sahou` on PATH)
- `SAHOU_LINK_ARGS`: extra arguments when spawning (e.g. `--no-multicast --grace 4`)

## Development
- `npm run build:core` — generate the wasm core for both targets (node/browser) (requires wasm-pack)
- `npm run build` — tsc / `npm test` — vitest (integration tests require `cargo build -p sahou`)

## Known upstream limitations (tracked; outside the ②c completion criteria)

- zenoh-ts WS reconnect loop: `Session.open` against an unreachable locator retries indefinitely inside zenoh-ts
  (`RemoteLink.new` fails to increment retries). `openSessionWithTimeout` in `src/session.ts` only cuts off the
  "wait"; it cannot stop the loop itself. Do not hammer `connect()` in a tight loop. Re-evaluate the timeout
  handling once an upstream fix lands.
