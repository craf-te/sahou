# Overview

Sahou has a small mental model. Once these pieces click, the rest of the tool
follows from them.

- **[Node, Message, Connection](node-message-connection.md)** — the three things
  a schema names.
- **[Contract vs Endpoints](contract-vs-endpoints.md)** — what belongs in the
  shared contract, and what is environment-specific.
- **[The IR and round-trip](ir-and-round-trip.md)** — why a neutral
  intermediate representation sits between the YAML you write and everything
  Sahou generates.
- **[Say NO early](say-no-early.md)** — how, and where, a wrong type or a typo
  gets rejected.
- **[Contract evolution](contract-evolution.md)** — what happens when the
  contract changes while apps are running.

The one-paragraph version: you write a **contract** that names **nodes**, the
**messages** they exchange, and the **connections** that wire them. Sahou parses
that into a neutral **IR**, validates it with precise diagnostics, and uses it to
discover peers, carry traffic, and — optionally — generate typed stubs. Anything
that does not fit the contract is rejected at the boundary rather than allowed to
propagate.
