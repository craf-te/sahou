# CLI reference

The `sahou` binary is a single command with subcommands. Run `sahou <cmd> --help`
for the exact, version-specific flags; this page summarizes each command.

## `sahou init`

Scaffold a new project — a minimal valid seed to start from an empty directory.

```bash
sahou init [DIR] [--name <name>] [--force]
```

- positional `DIR` — target directory (default: current; created if missing).
- `--name <name>` — schema name, also the namespace (default: the directory's
  basename).
- `--force` — overwrite even if a `schema.sahou.yaml` already exists.

## `sahou validate`

Self-validate a contract with positional, structured diagnostics.

```bash
sahou validate schema.sahou.yaml
```

On success it prints `[ok] <file>: valid`. On failure it prints one
`[code] @path: message` line per diagnostic and exits non-zero. See the
[diagnostic codes](schema-authoring.md#diagnostic-codes).

## `sahou fmt`

Normalize a contract to deterministic canonical YAML.

```bash
sahou fmt schema.sahou.yaml           # print to stdout
sahou fmt schema.sahou.yaml --write   # overwrite in place
```

- `--write` — overwrite the file (prints to stdout when omitted).
- Comments are **not** preserved (a known limitation); `--write` warns when the
  input contained comments.

## `sahou gen`

Turn a contract + endpoints into the full IR, and optionally typed stubs.

```bash
sahou gen                                   # IR only → gen/descriptor.json
sahou gen --lang python                     # + whole-descriptor typed layer
sahou gen --lang ts --target browser        # TS layer for the browser transport
sahou gen --lang python --node sensor       # + per-node stub → gen/sensor/
```

- positional `schema` — the contract file (default `schema.sahou.yaml`).
- `--endpoints <file>` — an `endpoints.<env>.yaml` (default: the LAN-auto
  default, namespace `sahou`).
- `--out-dir <dir>` — output directory (default `gen`); the IR is written to
  `<out-dir>/descriptor.json`.
- `--lang python|ts` — generate a typed layer (opt-in). Alone → a
  whole-descriptor typed module; with `--node` → a per-node stub under
  `<out-dir>/<node>/`.
- `--node <name>` — generate a per-node stub (requires `--lang`).
- `--target node|browser` — TS transport for the whole-descriptor stub
  (`--lang ts` without `--node`); `node` is the default.

See [Runtimes](runtimes/index.md) for how to consume the generated stubs.

## `sahou check`

Detect stub ↔ IR drift, for CI. It compares the hashes embedded in a generated
stub against the current descriptor and rejects on mismatch.

```bash
sahou check gen/descriptor.json               # all node stubs under gen/
sahou check gen/descriptor.json --node sensor # just one node
```

- positional `descriptor` — the full IR (`descriptor.json`).
- `--gen-dir <dir>` — the gen output directory to scan (default `gen`).
- `--node <name>` — check only this node's stub.

## `sahou tap`

Observe or inject traffic without an app: subscribe and show the core's
validation results; `--send` publishes a valid sample.

```bash
sahou tap gen/descriptor.json --node sensor
```

## `sahou link`

One relay per machine plus a WebSocket entrypoint for Node/browser runtimes
(built on Zenoh's remote-api). **The engine spawns it automatically** — you
rarely run it by hand. See [Networking & deployment](networking-and-deployment.md).

## `sahou doctor`

Environment preflight diagnostics: probe loopback, ping, this binary's real
Zenoh scout, and the link WebSocket.

```bash
sahou doctor
```

## `sahou gui`

Open the browser-based node editor on localhost. All contract interpretation
happens in the in-browser core (compiled to WebAssembly); the backend only moves
raw bytes. See [Visual editor (GUI)](gui.md).

```bash
sahou gui [PATH] [--env dev] [--port 4649] [--no-open]
```

- positional `PATH` — target directory (default: current); it handles
  `schema.sahou.yaml` / `layout.sahou.json` / `endpoints.<env>.yaml`.
- `--env <name>` — the endpoints environment (default `dev`).
- `--port <n>` — listen port, `127.0.0.1` only (default `4649`); if the port is
  occupied it stops rather than silently switching.
- `--no-open` — do not open a browser on startup (by default an app-mode window
  opens automatically).

## `sahou reference`

Print a compact schema-authoring reference intended for AI coding agents.

```bash
sahou reference
```

## `sahou licenses`

Print the bundled third-party license notices (retained for redistribution).

```bash
sahou licenses
```
