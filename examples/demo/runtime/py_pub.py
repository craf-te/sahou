"""Publishes touch as the demo contract's sensor (with a send-boundary NO to experience).
Run: cd runtimes/py && uv run python ../../examples/demo/runtime/py_pub.py
Environments without multicast: add SAHOU_CONNECT=tcp/[::1]:7448 (link's peer port; on Windows use the IPv6 loopback).
"""
import os
import time
from pathlib import Path

import sahou

HERE = Path(__file__).parent
connect = [os.environ["SAHOU_CONNECT"]] if "SAHOU_CONNECT" in os.environ else None
node = sahou.connect(str(HERE / "gen" / "descriptor.json"), "sensor", connect=connect)
print("[py_pub] connected as sensor (Ctrl+C to quit)")
try:
    i = 0
    while True:
        node.publish("touch", {"x": (i % 100) / 100, "phase": "move", "meta": {"ts": int(time.time() * 1000)}})
        i += 1
        if i % 50 == 0:
            try:
                node.publish("touch", {"x": "oops", "phase": "move", "meta": {"ts": 0}})
            except sahou.SahouRejected as e:
                print(f"[py_pub] send-boundary NO (intentionally corrupt payload; not put): {e}")
        if i % 100 == 0:
            try:
                res = node.query_confirmed("get_state", {"sel": "levels"}, timeout=2.0, retries=2, backoff=0.2)
                print(f"[py_pub] get_state -> {res} (delivery-confirmed query; equivalent to a 200)")
            except sahou.SahouUnreachable:
                print("[py_pub] get_state delivery unconfirmed (node_state not running? exhausted retries of a retryable = the smart-retry NO)")
        time.sleep(0.05)
except KeyboardInterrupt:
    pass
finally:
    node.close()
