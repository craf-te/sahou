// Pane tests. First half (FieldsTable / ShapeEditor):
// immutable updates (the editing-layer pure functions emit a new array) · the default field (§5.1) ·
// wasm_sample suggestions · inline display of the core's NO for a default type mismatch (validatePayload —
// the GUI does not reimplement validation §4).
// Second half (Task 14): diag-target / ConnectionPane selector (§5.2) / DiagPane jump /
// ConflictDialog 3-way choice + raw-text diff (§5.3) / NodePane / DeployPane.
import { mount } from "@vue/test-utils";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";
import type { Contract, Endpoints, Field, SahouDiag, Slot } from "../src/core-bridge";
import { initCore } from "../src/core-bridge";
import ConflictDialog from "../src/panes/ConflictDialog.vue";
import ConnectionPane from "../src/panes/ConnectionPane.vue";
import DeployPane from "../src/panes/DeployPane.vue";
import { targetOf } from "../src/panes/diag-target";
import DiagPane from "../src/panes/DiagPane.vue";
import FieldsTable from "../src/panes/FieldsTable.vue";
import NodePane from "../src/panes/NodePane.vue";
import ShapeEditor from "../src/panes/ShapeEditor.vue";

const at = (rel: string) => fileURLToPath(new URL(rel, import.meta.url));

beforeAll(async () => {
  await initCore(readFileSync(at("../src/core-wasm/sahou_core_bg.wasm")));
});

describe("FieldsTable (with the default field · immutable update)", () => {
  const fields: Field[] = [
    { name: "x", type: "float" },
    { name: "phase", type: "enum", values: ["down", "up"] },
  ];

  it("a name change emits a new array and the prop is unchanged", async () => {
    const w = mount(FieldsTable, { props: { fields } });
    const nameInput = w.findAll("input[placeholder='name']")[0];
    await nameInput.setValue("posx");
    await nameInput.trigger("change");
    const emitted = w.emitted("change")![0][0] as Field[];
    expect(emitted[0].name).toBe("posx");
    expect(fields[0].name).toBe("x"); // unchanged
  });

  it("default field: JSON input → value / empty → delete the key (§5.1)", async () => {
    const w = mount(FieldsTable, { props: { fields } });
    const def = w.findAll("input.default")[0];
    await def.setValue("0.5");
    await def.trigger("change");
    expect((w.emitted("change")![0][0] as Field[])[0].default).toBe(0.5);
    const w2 = mount(FieldsTable, { props: { fields: [{ name: "x", type: "float", default: 1 }] } });
    const def2 = w2.findAll("input.default")[0];
    await def2.setValue("");
    await def2.trigger("change");
    expect("default" in (w2.emitted("change")![0][0] as Field[])[0]).toBe(false);
  });

  it("if there's a suggestion, the \"e.g.\" link adopts it as the default (from wasm_sample §5.1)", async () => {
    const w = mount(FieldsTable, { props: { fields, suggestion: { x: 0.25 } } });
    await w.findAll("a.adopt")[0].trigger("click");
    expect((w.emitted("change")![0][0] as Field[])[0].default).toBe(0.25);
  });

  it("a type-mismatch default shows the core's NO ({code,path,message}) inline", () => {
    const bad: Field[] = [{ name: "x", type: "float", default: "abc" }];
    const w = mount(FieldsTable, { props: { fields: bad } });
    const diag = w.find("[data-testid='field-default-diag']");
    expect(diag.exists()).toBe(true);
    expect(diag.text()).toContain("type_mismatch"); // the core's code verbatim
    // if the type is consistent, no NO
    const ok = mount(FieldsTable, {
      props: { fields: [{ name: "x", type: "float", default: 1.5 }] },
    });
    expect(ok.find("[data-testid='field-default-diag']").exists()).toBe(false);
  });
});

describe("ShapeEditor", () => {
  it("any shows a warning · typed record shows a FieldsTable", () => {
    const anySlot: Slot = { typing: "any" };
    const w1 = mount(ShapeEditor, { props: { shape: anySlot, label: "payload" } });
    expect(w1.text()).toContain("Unvalidated");
    const typed: Slot = { typing: "typed", fields: [{ name: "x", type: "float" }] };
    const w2 = mount(ShapeEditor, { props: { shape: typed, label: "payload" } });
    expect(w2.findComponent(FieldsTable).exists()).toBe(true);
  });

  it("for a typed record it builds a suggestion from the core sample (no suggestion if fields are empty)", () => {
    const typed: Slot = {
      typing: "typed",
      fields: [{ name: "phase", type: "enum", values: ["down", "up"] }],
    };
    const w = mount(ShapeEditor, { props: { shape: typed, label: "payload" } });
    const table = w.findComponent(FieldsTable);
    const sug = table.props("suggestion") as Record<string, unknown>;
    expect(["down", "up"]).toContain(sug["phase"]);
    // empty fields = no suggestion (doesn't call sample · doesn't throw)
    const empty: Slot = { typing: "typed", fields: [] };
    const w2 = mount(ShapeEditor, { props: { shape: empty, label: "payload" } });
    expect(w2.findComponent(FieldsTable).props("suggestion")).toBeUndefined();
  });
});

// ---- Task 14 (later panes) starts here ----

describe("diag-target (unified path grammar spec §4 → pane jump)", () => {
  it("connections.<id>… is an edge / nodes.<id> is a node / anything else is null", () => {
    expect(targetOf("connections.touch.payload.fields[2].name")).toEqual({ kind: "edge", id: "touch" });
    expect(targetOf("connections.get_state.selector")).toEqual({ kind: "edge", id: "get_state" });
    expect(targetOf("nodes.sensor")).toEqual({ kind: "node", id: "sensor" });
    expect(targetOf("$")).toBeNull();
  });
});

describe("ConnectionPane's selector field (§5.2)", () => {
  const c: Contract = {
    schema: "s", version: "1",
    nodes: { a: {}, b: {} },
    connections: {
      p: { pattern: "pub_sub", from: "a", to: ["b"], payload: { typing: "any" } },
      q: { pattern: "query", from: "a", to: ["b"], request: { typing: "any" }, response: { typing: "any" } },
    },
  };

  it("the selector field appears only for query, and editing it lands in the contract JSON", async () => {
    const wq = mount(ConnectionPane, { props: { contract: c, id: "q", descriptor: null, descriptorStale: false } });
    const sel = wq.find("input[placeholder='?level=info']");
    expect(sel.exists()).toBe(true);
    await sel.setValue("?level=warn");
    await sel.trigger("change");
    const emitted = wq.emitted("change")![0][0] as Contract;
    expect(emitted.connections["q"].selector).toBe("?level=warn");
    const wp = mount(ConnectionPane, { props: { contract: c, id: "p", descriptor: null, descriptorStale: false } });
    expect(wp.find("input[placeholder='?level=info']").exists()).toBe(false);
  });

  it("declarative to (§9): from itself is not offered as a target (you can't draw a self-loop)", () => {
    const w = mount(ConnectionPane, { props: { contract: c, id: "p", descriptor: null, descriptorStale: false } });
    const labels = w.findAll(".to-list label").map((l) => l.text());
    expect(labels).toContain("b");
    expect(labels).not.toContain("a"); // from = a is not among the candidates
  });

  it("when descriptorStale, the effective keyexpr shows a stale marker (§7)", () => {
    const w = mount(ConnectionPane, {
      props: { contract: c, id: "p", descriptor: null, descriptorStale: true },
    });
    expect(w.find("[data-testid='keyexpr-stale']").exists()).toBe(true);
  });
});

describe("DiagPane (shows the core's diagnostics verbatim · path jump)", () => {
  const diags: SahouDiag[] = [
    { code: "unknown_node", path: "connections.touch.to[0]", message: "undefined node 'ghost'" },
    { code: "parse_error", path: "$", message: "NO at root" },
  ];

  it("outputs {code, path, message} verbatim without alteration (zero GUI-authored error text §6)", () => {
    const w = mount(DiagPane, { props: { diags } });
    const rows = w.findAll("[data-testid='diag-row']");
    expect(rows.length).toBe(2);
    expect(rows[0].text()).toContain("unknown_node");
    expect(rows[0].text()).toContain("connections.touch.to[0]");
    expect(rows[0].text()).toContain("undefined node 'ghost'");
  });

  it("clicking emits the targetOf result to jump / an unmappable path does not emit", async () => {
    const w = mount(DiagPane, { props: { diags } });
    const rows = w.findAll("[data-testid='diag-row']");
    await rows[0].trigger("click");
    expect(w.emitted("jump")![0][0]).toEqual({ kind: "edge", id: "touch" });
    await rows[1].trigger("click"); // the path "$" maps to no pane
    expect(w.emitted("jump")!.length).toBe(1);
  });
});

describe("ConflictDialog (§5.3: 3-way choice + raw-text diff · never auto-overwrites)", () => {
  const props = { kind: "schema", mine: "a\nb\n", theirs: "a\nc\n" };

  it("all 3 choices are present, and keep/discard each emit resolve", async () => {
    const w = mount(ConflictDialog, { props });
    const texts = w.findAll("button").map((b) => b.text());
    expect(texts.some((t) => t.includes("Keep"))).toBe(true);
    expect(texts.some((t) => t.includes("Discard"))).toBe(true);
    expect(texts.some((t) => t.includes("diff"))).toBe(true);
    await w.findAll("button").find((b) => b.text().includes("Keep"))!.trigger("click");
    expect(w.emitted("resolve")![0]).toEqual(["keep"]);
    await w.findAll("button").find((b) => b.text().includes("Discard"))!.trigger("click");
    expect(w.emitted("resolve")![1]).toEqual(["reload"]);
  });

  it("\"Show diff\" shows a line-wise raw-text diff (- local / + external)", async () => {
    const w = mount(ConflictDialog, { props });
    expect(w.find("[data-testid='conflict-diff']").exists()).toBe(false);
    await w.findAll("button").find((b) => b.text().includes("diff"))!.trigger("click");
    const diff = w.find("[data-testid='conflict-diff']");
    expect(diff.exists()).toBe(true);
    expect(diff.text()).toContain("- b");
    expect(diff.text()).toContain("+ c");
  });
});

describe("NodePane (rename / kind / delete — all via the editing-layer pure functions)", () => {
  const c: Contract = {
    schema: "s", version: "1",
    nodes: { a: {}, b: {} },
    connections: { l: { pattern: "pub_sub", from: "a", to: ["b"], payload: { typing: "any" } } },
  };

  it("rename emits change + renamed, and the wiring's from/to follow along (pure renameNode)", async () => {
    const w = mount(NodePane, { props: { contract: c, id: "a" } });
    const name = w.find("input");
    await name.setValue("sensor");
    await name.trigger("change");
    const next = w.emitted("change")![0][0] as Contract;
    expect("sensor" in next.nodes).toBe(true);
    expect(next.connections["l"].from).toBe("sensor");
    expect(w.emitted("renamed")![0]).toEqual(["sensor"]);
    expect("a" in c.nodes).toBe(true); // unchanged
  });

  it("＋ Connection (from this node): emits connect (the contextual add-connection affordance)", async () => {
    const w = mount(NodePane, { props: { contract: c, id: "a" } });
    await w.find("[data-testid='node-add-conn']").trigger("click");
    expect(w.emitted("connect")).toBeTruthy();
    expect(w.emitted("change")).toBeUndefined(); // the contract isn't changed here (added on modal confirm)
  });

  it("kind switch and delete (delete takes the connections with it = deleteNode)", async () => {
    const w = mount(NodePane, { props: { contract: c, id: "a" } });
    await w.findAll("button").find((b) => b.text() === "external")!.trigger("click");
    expect((w.emitted("change")![0][0] as Contract).nodes["a"].kind).toBe("external");
    await w.find("button.danger").trigger("click");
    const afterDel = w.emitted("change")![1][0] as Contract;
    expect("a" in afterDel.nodes).toBe(false);
    expect(Object.keys(afterDel.connections)).toEqual([]);
    expect(w.emitted("deleted")).toBeTruthy();
  });
});

describe("DeployPane (endpoints-edits pure functions → change emit · separate from the contract §6)", () => {
  const e: Endpoints = { env: "dev", namespace: "sahou", nodes: {} };

  it("editing namespace emits the setNamespace result", async () => {
    const w = mount(DeployPane, { props: { endpoints: e, nodes: ["a"] } });
    const ns = w.findAll("input").find((i) => i.attributes("aria-label") === "namespace");
    await ns!.setValue("stage");
    await ns!.trigger("change");
    expect((w.emitted("change")![0][0] as Endpoints).namespace).toBe("stage");
    expect(e.namespace).toBe("sahou"); // unchanged
  });

  it("enabling router and overriding a node's mode land in the endpoints JSON", async () => {
    const w = mount(DeployPane, { props: { endpoints: e, nodes: ["a"] } });
    const chk = w.find("input[type='checkbox']");
    await chk.setValue(true);
    expect((w.emitted("change")![0][0] as Endpoints).router?.enabled).toBe(true);
    const mode = w.find("table.dep select");
    await mode.setValue("client");
    expect((w.emitted("change")![1][0] as Endpoints).nodes["a"]?.mode).toBe("client");
  });
});
