# Python

The Python runtime is [`sahou`](https://pypi.org/project/sahou/) on PyPI. It
gives you typed communication over real Zenoh from just a contract
(`descriptor.json`) plus a node name. Boundary semantics are delegated to the
Rust core; the Python layer only handles transport (zenoh-python) and threading.

## Install

```bash
pip install sahou
```

## Usage

```python
import sahou

node = sahou.connect("gen/descriptor.json", node="sensor")  # LAN auto-discovery by default

# pub_sub — publish. A boundary NO raises SahouRejected (nothing is sent).
node.publish("touch", {"x": 0.5, "phase": "move"})

# pub_sub — subscribe. A NO never reaches your handler (it goes to on_reject).
@node.subscribe("touch")
def on_touch(payload):
    ...

# query — ask (with smart retry) and answer.
res = node.query_confirmed("get_state", {"sel": "levels"})
node.answer("get_state", lambda req: {"level": 3})
```

## Typed stubs (opt-in)

Generate a typed layer for editor completion and build-time checks:

```bash
sahou gen schema.sahou.yaml --out-dir gen --lang python
```

Then import the generated `connect` (`from gen.sahou_gen import connect`) for a
node-specific, payload-typed facade. A per-node stub
(`sahou gen --lang python --node <name>` → `typed_node`) is also available. Run
`sahou check` in CI to catch stub↔IR drift.

## Notes

- Contract version differences are settled by a handshake: additive passes,
  breaking is a NO, and an unfetchable contract is a conservative NO — there is no
  path that silently passes or drops. See
  [Contract evolution](../concepts/contract-evolution.md).
- A runnable publisher/subscriber lives in `examples/demo/runtime/` in the
  repository.

See [`runtimes/python/README.md`](https://github.com/craf-te/sahou/blob/main/runtimes/python/README.md)
for development details.
