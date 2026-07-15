# TouchDesigner

> **Status: experimental.** The Sahou Out CHOP is currently **macOS / arm64
> only**. Building it requires the TouchDesigner C++ SDK vendored into
> `runtimes/touchdesigner/vendor/` (Derivative Shared Use License — copy it from
> your local TouchDesigner install; it is not redistributed here).

Sahou treats TouchDesigner as a first-class node: you send and receive over the
typed contract from custom C++ operators, **without going through TD's Python** —
a thin C++ glue plus the statically-linked Rust core.

## Sahou Out CHOP

- Reads the input CHOP's channels, projects them to a JSON payload, and runs the
  **send boundary** through the Rust core. On a contract violation the node goes
  red with the structured diagnostic — "say NO in the right place", the same
  boundary behavior as every other runtime.
- **Test Send** — a pulse that publishes one IR-valid sample of the selected
  connection over Zenoh (via the bundled transport), for a quick connectivity
  check with `sahou tap`.

## Not yet

Continuous per-frame send, a Sahou In CHOP, DAT operators, a Windows build, a
universal (Intel) binary, and distribution signing are future work.

## Building

The C++ op source, the tests, the Rust transport, and the examples are shared
across platforms; only the per-platform build projects differ (Xcode → `.plugin`
on macOS). See
[`runtimes/touchdesigner/README.md`](https://github.com/craf-te/sahou/blob/main/runtimes/touchdesigner/README.md)
for the SDK prerequisite and the `just build-td-macos` / `just test-td` tasks.
