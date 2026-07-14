<script setup lang="ts">
// App = assembly only (design §2.2): binds store (state / autosave / conflict machinery)
// × graph (rendering) × pane (editing UI). No logic lives here — validation is in the core,
// editing in edits, state in store.
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch } from "vue";
import { openWatch, type FileKind } from "./api";
import type { Contract, Endpoints } from "./core-bridge";
import {
  addConnection, addNode, type AddConnectionInput, type AddNodeInput,
} from "./edits/contract-edits";
import { buildData, type WiringMode } from "./graph/build-data";
import { nodeCaps } from "./graph/edge-style";
import { createGraphView, type Selected } from "./graph/use-graph";
import { withNodePos } from "./layout";
import AddConnectionDialog from "./panes/AddConnectionDialog.vue";
import AddNodeDialog from "./panes/AddNodeDialog.vue";
import ConflictDialog from "./panes/ConflictDialog.vue";
import ConnectionPane from "./panes/ConnectionPane.vue";
import DeployPane from "./panes/DeployPane.vue";
import DiagPane from "./panes/DiagPane.vue";
import NodePane from "./panes/NodePane.vue";
import TableView from "./panes/TableView.vue";
import { createStore } from "./store";

const store = createStore();
const { state } = store;
const selected = ref<Selected | null>(null);
const mode = ref<WiringMode>("direct");
const tab = ref<"design" | "deploy" | "diag">("design");
// Design view: the node graph or the spreadsheet workbook (both edit the same contract IR).
const designView = ref<"graph" | "table">("graph");
const container = ref<HTMLElement | null>(null);
const view = shallowRef<ReturnType<typeof createGraphView> | null>(null);
const conflictView = ref<{ kind: FileKind; mine: string; theirs: string } | null>(null);
const serverDown = ref(false);
let stopWatch: (() => void) | null = null;
// A node drag is already reflected in the graph by G6; persisting it changes state.layout, which would
// otherwise trigger a full setData re-render that fights the interactive position (the node snaps back).
// So skip exactly one re-render when the layout change originated from our own drag.
let skipGraphRenderOnce = false;

// When the backend stops, the SSE connection dies (openWatch.onDown). In an app-mode window (opened via
// `sahou gui` with --app) the page can close itself; a normal tab cannot be closed by script, so the
// overlay stays as the visible fallback.
function onServerDown() {
  serverDown.value = true;
  try {
    window.close();
  } catch {
    /* a normal tab cannot be closed by script — the overlay remains */
  }
}

const firstConflict = computed(() => (Object.keys(state.conflicts) as FileKind[])[0]);
const allDiags = computed(() => [...state.schemaParseDiags, ...state.endpointsParseDiags, ...state.diags]);
const saving = computed(() => state.dirty.schema || state.dirty.layout || state.dirty.endpoints);
// Unwired nodes are a soft advisory (a GUI UX hint, not a NO — kept on the GUI side per the
// division of labor in design §4)
const unwired = computed(() =>
  state.contract
    ? Object.keys(state.contract.nodes).filter((id) => nodeCaps(state.contract!, id).length === 0)
    : [],
);
const val = (e: Event) => (e.target as HTMLInputElement).value;

onMounted(async () => {
  await store.load();
  if (state.phase !== "ready") return;
  stopWatch = openWatch((e) => store.onWatch(e), { onDown: onServerDown });
});
onBeforeUnmount(() => {
  stopWatch?.();
  view.value?.destroy();
});

// Graph view lifecycle (§3.2 + bug fix): on the parse NO ⇄ OK transition, #graph (container) is
// unmounted/mounted by v-if, so the container element itself is swapped out (the broken-YAML screen
// in §7). A one-time creation in onMounted would fail to recreate the view when recovering from the
// broken state, leaving the graph blank with no way back. We watch container / contract presence and
// (re)create / destroy the view each time to prevent this (flush: "post" so it runs after the DOM
// update and never holds onto the stale element).
watch(
  [container, () => state.contract, () => state.layout, mode],
  ([el, contract, layout, m]) => {
    if (!el || !contract) {
      view.value?.destroy();
      view.value = null;
      return;
    }
    if (!view.value) {
      view.value = createGraphView(el, buildData(contract, layout, m), {
        onSelect: (s) => (selected.value = s),
        onNodeMoved: (id, x, y) => {
          skipGraphRenderOnce = true; // the graph already shows the drop; just persist the position
          store.updateLayout(withNodePos(state.layout, id, x, y));
        },
      });
      void view.value.render();
    } else if (skipGraphRenderOnce) {
      skipGraphRenderOnce = false; // drag-originated layout change: keep G6's interactive position
    } else {
      view.value.setData(buildData(contract, layout, m));
    }
  },
  { flush: "post" },
);

// When a conflict appears, fetch the text for diffing (§5.3 — resolution is the store state machine)
watch(firstConflict, async (kind) => {
  conflictView.value = kind ? { kind, ...(await store.conflictTexts(kind)) } : null;
});

const setContract = (c: Contract) => store.updateContract(c);
const setEndpoints = (e: Endpoints) => store.updateEndpoints(e);

// Adding happens only on modal confirm (UX redesign requirement 8: no more instant add).
// Buttons only open a dialog — confirming a value calls the pure contract-edits functions.
const showAddNode = ref(false);
const showAddConn = ref(false);
const addConnFrom = ref<string | null>(null);

/** Open the add-connection dialog. Auto-selects the currently selected node as `from`
 *  (requirement 7 / contextual affordance requirement 6). */
function openAddConn() {
  addConnFrom.value = selNodeId.value;
  showAddConn.value = true;
}
function onAddNodeConfirm(v: AddNodeInput) {
  showAddNode.value = false;
  if (!state.contract) return;
  const r = addNode(state.contract, v); // empty/duplicate is rejected by the dialog (the null at the boundary is a safeguard)
  if (!r) return;
  store.updateContract(r.contract);
  selected.value = { kind: "node", id: r.id };
}
function onAddConnConfirm(v: AddConnectionInput) {
  showAddConn.value = false;
  if (!state.contract) return;
  const r = addConnection(state.contract, v);
  if (!r) return;
  store.updateContract(r.contract);
  selected.value = { kind: "edge", id: r.id };
}
async function onAutoLayout() {
  if (!view.value || !state.contract) return;
  const pos = await view.value.autoLayout(Object.keys(state.contract.nodes));
  let l = state.layout;
  for (const [id, p] of Object.entries(pos)) l = withNodePos(l, id, p.x, p.y);
  store.updateLayout(l);
}
// Diagnostic jump: core path (unified grammar) → diag-target → select the matching pane (§2.2)
function onJump(t: Selected) {
  tab.value = "design";
  selected.value = t;
}
const selNodeId = computed(() =>
  selected.value?.kind === "node" && state.contract?.nodes[selected.value.id] ? selected.value.id : null,
);
const selConnId = computed(() =>
  selected.value?.kind === "edge" && state.contract?.connections[selected.value.id] ? selected.value.id : null,
);
</script>

<template>
  <!-- Server stopped: the SSE connection dropped past the grace period. In an app-mode window this tab
       also calls window.close(); otherwise this overlay is the visible signal. -->
  <div v-if="serverDown" class="server-down" role="alertdialog" aria-modal="true" data-testid="server-down">
    <div class="server-down-card">
      <h2>Sahou GUI server stopped</h2>
      <p>The backend is no longer running. You can close this tab.</p>
    </div>
  </div>

  <!-- Fatal state: never make it silently stop working (§7) -->
  <pre v-if="state.phase === 'fatal'" class="fatal">{{ state.fatalMessage }}</pre>

  <div v-else-if="state.phase === 'ready'" class="app">
    <!-- Comment warning (§0): detected on load, surfaced before editing + kept visible -->
    <div v-if="state.hasComments" class="banner" role="status" data-testid="comment-warn">
      ⚠ This schema contains YAML comments — editing in the GUI drops them on save (autosave)
      because the file is canonicalized. Edit the file directly if you need to keep comments.
    </div>

    <header class="topbar">
      <h1>Sahou Editor</h1>
      <template v-if="state.contract">
        <label class="inline">schema
          <input
            class="sm"
            :value="state.contract.schema"
            aria-label="schema name"
            @change="setContract({ ...state.contract!, schema: val($event) })"
          />
        </label>
        <label class="inline">v
          <input
            class="xs"
            :value="state.contract.version"
            aria-label="version"
            @change="setContract({ ...state.contract!, version: val($event) || '1' })"
          />
        </label>
      </template>
      <span class="meta">{{ state.status }}</span>
    </header>

    <!-- Main nav (UX redesign requirement 1/3/4): the three top-level tabs Design / Deploy /
         Diagnostics. Miscellaneous toolbar items are demoted into the canvas (requirement 2/5) -->
    <nav class="tabs main-tabs" aria-label="Main navigation">
      <button
        type="button"
        :class="{ on: tab === 'design' }"
        :aria-current="tab === 'design' ? 'true' : undefined"
        title="Design nodes, connections, and types (the connection blueprint)"
        @click="tab = 'design'"
      >
        <span class="tab-label">Design</span>
        <span class="tab-sub">Nodes · Connections · Types</span>
      </button>
      <button
        type="button"
        :class="{ on: tab === 'deploy' }"
        :aria-current="tab === 'deploy' ? 'true' : undefined"
        title="Configure endpoints and environments (endpoints)"
        @click="tab = 'deploy'"
      >
        <span class="tab-label">Deploy</span>
        <span class="tab-sub">Endpoints · Environments</span>
      </button>
      <button
        type="button"
        :class="{ on: tab === 'diag' }"
        :aria-current="tab === 'diag' ? 'true' : undefined"
        title="List of core diagnostics (NO)"
        @click="tab = 'diag'"
      >
        <span class="tab-label">Diagnostics<span v-if="allDiags.length" class="tab-badge">{{ allDiags.length }}</span></span>
        <span class="tab-sub">List of NOs</span>
      </button>
    </nav>

    <!-- Broken YAML (parse NO): preserve raw text, disable structural editing, and point the user to
         fixing it in text (§7) -->
    <div v-if="!state.contract" class="broken" data-testid="broken-yaml">
      <h3>schema.sahou.yaml cannot be read (structural editing disabled)</h3>
      <DiagPane :diags="state.schemaParseDiags" />
      <div class="hint">
        Fix and save it in an external editor and the watcher will pick it up and reload
        automatically. The GUI never guesses a repair or overwrites it.
      </div>
      <textarea readonly aria-label="schema raw text (read-only)" :value="state.schemaText"></textarea>
    </div>

    <template v-else>
      <!-- Design tab: graph view or spreadsheet workbook (Graph | Table toggle; both edit the same IR) -->
      <div v-show="tab === 'design'" class="design">
        <div class="design-bar">
          <div class="seg view-toggle" role="group" aria-label="Design view">
            <button
              type="button"
              :class="{ on: designView === 'graph' }"
              data-testid="view-graph"
              title="Node graph (types on connection lines)"
              @click="designView = 'graph'"
            >Graph</button>
            <button
              type="button"
              :class="{ on: designView === 'table' }"
              data-testid="view-table"
              title="Spreadsheet workbook (Nodes / Connections / Messages sheets)"
              @click="designView = 'table'"
            >Table</button>
          </div>
        </div>
        <div class="body">
          <div v-show="designView === 'graph'" class="canvas">
            <div id="graph" ref="container"></div>
            <!-- In-canvas tools (requirement 5: add buttons on the main screen — pressing them opens a modal first) -->
            <div class="canvas-tools" role="toolbar" aria-label="Canvas tools">
              <button type="button" class="primary" data-testid="canvas-add-node" @click="showAddNode = true">
                ＋ Node
              </button>
              <button type="button" data-testid="canvas-add-conn" @click="openAddConn">＋ Connection</button>
              <button type="button" title="Auto-arrange nodes (antv-dagre)" @click="onAutoLayout">Arrange</button>
            </div>
            <!-- Wiring display (requirement 2: a subtle segmented toggle in the canvas top-right) -->
            <div class="canvas-mode seg" role="group" aria-label="Wiring display">
              <button
                type="button"
                :class="{ on: mode === 'direct' }"
                title="Draw one from→to edge per connection"
                @click="mode = 'direct'"
              >Per-connection</button>
              <button
                type="button"
                :class="{ on: mode === 'bus' }"
                title="Aggregate via a topic waypoint"
                @click="mode = 'bus'"
              >Topic bus</button>
            </div>
          </div>
          <aside v-show="designView === 'graph'" class="panel">
            <NodePane
              v-if="selNodeId"
              :contract="state.contract"
              :id="selNodeId"
              @change="setContract"
              @renamed="(id: string) => (selected = { kind: 'node', id })"
              @deleted="selected = null"
              @connect="openAddConn"
            />
            <ConnectionPane
              v-else-if="selConnId"
              :contract="state.contract"
              :id="selConnId"
              :descriptor="state.descriptor"
              :descriptor-stale="state.descriptorStale"
              @change="setContract"
              @renamed="(id: string) => (selected = { kind: 'edge', id })"
              @deleted="selected = null"
            />
            <section v-else>
              <div class="hint">
                Click a node / edge to select it, then edit it here. To add, use "＋ Node / ＋ Connection"
                in the canvas top-left. Or switch to the Table view to edit everything as a spreadsheet.
              </div>
            </section>
          </aside>
          <!-- Table (workbook) view: full-width, no right panel (the sheets edit everything directly) -->
          <div v-show="designView === 'table'" class="table-canvas">
            <TableView
              :contract="state.contract"
              :descriptor="state.descriptor"
              :selected="selected"
              @change="setContract"
              @select="selected = $event"
            />
          </div>
        </div>
      </div>

      <!-- Deploy tab -->
      <div v-show="tab === 'deploy'" class="pane-full">
        <DeployPane :endpoints="state.endpoints" :nodes="Object.keys(state.contract.nodes)" @change="setEndpoints" />
        <DiagPane v-if="state.endpointsParseDiags.length" :diags="state.endpointsParseDiags" />
      </div>

      <!-- Diagnostics tab -->
      <div v-show="tab === 'diag'" class="pane-full">
        <DiagPane :diags="allDiags" @jump="onJump" />
      </div>

      <!-- Bottom status bar: always show diagnostics / stale / unwired advisory / autosave -->
      <footer class="statusbar" role="status" @click="tab = 'diag'">
        <span :class="allDiags.length ? 'sb-danger' : 'sb-ok'">{{ allDiags.length }} diagnostics</span>
        <span v-if="state.descriptorStale" class="sb-warn">keyexpr/hash display is stale</span>
        <span v-if="unwired.length" class="sb-warn">unwired node: {{ unwired.join(", ") }}</span>
        <span
          class="sb-save"
          :class="state.saveError ? 'sb-danger' : saving ? 'sb-warn' : 'sb-ok'"
        >{{ state.saveError || (saving ? "autosave …" : "autosave ✓") }}</span>
      </footer>
    </template>

    <!-- Add modals (requirement 8: an item is added only once confirmed in the modal) -->
    <AddNodeDialog
      v-if="showAddNode && state.contract"
      :contract="state.contract"
      @close="showAddNode = false"
      @add="onAddNodeConfirm"
    />
    <AddConnectionDialog
      v-if="showAddConn && state.contract"
      :contract="state.contract"
      :initial-from="addConnFrom"
      @close="showAddConn = false"
      @add="onAddConnConfirm"
    />

    <!-- Conflict resolution (§5.3 — the 3-way choice driven by the store state machine. Never auto-overwrites) -->
    <ConflictDialog
      v-if="conflictView"
      :kind="conflictView.kind"
      :mine="conflictView.mine"
      :theirs="conflictView.theirs"
      @resolve="(choice: 'keep' | 'reload') => store.resolveConflict(conflictView!.kind, choice)"
    />
  </div>

  <div v-else class="loading" role="status">Loading…</div>
</template>
