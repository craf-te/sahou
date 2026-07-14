// Integration tests for the App assembly (Task 14). G6 (canvas) and SSE are module-mocked, and the
// store → pane wiring (fatal / comment warning §0 / broken YAML §7 / diagnostic jump) is verified
// through real behavior.
import { flushPromises, mount } from "@vue/test-utils";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import App from "../src/App.vue";
import { initCore } from "../src/core-bridge";
import ConnectionPane from "../src/panes/ConnectionPane.vue";

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

// G6 needs canvas and doesn't run under happy-dom — stub only the rendering layer (App's wiring is real)
vi.mock("../src/graph/use-graph", () => ({
  createGraphView: () => ({
    render: async () => {},
    setData: () => {},
    autoLayout: async () => ({}),
    destroy: () => {},
  }),
}));

const files = (schema: string | null) => ({
  schema: schema === null ? null : { text: schema, etag: "e1" },
  layout: null,
  endpoints: null,
  env: "dev",
});

beforeAll(async () => {
  await initCore(readFileSync(at("../src/core-wasm/sahou_core_bg.wasm")));
});
beforeEach(() => vi.clearAllMocks());

describe("App.vue (assembly · state visibility)", () => {
  it("an unreachable API shows fatal across the whole screen (never silently stop working §7)", async () => {
    mocks.getFiles.mockRejectedValue(new Error("cannot connect"));
    const w = mount(App);
    await flushPromises();
    expect(w.find(".fatal").exists()).toBe(true);
    expect(w.find(".fatal").text()).toContain("cannot connect");
  });

  it("a normal load shows the graph + topbar", async () => {
    mocks.getFiles.mockResolvedValue(files(DEMO));
    const w = mount(App);
    await flushPromises();
    expect(w.find(".topbar h1").text()).toBe("Sahou Editor");
    expect(w.find("#graph").exists()).toBe(true);
    expect(w.find("[data-testid='comment-warn']").exists()).toBe(false);
  });

  it("a commented schema warns before editing (§0 · never silently drop them)", async () => {
    mocks.getFiles.mockResolvedValue(files("# important note\n" + DEMO));
    const w = mount(App);
    await flushPromises();
    const banner = w.find("[data-testid='comment-warn']");
    expect(banner.exists()).toBe(true);
    expect(banner.text()).toContain("comments");
  });

  it("broken YAML: preserve raw text, disable structural editing, point to fixing it in text (§7)", async () => {
    const broken = "schema: s\nnodes:\n  a: {}\n  a: {}\nconnections: {}\n";
    mocks.getFiles.mockResolvedValue(files(broken));
    const w = mount(App);
    await flushPromises();
    const screen = w.find("[data-testid='broken-yaml']");
    expect(screen.exists()).toBe(true);
    expect(screen.find("textarea").element.value).toBe(broken); // raw text preserved
    expect(screen.find("textarea").attributes("readonly")).toBeDefined(); // the GUI doesn't write it
    expect(screen.text()).toContain("parse_error"); // the core's diagnostic verbatim
    expect(w.find("#graph").exists()).toBe(false); // structural editing disabled
  });

  it("diagnostic jump: click a row in the diagnostics tab → the matching connection pane in the design tab", async () => {
    const bad = [
      "schema: s", "version: 1",
      "nodes:", "  a: {}", "  b: {}",
      "connections:",
      "  touch:", "    pattern: pub_sub", "    from: a", "    to: [ghost]",
      "    payload: { typing: any }", "",
    ].join("\n");
    mocks.getFiles.mockResolvedValue(files(bad));
    const w = mount(App);
    await flushPromises();
    await w.findAll(".tabs button").find((b) => b.text().includes("Diagnostics"))!.trigger("click");
    const row = w.find("[data-testid='diag-row']");
    expect(row.text()).toContain("unknown_node");
    await row.trigger("click");
    const pane = w.findComponent(ConnectionPane);
    expect(pane.exists()).toBe(true);
    expect(pane.props("id")).toBe("touch");
  });
});
