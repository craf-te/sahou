# demo runtime example — py pub → node sub → browser display

Using a single contract (../schema.sahou.yaml), connect Python(native) → link → Node/browser(WS)
with zero IP configuration, and experience how a boundary NO produces the same diagnostics across all three.

## Prerequisites (one time only)
1. `cargo build --release -p sahou` (or `cargo install --path cli`)
2. `cd runtimes/ts && npm install && npm run build:core && npm run build`
3. Generate IR: `./target/release/sahou gen examples/demo/schema.sahou.yaml --out-dir examples/demo/runtime/gen`
   (In ②d the output location was reorganized under `gen/`. Old layout: `descriptor.json` directly in cwd. Delete any leftover old files.)
4. `cd examples/demo/runtime && npm install` / `cd browser && npm install`

## Running
- Terminal A: `cd examples/demo/runtime && node --experimental-wasm-modules node_sub.mjs`
  (Auto-spawns link if it isn't running. The executable is `sahou` on PATH or `SAHOU_LINK_CMD`.
  `--experimental-wasm-modules` is required on the Node side because zenoh-ts imports the core wasm as an ESM module
  — a real constraint discovered in ②b Task 7. vitest adds it automatically via execArgv, but a bare `node` run needs it set manually.)
- Terminal B: `cd examples/demo/runtime && node --experimental-wasm-modules node_state.mjs`
  (Responds to get_state as the archive. Aggregates touch and returns a test response to py_pub's query_confirmed.
  Stop this terminal to experience the "delivery unconfirmed" NO on the py_pub side.)
- Terminal C: `cd runtimes/py && SAHOU_CONNECT="tcp/[::1]:7448" uv run python ../../examples/demo/runtime/py_pub.py`
  (In environments where multicast is unavailable, add `SAHOU_CONNECT=tcp/[::1]:7448`. On Windows the link's peer_listen
  is IPv6-only, so use the **IPv6 loopback `[::1]`** rather than `127.0.0.1`.)
- Browser: `cd examples/demo/runtime/browser && npm run dev` → http://localhost:5173

## Operational tooling (②c: tap / doctor)

Peek at, poke, and diagnose the field without writing an app. Inspection uses the same core functions as the engine, so diagnostics are byte-identical.

- Environment check (run this first, before setup): `sahou doctor`
  (On failure, returns a NO with a cause-specific remedy. If scout gets lost on a multi-NIC host, use `--iface <NIC>`.)
- Peek: `cd examples/demo/runtime && sahou tap gen/descriptor.json --node visuals --connect "tcp/[::1]:7448"`
  (Loopback operation while link is running. If the LAN supports multicast, `--connect` is not needed.
   On each receive, prints `[touch] #seq OK/NO key payload/diagnostic`. Corruption surfaces as the core's receive-boundary NO verbatim.)
- Poke: `cd examples/demo/runtime && sahou tap gen/descriptor.json --send touch --sample --connect "tcp/[::1]:7448"`
  (Sends a valid sample from the core's sample_slot. Reaches node_sub/browser even when py_pub is absent.
   `--send get_state --sample` pokes the query and shows the response/diagnostic.)

## Type stubs and drift detection (②d: main track ②)

Generate a **consumer-side local type stub** from the contract to enable IDE completion + compile-time checking (opt-in;
runtime behavior is unchanged = it runs identically without the stub).

- Generate (Python): `sahou gen examples/demo/schema.sahou.yaml --out-dir examples/demo/runtime/gen --lang python --node sensor`
  → `gen/sensor/sahou_stub.py` + `.pyi`. Usage: `node = typed_node(sahou.connect(...))` enables connection-name completion
  and payload type checking on `node.publish("touch", {...})`.
- Generate (TS): `... --lang ts --node visuals` → `gen/visuals/sahou_stub.mjs` + `.d.mts`.
  `const node = typedNode(await connect(...))` infers the handler argument types.
- Drift detection (for CI): `sahou check examples/demo/runtime/gen/descriptor.json --gen-dir examples/demo/runtime/gen`
  → compares the stub's embedded hash against the IR. Changing the contract and regenerating only the IR produces a `stub_hash_drift` NO
  (= a sign that the stub should be regenerated). **The engine does not read the stub** (drift detection is a CLI/CI responsibility; design §8/§13).
  (The default for `--gen-dir` is `gen` = relative to cwd. When running from the repo root, spell out the full path as above.
   Once you have `cd examples/demo/runtime`, `sahou check gen/descriptor.json` passes with the defaults.)

## Trust boundary (assumptions of this demo)

This direct-connect setup (browser → link WS direct connection) targets a **trusted LAN** (design §6, Z27).
**Do not expose the link WS to untrusted/public browsers** — for that use case, a BFF (a server-side node runtime connects
to link, and the browser goes through an app-authenticated API) is the primary recommendation (production spec §12.1; implementation deferred).

## Manual checklist (the browser verification of ②b is covered here; extended in ②c)
- [ ] `touch { x: ... }` keeps flowing into node_sub (Py→Node, zero IP configuration)
- [ ] py_pub prints a "send-boundary NO" every 50 messages (the corrupt payload is not put)
- [ ] The browser displays the latest touch value (Py→browser)
- [ ] Stopping link and reloading the browser shows `link_unavailable` (a NO with startup instructions)
- [ ] After all processes stop, link self-terminates in ~15 seconds (`netstat -ano | findstr :10000` is empty)
- [ ] While node_state runs: py_pub prints `get_state -> {'level': N}` every 100 messages (query 4 boundaries + delivery confirmation)
- [ ] While node_state is stopped: py_pub prints a "delivery unconfirmed" NO and recovers on restart (a retryable is not misfired as fatal)
- [ ] `sahou tap ... --node visuals` streams OK lines for touch (peek without an app)
- [ ] `sahou tap ... --send touch --sample` reaches node_sub / browser (poke without an app)
- [ ] `sahou doctor` reports healthy / `sahou doctor --iface nonexist99` returns a NIC NO with a remedy
- [ ] `sahou gen ... --lang python --node sensor` emits `gen/sensor/sahou_stub.{py,pyi}`
- [ ] Change the contract → regenerate only the IR → `sahou check` emits `stub_hash_drift` (prompting stub regeneration)
</content>
</invoke>
