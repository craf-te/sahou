import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { loadCore } from "../src/core-node.js";
import { connect, type Diag, type SahouNode } from "../src/node.js";
import { pump, pumpRaw, rawSession, spawnLink, waitFor, type LinkHandle } from "./helpers.js";

const fixture = (name: string) =>
  readFileSync(fileURLToPath(new URL(`../../python/tests/fixtures/${name}`, import.meta.url)), "utf-8");
const descBase = fixture("descriptor_base.json");
const core = loadCore();

let link: LinkHandle;
beforeAll(async () => {
  link = await spawnLink();
});
afterAll(async () => {
  await link?.stop();
});

/** A pair where only the sender holds a different contract version (reproduces a live rollout; symmetric with pytest's make_mixed). */
async function makeMixed(senderDesc: string): Promise<[SahouNode, SahouNode]> {
  const a = await connect(fixture(`descriptor_${senderDesc}.json`), { node: "sensor", port: link.port, spawnLink: false });
  const b = await connect(descBase, { node: "display", port: link.port, spawnLink: false });
  return [a, b];
}

describe("compat handshake (additive passes / breaking is blocked; diagnostics are replayed from the core)", () => {
  it("additive rollout: flows after the handshake succeeds, and pending is a counted NO", async () => {
    const [a, b] = await makeMixed("additive");
    try {
      const got: Record<string, unknown>[] = [];
      const rejects: Diag[][] = [];
      await b.subscribe("touch", (p) => got.push(p), { onReject: (_c, d) => rejects.push(d) });
      const payload = { x: 0.5, phase: "move", pressure: 0.8 };
      expect(await pump(a, "touch", payload, () => got.length > 0)).toBe(true);
      expect(rejects.some((d) => d[0]?.code === "handshake_pending")).toBe(true);
      // forward compat: the receiver validates against its own type (pressure is an unknown drop during validation), and the payload is delivered as-is
      expect(got[0].x).toBe(0.5);
      expect(got[0].pressure).toBe(0.8);
      expect(b.rejectCounts.get("handshake_pending") ?? 0).toBeGreaterThanOrEqual(1);
    } finally {
      await b.close();
      await a.close();
    }
  });

  it("breaking: explicit NO with schema_incompatible, zero handler calls, diagnostics replayed byte-identically from the core", async () => {
    const [a, b] = await makeMixed("breaking");
    try {
      const got: unknown[] = [];
      const rejects: Diag[][] = [];
      await b.subscribe("touch", (p) => got.push(p), { onReject: (_c, d) => rejects.push(d) });
      const blocked = () => rejects.some((d) => d[0]?.code === "schema_incompatible");
      await pump(a, "touch", { x: "whatever", phase: "move" }, blocked);
      expect(blocked()).toBe(true);
      expect(got).toEqual([]); // not a single message from the breaking sender reaches the handler
      // Expected = the core's handshake verdict itself (cross-check of Fable Important-2)
      const rtB = new core.WasmRuntime(descBase);
      const rtA = new core.WasmRuntime(fixture("descriptor_breaking.json"));
      const frag = rtA.contract_fragment("touch");
      const senderHash = JSON.parse(frag).hash as string;
      const expected = JSON.parse(rtB.handshake("touch", senderHash, frag));
      expect(expected.verdict).toBe("blocked");
      const gotDiags = rejects.find((d) => d[0]?.code === "schema_incompatible");
      expect(gotDiags).toEqual(expected.diags);
    } finally {
      await b.close();
      await a.close();
    }
  });

  it("no handshake runs at all when the contracts are identical", async () => {
    const [a, b] = await makeMixed("base");
    try {
      const got: unknown[] = [];
      await b.subscribe("touch", (p) => got.push(p));
      await pump(a, "touch", { x: 0.5, phase: "move" }, () => got.length > 0);
      expect(b.rejectCounts.get("handshake_pending") ?? 0).toBe(0);
    } finally {
      await b.close();
      await a.close();
    }
  });

  it("unreachable (an undecodable fragment) is not cached = it does not turn into blocked and pending accumulates", async () => {
    // The TS counterpart to spec §5-4 / Python test_undecodable_fragment_is_not_cached_as_blocked.
    const b = await connect(descBase, { node: "display", port: link.port, spawnLink: false });
    const raw = await rawSession(link.port);
    try {
      const got: unknown[] = [];
      const rejects: Diag[][] = [];
      await b.subscribe("touch", (p) => got.push(p), { onReject: (_c, d) => rejects.push(d) });
      const ns = (JSON.parse(descBase) as { namespace: string }).namespace;
      const fakeHash = "deadbeef00000000";
      const contractKey = `${ns}/@sahou/contract/touch/${fakeHash}`;
      // a fake responder that returns garbage as the "real contract"
      const qh = await raw.declareQueryable(contractKey, {
        handler: async (query) => {
          try {
            await query.reply(contractKey, "{not json");
          } finally {
            await query.finalize();
          }
        },
      });
      const key = b.connectionInfo("touch").key;
      const pendingMany = () => (b.rejectCounts.get("handshake_pending") ?? 0) >= 3;
      await pumpRaw(raw, key, '{"x":0.5,"phase":"move"}', pendingMany, { attachment: fakeHash });
      expect(pendingMany()).toBe(true); // not cached → re-handshake on every mismatch detection = pending accumulates
      expect(rejects.some((d) => d[0]?.code === "schema_incompatible")).toBe(false); // does not turn into blocked
      expect(got).toEqual([]); // not a single message reaches the handler
      await qh.undeclare();
    } finally {
      await raw.close();
      await b.close();
    }
  });
});
