// Node-centric spreadsheet view: one sheet (tab) per node, with Sends (from === node) and Receives
// (node in to) sections. Each row is a connection whose type is edited inline on expand. Editing goes
// through the pure contract-edits functions and emits a new contract (no validation here §4).
import { mount } from "@vue/test-utils";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";
import type { Contract, Field } from "../src/core-bridge";
import { initCore } from "../src/core-bridge";
import TableView from "../src/panes/TableView.vue";

const at = (rel: string) => fileURLToPath(new URL(rel, import.meta.url));
beforeAll(async () => {
  await initCore(readFileSync(at("../src/core-wasm/sahou_core_bg.wasm")));
});

const base = (): Contract => ({
  schema: "s",
  version: "1",
  nodes: { a: {}, b: {} },
  connections: {
    c1: {
      pattern: "pub_sub",
      from: "a",
      to: ["b"],
      payload: { typing: "typed", fields: [{ name: "x", type: "float" }] },
    },
  },
});

const props = (contract = base()) => ({ contract, descriptor: null, selected: null });
const changes = (w: ReturnType<typeof mount>) => (w.emitted("change") ?? []) as unknown[][];
const firstChange = (w: ReturnType<typeof mount>) => changes(w)[0][0] as Contract;
const lastChange = (w: ReturnType<typeof mount>) => changes(w).at(-1)![0] as Contract;

describe("TableView (node-centric sheets)", () => {
  it("shows one tab per node and lists the active node's sends", () => {
    const w = mount(TableView, { props: props() });
    expect(w.find("[data-testid='node-tab-a']").exists()).toBe(true);
    expect(w.find("[data-testid='node-tab-b']").exists()).toBe(true);
    // default active = first node (a): c1 is a send, no receives
    expect(w.findAll("[data-testid='conn-row-send']").length).toBe(1);
    const recv = w.find("[data-testid='receives-section']");
    expect(recv.findAll("[data-testid='conn-row-receive']").length).toBe(0);
  });

  it("switching to node b shows c1 as a receive", async () => {
    const w = mount(TableView, { props: props() });
    await w.find("[data-testid='node-tab-b']").trigger("click");
    expect(w.emitted("select")!.at(-1)![0]).toEqual({ kind: "node", id: "b" });
    expect(w.findAll("[data-testid='conn-row-send']").length).toBe(0);
    expect(w.findAll("[data-testid='conn-row-receive']").length).toBe(1);
  });

  it("＋ adds a node (new sheet) and selects it", async () => {
    const w = mount(TableView, { props: props() });
    await w.find("[data-testid='add-node-sheet']").trigger("click");
    expect(Object.keys(lastChange(w).nodes).length).toBe(3);
    expect((w.emitted("select")!.at(-1)![0] as { kind: string }).kind).toBe("node");
  });

  it("＋ send adds a connection from the active node", async () => {
    const w = mount(TableView, { props: props() });
    await w.find("[data-testid='add-send']").trigger("click");
    const next = lastChange(w);
    const added = Object.values(next.connections).find((c) => c.from === "a" && c.to.length === 0);
    expect(added).toBeDefined();
  });

  it("expanding a send row edits its type inline (patches the connection's slot)", async () => {
    const w = mount(TableView, { props: props() });
    await w.find("[data-testid='expand-c1']").trigger("click");
    expect(w.find("[data-testid='shape-editor']").exists()).toBe(true);
    const nameInput = w.find("input[placeholder='name']"); // c1.payload's field x
    await nameInput.setValue("y");
    await nameInput.trigger("change");
    expect((firstChange(w).connections.c1.payload!.fields as Field[])[0].name).toBe("y");
  });

  it("editing the key sets a per-connection override", async () => {
    const w = mount(TableView, { props: props() });
    const keyInput = w.find("input[aria-label='Key (keyexpr)']");
    await keyInput.setValue("custom/key");
    await keyInput.trigger("change");
    expect(lastChange(w).connections.c1.key).toBe("custom/key");
  });

  it("deleting a send removes the connection; a receive delete only unsubscribes the node", async () => {
    const del = mount(TableView, { props: props() });
    await del.find("[data-testid='conn-row-send'] .del").trigger("click");
    expect(Object.keys(lastChange(del).connections)).toEqual([]);

    const unsub = mount(TableView, { props: props() });
    await unsub.find("[data-testid='node-tab-b']").trigger("click");
    await unsub.find("[data-testid='conn-row-receive'] .del").trigger("click");
    const next = lastChange(unsub);
    expect(next.connections.c1).toBeDefined(); // connection kept
    expect(next.connections.c1.to).toEqual([]); // b removed from targets
  });

  it("renaming the active node in the header rewrites the node id", async () => {
    const w = mount(TableView, { props: props() });
    const nameInput = w.find("input[aria-label='Node name']");
    await nameInput.setValue("aa");
    await nameInput.trigger("change");
    expect(lastChange(w).nodes.aa).toBeDefined();
    expect(lastChange(w).nodes.a).toBeUndefined();
  });
});
