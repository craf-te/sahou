# Say NO early

The core idea of Sahou: a wrong type or a mistyped field should be rejected **the
moment it crosses the boundary** — and, with generated type stubs, **in your
editor at build time** — rather than failing later, somewhere far away, in the
hardest place to debug.

## Two boundaries where NO happens

1. **At build time, in your editor (opt-in).** Generate a typed stub with
   `sahou gen --lang <python|ts>` and your editor type-checks the messages a node
   sends and receives before anything runs. This is the earliest possible NO.
2. **At runtime, at the boundary.** When a message is sent or received, the core
   validates it against the contract. A send-boundary rejection raises an error
   (`SahouRejected`); a receive-boundary rejection is routed to a reject handler,
   never to your message handler. Either way the bad value does not propagate.

## Never silently fold the contract

Sahou refuses the quiet failure modes that serde-style parsing defaults to:

- **Duplicate keys** are rejected, not silently resolved "last one wins".
- **Unknown keys** (typos) are rejected, not silently dropped.

This is enforced with `deny_unknown_fields` and unique-map checks. A typo in a
field name is a boundary NO, not a field that silently vanishes.

## Structured diagnostics

`sahou validate` returns a list of `{ code, path, message }` — each pointing at
the exact place. A few common codes:

| code | meaning |
|---|---|
| `unknown_node` | `from`/`to` references an identifier not in `nodes` |
| `self_loop` | `from` and `to` are the same node |
| `missing_slot` / `unexpected_slot` | slots don't match the `pattern` |
| `duplicate_field` | a field name repeats within one slot |
| `missing_items` | `array`/`map` omitted its element/value type |
| `invalid_default` | a `default` violates its own type/constraints |

The full list is in [Schema authoring](../schema-authoring.md).

## Runtime interprets the IR; stubs are opt-in

At runtime, the engine plus its interpretation of the IR does the validation —
so the boundary NO exists whether or not you generated stubs. Static types are an
**opt-in, consumer-side** convenience layer; the engine never reads them. Because
a stub is generated and code drifts, `sahou check` compares a stub against the
current IR and rejects on drift — a natural CI gate.
