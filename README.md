# Sahou

Sahou is a **schema-first tool for building the interface layer between systems**.
You describe your messages and wiring once, in a schema. Sahou then lets your apps
talk to each other on the same LAN — **without hand-managing IP addresses or ports** —
and rejects wrong types and mistyped fields at the boundary, instead of letting a
mistake propagate and surface somewhere far away.

> "Sahou" (作法) means *proper form / etiquette* — the small set of conventions that
> let independent programs talk to each other comfortably and without mistakes.

## Why Sahou

- **No addresses, no ports.** On a shared LAN, nodes discover each other by name. You
  declare connections in a schema; Sahou handles discovery and transport for you
  (built on Zenoh).
- **Say NO early.** A wrong type or a mistyped field is rejected the moment it crosses
  the boundary — and, with generated type stubs, in your editor at build time — rather
  than failing later in the hardest place to debug.
- **One schema, many environments.** The same contract drives Rust, Python, Node.js,
  the browser, and TouchDesigner. The core is a neutral data model (an IR); the
  per-language runtimes are thin.

## Concepts

- **Node** — a participant: an app, a process, or a device.
- **Message** — a named, reusable shape of data (its fields and their types).
- **Connection** — the wiring: which Message, from which Node to which, and how it is
  carried.

The **contract** (types, names, wiring) lives in `schema.sahou.yaml`. The **endpoints**
(which machine plays which node) live in a separate `endpoints.<env>.yaml`, so one
contract runs unchanged across environments.

## Install

From source (installs the `sahou` CLI with the GUI embedded):

```bash
git clone https://github.com/craf-te/sahou
cd sahou
just install-full
```

`just install-full` builds the browser GUI and then runs `cargo install --path cli`.
To install without rebuilding the GUI, use `cargo install --path cli`.

## Quickstart

```bash
# 1. Scaffold a project (or write schema.sahou.yaml by hand; see examples/demo/).
sahou init

# 2. Validate the contract, with positional, structured diagnostics.
sahou validate

# 3. Build the IR — and, optionally, a typed stub for one node.
sahou gen --lang python --node sensor      # writes gen/descriptor.json (+ gen/sensor/ stub)

# 4. Observe what actually flows, from a node's vantage point.
sahou tap gen/descriptor.json --node sensor
```

Open the visual editor at any time:

```bash
sahou gui
```

## CLI

| Command | Purpose |
|---|---|
| `sahou init` | Scaffold a new project (a minimal valid seed) |
| `sahou validate` | Self-validate a contract, with positional, structured diagnostics |
| `sahou gen` | Build the full IR (`descriptor.json`); opt-in type stubs via `--lang`/`--node` |
| `sahou fmt` | Normalize a schema to canonical YAML (comments are not preserved) |
| `sahou check` | Detect stub ↔ IR drift (for CI) |
| `sahou tap` | Observe or inject traffic without an app |
| `sahou link` | Per-machine relay + WS entrypoint (Node/browser); usually spawned automatically |
| `sahou doctor` | Environment preflight diagnostics |
| `sahou gui` | Open the browser-based node editor |
| `sahou reference` | Print a schema-authoring reference for AI agents |
| `sahou licenses` | Print bundled third-party license notices |

## Runtimes

Thin, hand-written libraries that share the Rust core:

- **Python** — package `sahou` (PyPI). See [`runtimes/py/README.md`](runtimes/py/README.md).
- **Node.js / browser** — package `@sahou/runtime` (npm). See [`runtimes/ts/README.md`](runtimes/ts/README.md).

## TouchDesigner (experimental)

A native **Sahou Out CHOP** for TouchDesigner is included. It is **experimental and
macOS/arm64 only** today, and building it needs the TouchDesigner C++ SDK.
See [`td/README.md`](td/README.md).

## License

Apache-2.0. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).
