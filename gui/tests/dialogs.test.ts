// Add-style modals (UX redesign): "press it and it sprouts instantly" is gone; these are part tests
// for the new flow where a value is confirmed in the modal before adding. The add itself is the pure
// contract-edits functions (App side) — here we check value collection, the pre-check (duplicate
// name), declarative to (§9: offer only nodes other than from), and a11y (Esc / focus).
import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import type { Contract } from "../src/core-bridge";
import AddConnectionDialog from "../src/panes/AddConnectionDialog.vue";
import AddNodeDialog from "../src/panes/AddNodeDialog.vue";

const base = (): Contract => ({
  schema: "s",
  version: "1",
  nodes: { a: {}, b: {}, c: {} },
  connections: {},
});

describe("AddNodeDialog (adding a node happens only on modal confirm)", () => {
  it("the initial name is a unique suggestion · confirm emits add(name, kind)", async () => {
    const w = mount(AddNodeDialog, { props: { contract: base() } });
    const name = w.find("input[aria-label='new node name']");
    expect((name.element as HTMLInputElement).value).not.toBe(""); // there is a suggestion
    await name.setValue("mixer");
    await w.findAll("button").find((b) => b.text() === "external")!.trigger("click");
    await w.find("[data-testid='dialog-confirm']").trigger("click");
    expect(w.emitted("add")![0]).toEqual([{ name: "mixer", kind: "external" }]);
  });

  it("a duplicate / empty name can't be confirmed + shows the reason (never a silent no-op)", async () => {
    const w = mount(AddNodeDialog, { props: { contract: base() } });
    const name = w.find("input[aria-label='new node name']");
    await name.setValue("a"); // collides with an existing node
    expect(w.find("[data-testid='dialog-error']").exists()).toBe(true);
    expect(w.find("[data-testid='dialog-confirm']").attributes("disabled")).toBeDefined();
    await name.setValue("");
    expect(w.find("[data-testid='dialog-confirm']").attributes("disabled")).toBeDefined();
    expect(w.emitted("add")).toBeUndefined();
  });

  it("Esc / Cancel closes (closes without adding)", async () => {
    const w = mount(AddNodeDialog, { props: { contract: base() } });
    await w.find("input").trigger("keydown", { key: "Escape" });
    expect(w.emitted("close")).toBeTruthy();
    await w.find("[data-testid='dialog-cancel']").trigger("click");
    expect(w.emitted("close")!.length).toBe(2);
    expect(w.emitted("add")).toBeUndefined();
  });

  it("initial focus goes to the name input when opened (keyboard interaction)", () => {
    const w = mount(AddNodeDialog, { props: { contract: base() }, attachTo: document.body });
    expect(document.activeElement).toBe(w.find("input[aria-label='new node name']").element);
    w.unmount();
  });
});

describe("AddConnectionDialog (from pre-selected + declarative to §9)", () => {
  it("initialFrom is pre-selected as from · to candidates are only nodes other than from", () => {
    const w = mount(AddConnectionDialog, { props: { contract: base(), initialFrom: "b" } });
    const from = w.find("select[aria-label='from']");
    expect((from.element as HTMLSelectElement).value).toBe("b");
    const labels = w.findAll(".to-list label").map((l) => l.text());
    expect(labels).toEqual(["a", "c"]); // a self-loop is impossible to draw by construction
  });

  it("without initialFrom it's the first node · changing from removes the new from from to", async () => {
    const w = mount(AddConnectionDialog, { props: { contract: base() } });
    const from = w.find("select[aria-label='from']");
    expect((from.element as HTMLSelectElement).value).toBe("a");
    // pick b, c as targets → change from to b → b is removed from to
    for (const t of ["b", "c"]) {
      await w.findAll(".to-list label").find((l) => l.text() === t)!.find("input").setValue(true);
    }
    await from.setValue("b");
    await w.find("[data-testid='dialog-confirm']").trigger("click");
    expect(w.emitted("add")![0]).toEqual([{ from: "b", to: ["c"], pattern: "pub_sub" }]);
  });

  it("can set pattern to query and confirm", async () => {
    const w = mount(AddConnectionDialog, { props: { contract: base(), initialFrom: "a" } });
    await w.findAll("button").find((b) => b.text() === "query")!.trigger("click");
    await w.find("[data-testid='dialog-confirm']").trigger("click");
    expect(w.emitted("add")![0]).toEqual([{ from: "a", to: [], pattern: "query" }]);
  });

  it("with 0 nodes it can't be confirmed + shows an affordance", () => {
    const empty: Contract = { schema: "s", version: "1", nodes: {}, connections: {} };
    const w = mount(AddConnectionDialog, { props: { contract: empty } });
    expect(w.find("[data-testid='dialog-confirm']").attributes("disabled")).toBeDefined();
    expect(w.text()).toContain("＋ Node");
  });

  it("Esc closes (closes without adding)", async () => {
    const w = mount(AddConnectionDialog, { props: { contract: base() } });
    await w.find("select").trigger("keydown", { key: "Escape" });
    expect(w.emitted("close")).toBeTruthy();
    expect(w.emitted("add")).toBeUndefined();
  });
});
