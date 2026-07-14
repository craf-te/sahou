import { describe, expect, it } from "vitest";
import type { Contract, Endpoints, Field } from "../src/core-bridge";
import {
  addConnection, addFieldAt, addNode, deleteConnection, deleteNode, groupFieldsAt,
  removeFieldAt, renameConnection, renameNode, setDelivery, setFrom, setNodeKind, setPattern,
  toggleTarget, ungroupFieldAt, updateConnection, updateFieldAt,
} from "../src/edits/contract-edits";
import {
  setEnv, setNamespace, setNodeConnect, setNodeMode, setPlugins, setRouter,
} from "../src/edits/endpoints-edits";
import { diffLines } from "../src/edits/diff";

const base = (): Contract => ({
  schema: "s",
  version: "1",
  nodes: { a: {}, b: {} },
  connections: {
    t: { pattern: "pub_sub", from: "a", to: ["b"], payload: { typing: "any" } },
    q: { pattern: "query", from: "a", to: ["b"], selector: "?x=1", request: { typing: "any" }, response: { typing: "any" } },
  },
});

describe("contract-edits (pure functions with immutable updates)", () => {
  it("immutable update: the original Contract never changes", () => {
    const c = base();
    const snapshot = JSON.stringify(c);
    addNode(c, { name: "z", kind: "sahou" });
    addConnection(c, { from: "a", to: ["b"], pattern: "pub_sub" });
    deleteNode(c, "a");
    renameNode(c, "a", "z");
    updateConnection(c, "t", { reliability: "reliable" });
    setPattern(c, "t", "query");
    expect(JSON.stringify(c)).toBe(snapshot);
  });

  it("addNode: adds with the name/kind confirmed in the modal · empty/duplicate is null (nothing sprouts on its own)", () => {
    const r = addNode(base(), { name: "mixer", kind: "external" })!;
    expect(r.id).toBe("mixer");
    expect(r.contract.nodes["mixer"]).toEqual({ kind: "external" });
    expect(addNode(base(), { name: "a", kind: "sahou" })).toBeNull(); // duplicate = NO
    expect(addNode(base(), { name: "  ", kind: "sahou" })).toBeNull(); // empty = NO
  });

  it("renameNode propagates to from/to, and a duplicate name is a no-op", () => {
    const c = renameNode(base(), "b", "vis");
    expect(c.nodes["vis"]).toBeDefined();
    expect(c.connections["t"].to).toEqual(["vis"]);
    expect(renameNode(c, "a", "vis")).toBe(c); // collision = no-op (same reference)
  });

  it("deleteNode removes the participating connections too", () => {
    const c = deleteNode(base(), "b");
    expect(Object.keys(c.connections)).toEqual([]);
  });

  it("setPattern drops the unrelated slots (and selector) (an edit that never creates unexpected_slot)", () => {
    const c1 = setPattern(base(), "t", "query");
    expect(c1.connections["t"].payload).toBeUndefined();
    expect(c1.connections["t"].request).toEqual({ typing: "any" });
    expect(c1.connections["t"].response).toEqual({ typing: "any" });
    const c2 = setPattern(base(), "q", "pub_sub");
    expect(c2.connections["q"].request).toBeUndefined();
    expect(c2.connections["q"].selector).toBeUndefined(); // selector is query-only (Task 5)
    expect(c2.connections["q"].payload).toEqual({ typing: "any" });
  });

  it("toggleTarget / setDelivery / updateConnection: undefined deletes the key", () => {
    const c1 = toggleTarget(base(), "t", "b");
    expect(c1.connections["t"].to).toEqual([]);
    const c2 = setDelivery(base(), "t", "reliable");
    expect(c2.connections["t"].reliability).toBe("reliable");
    expect(c2.connections["t"].congestion).toBe("block");
    const c3 = updateConnection(base(), "q", { selector: undefined });
    expect("selector" in c3.connections["q"]).toBe(false);
  });

  it("setNodeKind: sets kind without mutating", () => {
    const c = base();
    const snapshot = JSON.stringify(c);
    const r = setNodeKind(c, "a", "external");
    expect(r.nodes["a"].kind).toBe("external");
    expect(r.nodes["b"]).toEqual({}); // unrelated nodes are unchanged
    expect(JSON.stringify(c)).toBe(snapshot); // original is unchanged
  });

  it("addConnection: numbers the id from the from/to/pattern confirmed in the modal · a missing from is null", () => {
    const c = base();
    const snapshot = JSON.stringify(c);
    const r = addConnection(c, { from: "a", to: ["b"], pattern: "pub_sub" })!;
    expect(r.id).toBe("a_to_b");
    expect(r.contract.connections[r.id]).toEqual({
      pattern: "pub_sub", from: "a", to: ["b"],
      reliability: "reliable", congestion: "block", payload: { typing: "any" },
    });
    expect(JSON.stringify(c)).toBe(snapshot); // original is unchanged
    // query gets request/response slots (no payload = an edit that never creates unexpected_slot)
    const rq = addConnection(c, { from: "a", to: [], pattern: "query" })!;
    expect(rq.id).toBe("a_to_x");
    const q = rq.contract.connections[rq.id];
    expect(q.request).toEqual({ typing: "any" });
    expect(q.response).toEqual({ typing: "any" });
    expect(q.payload).toBeUndefined();
    // self-reference / unknown nodes are removed from to / a missing from · zero nodes is null
    const rf = addConnection(c, { from: "a", to: ["a", "ghost", "b"], pattern: "pub_sub" })!;
    expect(rf.contract.connections[rf.id].to).toEqual(["b"]);
    expect(addConnection(c, { from: "ghost", to: [], pattern: "pub_sub" })).toBeNull();
    expect(addConnection({ ...base(), nodes: {}, connections: {} }, { from: "a", to: [], pattern: "pub_sub" })).toBeNull();
  });

  it("deleteConnection: deletes only the target without mutating", () => {
    const c = base();
    const snapshot = JSON.stringify(c);
    const r = deleteConnection(c, "t");
    expect(Object.keys(r.connections)).toEqual(["q"]);
    expect(JSON.stringify(c)).toBe(snapshot); // original is unchanged
  });

  it("renameConnection: updates references · a collision is a no-op", () => {
    const c = base();
    const snapshot = JSON.stringify(c);
    const r = renameConnection(c, "t", "topic");
    expect(Object.keys(r.connections).sort()).toEqual(["q", "topic"]);
    expect(r.connections["topic"]).toEqual(c.connections["t"]);
    expect(renameConnection(r, "topic", "q")).toBe(r); // collision = no-op (same reference)
    expect(JSON.stringify(c)).toBe(snapshot); // original is unchanged
  });

  it("setFrom: removes the new from from to, without mutating", () => {
    const c = base();
    const snapshot = JSON.stringify(c);
    // t is from=a, to=[b]. Setting from to b removes b from to
    const r = setFrom(c, "t", "b");
    expect(r.connections["t"].from).toBe("b");
    expect(r.connections["t"].to).toEqual([]);
    expect(JSON.stringify(c)).toBe(snapshot); // original is unchanged
  });

  it("removeFieldAt: deletes a nested row without mutating", () => {
    const fields: Field[] = [
      { name: "x", type: "float" },
      { name: "meta", type: "group", fields: [{ name: "ts", type: "timestamp" }, { name: "seq", type: "int" }] },
    ];
    const snapshot = JSON.stringify(fields);
    // nested: delete meta.ts
    const r1 = removeFieldAt(fields, [1, 0]);
    expect(r1[1].fields!.map((f) => f.name)).toEqual(["seq"]);
    // root: delete x
    const r2 = removeFieldAt(fields, [0]);
    expect(r2.map((f) => f.name)).toEqual(["meta"]);
    expect(JSON.stringify(fields)).toBe(snapshot); // original is unchanged
  });

  it("fields tree: nested immutable update · group / ungroup", () => {
    const fields: Field[] = [
      { name: "x", type: "float" },
      { name: "meta", type: "group", fields: [{ name: "ts", type: "timestamp" }] },
    ];
    const snapshot = JSON.stringify(fields);
    // nested update (change the type of meta.ts to string)
    const f1 = updateFieldAt(fields, [1, 0], { type: "string" });
    expect(f1[1].fields![0].type).toBe("string");
    // set and delete a default
    const f2 = updateFieldAt(fields, [0], { default: 0.5 });
    expect(f2[0].default).toBe(0.5);
    expect("default" in updateFieldAt(f2, [0], { default: undefined })[0]).toBe(false);
    // add (into a nested level) and group / ungroup
    const f3 = addFieldAt(fields, [1]);
    expect(f3[1].fields!.length).toBe(2);
    const g = groupFieldsAt(fields, [], [0, 1]);
    expect(g.length).toBe(1);
    expect(g[0].type).toBe("group");
    expect(g[0].fields!.map((f) => f.name)).toEqual(["x", "meta"]);
    const ug = ungroupFieldAt(g, [0]);
    expect(ug.map((f) => f.name)).toEqual(["x", "meta"]);
    expect(JSON.stringify(fields)).toBe(snapshot); // original is unchanged across all operations
  });
});

describe("endpoints-edits / diff", () => {
  const baseE = (): Endpoints => ({ namespace: "sahou", nodes: {} });

  it("setNodeMode: switching back to auto removes the whole entry (progressive disclosure)", () => {
    const e = { namespace: "sahou", nodes: {} };
    const e1 = setNodeMode(e, "a", "client");
    expect(e1.nodes["a"]).toEqual({ mode: "client" });
    const e2 = setNodeMode(e1, "a", "auto");
    expect("a" in e2.nodes).toBe(false);
  });

  it("setNamespace: an empty string falls back to the default sahou · without mutating", () => {
    const e = baseE();
    const snapshot = JSON.stringify(e);
    expect(setNamespace(e, "prod").namespace).toBe("prod");
    expect(setNamespace(e, "").namespace).toBe("sahou");
    expect(JSON.stringify(e)).toBe(snapshot); // original is unchanged
  });

  it("setEnv: sets, and an empty string deletes the entry · without mutating", () => {
    const e = baseE();
    const snapshot = JSON.stringify(e);
    const e1 = setEnv(e, "stg");
    expect(e1.env).toBe("stg");
    const e2 = setEnv(e1, "");
    expect("env" in e2).toBe(false); // empty = delete the key
    expect(JSON.stringify(e)).toBe(snapshot); // original is unchanged
  });

  it("setRouter: enabled=false removes the router key · without mutating", () => {
    const e = baseE();
    const snapshot = JSON.stringify(e);
    const e1 = setRouter(e, true, "tcp/1.2.3.4:7447");
    expect(e1.router).toEqual({ enabled: true, endpoint: "tcp/1.2.3.4:7447" });
    const e2 = setRouter(e1, true); // no endpoint
    expect(e2.router).toEqual({ enabled: true });
    const e3 = setRouter(e1, false);
    expect("router" in e3).toBe(false); // false = delete the key
    expect(JSON.stringify(e)).toBe(snapshot); // original is unchanged
  });

  it("setNodeConnect: turns CSV into an array · empty deletes the connect key (the whole entry disappears) · without mutating", () => {
    const e = baseE();
    const snapshot = JSON.stringify(e);
    const e1 = setNodeConnect(e, "a", " tcp/a:1 , tcp/b:2 ");
    expect(e1.nodes["a"].connect).toEqual(["tcp/a:1", "tcp/b:2"]);
    const e2 = setNodeConnect(e1, "a", "");
    expect("a" in e2.nodes).toBe(false); // an entry that had only connect becomes empty and is removed
    expect(JSON.stringify(e)).toBe(snapshot); // original is unchanged
  });

  it("setPlugins: turns CSV into an array · empty deletes the plugins key · without mutating", () => {
    const e = baseE();
    const snapshot = JSON.stringify(e);
    const e1 = setPlugins(e, "storage, webserver");
    expect(e1.plugins).toEqual(["storage", "webserver"]);
    const e2 = setPlugins(e1, "");
    expect("plugins" in e2).toBe(false); // empty = delete the key
    expect(JSON.stringify(e)).toBe(snapshot); // original is unchanged
  });

  it("diffLines: line-wise add/del/same", () => {
    const d = diffLines("a\nb\nc", "a\nx\nc");
    expect(d).toEqual([
      { kind: "same", text: "a" },
      { kind: "del", text: "b" },
      { kind: "add", text: "x" },
      { kind: "same", text: "c" },
    ]);
  });

  it("diffLines: empty-input edge cases (documenting known behavior)", () => {
    // Because "".split("\n") === [""], two empty strings return a meaningless single "same" line.
    // This is a display-only diff so it's harmless; the behavior is fixed on the assumption that the
    // caller (Task 13/14) rejects an empty contract.
    expect(diffLines("", "")).toEqual([{ kind: "same", text: "" }]);
    // one side empty: the other side's full lines + one empty line
    expect(diffLines("", "a\nb")).toEqual([
      { kind: "del", text: "" },
      { kind: "add", text: "a" },
      { kind: "add", text: "b" },
    ]);
  });
});
