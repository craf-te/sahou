"""Vitals (spec: notes/sahou-vitals-spec.md): the engine declares a liveliness token +
a vitals queryable at <ns>/@sahou/vitals/<node>; the payload is built by the core."""
import importlib.metadata as md
import json

import sahou
from conftest import descriptor, free_port, wait_until


def _fetch_one(session, key: str, timeout: float = 2.0) -> str | None:
    """First OK reply's payload as str, else None (same reply-iteration shape as the engine)."""
    for reply in session.get(key, timeout=timeout):
        sample = getattr(reply, "ok", None)
        if sample is not None:
            return bytes(sample.payload).decode()
    return None


def _token_visible(session, key: str) -> bool:
    for reply in session.liveliness().get(key, timeout=1.0):
        if getattr(reply, "ok", None) is not None:
            return True
    return False


def test_vitals_queryable_serves_versioned_payload(raw_session):
    port = free_port()
    node = sahou.connect(descriptor("base"), "sensor",
                         listen=[f"tcp/127.0.0.1:{port}"], multicast=False)
    try:
        raw = raw_session(port)
        vkey = node._rt.vitals_key("sensor")  # the core is the single source of the key shape
        got = {}

        def fetched():
            payload = _fetch_one(raw, vkey)
            if payload is not None:
                got["payload"] = payload
                return True
            return False

        assert wait_until(fetched), "vitals query returned nothing"
        v = json.loads(got["payload"])
        assert v["vitals_format"] == 1
        assert v["node"] == "sensor"
        assert v["runtime"]["lang"] == "python"
        assert v["runtime"]["transport"] == "native"
        # versions come from installed-package metadata, not faked (spec §1.2)
        assert v["runtime"]["sahou"] == md.version("sahou")
        assert v["runtime"]["zenoh"] == md.version("eclipse-zenoh")
        assert isinstance(v["uptime_secs"], int) and v["uptime_secs"] >= 0
        assert v["connections"], "joined connections should be reported"
        for c in v["connections"].values():
            assert c["role"] in ("from", "to")
            assert len(c["hash"]) == 16
        assert v["handshake"] == {}  # no mismatches seen in this test
    finally:
        node.close()


def test_liveliness_token_appears_and_disappears_on_close(raw_session):
    port = free_port()
    node = sahou.connect(descriptor("base"), "sensor",
                         listen=[f"tcp/127.0.0.1:{port}"], multicast=False)
    raw = raw_session(port)
    vkey = node._rt.vitals_key("sensor")
    try:
        assert wait_until(lambda: _token_visible(raw, vkey)), "liveliness token not visible"
    finally:
        node.close()
    assert wait_until(lambda: not _token_visible(raw, vkey)), \
        "the token should be auto-removed after close"


def test_vitals_false_declares_neither(raw_session):
    port = free_port()
    node = sahou.connect(descriptor("base"), "sensor",
                         listen=[f"tcp/127.0.0.1:{port}"], multicast=False, vitals=False)
    try:
        raw = raw_session(port)
        # first prove the mesh routes at all, via a contract queryable this node DOES declare
        conn, info = next(iter(node._conn_info.items()))
        contract_key = f"{node._ns}/@sahou/contract/{conn}/{info['hash']}"
        assert wait_until(lambda: _fetch_one(raw, contract_key) is not None), \
            "mesh did not converge (contract queryable unreachable)"
        # …then assert the vitals surface is absent, distinguishing opt-out from non-convergence
        vkey = node._rt.vitals_key("sensor")
        assert _fetch_one(raw, vkey) is None, "vitals queryable must not be declared"
        assert not _token_visible(raw, vkey), "liveliness token must not be declared"
    finally:
        node.close()
