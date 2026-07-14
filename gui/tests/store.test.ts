import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { FileKind, FilesResponse } from "../src/api";
import { PutConflict } from "../src/api";
import { initCore } from "../src/core-bridge";
import { emptyLayout } from "../src/layout";
import { AUTOSAVE_DEBOUNCE_MS, createStore, detectComments } from "../src/store";

const at = (rel: string) => fileURLToPath(new URL(rel, import.meta.url));
const DEMO = readFileSync(at("../../examples/demo/schema.sahou.yaml"), "utf-8");

beforeAll(async () => {
  await initCore(readFileSync(at("../src/core-wasm/sahou_core_bg.wasm")));
});
beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

/** A fake backend: reproduces getFiles/putFile in memory. 409 is injected once via armConflict(). */
function fakeApi(initialSchema: string) {
  const files: FilesResponse = {
    schema: { text: initialSchema, etag: "e1" },
    layout: null,
    endpoints: null,
    env: "dev",
  };
  let seq = 2;
  let conflictOnce = false;
  let getFilesFailOnce = false;
  const puts: { kind: FileKind; text: string; etag: string | null }[] = [];
  return {
    files,
    puts,
    setServerSchema(text: string, etag: string) {
      files.schema = { text, etag };
    },
    armConflict() {
      conflictOnce = true;
    },
    /** Make only the next getFiles fail (network down, etc.) — to verify the catch in reloadKind/resolveConflict. */
    armGetFilesFailure() {
      getFilesFailOnce = true;
    },
    api: {
      async getFiles(): Promise<FilesResponse> {
        if (getFilesFailOnce) {
          getFilesFailOnce = false;
          throw new Error("network down");
        }
        return structuredClone(files);
      },
      async putFile(kind: FileKind, text: string, _etag: string | null): Promise<string> {
        if (conflictOnce) {
          conflictOnce = false;
          throw new PutConflict(files.schema?.etag ?? null);
        }
        puts.push({ kind, text, etag: _etag });
        const e = `e${seq++}`;
        files[kind] = { text, etag: e };
        return e;
      },
    },
  };
}

describe("store (state · autosave · self-echo · conflict state machine)", () => {
  it("load: parse OK · zero diagnostics · descriptor derived · no comments", async () => {
    const f = fakeApi(DEMO);
    const s = createStore(f.api);
    await s.load();
    expect(s.state.phase).toBe("ready");
    expect(s.state.contract?.schema).toBe("demo_installation");
    expect(s.state.diags).toEqual([]);
    expect(s.state.descriptor?.connections["touch"].key).toBe("sahou/touch");
    expect(s.state.hasComments).toBe(false);
  });

  it("edit → 400ms debounced autosave (rapid edits coalesce into one · If-Match = the etag from load)", async () => {
    const f = fakeApi(DEMO);
    const s = createStore(f.api);
    await s.load();
    const c = s.state.contract!;
    s.updateContract({ ...c, version: "2" });
    s.updateContract({ ...c, version: "3" });
    expect(s.state.dirty.schema).toBe(true);
    await vi.advanceTimersByTimeAsync(AUTOSAVE_DEBOUNCE_MS + 10);
    expect(f.puts.length).toBe(1);
    expect(f.puts[0].etag).toBe("e1");
    expect(s.state.dirty.schema).toBe(false);
    expect(s.state.etags.schema).toBe("e2"); // remember the new etag (the basis for self-echo suppression §3.3)
  });

  it("watch: self-echo (known etag) is ignored · a non-dirty external change reloads (§3.4)", async () => {
    const f = fakeApi(DEMO);
    const s = createStore(f.api);
    await s.load();
    s.onWatch({ kind: "schema", etag: "e1" }); // known etag = same as our state → ignore
    expect(s.state.contract?.schema).toBe("demo_installation");
    f.setServerSchema(DEMO.replace("demo_installation", "renamed"), "eX");
    s.onWatch({ kind: "schema", etag: "eX" });
    await vi.advanceTimersByTimeAsync(0); // microtask flush
    expect(s.state.contract?.schema).toBe("renamed");
    expect(s.state.etags.schema).toBe("eX");
  });

  it("watch: an external change while dirty is a conflict (no auto-overwrite) → keep continues with local as authoritative (§5.3)", async () => {
    const f = fakeApi(DEMO);
    const s = createStore(f.api);
    await s.load();
    const c = s.state.contract!;
    s.updateContract({ ...c, version: "9" }); // dirty (before autosave)
    f.setServerSchema(DEMO, "eX");
    s.onWatch({ kind: "schema", etag: "eX" });
    expect(s.state.conflicts.schema).toBe("eX");
    await vi.advanceTimersByTimeAsync(AUTOSAVE_DEBOUNCE_MS + 10);
    expect(f.puts.length).toBe(0); // autosave doesn't write while awaiting conflict resolution
    await s.resolveConflict("schema", "keep");
    expect(f.puts.length).toBe(1);
    expect(f.puts[0].etag).toBe("eX"); // re-fetches the latest etag and saves (treats local as authoritative)
    expect(s.state.conflicts.schema).toBeUndefined();
  });

  it("PUT 409 (last line of defense) → conflict → reload discards local (only an explicit destructive action)", async () => {
    const f = fakeApi(DEMO);
    const s = createStore(f.api);
    await s.load();
    f.armConflict();
    s.updateContract({ ...s.state.contract!, version: "9" });
    await vi.advanceTimersByTimeAsync(AUTOSAVE_DEBOUNCE_MS + 10);
    expect(s.state.conflicts.schema).not.toBeUndefined();
    f.setServerSchema(DEMO, "eS");
    await s.resolveConflict("schema", "reload");
    expect(s.state.contract?.version).toBe("1");
    expect(s.state.dirty.schema).toBe(false);
  });

  it("watch: non-dirty external deletion of schema → surface the fatal state instead of silently going green (§7)", async () => {
    const f = fakeApi(DEMO);
    const s = createStore(f.api);
    await s.load();
    f.files.schema = null; // the schema file is removed externally
    s.onWatch({ kind: "schema", etag: null }); // deletion = etag null (not a self-echo target)
    await vi.advanceTimersByTimeAsync(0); // reloadKind microtask flush
    expect(s.state.phase).toBe("fatal");
    expect(s.state.status).not.toBe("reflected the external edit ✓"); // doesn't go silently green
    expect(s.state.fatalMessage).toContain("schema");
  });

  it("broken YAML: preserve raw text · disable structural editing · positioned diagnostics (§7)", async () => {
    const broken = "schema: s\nnodes:\n  a: {}\n  a: {}\nconnections: {}\n";
    const f = fakeApi(broken);
    const s = createStore(f.api);
    await s.load();
    expect(s.state.contract).toBeNull();
    expect(s.state.schemaText).toBe(broken);
    expect(s.state.schemaParseDiags[0].code).toBe("parse_error");
  });

  it("comment detection (§0): warns on load · gone after a canonical save", async () => {
    const commented = "# important note\n" + DEMO;
    expect(detectComments(commented)).toBe(true);
    const f = fakeApi(commented);
    const s = createStore(f.api);
    await s.load();
    expect(s.state.hasComments).toBe(true);
    s.updateContract({ ...s.state.contract!, version: "2" });
    await vi.advanceTimersByTimeAsync(AUTOSAVE_DEBOUNCE_MS + 10);
    expect(s.state.hasComments).toBe(false); // there was nothing left to drop (dropped after the warning)
  });

  it("while there are schema diagnostics the descriptor is stale (keeps the last good value §7)", async () => {
    const f = fakeApi(DEMO);
    const s = createStore(f.api);
    await s.load();
    const c = s.state.contract!;
    const bad = { ...c.connections["touch"], to: ["ghost"] };
    s.updateContract({ ...c, connections: { ...c.connections, touch: bad } });
    expect(s.state.diags.some((d) => d.code === "unknown_node")).toBe(true);
    expect(s.state.descriptor).not.toBeNull();
    expect(s.state.descriptorStale).toBe(true);
  });

  // --- final-fixes #2: swallowed async errors & over-eager fatal on layout corruption ---

  it("load: a broken layout.sahou.json resets to emptyLayout · schema/contract stay alive, not fatal (§7)", async () => {
    const f = fakeApi(DEMO);
    f.files.layout = { text: "{not valid json", etag: "eL" };
    const s = createStore(f.api);
    await s.load();
    expect(s.state.phase).toBe("ready"); // layout corruption is no reason to kill contract editing
    expect(s.state.contract?.schema).toBe("demo_installation");
    expect(s.state.layout).toEqual(emptyLayout());
    expect(s.state.status).toMatch(/layout/); // the warning is surfaced (not swallowed)
  });

  it("reloadKind: on a getFiles failure it doesn't throw but falls into saveError (no unhandled rejection from void reloadKind)", async () => {
    const f = fakeApi(DEMO);
    const s = createStore(f.api);
    await s.load();
    f.armGetFilesFailure();
    s.onWatch({ kind: "layout", etag: "eBad" }); // non-dirty → effectively void reloadKind internally
    await vi.advanceTimersByTimeAsync(0);
    expect(s.state.saveError).not.toBe("");
    expect(s.state.phase).toBe("ready"); // a mere fetch failure doesn't go fatal
  });

  it("reloadKind: even if layout.sahou.json breaks externally it doesn't throw but falls into emptyLayout + a warning (not fatal)", async () => {
    const f = fakeApi(DEMO);
    const s = createStore(f.api);
    await s.load();
    f.files.layout = { text: "{not valid json", etag: "eBad2" };
    s.onWatch({ kind: "layout", etag: "eBad2" });
    await vi.advanceTimersByTimeAsync(0);
    expect(s.state.layout).toEqual(emptyLayout());
    expect(s.state.phase).toBe("ready");
    expect(s.state.dirty.layout).toBe(false);
  });

  it("watch: a self-echo that races the PUT response (echo arrives before the etag is stored) is NOT a phantom conflict (§bug: layout autosave loop)", async () => {
    const f = fakeApi(DEMO);
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    // Hold the PUT response in-flight so we can fire the watcher echo before putFile resolves (the race).
    const api = {
      getFiles: f.api.getFiles,
      async putFile(kind: FileKind, text: string, etag: string | null): Promise<string> {
        const e = await f.api.putFile(kind, text, etag); // computes the new etag + updates the fake file
        await gate; // stay in-flight until released
        return e;
      },
    };
    const s = createStore(api);
    await s.load();
    s.updateLayout({ ...s.state.layout, nodes: { a: { x: 1, y: 2 } } }); // dirty layout
    await vi.advanceTimersByTimeAsync(AUTOSAVE_DEBOUNCE_MS + 10); // save fires → putFile in-flight (awaiting gate)
    // The SSE echo for OUR OWN write arrives before the PUT response resolves, carrying the etag the write produced.
    s.onWatch({ kind: "layout", etag: f.files.layout!.etag });
    expect(s.state.conflicts.layout).toBeUndefined(); // must not be misread as an external edit
    release();
    await vi.advanceTimersByTimeAsync(0);
    expect(s.state.conflicts.layout).toBeUndefined();
    expect(s.state.dirty.layout).toBe(false);
  });

  it("resolveConflict(keep): even if getFiles fails it doesn't throw but falls into saveError", async () => {
    const f = fakeApi(DEMO);
    const s = createStore(f.api);
    await s.load();
    const c = s.state.contract!;
    s.updateContract({ ...c, version: "9" }); // dirty
    f.setServerSchema(DEMO, "eX");
    s.onWatch({ kind: "schema", etag: "eX" }); // becomes a conflict
    expect(s.state.conflicts.schema).toBe("eX");
    f.armGetFilesFailure();
    await s.resolveConflict("schema", "keep");
    expect(s.state.saveError).not.toBe("");
  });
});
