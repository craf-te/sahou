# Visual editor (GUI)

Sahou ships a browser-based node editor. Open it on any contract:

```bash
sahou gui [PATH] [--env dev] [--port 4649] [--no-open]
```

By default it opens an app-mode window automatically; pass `--no-open` to
suppress that. It listens on `127.0.0.1` only, and if the port is occupied it
stops rather than silently switching.

## What it does

- **Edit the contract visually** — nodes and connections as a graph, with a
  details panel for messages and fields.
- **Same validation as the CLI** — all contract interpretation happens in the
  in-browser core, compiled to WebAssembly. The Rust backend only moves raw
  bytes; it does not re-implement any validation. So a NO in the GUI is the same
  NO you get from `sahou validate`.
- **Live with your files** — the GUI reads and writes `schema.sahou.yaml`,
  `endpoints.<env>.yaml`, and `layout.sahou.json` in `PATH`. Edits you make in
  your text editor and edits you make in the GUI stay in sync.

Because the GUI runs the exact same core as the CLI and the runtimes, there is no
second implementation to drift: the graph you see, the diagnostics it shows, and
the bytes on the wire all come from one source of truth (the
[IR](concepts/ir-and-round-trip.md)).
