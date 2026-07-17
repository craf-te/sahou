"""Cross-language live wire: keep publishing touch under the given contract (the Node-side vitest verifies receipt)."""
import argparse
import time
from pathlib import Path

import sahou

FIX = Path(__file__).resolve().parents[3] / "python" / "tests" / "fixtures"

p = argparse.ArgumentParser()
p.add_argument("--desc", default="base", choices=["base", "additive", "breaking"])
p.add_argument("--connect", required=True, help="the link's peer endpoint (e.g. tcp/127.0.0.1:7448)")
p.add_argument("--seconds", type=float, default=15.0)
args = p.parse_args()

desc = (FIX / f"descriptor_{args.desc}.json").read_text(encoding="utf-8")
node = sahou.connect(desc, "sensor", connect=[args.connect], multicast=False)
# under the breaking contract, x: string is valid (it passes the sender's contract but is blocked at the receiver's handshake)
payload = {"x": "whatever", "phase": "move"} if args.desc == "breaking" else {"x": 0.5, "phase": "move"}
deadline = time.time() + args.seconds
try:
    while time.time() < deadline:
        node.publish("touch", payload)
        time.sleep(0.05)
finally:
    node.close()
