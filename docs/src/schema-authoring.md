# Schema authoring

This page is the reference for writing `schema.sahou.yaml` by hand. (The CLI can
print a compact version of the same reference for AI coding agents with
`sahou reference`.)

## File shape

Top level:

```yaml
schema: my_installation   # required; also the namespace
version: 1                # default "1"
nodes: { ... }            # required; may be an empty map
connections: { ... }      # required; may be an empty map
```

Remember the [three-file separation](concepts/contract-vs-endpoints.md): the
contract goes here, deployment goes in `endpoints.<env>.yaml`, and GUI
coordinates go in `layout.sahou.json`. Never write deployment or coordinates in
the contract.

## Nodes

```yaml
nodes:
  sensor: { kind: sahou }            # default when kind is omitted
  legacy_console: { kind: external } # a wiring target that doesn't speak the contract
```

## Connections

A connection's `pattern` decides which slots it has.

```yaml
connections:
  <id>:
    pattern: pub_sub | query    # determines the slots
    from: <node_id>
    to: [<node_id>, ...]        # a single value may be a bare string; must be in nodes; cannot equal from
    key: <keyexpr>              # optional; auto-derived from namespace + connection id when omitted
    selector: <string>          # query only (on pub_sub → unexpected_selector)
    reliability: best_effort | reliable   # default best_effort
    congestion:  drop | block              # default drop
    priority: realtime | interactive_high | interactive_low | data_high | data | data_low | background  # default data
    express: true | false       # default false
    encoding: json              # a contract attribute; currently json only; default json
    validate: full | sampled | off  # default full
    # slots (pattern-dependent):
    payload:  <Slot>            # pub_sub only
    request:  <Slot>            # query only
    response: <Slot>            # query only
```

- **`pub_sub`** — one-to-many. Only `payload`.
- **`query`** — one-to-one request/response. Only `request` + `response`.

Using the wrong slot for the pattern is rejected (`missing_slot` /
`unexpected_slot`).

## Slots

```yaml
typing: any | typed     # any = unvalidated (no type safety; the GUI marks it red)
kind: record | opaque   # default record; opaque is raw data with no fields
fields: [<Field>, ...]  # kind: record only
encoding: <string>      # free-form text for kind: opaque (e.g. "video/raw")
```

Use `typing: typed` for anything whose shape you want guaranteed. `typing: any`
turns off validation for that slot.

## Fields and types

```yaml
name: <string>
type: int | float | bool | string | bytes | timestamp | enum | array | map | group | union
required: true | false   # default true
default: <value>         # must match the type/constraints, else invalid_default
# type-specific attributes:
min: <number>            # int/float
max: <number>            # int/float
max_len: <uint>          # string (character-count limit)
values: [<string>, ...]  # enum (empty → empty_enum)
items: <TypeSpec>        # array element type / map value type (omitting → missing_items)
any_of: [<TypeSpec>, ...] # union candidates (empty → empty_union)
fields: [<Field>, ...]   # group (anonymous inline hierarchy)
```

The eleven types:

| type | notes |
|---|---|
| `int` | `min`/`max`. `int` → `float` is a compatible promotion. |
| `float` | `min`/`max`. |
| `bool` | booleans only. |
| `string` | `max_len` sets the character-count limit. |
| `bytes` | a base64 string on the wire (JSON). |
| `timestamp` | an **integer** of epoch milliseconds (non-integer → `type_mismatch`). |
| `enum` | `values` required (empty → `empty_enum`). |
| `array` | `items` required (omit → `missing_items`). Can nest: `array<array<float>>`. |
| `map` | `items` is the **value** type; required (omit → `missing_items`). |
| `group` | anonymous inline hierarchy (an alternative to named nesting). |
| `union` | `any_of` required (empty → `empty_union`). Untagged: accepted if it matches any candidate. |

## What gets rejected

Sahou refuses to silently fold the contract. In addition to the type checks
above:

- **Duplicate keys** → rejected (not "last-wins").
- **Unknown keys** (typos) → rejected (not silently dropped).

## Diagnostic codes

`sahou validate` returns a list of `{ code, path, message }`. The main codes:

| code | meaning |
|---|---|
| `parse_error` | YAML syntax / type mismatch / duplicate key / unknown key |
| `unknown_node` | `from`/`to` points at an id not in `nodes` |
| `self_loop` | `from` equals `to` |
| `missing_slot` | a slot the pattern requires is missing |
| `unexpected_slot` | a slot the pattern cannot have is present |
| `unexpected_selector` | `selector` on a `pub_sub` |
| `duplicate_field` | a field name repeats within one slot |
| `non_finite_bound` | `min`/`max` is NaN/Inf |
| `invalid_range` | `min > max` |
| `empty_enum` | `enum` with empty `values` |
| `missing_items` | `array`/`map` without `items` |
| `empty_union` | `union` with empty `any_of` |
| `invalid_default` | a `default` violates its own type/constraints |

## The edit loop

1. Edit `schema.sahou.yaml`.
2. Run `sahou validate <file>`.
3. Fix each `{ code, path, message }` at the location its `path` points to.
4. Repeat until there are zero rejections.
5. If `sahou gui` is open, edits are reflected immediately.

## A complete example

```yaml
schema: stage_demo
version: 1
nodes:
  sensor: { kind: sahou }
  visuals: { kind: sahou }
  archive: { kind: sahou }
  legacy_console: { kind: external }
connections:
  touch:
    pattern: pub_sub
    from: sensor
    to: [visuals, archive]
    reliability: reliable
    congestion: block
    payload:
      typing: typed
      fields:
        - { name: x, type: float, min: 0, max: 1 }
        - { name: active, type: bool }
        - { name: phase, type: enum, values: [down, move, up] }
        - name: meta
          type: group
          fields:
            - { name: ts, type: timestamp }
            - { name: thumb, type: bytes, required: false }
            - { name: note, type: string, required: false, max_len: 120 }
  points:
    pattern: pub_sub
    from: sensor
    to: [visuals]
    payload:
      typing: typed
      fields:
        - { name: pts, type: array, items: { type: array, items: float } }
        - { name: tags, type: map, items: string }
        - { name: extra, type: union, any_of: [int, string] }
  get_level:
    pattern: query
    from: sensor
    to: [archive]
    selector: latest
    request:
      typing: typed
      fields:
        - { name: sel, type: string, max_len: 64 }
    response:
      typing: typed
      fields:
        - { name: level, type: int }
  raw_video:
    pattern: pub_sub
    from: legacy_console
    to: [visuals]
    payload: { typing: any, kind: opaque, encoding: "video/raw" }
```
