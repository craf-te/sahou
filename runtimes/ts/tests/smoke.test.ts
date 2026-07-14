import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { loadCore } from "../src/core-node.js";

// Test contracts are shared with the Python side (single source; the cross-language premise)
const fixture = (name: string) =>
  readFileSync(fileURLToPath(new URL(`../../py/tests/fixtures/${name}`, import.meta.url)), "utf-8");

const core = loadCore();
const descBase = fixture("descriptor_base.json");
const enc = new TextEncoder();

describe("wasm core smoke", () => {
  it("prepare→accept round-trips and each boundary NO returns its tag", () => {
    const rt = new core.WasmRuntime(descBase);
    expect(rt.namespace()).toBeTypeOf("string");
    const res = JSON.parse(rt.prepare_publish("sensor", "touch", JSON.stringify({ x: 0.5, phase: "move" }), 0n));
    expect(res.ok).toBe(true);
    const out = JSON.parse(
      rt.accept_sample("display", "touch", enc.encode(res.msg.wire), res.msg.attachment, 0n, undefined),
    );
    expect(out.result).toBe("accept");
    // send-boundary NO
    const ng = JSON.parse(rt.prepare_publish("sensor", "touch", JSON.stringify({ x: "bad", phase: "move" }), 0n));
    expect(ng.ok).toBe(false);
    expect(ng.diags[0].code).toBe("type_mismatch");
    // no attachment → missing_schema_hash / unknown hash → hash_mismatch (never let it through silently)
    const miss = JSON.parse(rt.accept_sample("display", "touch", enc.encode("{}"), undefined, 0n, undefined));
    expect(miss.result).toBe("reject");
    expect(miss.diags[0].code).toBe("missing_schema_hash");
    const mm = JSON.parse(
      rt.accept_sample("display", "touch", enc.encode(res.msg.wire), "deadbeef00000000", 0n, undefined),
    );
    expect(mm.result).toBe("hash_mismatch");
  });

  it("an invalid descriptor throws (message = diags JSON)", () => {
    try {
      new core.WasmRuntime("{}");
      expect.unreachable("should throw");
    } catch (e) {
      const diags = JSON.parse((e as Error).message);
      expect(diags[0].code).toBe("descriptor_error");
    }
  });

  it("handshake three-valued / classify_delivery / parse_reply_err", () => {
    const rt = new core.WasmRuntime(descBase);
    const un = JSON.parse(rt.handshake("touch", "deadbeef00000000", "{not json"));
    expect(un.verdict).toBe("unreachable");
    const frag = rt.contract_fragment("touch");
    const hash = JSON.parse(frag).hash as string;
    expect(JSON.parse(rt.handshake("touch", hash, frag)).verdict).toBe("accepted");
    expect(core.wasm_classify_delivery(true, "")).toBe("retryable");
    const bad = JSON.parse(core.wasm_parse_reply_err(enc.encode("garbage")));
    expect(bad[0].code).toBe("bad_reply_envelope");
  });

  it("handshake on an unknown connection yields an unreachable envelope (contract_unreachable; does not throw)", () => {
    const rt = new core.WasmRuntime(descBase);
    const res = JSON.parse(rt.handshake("ghost", "deadbeef00000000", "{}"));
    expect(res.verdict).toBe("unreachable");
    expect(res.diags[0].code).toBe("contract_unreachable");
  });
});
