"""mypy-only fixture (negative): wrong node name / unknown connection / payload mismatch / non-participating
direction must all become type errors. If you add or remove lines, update the test too.
"""
from sahou_gen import connect


def use() -> None:
    ghost = connect("d", "ghost")  # L8: unknown node name (no connect overload matches)
    visuals = connect("d", "visuals")
    visuals.publish("touch", {"x": 0.5})  # L10: publish does not exist on VisualsNode (non-participating direction)
    sensor = connect("d", "sensor")
    sensor.publish("touch", {"x": "oops", "phase": "move", "meta": {"ts": 0}})  # L12: type mismatch (x is float)
    sensor.publish("ghost", {"x": 0.5})  # L13: unknown connection name (Literal mismatch)
    _ = (ghost, visuals, sensor)
