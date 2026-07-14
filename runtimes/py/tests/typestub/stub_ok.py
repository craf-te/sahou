"""mypy-only fixture (positive): the generated stub's facade / TypedDict must pass cleanly.

sahou_stub is resolved via MYPYPATH (examples/demo/runtime/gen/sensor).
It does not import the engine (the sahou package) — doubling as evidence that the stub is engine-independent.
"""
from sahou_stub import GetStateResponse, Touch, typed_node


def use(node_obj: object) -> None:
    node = typed_node(node_obj)
    touch: Touch = {"x": 0.5, "phase": "move", "meta": {"ts": 0}}  # source is NotRequired
    node.publish("touch", touch)
    node.publish("points", {"pts": [[0.1, 0.2], [0.3, 0.4]]})
    node.publish("debug_tap", {"anything": True})  # typing:any is Any
    resp: GetStateResponse = node.query_confirmed("get_state", {"sel": "levels"})
    level: int = resp["level"]
    node.close()
    _ = level
