# The IR and round-trip

At the center of Sahou is a neutral data model called the **IR** (intermediate
representation). The surface you write (YAML) is never wired directly to the
output (codegen, transport, the GUI). The IR always sits in between.

```text
schema.sahou.yaml  ──parse──▶   IR   ──serialize──▶  canonical YAML
                                 │
                                 ├──▶  descriptor.json (the full IR)
                                 ├──▶  typed stubs (opt-in)
                                 └──▶  discovery + transport
```

## Why an IR

Wiring surface syntax straight to output couples the two: every consumer would
have to re-parse YAML and re-implement validation, and they would drift. With the
IR as the single neutral model, parsing, validation, and every output share one
source of truth. The CLI, the browser GUI, and the language runtimes all consume
the same IR.

## The three properties the core guarantees

The core (`sahou-core`, written in Rust and WASM-capable) keeps three properties
so the same logic can back a CLI, a browser GUI, and future tooling:

1. **Parse *and* serialize** — reading a contract into the IR and writing it back
   out is round-trip stable. (Comments are the one known exception; `sahou fmt`
   notes this.)
2. **Structured diagnostics** — validation returns positional, structured
   results: a `code`, a `path` to the exact location, and a `message`. Not a
   single opaque error string. See [Say NO early](say-no-early.md).
3. **Pure and WASM-capable** — parse / serialize / validate are pure functions
   with no I/O, so they run identically in the CLI and in the browser (the GUI
   runs the core compiled to WebAssembly).

Everything under `gen/` — `descriptor.json` (the full IR) and any typed stubs —
is produced from the IR by `sahou gen`. Treat it as generated output; edit the
contract, not `gen/`.
