# Quickstart

This walkthrough takes you from zero to a validated contract and a message
flowing on your LAN in about 15 minutes. You will install the `sahou` CLI,
look at a contract, validate it, build its IR, and observe traffic — no app
code required.

## 1. Install the CLI

From crates.io (needs Rust):

```bash
cargo install sahou-cli        # installs the `sahou` command (GUI embedded)
```

Or grab a prebuilt binary — **no Rust needed**:

```bash
# macOS / Linux
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/craf-te/sahou/releases/latest/download/sahou-cli-installer.sh | sh
```

```powershell
# Windows (PowerShell)
irm https://github.com/craf-te/sahou/releases/latest/download/sahou-cli-installer.ps1 | iex
```

Prebuilt binaries for macOS (arm64/x64), Linux (arm64/x64), and Windows (x64) —
with the GUI embedded — are on the
[releases page](https://github.com/craf-te/sahou/releases).

## 2. Get a contract

Scaffold a fresh project (a minimal valid seed):

```bash
sahou init
```

Or follow along with the bundled demo, whose contract lives in
`examples/demo/schema.sahou.yaml`. A contract names its **nodes** and the
**connections** between them. Here is one connection from the demo — a
publish/subscribe stream of typed touch events from `sensor` to `visuals` and
`archive`:

```yaml
schema: demo_installation
version: 1
nodes:
  sensor:  { kind: sahou }
  visuals: { kind: sahou }
  archive: { kind: sahou }
connections:
  touch:
    pattern: pub_sub
    from: sensor
    to: [visuals, archive]
    payload:
      typing: typed
      fields:
        - { name: x, type: float, min: 0, max: 1 }
        - { name: phase, type: enum, values: [down, move, up] }
        - name: meta
          type: group
          fields:
            - { name: ts, type: timestamp }
            - { name: source, type: string, required: false }
    reliability: reliable
    congestion: block
```

You do not manage any IP addresses or ports here — only *who* talks to *whom*
and *what shape* the messages are.

## 3. Contract vs endpoints

The **contract** (types, names, wiring) lives in `schema.sahou.yaml`. The
**endpoints** — which machine plays which node, and any environment-specific
transport settings — live in a separate `endpoints.<env>.yaml`, so one contract
runs unchanged across environments. The demo's `endpoints.dev.yaml` is tiny,
because LAN auto-discovery is the default:

```yaml
env: dev
namespace: sahou/demo
# LAN auto-discovery is the default. Only specify a router / explicit endpoint when needed.
```

See [Contract vs Endpoints](concepts/contract-vs-endpoints.md) for the full
picture.

## 4. Validate the contract

```bash
sahou validate schema.sahou.yaml
```

Validation returns **positional, structured diagnostics** — it points at the
exact field or connection, and duplicate keys or mistyped (unknown) keys are
rejected rather than silently dropped. See [Say NO early](concepts/say-no-early.md).

## 5. Build the IR (and an optional typed stub)

```bash
sahou gen --lang python --node sensor
```

This writes the full IR to `gen/descriptor.json` and, because you passed
`--lang`/`--node`, a typed stub for the `sensor` node into `gen/sensor/`. The
stub gives your editor build-time type checking for the messages that node
sends and receives — this is the "say NO in your editor" half of Sahou. Drift
between a stub and the contract is caught later by `sahou check`.

## 6. Observe what actually flows

You do not need an app to see traffic. `sahou tap` can observe (and inject)
messages from a node's vantage point:

```bash
sahou tap gen/descriptor.json --node sensor
```

To build real senders and receivers, add a runtime for your language — see
[Runtimes](runtimes/index.md).

## 7. Open the visual editor

At any point, edit the contract visually in your browser:

```bash
sahou gui
```

## Next steps

- **[Concepts](concepts/index.md)** — nodes, messages, connections, and the IR.
- **[Schema authoring](schema-authoring.md)** — every field type and connection
  option.
- **[CLI reference](cli-reference.md)** — every command and flag.
