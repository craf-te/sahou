import json
import socket
import time
from pathlib import Path

import pytest

FIX = Path(__file__).parent / "fixtures"


def free_port() -> int:
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


def wait_until(pred, timeout=8.0, interval=0.05):
    """Wait until the predicate becomes true (absorbs loopback discovery/delivery jitter)."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        if pred():
            return True
        time.sleep(interval)
    return False


def descriptor(name: str) -> str:
    return (FIX / f"descriptor_{name}.json").read_text(encoding="utf-8")


@pytest.fixture
def pair():
    """Two nodes connected explicitly over loopback (multicast disabled; CI-stable; no dependence on TCC/LAN)."""
    import sahou

    port = free_port()
    a = sahou.connect(descriptor("base"), "sensor", listen=[f"tcp/127.0.0.1:{port}"], multicast=False)
    b = sahou.connect(descriptor("base"), "display", connect=[f"tcp/127.0.0.1:{port}"], multicast=False)
    yield a, b
    b.close()
    a.close()


@pytest.fixture
def raw_session():
    """A raw zenoh session that bypasses validation (for receive-boundary tests). Configured by the caller."""
    import zenoh

    sessions = []

    def make(connect_port: int):
        conf = zenoh.Config()
        conf.insert_json5("connect/endpoints", json.dumps([f"tcp/127.0.0.1:{connect_port}"]))
        conf.insert_json5("scouting/multicast/enabled", "false")
        s = zenoh.open(conf)
        sessions.append(s)
        return s

    yield make
    for s in sessions:
        s.close()
