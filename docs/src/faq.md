# FAQ & troubleshooting

## Do I have to manage IP addresses and ports?

No. On a shared LAN, nodes discover each other by name. You declare connections
in the contract; Sahou handles discovery and transport (built on Zenoh). You only
name an explicit endpoint when the network requires it. See
[Networking & deployment](networking-and-deployment.md).

## What happens when a field has the wrong type?

It is rejected at the boundary. A send-boundary NO raises `SahouRejected` and
nothing is sent; a receive-boundary NO is routed to a reject handler, never to
your message handler. With generated type stubs, many mistakes are caught even
earlier — in your editor at build time. See
[Say NO early](concepts/say-no-early.md).

## Will a typo in a field name be silently ignored?

No. Unknown keys (typos) and duplicate keys are rejected, not silently dropped or
resolved "last-wins". Run `sahou validate` and fix the reported `{ code, path,
message }`.

## How do I get build-time type checking?

Generate a typed stub: `sahou gen --lang python` or `sahou gen --lang ts`. Import
the generated `connect` for node-name, connection-name, and payload types. The
engine behaves identically with or without it.

## How do I catch a stub that has drifted from the contract?

Run `sahou check gen/descriptor.json` — it compares the hashes embedded in a
generated stub against the current IR and rejects on mismatch. It is meant to run
in CI.

## Which languages and environments are supported?

Python (PyPI), TypeScript for Node and the browser (npm), the Rust core
(`sahou-core`; the app-facing `sahou` crate is a placeholder), and TouchDesigner
(experimental, macOS/arm64). See [Runtimes](runtimes/index.md).

## How do I see what is actually flowing, without writing an app?

Use `sahou tap gen/descriptor.json --node <name>` to observe (and, with `--send`,
inject) traffic from a node's vantage point.

## Something can't connect — where do I start?

Run `sahou doctor`. It probes loopback, ping, this binary's real Zenoh scout
(surfacing permission or NIC issues), and the link's WebSocket reachability.

## Can I evolve the contract while apps are running?

Yes, additively. Compatibility is judged per connection and structurally:
additive changes pass, breaking changes are refused, and the decision is made by
the delivery handshake. See [Contract evolution](concepts/contract-evolution.md).

## Can I edit the contract visually?

Yes — `sahou gui` opens a browser node editor that runs the same core (as
WebAssembly) as the CLI, so its diagnostics match `sahou validate`. See
[Visual editor (GUI)](gui.md).
