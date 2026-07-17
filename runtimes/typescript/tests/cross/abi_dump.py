"""ABI byte-identity matrix: emit the PyO3-side raw envelope strings as `<case>\t<raw>`.
The case definitions correspond 1:1 with the wasm side in cross.test.ts (if you change one, always change both)."""
import json
import sys
from pathlib import Path

from sahou._core import SahouRuntime, classify_delivery, parse_reply_err

# Windows text-mode stdout converts "\n" to "\r\n" (universal newlines).
# The JS side splits on "\n" only, so a stray "\r" gets mixed into the end of the value, an invisible byte difference.
# This is an I/O configuration issue in the driver, not in the ABI matrix's inputs/outputs themselves, so pin it here.
sys.stdout.reconfigure(newline="\n")

FIX = Path(__file__).resolve().parents[3] / "python" / "tests" / "fixtures"

base = (FIX / "descriptor_base.json").read_text(encoding="utf-8")
additive = (FIX / "descriptor_additive.json").read_text(encoding="utf-8")
breaking = (FIX / "descriptor_breaking.json").read_text(encoding="utf-8")

rt = SahouRuntime(base)
rt_add = SahouRuntime(additive)
rt_brk = SahouRuntime(breaking)
frag_base = rt.contract_fragment("touch")
frag_add = rt_add.contract_fragment("touch")
frag_brk = rt_brk.contract_fragment("touch")
hash_base = json.loads(frag_base)["hash"]
hash_add = json.loads(frag_add)["hash"]
hash_brk = json.loads(frag_brk)["hash"]

VALID = b'{"x":0.5,"phase":"move"}'
BAD = b'{"x":"bad","phase":"move"}'

cases = {
    "plan": rt.node_plan("display"),
    "fragment": frag_base,
    "prepare_ok": rt.prepare_publish("sensor", "touch", '{"x":0.5,"phase":"move"}', 0),
    "prepare_ng": rt.prepare_publish("sensor", "touch", '{"x":"bad","phase":"move"}', 0),
    "prepare_role": rt.prepare_publish("display", "touch", "{}", 0),
    "accept_ok": rt.accept_sample("display", "touch", VALID, hash_base, 0, None),
    "accept_ng": rt.accept_sample("display", "touch", BAD, hash_base, 0, None),
    "accept_nohash": rt.accept_sample("display", "touch", VALID, None, 0, None),
    "accept_mismatch": rt.accept_sample("display", "touch", VALID, "deadbeef00000000", 0, None),
    "hs_accept": rt.handshake("touch", hash_add, frag_add),
    "hs_block": rt.handshake("touch", hash_brk, frag_brk),
    "hs_undecodable": rt.handshake("touch", "deadbeef00000000", "{not json"),
    "hs_wrong_hash": rt.handshake("touch", "0000000000000000", frag_base),
    "hs_unknown_conn": rt.handshake("ghost", "deadbeef00000000", "{}"),
    "reply_err_ok": parse_reply_err(b'{"diags":[{"code":"handler_error","path":"$","message":"boom"}]}'),
    "reply_err_bad": parse_reply_err(b"garbage"),
    "classify_fatal": classify_delivery(False, '[{"code":"type_mismatch","path":"$.x","message":"m"}]'),
    "classify_retry": classify_delivery(True, ""),
}
for name, raw in cases.items():
    sys.stdout.write(f"{name}\t{raw}\n")
