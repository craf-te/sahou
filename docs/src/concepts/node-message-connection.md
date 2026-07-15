# Node, Message, Connection

The IR — and therefore the schema — is built from three elements. Types are kept
separate from wiring: a Message describes *shape*, a Connection describes *who
talks to whom and how*.

## Node

A **participant**: an app, a process, or a device. In the schema, a node has a
`kind`:

```yaml
nodes:
  sensor:  { kind: sahou }             # speaks the Sahou contract (the default)
  visuals: { kind: sahou }
  legacy_console: { kind: external }   # a non-Sahou device
```

- `kind: sahou` — a device or process that speaks the Sahou contract. This is
  the default when `kind` is omitted.
- `kind: external` — a non-Sahou device. It can be a wiring target (something a
  connection sends *to*) but it does not itself speak the contract.

## Message

A named, reusable **shape of data** — its fields and their types. A Field belongs
to a Message. Sahou supports eleven field types (`int`, `float`, `bool`,
`string`, `bytes`, `timestamp`, `enum`, `array`, `map`, `group`, `union`); see
[Schema authoring](../schema-authoring.md) for each. In the schema, a message
shape appears inside a connection's slot (`payload`, or `request`/`response`):

```yaml
payload:
  typing: typed
  fields:
    - { name: x, type: float, min: 0, max: 1 }
    - { name: phase, type: enum, values: [down, move, up] }
```

## Connection

The **wiring**: which message, from which node to which, and how it is carried.
The transport-related settings (reliability, congestion, encoding, and so on) are
Connection attributes. Sahou has two patterns:

- **`pub_sub`** — one-to-many distribution. Its only slot is `payload`.
- **`query`** — one-to-one request/response. Its slots are `request` and
  `response`.

```yaml
connections:
  touch:                     # pub_sub
    pattern: pub_sub
    from: sensor
    to: [visuals, archive]
    payload: { typing: typed, fields: [ ... ] }
  get_level:                 # query
    pattern: query
    from: sensor
    to: [archive]
    request:  { typing: typed, fields: [ ... ] }
    response: { typing: typed, fields: [ ... ] }
```

The `pattern` determines which slots are allowed — using `payload` on a `query`,
or `request` on a `pub_sub`, is rejected. `to` may only reference nodes defined
in `nodes`, and cannot equal `from`.
