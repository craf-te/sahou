# sahou (Python runtime)

Typed communication over real Zenoh, using just a contract (descriptor.json) plus a node name. The
boundary semantics are delegated to the Rust core (sahou-core / `sahou._core`); this glue layer only
handles the transport (zenoh-python) and threading.

## Usage

    import sahou
    node = sahou.connect("descriptor.json", node="sensor")   # default = automatic discovery on the same LAN
    # (the path is up to you. When using the output of `sahou gen --out-dir gen`, use "gen/descriptor.json")
    node.publish("touch", {"x": 0.5, "phase": "move"})        # a NO is SahouRejected (nothing is put)

    @node.subscribe("cue")
    def on_cue(payload): ...                                   # a NO never reaches the handler (goes to on_reject)

    res = node.query_confirmed("get_state", {"sel": "levels"})  # 200/400/500 equivalent + smart retry
    node.answer("get_state", lambda req: {"level": 3})

## Development

    uv venv && uv pip install maturin pytest "eclipse-zenoh>=1.9,<2"
    .venv/Scripts/maturin develop        # build the core with feature "python"
    .venv/Scripts/python -m pytest tests/

- Contract version differences are decided by a handshake (contract queryable + verdict cache):
  additive = pass through / breaking = NO / unfetchable = conservative NO. There is no path that
  silently passes or drops.
- Not yet implemented (planned follow-ups): auto-launch of sahou link (for Node/browser, ②b) /
  tap, doctor, type stubs (②c).

## Vitals (node self-report)

Each connected node declares, by default, a liveliness token and a small self-report
queryable at `<namespace>/@sahou/vitals/<node>` — its identity, schema generation
(per-connection hashes), runtime versions, uptime, and cached handshake verdicts.
The `sahou doctor --lan` roll call uses these.

```python
node = sahou.connect("descriptor.json", "sensor", vitals=False)  # opt out
```

**Exposure note, honestly:** the transport carries no authentication or encryption, so
*any peer on the same LAN can read a node's vitals* — just as it can already read the
full contract from the contract queryable. Vitals report only state the engine holds
anyway (versions, hashes, verdicts); if that is still too much for your network, opt
out with `vitals=False`.
