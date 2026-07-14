# @sahou/runtime

The TypeScript runtime for Sahou (two entry points: node / browser). Validation, envelopes, and handshake
verdicts are all delegated to the bundled Rust core (wasm), so diagnostics are **byte-identical** with Python / TD.

## Usage

```ts
// Node (auto-spawns a link if none is running)
import { connect } from "@sahou/runtime";
const node = await connect("descriptor.json", { node: "visuals" });
// (The path is up to you. To use the output of `sahou gen --out-dir gen`, pass "gen/descriptor.json".)

// browser (cannot spawn; when not connected, returns a NO with startup steps)
import { connect } from "@sahou/runtime/browser";
const node = await connect(descriptorJson, { node: "visuals" });

await node.subscribe("touch", (p) => { ... }, { onReject: (c, d) => { ... } });
await node.publish("cue", { ... });                     // send-boundary NO → SahouRejected
const res = await node.queryConfirmed("get_state", { ... }, { retries: 3 });
await node.answer("get_state", async (req) => ({ level: 3 }));
await node.close();
```

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
