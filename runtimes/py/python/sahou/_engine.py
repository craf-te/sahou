"""SahouNode — zenoh glue. All boundary semantics are delegated to the core (sahou._core) (design §1, option B)."""
from __future__ import annotations

import json
import logging
import threading
import time
from collections import Counter
from pathlib import Path

import zenoh

from ._core import SahouRuntime, classify_delivery, parse_reply_err
from ._diag import SahouRejected, SahouUnreachable

log = logging.getLogger("sahou")

_PRIORITY = {
    "realtime": zenoh.Priority.REAL_TIME,
    "interactive_high": zenoh.Priority.INTERACTIVE_HIGH,
    "interactive_low": zenoh.Priority.INTERACTIVE_LOW,
    "data_high": zenoh.Priority.DATA_HIGH,
    "data": zenoh.Priority.DATA,
    "data_low": zenoh.Priority.DATA_LOW,
    "background": zenoh.Priority.BACKGROUND,
}


def _read_descriptor(descriptor) -> str:
    if isinstance(descriptor, dict):
        return json.dumps(descriptor)
    text = str(descriptor)
    p = Path(text)
    if p.suffix == ".json" and p.exists():
        return p.read_text(encoding="utf-8")
    return text  # the JSON string itself


def _fmt_diags(diags: list[dict]) -> str:
    return "; ".join(f"[{d['code']}] @{d['path']}: {d['message']}" for d in diags)


def _decode_attachment(att_z):
    """Decode a wire attachment to str. Non-UTF-8 (a non-sahou sender) becomes None → the core returns missing_schema_hash."""
    if att_z is None:
        return None
    try:
        return bytes(att_z).decode()
    except UnicodeDecodeError:
        return None


def connect(descriptor, node: str, *, connect=None, listen=None, multicast: bool = True) -> "SahouNode":
    """Connect with a descriptor (path / JSON string / dict) plus a node name. Default is automatic LAN discovery (multicast peer)."""
    desc_json = _read_descriptor(descriptor)
    try:
        rt = SahouRuntime(desc_json)
        plan = json.loads(rt.node_plan(node))
    except ValueError as e:  # turn the core's diagnostics (JSON) into an exception
        raise SahouRejected(json.loads(str(e))) from None
    conf = zenoh.Config()
    if connect:
        conf.insert_json5("connect/endpoints", json.dumps(list(connect)))
    if listen:
        conf.insert_json5("listen/endpoints", json.dumps(list(listen)))
    if not multicast:
        conf.insert_json5("scouting/multicast/enabled", "false")
    session = zenoh.open(conf)
    listen_port = None
    # Test aid: remember the rendezvous port of the mesh this node joins
    # (if listening, that port; if only connecting, the target port — i.e. the only entry point
    #  through which a raw session can join the same mesh and have a route to this node).
    endpoints = listen or connect
    if endpoints:
        try:
            listen_port = int(str(endpoints[0]).rsplit(":", 1)[1])
        except (IndexError, ValueError):
            listen_port = None
    return SahouNode(rt, node, session, plan, listen_port)


class SahouNode:
    def __init__(self, rt: SahouRuntime, node: str, session, plan: dict, listen_port=None):
        self._rt = rt
        self._node = node
        self._session = session
        self._plan = plan
        self._ns = rt.namespace()
        self._listen_port = listen_port  # test aid (for raw bypass)
        self._publishers: dict[str, object] = {}
        self._tx_seq: Counter = Counter()
        self._rx_seq: Counter = Counter()
        self._handles: list = []  # keep subscribers/queryables alive
        self._conn_info: dict[str, dict] = {}
        self._verdicts: dict[tuple[str, str], tuple[str, list]] = {}  # (conn, sender_hash) -> (verdict, core diags)
        self._pending: set[tuple[str, str]] = set()
        self._lock = threading.Lock()
        self.reject_counts: Counter = Counter()
        self._on_reject_global = None
        # contract queryable for every connection this node joins (design §5-1: content-addressed, declared by all participants)
        for conn in set(plan["publishes"]) | set(plan["subscribes"]) | set(plan["queries"]) | set(plan["answers"]):
            frag_json = rt.contract_fragment(conn)
            frag = json.loads(frag_json)
            self._conn_info[conn] = {"key": frag["key"], "hash": frag["hash"]}
            contract_key = f"{self._ns}/@sahou/contract/{conn}/{frag['hash']}"
            self._handles.append(
                self._session.declare_queryable(contract_key, self._make_contract_cb(contract_key, frag_json))
            )

    # ---- public API -----------------------------------------------------

    def connection_info(self, conn: str) -> dict:
        return dict(self._conn_info[conn])

    def on_reject(self, cb):
        self._on_reject_global = cb

    def publish(self, conn: str, payload: dict) -> None:
        seq = self._next_seq(self._tx_seq, conn)
        res = json.loads(self._rt.prepare_publish(self._node, conn, json.dumps(payload), seq))
        if not res["ok"]:
            raise SahouRejected(res["diags"])
        msg = res["msg"]
        self._publisher(conn, msg["qos"]).put(msg["wire"].encode(), attachment=msg["attachment"].encode())

    def subscribe(self, conn: str, handler=None, *, on_reject=None):
        def register(fn):
            key = self._conn_info[conn]["key"]  # a connection not joined raises KeyError → made an explicit error
            def cb(sample):
                self._handle_sample(conn, fn, on_reject, sample)
            sub = self._session.declare_subscriber(key, cb)
            self._handles.append(sub)
            return fn
        if conn not in self._conn_info or conn not in self._plan["subscribes"]:
            raise SahouRejected([{"code": "role_mismatch", "path": f"connections.{conn}",
                                  "message": f"node '{self._node}' is not a receiver on this connection"}])
        return register(handler) if handler is not None else register

    def query(self, conn: str, payload: dict, timeout: float = 2.0) -> dict:
        """Requester side of the query ①-④ boundaries. Returns {delivered, response, diags, timed_out}."""
        seq = self._next_seq(self._tx_seq, conn)
        res = json.loads(self._rt.prepare_request(self._node, conn, json.dumps(payload), seq))
        if not res["ok"]:
            raise SahouRejected(res["diags"])  # ① send boundary = do not even issue the get
        msg = res["msg"]
        delivered, response, diags_all, got_any = False, None, [], False
        try:
            replies = self._session.get(
                msg["key"], payload=msg["wire"].encode(), attachment=msg["attachment"].encode(), timeout=timeout
            )
            for reply in replies:
                got_any = True
                sample = getattr(reply, "ok", None)
                if sample is not None:
                    rseq = self._next_seq(self._rx_seq, conn)
                    wire = bytes(sample.payload)
                    att_z = sample.attachment
                    att = _decode_attachment(att_z)
                    out = json.loads(self._rt.accept_reply(self._node, conn, wire, att, rseq, None))
                    if out["result"] == "hash_mismatch":
                        out = self._resolve_mismatch(conn, att, wire, rseq, kind="reply")
                    if out["result"] == "accept":  # ④ reply-receive boundary
                        delivered, response = True, json.loads(out["payload"])
                        break
                    diags_all.extend(out["diags"])
                else:
                    err = reply.err
                    diags_all.extend(json.loads(parse_reply_err(bytes(err.payload))))
        except Exception:  # noqa: BLE001 - a failure of the get itself is treated as a timeout (retryable)
            log.exception("query get failed on '%s'", conn)
        return {"delivered": delivered, "response": response, "diags": diags_all,
                "timed_out": not got_any}

    def query_confirmed(self, conn: str, payload: dict, *, timeout: float = 2.0,
                        retries: int = 3, backoff: float = 0.3) -> dict:
        """Confirmed delivery (Z20): return on a 200 equivalent / raise SahouRejected immediately on fatal (4xx) / resend on retryable."""
        attempt = 0
        while True:
            attempt += 1
            r = self.query(conn, payload, timeout=timeout)
            if r["delivered"]:
                return r["response"]
            cls = classify_delivery(r["timed_out"], json.dumps(r["diags"]))
            if cls == "fatal":
                raise SahouRejected(r["diags"])
            if attempt > retries:
                raise SahouUnreachable(conn, attempt)
            time.sleep(backoff * attempt)

    def answer(self, conn: str, fn):
        """Responder side of a query (② receive boundary + ③ reply-send boundary). Usable as a decorator too."""
        if conn not in self._plan["answers"]:
            raise SahouRejected([{"code": "role_mismatch", "path": f"connections.{conn}",
                                  "message": f"node '{self._node}' is not a responder on this connection"}])
        key = self._conn_info[conn]["key"]

        def qcb(query):
            try:
                rseq = self._next_seq(self._rx_seq, conn)
                payload_z = query.payload
                wire = bytes(payload_z) if payload_z is not None else b""
                att_z = query.attachment
                att = _decode_attachment(att_z)
                out = json.loads(self._rt.accept_request(self._node, conn, wire, att, rseq, None))
                if out["result"] == "hash_mismatch":
                    out = self._resolve_mismatch(conn, att, wire, rseq, kind="request")
                if out["result"] != "accept":  # ② request-receive boundary
                    query.reply_err(json.dumps({"diags": out["diags"]}).encode())
                    return
                try:
                    resp = fn(json.loads(out["payload"]))
                except Exception as e:  # noqa: BLE001 - a handler exception is returned as a 5xx equivalent
                    query.reply_err(json.dumps({"diags": [{"code": "handler_error", "path": "$",
                                                           "message": str(e)}]}).encode())
                    return
                tseq = self._next_seq(self._tx_seq, conn)
                res = json.loads(self._rt.prepare_reply(self._node, conn, json.dumps(resp), tseq))
                if not res["ok"]:  # ③ reply-send boundary = do not reply with a broken response
                    query.reply_err(json.dumps({"diags": res["diags"]}).encode())
                    return
                msg = res["msg"]
                query.reply(msg["key"], msg["wire"].encode(), attachment=msg["attachment"].encode())
            except Exception:  # noqa: BLE001 - last line of defense
                log.exception("internal error in queryable '%s'", conn)

        self._handles.append(self._session.declare_queryable(key, qcb))
        return fn

    def close(self):
        for h in self._handles:
            try:
                h.undeclare()
            except Exception:  # noqa: BLE001 - close is best-effort (diagnostics are logged)
                log.debug("undeclare failed", exc_info=True)
        try:
            self._session.close()
        except Exception:  # noqa: BLE001 - session.close occasionally times out during loopback teardown.
            # Best-effort since this is teardown (the OS reclaims the socket). Log it rather than swallow it.
            log.debug("session close failed", exc_info=True)

    # ---- internals ------------------------------------------------------

    def _next_seq(self, counter, conn: str) -> int:
        """Allocate the per-connection sequence number atomically (safe across zenoh callbacks / multiple threads).
        The query/answer seq of Tasks 8/9 also use this helper."""
        with self._lock:
            seq = counter[conn]
            counter[conn] += 1
            return seq

    def _publisher(self, conn: str, qos: dict):
        if conn not in self._publishers:
            key = self._conn_info[conn]["key"]
            self._publishers[conn] = self._session.declare_publisher(
                key,
                reliability=zenoh.Reliability.RELIABLE if qos["reliability"] == "reliable" else zenoh.Reliability.BEST_EFFORT,
                congestion_control=zenoh.CongestionControl.BLOCK if qos["congestion"] == "block" else zenoh.CongestionControl.DROP,
                priority=_PRIORITY[qos["priority"]],
                express=qos["express"],
            )
        return self._publishers[conn]

    def _count_reject(self, conn: str, diags: list[dict], on_reject):
        with self._lock:
            for d in diags:
                self.reject_counts[d["code"]] += 1
        cb = on_reject or self._on_reject_global
        if cb is not None:
            try:
                cb(conn, diags)
            except Exception:  # noqa: BLE001 - a failure in the user callback must not kill the zenoh thread
                log.exception("on_reject callback failed")
        else:
            log.warning("sahou reject on '%s': %s", conn, _fmt_diags(diags))

    def _handle_sample(self, conn: str, fn, on_reject, sample):
        try:
            seq = self._next_seq(self._rx_seq, conn)
            wire = bytes(sample.payload)
            att_z = sample.attachment
            att = _decode_attachment(att_z)
            out = json.loads(self._rt.accept_sample(self._node, conn, wire, att, seq, None))
            if out["result"] == "hash_mismatch":
                out = self._resolve_mismatch(conn, att, wire, seq, kind="sample")
            if out["result"] == "accept":
                try:
                    fn(json.loads(out["payload"]))
                except Exception:  # noqa: BLE001 - a handler exception must not kill the zenoh thread
                    log.exception("handler failed on '%s'", conn)
            else:
                self._count_reject(conn, out["diags"], on_reject)
        except Exception:  # noqa: BLE001 - last line of defense (do not die silently)
            log.exception("internal error while handling sample on '%s'", conn)

    def _resolve_mismatch(self, conn: str, sender_hash: str, wire: bytes, seq: int, *, kind: str) -> dict:
        """Look up the verdict cache; if unknown, start a handshake and return a handshake_pending reject."""
        with self._lock:
            entry = self._verdicts.get((conn, sender_hash))
        if entry is not None:
            verdict, diags = entry
            if verdict == "accepted":
                accept = {
                    "sample": self._rt.accept_sample,
                    "request": self._rt.accept_request,
                    "reply": self._rt.accept_reply,
                }[kind]
                return json.loads(accept(self._node, conn, wire, sender_hash, seq, sender_hash))
            # blocked: replay the core's handshake-verdict diags as-is (no glue-authored wording; byte-identical across the 3 languages)
            return {"result": "reject", "diags": diags}
        self._start_handshake(conn, sender_hash)
        return {"result": "reject", "diags": [{"code": "handshake_pending", "path": f"connections.{conn}",
                                               "message": f"contract version mismatch detected; handshake in progress (sender_hash={sender_hash})"}]}

    def _make_contract_cb(self, contract_key: str, fragment_json: str):
        def cb(query):
            try:
                query.reply(contract_key, fragment_json.encode())
            except Exception:  # noqa: BLE001 - a failure to reply to the queryable must not kill the zenoh thread
                log.exception("contract queryable reply failed on %s", contract_key)
        return cb

    def _start_handshake(self, conn: str, sender_hash: str) -> None:
        key = (conn, sender_hash)
        with self._lock:
            if key in self._pending or key in self._verdicts:
                return
            self._pending.add(key)
        threading.Thread(target=self._fetch_and_judge, args=(conn, sender_hash), daemon=True).start()

    def _fetch_and_judge(self, conn: str, sender_hash: str) -> None:
        """Design §5-2 to 5-4: fetch the peer's contract exactly once and cache the verdict from the core's 3-way decision.
        unreachable (unfetchable / undecodable / fragment hash mismatch) is not cached = retried on the next mismatch detection."""
        try:
            sel = f"{self._ns}/@sahou/contract/{conn}/{sender_hash}"
            fragment = None
            try:
                for reply in self._session.get(sel, timeout=2.0):
                    sample = getattr(reply, "ok", None)
                    if sample is not None:
                        fragment = bytes(sample.payload).decode()
                        break
            except Exception:  # noqa: BLE001 - a failure of the get itself is treated as unreachable
                log.exception("contract fetch failed: %s", sel)
            if fragment is None:
                log.warning("[contract_unreachable] %s: cannot fetch the peer's contract (not cached; retried on the next mismatch detection)", sel)
                return
            res = json.loads(self._rt.handshake(conn, sender_hash, fragment))
            if res["verdict"] == "unreachable":
                log.warning("[contract_unreachable] '%s' (sender=%s): %s (not cached; retried on the next mismatch detection)",
                            conn, sender_hash, _fmt_diags(res["diags"]))
                return
            with self._lock:
                self._verdicts[(conn, sender_hash)] = (res["verdict"], res.get("diags", []))
            if res["verdict"] == "accepted":
                log.info("handshake accepted on '%s' (sender=%s, additive)", conn, sender_hash)
            else:
                log.warning("[schema_incompatible] '%s' (sender=%s): %s", conn, sender_hash, _fmt_diags(res.get("diags", [])))
        except Exception:  # noqa: BLE001 - an unexpected exception is also treated as unreachable (not cached; do not die silently)
            log.exception(
                "[contract_unreachable] '%s' (sender=%s): unexpected exception during handshake (not cached; retried on the next mismatch detection)",
                conn, sender_hash,
            )
        finally:
            with self._lock:
                self._pending.discard((conn, sender_hash))
