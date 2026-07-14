"""mypy-only fixture (negative): typo / type mismatch / unknown connection must become type errors. If you add or remove lines, update the test too."""
from sahou_stub import typed_node


def use(node_obj: object) -> None:
    node = typed_node(node_obj)
    node.publish("touch", {"x": 0.5, "phse": "move", "meta": {"ts": 0}})  # L7: key typo (phse)
    node.publish("touch", {"x": "oops", "phase": "move", "meta": {"ts": 0}})  # L8: type mismatch (x is float)
    node.publish("ghost", {"x": 0.5})  # L9: unknown connection name (Literal mismatch)
