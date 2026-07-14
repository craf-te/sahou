import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";
import * as core from "../src/core-bridge";

const at = (rel: string) => fileURLToPath(new URL(rel, import.meta.url));
const DEMO = readFileSync(at("../../examples/demo/schema.sahou.yaml"), "utf-8");

beforeAll(async () => {
  // under node (vitest) we initialize from a byte array, not fetch
  await core.initCore(readFileSync(at("../src/core-wasm/sahou_core_bg.wasm")));
});

describe("core-bridge (the single entry point to the core wasm · design §2.2)", () => {
  it("parse→serialize→parse is structurally equal (round-trip idempotent · design §8)", () => {
    const c1 = core.parse(DEMO);
    expect(c1.schema).toBe("demo_installation");
    const yaml = core.serialize(c1);
    expect(core.parse(yaml)).toEqual(c1);
  });

  it("a parse NO is CoreNo (the core's positioned diagnostics verbatim · no GUI-authored text §6)", () => {
    const broken = "schema: s\nnodes:\n  a: {}\n  a: {}\nconnections: {}\n";
    expect(() => core.parse(broken)).toThrowError(core.CoreNo);
    try {
      core.parse(broken);
    } catch (e) {
      expect((e as core.CoreNo).diags[0].code).toBe("parse_error");
    }
  });

  it("validateSchema enumerates diagnostics without throwing (for the diagnostics pane)", () => {
    expect(core.validateSchema(DEMO)).toEqual([]);
    const diags = core.validateSchema(
      "schema: s\nnodes:\n  a: {}\nconnections:\n  bad:\n    pattern: pub_sub\n    from: a\n    to: [a]\n    payload: { typing: any }\n",
    );
    expect(diags[0].code).toBe("self_loop");
    expect(diags[0].path).toBe("connections.bad.to[0]");
  });

  it("sample passes validatePayload (the basis of the default suggestion §5.1)", () => {
    const c = core.parse(DEMO);
    const slot = c.connections["touch"].payload!;
    const s = core.sample(slot);
    expect(core.validatePayload(slot, s)).toEqual([]);
    expect(core.validatePayload(slot, { x: "bad" })[0].code).toBe("type_mismatch");
  });

  it("descriptor resolves keyexpr / hash (the source for the effective keyexpr display §3.2)", () => {
    const d = core.descriptor(DEMO, "namespace: sahou/demo\n");
    expect(d.connections["touch"].key).toBe("sahou/demo/touch");
    expect(d.connections["touch"].hash).toHaveLength(16);
    // while there are schema diagnostics it's a CoreNo (the GUI shows it as stale §7)
    expect(() =>
      core.descriptor(
        "schema: s\nnodes:\n  a: {}\nconnections:\n  bad:\n    pattern: pub_sub\n    from: a\n    to: [a]\n    payload: { typing: any }\n",
        "",
      ),
    ).toThrowError(core.CoreNo);
  });

  it("endpoints is symmetric with schema (parse/serialize round-trip · empty = default §1)", () => {
    const e = core.parseEndpoints("env: dev\nnamespace: sahou/demo\n");
    expect(e.namespace).toBe("sahou/demo");
    expect(core.parseEndpoints(core.serializeEndpoints(e))).toEqual(e);
    expect(core.parseEndpoints("").namespace).toBe("sahou");
  });
});
