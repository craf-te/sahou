# Introduction

Sahou is a **schema-first tool for building the interface layer between
systems**. You describe your messages and their wiring once, in a schema. Sahou
then lets your apps talk to each other on the same LAN — **without
hand-managing IP addresses or ports** — and rejects wrong types and mistyped
fields at the boundary, instead of letting a mistake propagate and surface
somewhere far away.

> "Sahou" (作法) means *proper form / etiquette* — the small set of conventions
> that let independent programs talk to each other comfortably and without
> mistakes.

## What you describe, and what you get

A Sahou schema names three things: the **nodes** that participate, the
**messages** they exchange, and the **connections** that wire them together.
From that one description you get:

- **Discovery and transport**, so nodes find each other by name on a shared LAN.
- **A boundary that says NO**, so a wrong type or a mistyped field is caught
  where it happens.
- **One contract across many languages** — Rust, Python, Node.js, the browser,
  and TouchDesigner all speak the same schema.

## Why Sahou

- **No addresses, no ports.** On a shared LAN, nodes discover each other by
  name. You declare connections in a schema; Sahou handles discovery and
  transport for you (built on [Zenoh](https://zenoh.io)).
- **Say NO early.** A wrong type or a mistyped field is rejected the moment it
  crosses the boundary — and, with generated type stubs, in your editor at build
  time — rather than failing later in the hardest place to debug.
- **One schema, many environments.** The same contract drives Rust, Python,
  Node.js, the browser, and TouchDesigner. The core is a neutral data model (an
  intermediate representation, or *IR*); the per-language runtimes are thin.

## Where to go next

- **[Quickstart](quickstart.md)** — from zero to a validated contract and a
  working LAN message flow.
- **[Concepts](concepts/index.md)** — the mental model: nodes, messages,
  connections, and why the IR sits in the middle.
