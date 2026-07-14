import { describe, expect, it } from "vitest";
import type { Contract } from "../src/core-bridge";
import { buildData, TOPIC_PREFIX } from "../src/graph/build-data";
import { connLoose, edgeStyle, GRAY, nodeCaps, RED, VIOLET } from "../src/graph/edge-style";

const contract: Contract = {
  schema: "s",
  version: "1",
  nodes: { a: {}, b: {}, c: { kind: "external" } },
  connections: {
    t: {
      pattern: "pub_sub", from: "a", to: ["b", "c"],
      reliability: "reliable", congestion: "block",
      payload: { typing: "typed", fields: [{ name: "x", type: "float" }] },
    },
    loose: { pattern: "pub_sub", from: "a", to: ["b"], payload: { typing: "any" } },
    q: {
      pattern: "query", from: "a", to: ["b"],
      request: { typing: "typed", fields: [{ name: "sel", type: "string" }] },
      response: { typing: "typed", fields: [{ name: "level", type: "int" }] },
    },
  },
};
const layout = { nodes: { a: { x: 0, y: 0 }, b: { x: 200, y: 0 }, c: { x: 200, y: 100 } } };

describe("graph derivation (a derived display of the contract · not validation §4)", () => {
  it("edgeStyle: typed+reliable=GRAY solid / any=RED dashed / typed query=VIOLET bidirectional", () => {
    expect(edgeStyle(contract.connections["t"]).stroke).toBe(GRAY);
    const loose = edgeStyle(contract.connections["loose"]);
    expect(loose.stroke).toBe(RED);
    expect(loose.lineDash).toEqual([3, 3]);
    expect(connLoose(contract.connections["loose"])).toBe(true);
    // loose is decided per slot (a query is red if either slot is any — visualizing "unvalidated" takes priority)
    const q = edgeStyle(contract.connections["q"]);
    expect(q.stroke).toBe(VIOLET);
    expect(q.startArrow).toBe(true);
    expect(connLoose(contract.connections["q"])).toBe(false);
  });

  it("nodeCaps: capabilities are derived from the wiring (no roles)", () => {
    expect(nodeCaps(contract, "a").sort()).toEqual(["pub", "qry"]);
    expect(nodeCaps(contract, "b").sort()).toEqual(["ans", "sub"]);
  });

  it("direct: one edge per target / bus: a topic node + 1 pub + fan-out", () => {
    const direct = buildData(contract, layout, "direct");
    expect(direct.nodes.length).toBe(3);
    expect(direct.edges.filter((e) => (e.id as string).startsWith("t__")).length).toBe(2);
    const bus = buildData(contract, layout, "bus");
    const topic = bus.nodes.find((n) => n.id === `${TOPIC_PREFIX}t`);
    expect(topic).toBeDefined();
    expect(bus.edges.filter((e) => (e.data as { connection: string }).connection === "t").length).toBe(3);
    // connections with a single target, and queries, stay direct even in bus mode
    expect(bus.nodes.find((n) => n.id === `${TOPIC_PREFIX}q`)).toBeUndefined();
  });

  it("pure function: does not mutate contract / layout", () => {
    const c0 = JSON.stringify(contract);
    const l0 = JSON.stringify(layout);
    buildData(contract, layout, "bus");
    expect(JSON.stringify(contract)).toBe(c0);
    expect(JSON.stringify(layout)).toBe(l0);
  });
});
