import json
from pathlib import Path

import pytest
from sahou._core import SahouRuntime, classify_delivery

FIX = Path(__file__).parent / "fixtures"


def rt() -> SahouRuntime:
    return SahouRuntime((FIX / "descriptor_base.json").read_text(encoding="utf-8"))


def test_load_and_plan():
    r = rt()
    assert r.namespace() == "sahou"
    plan = json.loads(r.node_plan("sensor"))
    assert plan["publishes"] == ["touch"]
    assert plan["queries"] == ["ask"]
    with pytest.raises(ValueError) as ei:
        r.node_plan("ghost")
    assert json.loads(str(ei.value))[0]["code"] == "unknown_node"


def test_prepare_accept_roundtrip_in_process():
    r = rt()
    res = json.loads(r.prepare_publish("sensor", "touch", json.dumps({"x": 0.5, "phase": "move"}), 0))
    assert res["ok"], res
    msg = res["msg"]
    out = json.loads(r.accept_sample("display", "touch", msg["wire"].encode(), msg["attachment"], 0, None))
    assert out["result"] == "accept"
    assert json.loads(out["payload"])["x"] == 0.5
    # send boundary NO
    bad = json.loads(r.prepare_publish("sensor", "touch", json.dumps({"x": 5.0, "phase": "move"}), 0))
    assert not bad["ok"] and bad["diags"][0]["code"] == "out_of_range"


def test_accept_sample_reject_and_hash_mismatch_tags():
    # pin the 3 tags of accept_sample (accept/reject/hash_mismatch) through the ABI (Tasks 7-9 branch on them)
    r = rt()
    touch_hash = json.loads(r.contract_fragment("touch"))["hash"]
    # reject: correct hash + raw with a broken type (receive-boundary NO)
    bad = json.dumps({"x": "bad", "phase": "move"}).encode()
    out = json.loads(r.accept_sample("display", "touch", bad, touch_hash, 0, None))
    assert out["result"] == "reject"
    assert out["diags"][0]["code"] == "type_mismatch"
    # hash_mismatch: an unknown attachment
    valid = json.dumps({"x": 0.5, "phase": "move"}).encode()
    out = json.loads(r.accept_sample("display", "touch", valid, "deadbeef00000000", 0, None))
    assert out["result"] == "hash_mismatch"
    assert out["sender_hash"] == "deadbeef00000000"


def test_handshake_and_classify():
    base = rt()
    additive = SahouRuntime((FIX / "descriptor_additive.json").read_text(encoding="utf-8"))
    breaking = SahouRuntime((FIX / "descriptor_breaking.json").read_text(encoding="utf-8"))
    frag_add = additive.contract_fragment("touch")
    hash_add = json.loads(frag_add)["hash"]
    assert json.loads(base.handshake("touch", hash_add, frag_add))["verdict"] == "accepted"
    frag_brk = breaking.contract_fragment("touch")
    hash_brk = json.loads(frag_brk)["hash"]
    res = json.loads(base.handshake("touch", hash_brk, frag_brk))
    assert res["verdict"] == "blocked" and res["diags"][0]["code"] == "schema_incompatible"
    assert classify_delivery(True, "[]") == "retryable"
    assert classify_delivery(False, json.dumps([{"code": "type_mismatch", "path": "$", "message": ""}])) == "fatal"
