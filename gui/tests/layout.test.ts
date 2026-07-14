import { describe, expect, it } from "vitest";
import { emptyLayout, withNodePos } from "../src/layout";

describe("layout (GUI-only coordinates · immutable update)", () => {
  it("emptyLayout: returns empty nodes", () => {
    expect(emptyLayout()).toEqual({ nodes: {} });
  });

  it("withNodePos: sets a coordinate without mutating", () => {
    const l = emptyLayout();
    const snapshot = JSON.stringify(l);
    const l1 = withNodePos(l, "a", 10, 20);
    expect(l1.nodes["a"]).toEqual({ x: 10, y: 20 });
    expect(JSON.stringify(l)).toBe(snapshot); // original is unchanged
  });

  it("withNodePos: overwriting an existing coordinate is also non-mutating (the original LayoutFile is unchanged)", () => {
    const l = withNodePos(emptyLayout(), "a", 1, 2);
    const snapshot = JSON.stringify(l);
    const l1 = withNodePos(l, "a", 3, 4); // update the same id
    expect(l1.nodes["a"]).toEqual({ x: 3, y: 4 });
    const l2 = withNodePos(l, "b", 5, 6); // add a different id
    expect(l2.nodes).toEqual({ a: { x: 1, y: 2 }, b: { x: 5, y: 6 } });
    expect(JSON.stringify(l)).toBe(snapshot); // original is unchanged
  });
});
