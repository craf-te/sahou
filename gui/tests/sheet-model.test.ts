import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";
import { initCore, parse } from "../src/core-bridge";
import { receives, sends, slotNamesOf, typeSummary } from "../src/table/sheet-model";

const at = (rel: string) => fileURLToPath(new URL(rel, import.meta.url));
const DEMO = readFileSync(at("../../examples/demo/schema.sahou.yaml"), "utf-8");

beforeAll(async () => {
  await initCore(readFileSync(at("../src/core-wasm/sahou_core_bg.wasm")));
});

describe("sheet-model (node-centric row models)", () => {
  it("slotNamesOf: pub_sub → [payload], query → [request, response]", () => {
    expect(slotNamesOf({ pattern: "pub_sub", from: "a", to: ["b"] })).toEqual(["payload"]);
    expect(slotNamesOf({ pattern: "query", from: "a", to: ["b"] })).toEqual(["request", "response"]);
  });

  it("sends/receives split a node's connections by direction", () => {
    const c = parse(DEMO);
    for (const node of Object.keys(c.nodes)) {
      for (const id of sends(c, node)) expect(c.connections[id].from).toBe(node);
      for (const id of receives(c, node)) expect(c.connections[id].to).toContain(node);
    }
    // sensor is the demo's producer: it sends but does not appear in any `to`
    expect(sends(c, "sensor").length).toBeGreaterThan(0);
  });

  it("typeSummary condenses a slot to one line", () => {
    expect(typeSummary(undefined)).toBe("—");
    expect(typeSummary({ typing: "any" })).toBe("any");
    expect(typeSummary({ typing: "typed", kind: "opaque", encoding: "video/raw" })).toBe("opaque (video/raw)");
    expect(typeSummary({ typing: "typed", fields: [] })).toBe("record {}");
    expect(
      typeSummary({ typing: "typed", fields: [{ name: "x", type: "float" }, { name: "y", type: "int" }] }),
    ).toBe("x: float, y: int");
    expect(
      typeSummary({
        typing: "typed",
        fields: [
          { name: "a", type: "int" }, { name: "b", type: "int" },
          { name: "c", type: "int" }, { name: "d", type: "int" },
        ],
      }),
    ).toBe("a: int, b: int, c: int, +1");
  });
});
