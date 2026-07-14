// Integration tests for the UX redesign: the three top-level tabs (Design/Deploy/Diagnostics) ·
// demoting the wiring-display toggle into the canvas (Per-connection/Topic bus) · the in-canvas
// "＋ Node" · the contextual "＋ Connection" on node selection (auto-select from) · adding only on
// modal confirm (instant add removed).
// G6 needs canvas, so the rendering layer is stubbed and the onSelect handler is captured to simulate selection.
import { flushPromises, mount } from "@vue/test-utils";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { initCore } from "../src/core-bridge";
import type { GraphHandlers } from "../src/graph/use-graph";
import ConnectionPane from "../src/panes/ConnectionPane.vue";
import NodePane from "../src/panes/NodePane.vue";

const at = (rel: string) => fileURLToPath(new URL(rel, import.meta.url));
const DEMO = readFileSync(at("../../examples/demo/schema.sahou.yaml"), "utf-8");

const mocks = vi.hoisted(() => ({
  getFiles: vi.fn(),
  putFile: vi.fn(async () => "e-next"),
  openWatch: vi.fn(() => () => {}),
}));
vi.mock("../src/api", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../src/api")>();
  return { ...orig, getFiles: mocks.getFiles, putFile: mocks.putFile, openWatch: mocks.openWatch };
});

// Rendering-layer stub + onSelect capture (lets the test fire a node click on the graph)
let graphHandlers: GraphHandlers | null = null;
vi.mock("../src/graph/use-graph", () => ({
  createGraphView: vi.fn((_el: HTMLElement, _d: unknown, h: GraphHandlers) => {
    graphHandlers = h;
    return { render: async () => {}, setData: () => {}, autoLayout: async () => ({}), destroy: () => {} };
  }),
}));

import App from "../src/App.vue";

beforeAll(async () => {
  await initCore(readFileSync(at("../src/core-wasm/sahou_core_bg.wasm")));
});
beforeEach(() => {
  vi.clearAllMocks();
  graphHandlers = null;
  mocks.getFiles.mockResolvedValue({
    schema: { text: DEMO, etag: "e1" },
    layout: null,
    endpoints: null,
    env: "dev",
  });
});

async function mounted() {
  const w = mount(App);
  await flushPromises();
  await flushPromises();
  return w;
}

describe("Main nav (requirement 1/3/4: the Design/Deploy/Diagnostics top-level tabs · Contract→Design rename)", () => {
  it("main-tabs has 3 tabs · says \"Design\" not \"Contract\" · the active one is explicit", async () => {
    const w = await mounted();
    const tabs = w.findAll(".main-tabs button");
    expect(tabs.length).toBe(3);
    const texts = tabs.map((t) => t.text());
    expect(texts[0]).toContain("Design");
    expect(texts[1]).toContain("Deploy");
    expect(texts[2]).toContain("Diagnostics");
    expect(texts.join()).not.toContain("Contract");
    expect(tabs[0].classes()).toContain("on"); // initial = Design
    expect(tabs[0].attributes("aria-current")).toBe("true");
    await tabs[1].trigger("click");
    expect(tabs[1].classes()).toContain("on");
    expect(tabs[0].attributes("aria-current")).toBeUndefined();
  });

  it("the old toolbar items (＋ node / ＋ connection / wiring toggle) are removed from the topbar", async () => {
    const w = await mounted();
    const top = w.find(".topbar").text();
    expect(top).not.toContain("＋ Node");
    expect(top).not.toContain("＋ Connection");
    expect(top).not.toContain("Per-connection");
    expect(top).not.toContain("Topic bus");
  });
});

describe("Wiring-display toggle (requirement 2: renamed + demoted to the canvas top-right)", () => {
  it("a subtle \"Per-connection / Topic bus\" toggle appears inside the canvas", async () => {
    const w = await mounted();
    const seg = w.find(".canvas .canvas-mode");
    expect(seg.exists()).toBe(true);
    const btns = seg.findAll("button");
    expect(btns.map((b) => b.text())).toEqual(["Per-connection", "Topic bus"]);
    expect(btns[0].classes()).toContain("on"); // default = Per-connection (direct)
    await btns[1].trigger("click");
    expect(btns[1].classes()).toContain("on");
    expect(w.text()).not.toContain("Direct N edges"); // old labels fully removed
    expect(w.text()).not.toContain("Bus aggregation");
  });
});

describe("Adding a node (requirement 5/8: in-canvas button → add only on modal confirm)", () => {
  it("＋ Node only opens a modal — nothing is added until confirmed", async () => {
    const w = await mounted();
    await w.find("[data-testid='canvas-add-node']").trigger("click");
    expect(w.find("[data-testid='add-node-dialog']").exists()).toBe(true);
    expect(w.findComponent(NodePane).exists()).toBe(false); // zero instant adds
    // confirm the name → it's added and that node is selected
    await w.find("input[aria-label='new node name']").setValue("mixer");
    await w.find("[data-testid='dialog-confirm']").trigger("click");
    expect(w.find("[data-testid='add-node-dialog']").exists()).toBe(false);
    const pane = w.findComponent(NodePane);
    expect(pane.exists()).toBe(true);
    expect(pane.props("id")).toBe("mixer");
  });

  it("closing with Esc adds nothing", async () => {
    const w = await mounted();
    await w.find("[data-testid='canvas-add-node']").trigger("click");
    await w.find("[data-testid='add-node-dialog'] input").trigger("keydown", { key: "Escape" });
    expect(w.find("[data-testid='add-node-dialog']").exists()).toBe(false);
    expect(w.findComponent(NodePane).exists()).toBe(false);
  });
});

describe("Adding a connection (requirement 6/7/8: contextual \"＋ Connection\" · auto-select the selected node as from · modal confirm)", () => {
  it("select a node → the pane's \"＋ Connection\" → the modal pre-selects the selected node as from", async () => {
    const w = await mounted();
    graphHandlers!.onSelect({ kind: "node", id: "visuals" });
    await flushPromises();
    expect(w.findComponent(NodePane).props("id")).toBe("visuals");
    await w.find("[data-testid='node-add-conn']").trigger("click");
    const dlg = w.find("[data-testid='add-conn-dialog']");
    expect(dlg.exists()).toBe(true);
    expect((dlg.find("select[aria-label='from']").element as HTMLSelectElement).value).toBe("visuals");
    // from itself is not among the to candidates (declarative pane §9 preserved)
    const toLabels = dlg.findAll(".to-list label").map((l) => l.text());
    expect(toLabels).not.toContain("visuals");
    // pick archive and confirm → it's added and that connection is selected
    await dlg.findAll(".to-list label").find((l) => l.text() === "archive")!.find("input").setValue(true);
    await dlg.find("[data-testid='dialog-confirm']").trigger("click");
    const pane = w.findComponent(ConnectionPane);
    expect(pane.exists()).toBe(true);
    expect(pane.props("id")).toBe("visuals_to_archive");
  });

  it("the canvas \"＋ Connection\" is also modal-first — cancel means zero adds", async () => {
    const w = await mounted();
    const before = w.findComponent(ConnectionPane).exists();
    await w.find("[data-testid='canvas-add-conn']").trigger("click");
    expect(w.find("[data-testid='add-conn-dialog']").exists()).toBe(true);
    await w.find("[data-testid='dialog-cancel']").trigger("click");
    expect(w.find("[data-testid='add-conn-dialog']").exists()).toBe(false);
    expect(w.findComponent(ConnectionPane).exists()).toBe(before); // nothing added
  });
});
