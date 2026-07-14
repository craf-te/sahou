// Centralized management of the editing state (design §2.2 store): contract JSON + layout +
// endpoints, dirty flags, debounced autosave, etag/self-echo, and the conflict-resolution state
// machine (§3 / §5.3). It holds no UI. All validation (NO) lives in the core (via core-bridge) —
// this is a container of state and holds no validation logic (§4).
import { reactive } from "vue";

import { getFiles as apiGetFiles, putFile as apiPutFile, PutConflict } from "./api";
import type { FileKind, FilesResponse, WatchEvent } from "./api";
import * as core from "./core-bridge";
import type { Contract, Descriptor, Endpoints, SahouDiag } from "./core-bridge";
import { emptyLayout, type LayoutFile } from "./layout";

/** Autosave debounce (decision 4 before Task 0: 300–500ms → fixed at 400ms). */
export const AUTOSAVE_DEBOUNCE_MS = 400;

export interface StoreApi {
  getFiles: typeof apiGetFiles;
  putFile: typeof apiPutFile;
}

export type ConflictChoice = "keep" | "reload";

export interface StoreState {
  phase: "loading" | "ready" | "fatal";
  fatalMessage: string; // wasm init failure / API unreachable (§7 — never silently stop working)
  env: string;
  contract: Contract | null; // only when parse is OK (the target of structural editing)
  schemaText: string; // the file's raw text (preserved even on a parse NO §7)
  schemaParseDiags: SahouDiag[]; // parse NO diagnostics (empty = parse OK)
  layout: LayoutFile;
  endpoints: Endpoints;
  endpointsParseDiags: SahouDiag[];
  diags: SahouDiag[]; // result of wasm_validate_schema (the core is authoritative §4)
  descriptor: Descriptor | null; // the most recent good descriptor
  descriptorStale: boolean; // true while diagnostics exist = shown as stale (§7)
  etags: Record<FileKind, string | null>;
  dirty: Record<FileKind, boolean>;
  conflicts: Partial<Record<FileKind, string | null>>; // kind → external etag (only while conflicting)
  saveError: string;
  hasComments: boolean; // comments detected on load (§0)
  status: string;
}

const defaultEndpoints = (env: string): Endpoints => ({ env, namespace: "sahou", nodes: {} });

/** Same heuristic as the `sahou fmt` CLI warning (leading # / mid-line " #"). */
export const detectComments = (text: string): boolean =>
  text.split("\n").some((l) => l.trim().startsWith("#") || l.includes(" #"));

export function createStore(a: StoreApi = { getFiles: apiGetFiles, putFile: apiPutFile }) {
  const state = reactive<StoreState>({
    phase: "loading",
    fatalMessage: "",
    env: "dev",
    contract: null,
    schemaText: "",
    schemaParseDiags: [],
    layout: emptyLayout(),
    endpoints: defaultEndpoints("dev"),
    endpointsParseDiags: [],
    diags: [],
    descriptor: null,
    descriptorStale: false,
    etags: { schema: null, layout: null, endpoints: null },
    dirty: { schema: false, layout: false, endpoints: false },
    conflicts: {},
    saveError: "",
    hasComments: false,
    status: "loading…",
  });

  const timers: Partial<Record<FileKind, ReturnType<typeof setTimeout>>> = {};
  // A save that has sent its PUT but not yet stored the returned etag. The watcher echoes our own write
  // back over SSE, and that echo can arrive before putFile resolves — so the post-await etag comparison
  // alone cannot recognize it (it would be misread as an external edit → a phantom conflict that loops).
  // While a kind is in-flight we treat any watch event for it as our own echo (§3.4).
  const inFlight: Partial<Record<FileKind, boolean>> = {};

  /** Refresh the contract's derived data via a core round-trip (§3.2: serialize → validate → descriptor). */
  function derive(): void {
    if (!state.contract) {
      state.diags = [];
      state.descriptorStale = state.descriptor !== null;
      return;
    }
    const yaml = core.serialize(state.contract);
    state.diags = core.validateSchema(yaml);
    if (state.diags.length === 0 && state.endpointsParseDiags.length === 0) {
      state.descriptor = core.descriptor(yaml, core.serializeEndpoints(state.endpoints));
      state.descriptorStale = false;
    } else {
      state.descriptorStale = state.descriptor !== null; // keep the last good value, shown as stale (§7)
    }
  }

  function adoptSchema(text: string, etag: string | null): void {
    state.schemaText = text;
    state.etags.schema = etag;
    state.hasComments = detectComments(text);
    try {
      state.contract = core.parse(text);
      state.schemaParseDiags = [];
    } catch (e) {
      // parse NO: preserve raw text, disable structural editing (never guess a repair §7)
      state.contract = null;
      state.schemaParseDiags =
        e instanceof core.CoreNo ? e.diags : [{ code: "parse_error", path: "$", message: String(e) }];
    }
    derive();
  }

  /**
   * layout.sahou.json holds only GUI-only coordinates and is unrelated to the contract (§6). Even if it is
   * broken there is no reason to block contract editing, so this is the one place we catch
   * individually and fall back to emptyLayout()
   * (§7 "never silently stop working" — corruption is not swallowed but surfaced as a warning).
   */
  function adoptLayoutText(text: string | null): { layout: LayoutFile; warning: string } {
    if (text === null) return { layout: emptyLayout(), warning: "" };
    try {
      return { layout: JSON.parse(text) as LayoutFile, warning: "" };
    } catch (e) {
      return {
        layout: emptyLayout(),
        warning: `layout.sahou.json is broken (resetting coordinates only and continuing): ${String(e)}`,
      };
    }
  }

  function adoptEndpoints(text: string | null, etag: string | null): void {
    state.etags.endpoints = etag;
    if (text === null) {
      // absent = treated as empty (created on first save §2.1)
      state.endpoints = defaultEndpoints(state.env);
      state.endpointsParseDiags = [];
    } else {
      try {
        state.endpoints = core.parseEndpoints(text);
        state.endpointsParseDiags = [];
      } catch (e) {
        state.endpointsParseDiags =
          e instanceof core.CoreNo ? e.diags : [{ code: "parse_error", path: "$", message: String(e) }];
      }
    }
    derive();
  }

  async function load(): Promise<void> {
    try {
      const files = await a.getFiles();
      state.env = files.env;
      if (!files.schema) {
        state.phase = "fatal";
        state.fatalMessage = "schema.sahou.yaml is missing (the backend should have said NO at startup)";
        return;
      }
      adoptSchema(files.schema.text, files.schema.etag);
      const { layout, warning } = adoptLayoutText(files.layout?.text ?? null);
      state.layout = layout;
      state.etags.layout = files.layout?.etag ?? null;
      adoptEndpoints(files.endpoints?.text ?? null, files.endpoints?.etag ?? null);
      state.dirty = { schema: false, layout: false, endpoints: false };
      state.phase = "ready";
      state.status = warning || "loaded";
    } catch (e) {
      state.phase = "fatal";
      state.fatalMessage = String(e); // an unreachable API is also surfaced across the whole screen (§7)
    }
  }

  function scheduleSave(kind: FileKind): void {
    state.dirty[kind] = true;
    clearTimeout(timers[kind]);
    timers[kind] = setTimeout(() => void save(kind), AUTOSAVE_DEBOUNCE_MS);
  }

  function textOf(kind: FileKind): string {
    if (kind === "schema") {
      if (!state.contract) throw new Error("won't save the schema during a parse NO (raw text preserved §7)");
      return core.serialize(state.contract);
    }
    if (kind === "layout") return JSON.stringify(state.layout, null, 2) + "\n";
    return core.serializeEndpoints(state.endpoints);
  }

  async function save(kind: FileKind): Promise<void> {
    if (state.conflicts[kind] !== undefined) return; // don't overwrite while awaiting conflict resolution (§5.3)
    if (!state.dirty[kind]) return;
    let text: string;
    try {
      text = textOf(kind);
    } catch (e) {
      state.saveError = String(e);
      return;
    }
    inFlight[kind] = true;
    try {
      const etag = await a.putFile(kind, text, state.etags[kind]);
      state.etags[kind] = etag; // self-echo (the watch triggered by our own PUT) can be ignored via etag matching (§3.4)
      state.dirty[kind] = false;
      state.saveError = "";
      state.status = "autosaved ✓";
      if (kind === "schema") {
        state.schemaText = text;
        state.hasComments = false; // no comments exist after a canonical save (the §0 warning already fired on load)
      }
    } catch (e) {
      if (e instanceof PutConflict) {
        state.conflicts = { ...state.conflicts, [kind]: e.currentEtag }; // the last line of defense (§5.3)
      } else {
        state.saveError = String(e); // dirty is kept = the edit is not lost (§7)
        state.status = "save failed";
      }
    } finally {
      inFlight[kind] = false;
    }
  }

  // --- Editing API (from panes; all receive new immutably-updated objects §3.2) ---
  function updateContract(next: Contract): void {
    state.contract = next;
    derive();
    scheduleSave("schema");
  }
  function updateLayout(next: LayoutFile): void {
    state.layout = next;
    scheduleSave("layout");
  }
  function updateEndpoints(next: Endpoints): void {
    state.endpoints = next;
    derive();
    scheduleSave("endpoints");
  }

  // --- watch (SSE §3.4) ---
  function onWatch(ev: WatchEvent): void {
    if (inFlight[ev.kind]) return; // our own write is in flight → this is its echo (avoids the race where the echo beats the PUT response §3.4)
    if (ev.etag !== null && ev.etag === state.etags[ev.kind]) return; // self-echo: ignore
    if (state.conflicts[ev.kind] !== undefined) {
      state.conflicts = { ...state.conflicts, [ev.kind]: ev.etag }; // while conflicting, keep tracking the moving etag
      return;
    }
    if (!state.dirty[ev.kind]) {
      void reloadKind(ev.kind); // non-conflicting: reflect the external edit
      return;
    }
    state.conflicts = { ...state.conflicts, [ev.kind]: ev.etag }; // detect ahead of time (§5.3)
  }

  async function reloadKind(kind: FileKind): Promise<void> {
    // Called fire-and-forget from onWatch as `void reloadKind(...)` (§3.4). Without catching here, a
    // getFiles failure becomes an unhandled rejection and reflecting external edits silently stops
    // (a §7 violation). Failures are surfaced via saveError/status.
    let files: FilesResponse;
    try {
      files = await a.getFiles();
    } catch (e) {
      state.saveError = String(e);
      state.status = "failed to fetch the external change";
      return;
    }
    if (kind === "schema") {
      if (!files.schema) {
        // schema was deleted externally (the edit target is gone) — match load()'s missing-schema
        // handling and surface the fatal state instead of a green "reflected ✓"
        // (§7 "never silently stop working").
        state.phase = "fatal";
        state.fatalMessage = "schema.sahou.yaml was deleted externally (cannot continue structural editing)";
        state.status = "the schema file is gone";
        return;
      }
      adoptSchema(files.schema.text, files.schema.etag);
    }
    if (kind === "layout") {
      const { layout, warning } = adoptLayoutText(files.layout?.text ?? null);
      state.layout = layout;
      state.etags.layout = files.layout?.etag ?? null;
      if (warning) {
        state.saveError = warning;
        state.dirty[kind] = false;
        state.status = "reset coordinates because layout.sahou.json is corrupt";
        return;
      }
    }
    if (kind === "endpoints") adoptEndpoints(files.endpoints?.text ?? null, files.endpoints?.etag ?? null);
    state.dirty[kind] = false;
    state.status = "reflected the external edit ✓";
  }

  // --- Conflict resolution (§5.3 — safe default = keep local. Only "discard and reload" is destructive) ---
  async function resolveConflict(kind: FileKind, choice: ConflictChoice): Promise<void> {
    const { [kind]: _resolved, ...rest } = state.conflicts;
    state.conflicts = rest;
    try {
      if (choice === "keep") {
        const files = await a.getFiles(); // treat local as authoritative, re-fetch the latest etag, and continue autosaving
        state.etags[kind] = files[kind]?.etag ?? null;
        state.dirty[kind] = true;
        await save(kind);
      } else {
        await reloadKind(kind);
      }
    } catch (e) {
      // don't silently swallow a getFiles failure etc. (§7). dirty is kept so the edit is not lost.
      state.saveError = String(e);
      state.status = "conflict resolution failed";
    }
  }

  /** For "Show diff" (decision 3: raw-text diff). mine = the text about to be saved / theirs = the current file. */
  async function conflictTexts(kind: FileKind): Promise<{ mine: string; theirs: string }> {
    const files = await a.getFiles();
    let mine = "";
    try {
      mine = textOf(kind);
    } catch {
      mine = state.schemaText; // during a parse NO, the raw text
    }
    return { mine, theirs: files[kind]?.text ?? "" };
  }

  return {
    state,
    load,
    updateContract,
    updateLayout,
    updateEndpoints,
    onWatch,
    resolveConflict,
    conflictTexts,
    save,
  };
}

export type Store = ReturnType<typeof createStore>;
