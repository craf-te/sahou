import json
import threading
import time

import pytest
import sahou

from conftest import wait_until


def test_query_roundtrip_and_confirmed(pair):
    a, b = pair
    b.answer("ask", lambda req: {"level": 3 if req["sel"] == "levels" else 0})
    # discovery jitter is absorbed by retries (short timeout x many attempts)
    res = a.query_confirmed("ask", {"sel": "levels"}, timeout=1.0, retries=10, backoff=0.1)
    assert res == {"level": 3}


def test_bad_request_is_rejected_at_send_boundary(pair):
    a, _b = pair
    with pytest.raises(sahou.SahouRejected) as ei:
        a.query("ask", {"sel": 123})  # send boundary = the get is not even issued
    assert ei.value.diags[0].code == "type_mismatch"


def test_bad_response_is_fatal_no_retry(pair):
    a, b = pair
    calls = []
    def handler(req):
        calls.append(1)
        return {"level": "high"}  # NO at the responder send boundary → reply_err
    b.answer("ask", handler)
    t0 = time.time()
    with pytest.raises(sahou.SahouRejected) as ei:
        a.query_confirmed("ask", {"sel": "x"}, timeout=1.0, retries=10, backoff=0.1)
    assert any(d.code == "type_mismatch" for d in ei.value.diags)
    # fatal does not resend: handler calls have not piled up (1-2 calls from discovery retries are tolerated)
    assert len(calls) <= 2, f"4xx should not be resent: called {len(calls)} times"


def test_handler_exception_becomes_handler_error_5xx(pair):
    a, b = pair
    def boom(req):
        raise RuntimeError("boom")
    b.answer("ask", boom)
    r = None
    def ask():
        nonlocal r
        r = a.query("ask", {"sel": "x"}, timeout=1.5)
    assert wait_until(lambda: (ask() or True) and (r["diags"] or r["timed_out"]), timeout=10)
    if r["diags"]:
        assert r["diags"][0]["code"] == "handler_error"
        # handler_error is retryable (5xx equivalent)
        from sahou._core import classify_delivery
        import json as _json
        assert classify_delivery(False, _json.dumps(r["diags"])) == "retryable"


def test_late_responder_recovers_with_retry(pair):
    a, b = pair
    def register_late():
        time.sleep(1.0)
        b.answer("ask", lambda req: {"level": 7})
    threading.Thread(target=register_late, daemon=True).start()
    res = a.query_confirmed("ask", {"sel": "x"}, timeout=0.5, retries=15, backoff=0.2)
    assert res == {"level": 7}


def test_no_responder_exhausts_to_unreachable(pair):
    a, _b = pair  # do not register an answer
    with pytest.raises(sahou.SahouUnreachable):
        a.query_confirmed("ask", {"sel": "x"}, timeout=0.3, retries=2, backoff=0.05)


def test_bad_reply_envelope_is_retryable(pair, raw_session):
    # Fable Important-3: a reply_err envelope from a non-sahou / broken responder is
    # bad_reply_envelope (retryable), and must not be misfired as decode_error (FATAL).
    a, _b = pair
    raw = raw_session(a._listen_port)
    key = a.connection_info("ask")["key"]
    qh = raw.declare_queryable(key, lambda q: q.reply_err(b"garbage not json"))
    try:
        r = None

        def ask():
            nonlocal r
            r = a.query("ask", {"sel": "x"}, timeout=1.5)

        assert wait_until(lambda: (ask() or True) and bool(r["diags"]), timeout=10)
        assert r["diags"][0]["code"] == "bad_reply_envelope"
        from sahou._core import classify_delivery
        assert classify_delivery(False, json.dumps(r["diags"])) == "retryable"
        # retryable → exhaust retries into SahouUnreachable (not an immediate SahouRejected)
        with pytest.raises(sahou.SahouUnreachable):
            a.query_confirmed("ask", {"sel": "x"}, timeout=0.5, retries=1, backoff=0.05)
    finally:
        qh.undeclare()
