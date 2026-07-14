// Regression test for App.vue's graph view lifecycle (final-fixes #1).
// When #graph (the container) is unmounted/mounted by v-if on the parse NO ⇄ OK transition, verify —
// with G6 mocked — that createGraphView follows along and is (re)created / destroyed.
// G6's actual rendering is out of scope (that's other tests' domain) — here we only watch whether
// view creation/destruction is called.
import { flushPromises, mount } from "@vue/test-utils";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { WatchEvent } from "../src/api";
import { initCore } from "../src/core-bridge";

const at = (rel: string) => fileURLToPath(new URL(rel, import.meta.url));
const DEMO = readFileSync(at("../../examples/demo/schema.sahou.yaml"), "utf-8");
const BROKEN = "schema: s\nnodes:\n  a: {}\n  a: {}\nconnections: {}\n"; // duplicate key = parse NO

beforeAll(async () => {
  await initCore(readFileSync(at("../src/core-wasm/sahou_core_bg.wasm")));
});

// --- Mock createGraphView: record a handle per call and track render/destroy calls ---
interface FakeViewHandle {
  renderCalled: boolean;
  destroyed: boolean;
}
let createdViews: FakeViewHandle[] = [];
vi.mock("../src/graph/use-graph", () => ({
  createGraphView: vi.fn(() => {
    const handle: FakeViewHandle = { renderCalled: false, destroyed: false };
    createdViews.push(handle);
    return {
      render: async () => {
        handle.renderCalled = true;
      },
      setData: () => {},
      autoLayout: async () => ({}),
      destroy: () => {
        handle.destroyed = true;
      },
    };
  }),
}));

// --- Mock api: getFiles returns currentSchema · capture the openWatch callback so the test can
//     manually fire an "external-edit SSE notification" ---
let currentSchema = DEMO;
let watchCallback: ((e: WatchEvent) => void) | null = null;
vi.mock("../src/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../src/api")>();
  return {
    ...actual,
    getFiles: vi.fn(async () => ({
      schema: { text: currentSchema, etag: "e1" },
      layout: null,
      endpoints: null,
      env: "dev",
    })),
    putFile: vi.fn(async () => "e2"),
    openWatch: vi.fn((onEvent: (e: WatchEvent) => void) => {
      watchCallback = onEvent;
      return () => {
        watchCallback = null;
      };
    }),
  };
});

import App from "../src/App.vue";

/** Simulate firing an external-edit SSE notification and wait for Vue's reaction (microtask + render + watch flush:post). */
async function simulateExternalEdit(next: string, etag: string): Promise<void> {
  currentSchema = next;
  watchCallback?.({ kind: "schema", etag });
  await flushPromises();
  await flushPromises();
}

beforeEach(() => {
  createdViews = [];
  currentSchema = DEMO;
  watchCallback = null;
});

describe("App: graph view lifecycle (a parse NO ⇄ OK transition never becomes unrecoverable)", () => {
  it("start on broken YAML → an external fix restores the contract and a view is created", async () => {
    currentSchema = BROKEN;
    const w = mount(App);
    await flushPromises();
    await flushPromises();

    expect(w.find("[data-testid='broken-yaml']").exists()).toBe(true);
    expect(createdViews.length).toBe(0); // no view while broken

    await simulateExternalEdit(DEMO, "e2"); // external fix

    expect(w.find("[data-testid='broken-yaml']").exists()).toBe(false);
    expect(w.find("#graph").exists()).toBe(true);
    expect(createdViews.length).toBe(1); // a view is created after recovery
    expect(createdViews[0].renderCalled).toBe(true);
  });

  it("even when transitioning OK → NO → OK while running, the view is destroyed/recreated each time", async () => {
    const w = mount(App);
    await flushPromises();
    await flushPromises();

    expect(createdViews.length).toBe(1);
    expect(createdViews[0].destroyed).toBe(false);

    await simulateExternalEdit(BROKEN, "e2"); // broken by an external edit while running
    expect(w.find("[data-testid='broken-yaml']").exists()).toBe(true);
    expect(createdViews[0].destroyed).toBe(true); // the old view is destroyed

    await simulateExternalEdit(DEMO, "e3"); // fixed again externally
    expect(w.find("[data-testid='broken-yaml']").exists()).toBe(false);
    expect(w.find("#graph").exists()).toBe(true);
    expect(createdViews.length).toBe(2); // a new view is created
    expect(createdViews[1].renderCalled).toBe(true);
  });
});
