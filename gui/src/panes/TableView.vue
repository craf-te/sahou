<script setup lang="ts">
// Node-centric spreadsheet view: one sheet (tab) per node. The active node's sheet lists what it sends
// (from === node) and what it receives (node in to); each row is a connection whose type is editable
// inline on expand. Adding a sheet = adding a node. Editing goes through the pure contract-edits
// functions and returns a new contract to the shared store (instant sync with the graph). No validation
// here — the core does every NO (§4).
import { computed, ref, watchEffect } from "vue";
import type { Contract, Descriptor } from "../core-bridge";
import { addConnection, addNode, deleteNode, renameNode, setNodeKind, uniqueId } from "../edits/contract-edits";
import type { Selected } from "../graph/use-graph";
import { receives, sends } from "../table/sheet-model";
import ConnRow from "./ConnRow.vue";

const props = defineProps<{
  contract: Contract;
  descriptor: Descriptor | null;
  selected: Selected | null;
}>();
const emit = defineEmits<{ change: [c: Contract]; select: [s: Selected | null] }>();

const active = ref<string | null>(null);
// Keep the active node valid (fall back to the first node when it is null or was deleted/renamed away).
watchEffect(() => {
  if (!active.value || !props.contract.nodes[active.value]) {
    active.value = Object.keys(props.contract.nodes)[0] ?? null;
  }
});

const nodeIds = computed(() => Object.keys(props.contract.nodes));
const otherNodes = computed(() => nodeIds.value.filter((n) => n !== active.value));
const sendIds = computed(() => (active.value ? sends(props.contract, active.value) : []));
const receiveIds = computed(() => (active.value ? receives(props.contract, active.value) : []));

const expanded = ref<Set<string>>(new Set());
function toggle(id: string) {
  const s = new Set(expanded.value);
  if (s.has(id)) s.delete(id);
  else s.add(id);
  expanded.value = s;
}

const val = (e: Event) => (e.target as HTMLInputElement).value;

function selectNode(id: string) {
  active.value = id;
  emit("select", { kind: "node", id });
}
function addNodeSheet() {
  const id = uniqueId("node", props.contract.nodes);
  const r = addNode(props.contract, { name: id, kind: "sahou" });
  if (!r) return;
  emit("change", r.contract);
  active.value = r.id;
  emit("select", { kind: "node", id: r.id });
}
function rename(next: string) {
  const n = next.trim();
  if (!active.value || n === active.value || n === "") return;
  emit("change", renameNode(props.contract, active.value, n));
  active.value = n;
  emit("select", { kind: "node", id: n });
}
function deleteActive() {
  if (!active.value) return;
  emit("change", deleteNode(props.contract, active.value));
  active.value = null; // watchEffect re-picks the first node
  emit("select", null);
}
function addSend() {
  if (!active.value) return;
  const r = addConnection(props.contract, { from: active.value, to: [], pattern: "pub_sub" });
  if (r) {
    emit("change", r.contract);
    expanded.value = new Set(expanded.value).add(r.id);
  }
}
function addReceive() {
  const from = otherNodes.value[0];
  if (!active.value || !from) return;
  const r = addConnection(props.contract, { from, to: [active.value], pattern: "pub_sub" });
  if (r) {
    emit("change", r.contract);
    expanded.value = new Set(expanded.value).add(r.id);
  }
}
</script>

<template>
  <div class="table-view" data-testid="table-view">
    <!-- Node tabs: one sheet per node; ＋ adds a node -->
    <nav class="node-tabs" role="tablist" aria-label="Node sheets">
      <button
        v-for="id in nodeIds"
        :key="id"
        type="button"
        role="tab"
        :class="{ on: id === active }"
        :aria-selected="id === active"
        :data-testid="`node-tab-${id}`"
        @click="selectNode(id)"
      >{{ id }}</button>
      <button type="button" class="add-tab" data-testid="add-node-sheet" title="Add a node (new sheet)" @click="addNodeSheet">＋</button>
    </nav>

    <p v-if="!active" class="hint">No nodes yet — press ＋ to add the first node.</p>

    <template v-else>
      <!-- Active node header: rename / kind / delete -->
      <div class="node-head" data-testid="node-head">
        <input class="node-name" :value="active" aria-label="Node name" @change="rename(val($event))" />
        <select
          :value="contract.nodes[active].kind ?? 'sahou'"
          aria-label="Node kind"
          @change="emit('change', setNodeKind(contract, active, val($event) as 'sahou' | 'external'))"
        >
          <option value="sahou">sahou</option>
          <option value="external">external</option>
        </select>
        <button type="button" class="icon-btn del" aria-label="Delete node" title="Delete node (also removes its connections)" @click="deleteActive">✕ node</button>
      </div>

      <!-- Sends: from === active -->
      <section class="dir-section" data-testid="sends-section">
        <h3>Sends <span class="hint">from {{ active }}</span></h3>
        <table class="sheet">
          <thead>
            <tr><th>To</th><th>Key</th><th>Pattern</th><th>Type (expand to edit)</th><th class="ops"></th></tr>
          </thead>
          <tbody>
            <ConnRow
              v-for="id in sendIds"
              :key="id"
              :contract="contract"
              :id="id"
              :node="active"
              direction="send"
              :descriptor="descriptor"
              :expanded="expanded.has(id)"
              @change="emit('change', $event)"
              @toggle="toggle"
            />
          </tbody>
        </table>
        <div class="row-btns">
          <button type="button" class="btn-ghost" data-testid="add-send" @click="addSend">＋ send</button>
          <span v-if="sendIds.length === 0" class="hint">nothing sent yet</span>
        </div>
      </section>

      <!-- Receives: active in to -->
      <section class="dir-section" data-testid="receives-section">
        <h3>Receives <span class="hint">to {{ active }}</span></h3>
        <table class="sheet">
          <thead>
            <tr><th>From</th><th>Key</th><th>Pattern</th><th>Type (expand to edit)</th><th class="ops"></th></tr>
          </thead>
          <tbody>
            <ConnRow
              v-for="id in receiveIds"
              :key="id"
              :contract="contract"
              :id="id"
              :node="active"
              direction="receive"
              :descriptor="descriptor"
              :expanded="expanded.has(id)"
              @change="emit('change', $event)"
              @toggle="toggle"
            />
          </tbody>
        </table>
        <div class="row-btns">
          <button type="button" class="btn-ghost" data-testid="add-receive" :disabled="otherNodes.length === 0" @click="addReceive">＋ receive</button>
          <span v-if="otherNodes.length === 0" class="hint">add another node first</span>
        </div>
      </section>
    </template>
  </div>
</template>

<style scoped>
.table-view { display: flex; flex-direction: column; gap: 12px; height: 100%; }
.node-tabs { display: flex; flex-wrap: wrap; gap: 4px; border-bottom: 1px solid var(--border); padding-bottom: 6px; }
.node-tabs button {
  border: 1px solid transparent;
  border-radius: var(--radius-md) var(--radius-md) 0 0;
  background: transparent;
  color: var(--text-2);
  font-family: var(--font-mono);
}
.node-tabs button.on { background: var(--accent-bg); border-color: var(--accent-border); color: var(--accent-strong); font-weight: 600; }
.node-tabs .add-tab { font-family: inherit; }
.node-head { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.node-head .node-name { width: 200px; font-family: var(--font-mono); font-weight: 600; }
.dir-section h3 { margin: 4px 0 6px; font-size: var(--text-md); color: var(--accent-strong); }
.dir-section h3 .hint { font-weight: 400; }
</style>
