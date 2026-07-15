"""mypy-only fixture (positive): the whole-descriptor typed connect gives node-name + connection +
payload typing from a single import. sahou_gen is resolved via MYPYPATH (examples/demo/runtime/gen).
"""
from sahou_gen import Touch, connect


def use() -> None:
    visuals = connect("gen/descriptor.json", "visuals")  # node name literal → VisualsNode

    def on_touch(p: Touch) -> None:
        _x: float = p["x"]  # handler argument typed as Touch

    visuals.subscribe("touch", on_touch)
    visuals.close()

    sensor = connect("gen/descriptor.json", "sensor")
    touch: Touch = {"x": 0.5, "phase": "move", "meta": {"ts": 0}}  # source is NotRequired
    sensor.publish("touch", touch)
    resp = sensor.query_confirmed("get_state", {"sel": "levels"})
    level: int = resp["level"]  # response typed as GetStateResponse
    sensor.close()
    _ = level
