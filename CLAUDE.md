# CLAUDE.md — Sahou

Instructions and constraints for working in this repository. Follow this file in
addition to (and with priority over) any global config.

## Language policy

Repository artifacts — source, comments, documentation, and UI text — are
**English-first**. Conversation with the maintainer stays in **Japanese**.

## What Sahou is

Sahou is a **schema-first tool for building the interface layer between systems**. A
schema describes the messages and the wiring; apps on the same LAN then talk to each
other **without hand-managing IP addresses or ports**, and wrong types / mistyped
fields are rejected at the boundary (and, via opt-in generated stubs, at build time)
rather than propagating. The transport is built on Zenoh.

## Repository layout

- `core/` — the Rust core: the IR (a neutral data model) plus parse / serialize /
  validate. WASM-capable. C ABI available behind the `capi` feature.
- `cli/` — the `sahou` binary (`gen` / `validate` / `check` / `fmt` / `tap` / `gui` /
  `licenses`). Embeds the built GUI via rust-embed.
- `gui/` — the browser node editor (Vue + G6), running the core as wasm.
- `runtimes/` — thin, hand-written language runtimes, one directory each: `python`
  (`sahou`, PyPI), `typescript` (`sahou`, npm), `rust` (`sahou`, crates.io — placeholder),
  and `touchdesigner` (see below).
- `examples/` — a runnable demo.
- `runtimes/touchdesigner/` — the TouchDesigner Sahou Out CHOP (C++/Rust). Experimental, macOS/arm64.
  `runtimes/touchdesigner/transport/` is the `sahou-transport` cdylib (a C ABI over Zenoh).

## Toolchain and tasks

- The main body is **Rust** (a single binary `sahou`; WASM-capable so the GUI and the
  core are shared).
- Tasks are collected in the `justfile`. Common ones:
  - `just build` / `just test` — build / test the Cargo workspace.
  - `just gui-build` — build the wasm core + Vue app into `gui/dist` (what the CLI embeds).
  - `just install-full` — `gui-build` then install the CLI with the fresh GUI.
  - `just gen-demo` — regenerate the committed demo IR + stubs (guarded by a freshness test).
  - `just licenses` — regenerate the bundled third-party notice (needs `cargo install cargo-about --features cli`).
  - `just build-ffi` — build the static lib + regenerate `core/sahou.h` (needs `cbindgen`).
  - `just build-td-macos` / `just test-td` — build/test the TouchDesigner plugin (macOS; needs the TD SDK vendored into `runtimes/touchdesigner/vendor/`).
- On this machine, `cc` is a shell alias — for C/C++ compilation use `/usr/bin/cc` / `/usr/bin/c++` explicitly.

## Releasing

Each channel ships independently via a prefixed git tag (`v*` → CLI + crates.io,
`py-v*` → PyPI, `npm-v*` → npm). Full step-by-step, version policy, and the
required secrets are in [RELEASING.md](RELEASING.md).

## Non-negotiable design invariants

- **The core is the IR.** Never wire the surface (YAML) directly to output (codegen);
  the IR sits in between.
- **The IR model is Node / Message / Connection** (types are separate from wiring; a
  Field belongs to a Message; the transport kind is a Connection attribute; endpoints
  live in a separate file).
- **Separate the contract from the endpoints** (`schema.sahou.yaml` / `endpoints.<env>.yaml`).
- The core keeps **three properties for a future GUI/AI**: ① parse *and* serialize
  (round-trip stable); ② validate returns positional, structured diagnostics; ③
  parse/serialize/validate stay pure and WASM-capable.
- **Never silently fold the contract:** duplicate keys and unknown keys (typos) are a
  boundary NO (`unique-map` + `deny_unknown_fields`), not serde's silent last-wins/drop.
- **Runtime = engine + runtime interpretation of the IR.** Static types are opt-in,
  consumer-side generated stubs; stub↔IR drift is caught by `sahou check`.
- **Contract evolution:** per-connection + structural compatibility (additive passes,
  breaking is a NO); the delivery handshake makes the compatibility decision.
- **transport / encoding are adapter-swappable** (the contract and IR are
  transport-independent). Encoding is a Connection attribute (default JSON;
  large numeric arrays are binary opt-in).

## Working notes

- Keep repo artifacts in English; converse with the maintainer in Japanese.
- Ask before running destructive commands (`rm` / `git reset` / `git push`).
- Do not launch costly multi-agent processing unless the maintainer explicitly asks for it.

## License

Apache-2.0. Sahou depends on Zenoh (dual EPL-2.0 / Apache-2.0), used here under
Apache-2.0. See `LICENSE` and `NOTICE`.
