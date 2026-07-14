import json
import logging

import sahou

from conftest import descriptor, free_port, wait_until
from test_pubsub import pump


def make_mixed(sender_desc: str):
    """A pair where only the sender holds a different contract version (reproducing a live rollout)."""
    port = free_port()
    a = sahou.connect(descriptor(sender_desc), "sensor", listen=[f"tcp/127.0.0.1:{port}"], multicast=False)
    b = sahou.connect(descriptor("base"), "display", connect=[f"tcp/127.0.0.1:{port}"], multicast=False)
    return a, b


def test_additive_rollout_flows_after_handshake():
    # user scenario (Z26): only the sender that added a field is updated; the receiver keeps the old contract
    a, b = make_mixed("additive")
    try:
        got, rejects = [], []
        b.subscribe("touch", got.append, on_reject=lambda c, d: rejects.append(d))
        payload = {"x": 0.5, "phase": "move", "pressure": 0.8}
        assert pump(a, "touch", payload, lambda: got, n=200), "flows once the handshake succeeds"
        # the first few messages during the handshake are counted NOs as handshake_pending (not silently dropped)
        assert any(d and d[0]["code"] == "handshake_pending" for d in rejects)
        # forward compatible: the receiver validates with its own type (pressure is unknown-dropped); the payload arrives as-is
        assert got[0]["x"] == 0.5 and got[0]["pressure"] == 0.8
        assert b.reject_counts["handshake_pending"] >= 1
    finally:
        b.close()
        a.close()


def test_breaking_sender_is_blocked_not_silent():
    a, b = make_mixed("breaking")
    try:
        got, rejects = [], []
        b.subscribe("touch", got.append, on_reject=lambda c, d: rejects.append(d))
        # valid under the sender's contract (x: string); breaking under the receiver's.
        def blocked():
            return any(d and d[0]["code"] == "schema_incompatible" for d in rejects)
        pump(a, "touch", {"x": "whatever", "phase": "move"}, blocked, n=200)
        assert blocked(), "after the handshake verdict, an explicit schema_incompatible NO"
        assert not got, "not a single message from the breaking sender reaches the handler"
    finally:
        b.close()
        a.close()


def test_same_contract_never_triggers_handshake(pair):
    a, b = pair
    got = []
    b.subscribe("touch", got.append)
    assert pump(a, "touch", {"x": 0.5, "phase": "move"}, lambda: got)
    assert b.reject_counts["handshake_pending"] == 0


def test_blocked_replays_core_diags_byte_identical():
    # Fable Important-2: a blocked reject diag replays the core's verdict itself, not glue-authored wording
    from sahou._core import SahouRuntime

    a, b = make_mixed("breaking")
    try:
        rejects = []
        b.subscribe("touch", lambda p: None, on_reject=lambda c, d: rejects.append(d))

        def blocked():
            return any(d and d[0]["code"] == "schema_incompatible" for d in rejects)

        pump(a, "touch", {"x": "whatever", "phase": "move"}, blocked, n=200)
        assert blocked()
        # expected = the core's handshake verdict itself
        rt_b = SahouRuntime(descriptor("base"))
        rt_a = SahouRuntime(descriptor("breaking"))
        frag = rt_a.contract_fragment("touch")
        sender_hash = json.loads(frag)["hash"]
        expected = json.loads(rt_b.handshake("touch", sender_hash, frag))
        assert expected["verdict"] == "blocked"
        got = next(d for d in rejects if d[0]["code"] == "schema_incompatible")
        assert got == expected["diags"], "a blocked reject is a byte-identical replay of the core diags"
    finally:
        b.close()
        a.close()


def test_undecodable_fragment_is_not_cached_as_blocked(raw_session):
    # spec §5-4: an undecidable verdict (unreachable) is not cached = keeps retrying as handshake_pending.
    # prevents regression of "permanently blocked on an undecodable fragment" (the 3-way split of Fable Important-4).
    import sahou
    from test_pubsub import pump_raw

    port = free_port()
    b = sahou.connect(descriptor("base"), "display", listen=[f"tcp/127.0.0.1:{port}"], multicast=False)
    raw = raw_session(port)
    try:
        got, rejects = [], []
        b.subscribe("touch", got.append, on_reject=lambda c, d: rejects.append(d))
        ns = json.loads(descriptor("base"))["namespace"]
        fake_hash = "deadbeef00000000"
        contract_key = f"{ns}/@sahou/contract/touch/{fake_hash}"
        # a fake responder that returns garbage as "the actual contract"
        qh = raw.declare_queryable(contract_key, lambda q: q.reply(contract_key, b"{not json"))
        key = b.connection_info("touch")["key"]

        def pending_many():
            return b.reject_counts["handshake_pending"] >= 3

        pump_raw(raw, key, b'{"x":0.5,"phase":"move"}', pending_many, attachment=fake_hash.encode(), n=200)
        assert pending_many(), "unreachable is not cached → re-handshake on every mismatch = pending piles up"
        assert not any(d and d[0]["code"] == "schema_incompatible" for d in rejects), "does not turn into blocked"
        assert not got, "not a single message reaches the handler"
        assert b._verdicts == {}, "unreachable is not cached"
        qh.undeclare()
    finally:
        b.close()


def test_fetch_and_judge_survives_missing_diags(caplog):
    """Even if the core returns handshake JSON with diags missing, it does not crash with KeyError, and the verdict is cached.

    Previously, directly referencing res["diags"] after writing the verdict cache raised KeyError → the outer except
    emitted a false log of "not cached; retrying" (when it was in fact already cached).
    """
    import sahou

    port = free_port()
    node = sahou.connect(descriptor("base"), "display", listen=[f"tcp/127.0.0.1:{port}"], multicast=False)
    try:
        class _FakeSample:
            payload = b'{"dummy": "fragment"}'

        class _FakeReply:
            ok = _FakeSample()

        class _FakeSession:
            def get(self, sel, timeout):
                return [_FakeReply()]

        class _FakeRt:
            def handshake(self, conn, sender_hash, fragment):
                return '{"verdict": "blocked"}'  # diags missing (the input being defended against)

        node._session, node._rt = _FakeSession(), _FakeRt()
        with caplog.at_level(logging.WARNING, logger="sahou"):
            node._fetch_and_judge("touch", "deadbeef00000000")
        verdict, diags = node._verdicts[("touch", "deadbeef00000000")]
        assert verdict == "blocked"
        assert diags == []  # a missing field is treated as empty diags (not a KeyError)
        assert "contract_unreachable" not in caplog.text, "a false not-cached log must not appear when it is already cached"
    finally:
        node.close()


def test_handshake_unknown_connection_is_unreachable_envelope():
    # ②c backlog: an unknown connection in the handshake context yields a contract_unreachable unreachable envelope (not an exception / FATAL misfire)
    from sahou._core import SahouRuntime

    rt = SahouRuntime(descriptor("base"))
    res = json.loads(rt.handshake("ghost", "deadbeef00000000", "{}"))
    assert res["verdict"] == "unreachable"
    assert res["diags"][0]["code"] == "contract_unreachable"


def test_fetch_and_judge_unexpected_exception_is_structured_unreachable(caplog):
    # ②b backlog: an unexpected exception on the handshake path must not kill the handshake thread with a bare traceback.
    # an unexpected exception = treated as contract_unreachable (not cached; structured log; do not die silently).
    import sahou

    port = free_port()
    b = sahou.connect(descriptor("base"), "display",
                      listen=[f"tcp/127.0.0.1:{port}"], multicast=False)

    class _FakeSample:
        payload = b'{"whatever": 1}'

    class _FakeReply:
        ok = _FakeSample()

    class _FakeSession:
        def get(self, sel, timeout):
            return [_FakeReply()]

    class _BoomRt:
        def handshake(self, *args):
            raise RuntimeError("boom")

    real_session, real_rt = b._session, b._rt
    b._session, b._rt = _FakeSession(), _BoomRt()
    try:
        b._pending.add(("touch", "deadbeef00000000"))
        with caplog.at_level(logging.ERROR, logger="sahou"):
            b._fetch_and_judge("touch", "deadbeef00000000")  # test fails if the exception leaks
        assert b._verdicts == {}, "an unexpected exception is not cached (same treatment as unreachable)"
        assert ("touch", "deadbeef00000000") not in b._pending, "pending is cleaned up in finally"
        assert "contract_unreachable" in caplog.text, "records the NO in a structured log (does not swallow it)"
    finally:
        b._session, b._rt = real_session, real_rt
        b.close()
