# Sahou　(Alpha version)

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

### `sahou` CLI (the tool)

```bash
# From crates.io (needs Rust):
cargo install sahou-cli        # installs the `sahou` command (GUI embedded)
```

Or grab a prebuilt binary — **no Rust needed** (macOS / Linux):

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/craf-te/sahou/releases/latest/download/sahou-cli-installer.sh | sh
```

On Windows (PowerShell): `irm https://github.com/craf-te/sahou/releases/latest/download/sahou-cli-installer.ps1 | iex`.
Prebuilt binaries for macOS (arm64/x64), Linux (arm64/x64), and Windows (x64) — with the GUI
embedded — are on the [releases page](https://github.com/craf-te/sahou/releases). From source:
`git clone … && just install-full`.

### Runtime library (to build apps)

Add the runtime for your language, then send and receive over a contract:

```bash
npm install sahou     # Node.js & browser
pip install sahou     # Python
```

(A Rust runtime — `cargo add sahou` — is reserved and coming; for now build on the core, `sahou-core`.)

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

Thin, hand-written libraries that share the Rust core (install commands above):

- **Node.js / browser** — [`sahou`](https://www.npmjs.com/package/sahou) on npm. See [`runtimes/typescript/README.md`](runtimes/typescript/README.md).
- **Python** — [`sahou`](https://pypi.org/project/sahou/) on PyPI. See [`runtimes/python/README.md`](runtimes/python/README.md).
- **Rust** — [`sahou`](https://crates.io/crates/sahou) on crates.io (placeholder for now); the core is [`sahou-core`](https://crates.io/crates/sahou-core).

## TouchDesigner (experimental)

A native **Sahou Out CHOP** for TouchDesigner is included. It is **experimental and
macOS/arm64 only** today, and building it needs the TouchDesigner C++ SDK.
See [`runtimes/touchdesigner/README.md`](runtimes/touchdesigner/README.md).

## License

Apache-2.0. See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE).
