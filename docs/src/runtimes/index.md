# Overview

A runtime is the small, hand-written library your app imports to send and receive
messages over a Sahou contract. Every runtime is **thin**: validation, envelopes,
and handshake verdicts are delegated to the shared Rust core (as a native library
or as WebAssembly), so a boundary NO is **byte-identical** across languages.

| Runtime | Install | Where it runs |
|---|---|---|
| [Python](python.md) | `pip install sahou` | CPython (PyPI) |
| [TypeScript](typescript.md) | `npm install sahou` | Node.js & the browser |
| [Rust](rust.md) | `cargo add sahou-core` | native (the `sahou` crate is a placeholder) |
| [TouchDesigner](touchdesigner.md) | build from source | macOS/arm64, **experimental** |

## The shared shape

Whatever the language, the flow is the same:

1. **Connect** with a descriptor (the IR) and a node name — discovery is
   automatic on the LAN.
2. **`publish`** to a `pub_sub` connection, or **`subscribe`** to receive.
3. For `query` connections, **`query_confirmed`** / **`queryConfirmed`** to ask,
   and **`answer`** to respond.
4. A send-boundary NO raises `SahouRejected`; a receive-boundary NO goes to a
   reject handler, never to your message handler.

## Types are opt-in

The base `connect` is untyped (node/connection names are strings, payloads are
loosely typed). To get editor completion and build-time type checking, generate a
typed layer with `sahou gen --lang <python|ts>` and import its `connect` instead.
The engine behaves identically with or without it, and `sahou check` catches
drift between a stub and the contract. See [Say NO early](../concepts/say-no-early.md).
