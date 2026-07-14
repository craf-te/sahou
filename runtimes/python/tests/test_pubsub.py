import json

import pytest
import sahou

from conftest import descriptor, free_port, wait_until


def pump(node, conn, payload, received, n=80):
    """Helper that keeps sending until delivered, to absorb discovery jitter."""
    import time
    for _ in range(n):
        node.publish(conn, payload)
        if received():
            return True
        time.sleep(0.05)
    return received()


def pump_raw(raw, key, payload, received, *, attachment=None, n=80):
    """A single put from a raw zenoh session can be silently dropped before discovery/route convergence
    (right after connecting, the raw session's route to b may not have converged yet).
    Following the same idea as pump(), keep sending until delivered (= a reject is observed) to absorb the jitter."""
    import time
    for _ in range(n):
        if attachment is None:
            raw.put(key, payload)
        else:
            raw.put(key, payload, attachment=attachment)
        if received():
            return True
        time.sleep(0.05)
    return received()


def test_valid_flows_and_send_boundary_rejects(pair):
    a, b = pair
    got, rejects = [], []
    b.subscribe("touch", lambda p: got.append(p), on_reject=lambda c, d: rejects.append((c, d)))
    assert pump(a, "touch", {"x": 0.5, "phase": "move"}, lambda: got), "valid is delivered"
    assert got[0]["x"] == 0.5 and got[0]["phase"] == "move"
    # send boundary: a type NO is not put, and raises SahouRejected on the spot
    with pytest.raises(sahou.SahouRejected) as ei:
        a.publish("touch", {"x": "bad", "phase": "move"})
    assert ei.value.diags[0].code == "type_mismatch"
    # a connection not in the contract
    with pytest.raises(sahou.SahouRejected) as ei:
        a.publish("ghost", {})
    assert ei.value.diags[0].code == "unknown_connection"


def test_recv_boundary_rejects_raw_bypass(pair, raw_session):
    a, b = pair
    got, rejects = [], []
    b.subscribe("touch", lambda p: got.append(p), on_reject=lambda c, d: rejects.append(d))
    # first establish the normal path (confirm the subscription is alive)
    assert pump(a, "touch", {"x": 0.1, "phase": "down"}, lambda: got)
    info = b.connection_info("touch")
    # from a raw session connecting directly to b's listen, send a correct hash + a broken payload
    raw = raw_session(b._listen_port)  # a raw session connecting directly to b's listen
    # on_reject is called with (conn, diags: list[dict]); rejects accumulates the diags
    assert pump_raw(
        raw, info["key"], json.dumps({"x": "bad", "phase": "move"}).encode(),
        lambda: any(d and d[0]["code"] == "type_mismatch" for d in rejects),
        attachment=info["hash"].encode(),
    )
    assert all(g["x"] != "bad" for g in got), "the broken payload is not passed to the handler"


def test_attachment_missing_is_rejected_not_silent(pair, raw_session):
    a, b = pair
    rejects = []
    b.subscribe("touch", lambda p: None, on_reject=lambda c, d: rejects.append(d))
    info = b.connection_info("touch")
    raw = raw_session(b._listen_port)
    assert pump_raw(
        raw, info["key"], json.dumps({"x": 0.5, "phase": "move"}).encode(),  # no attachment
        lambda: any(d and d[0]["code"] == "missing_schema_hash" for d in rejects),
    )


def test_non_utf8_attachment_is_rejected_not_silent(pair, raw_session):
    """Even for a non-sahou sender (an attachment that cannot be decoded), do not drop silently;
    reject it as a structured missing_schema_hash NO (do not swallow the UnicodeDecodeError)."""
    a, b = pair
    rejects = []
    b.subscribe("touch", lambda p: None, on_reject=lambda c, d: rejects.append(d))
    info = b.connection_info("touch")
    raw = raw_session(b._listen_port)
    pump_raw(
        raw, info["key"], json.dumps({"x": 0.5, "phase": "move"}).encode(),
        lambda: any(d and d[0]["code"] == "missing_schema_hash" for d in rejects),
        attachment=b"\xff\xfe\x00",  # non-UTF-8: input on which .decode() raises UnicodeDecodeError
    )
    assert wait_until(
        lambda: any(d and d[0]["code"] == "missing_schema_hash" for d in rejects)
    ), "a non-UTF-8 attachment is rejected as missing_schema_hash (not silently dropped)"
