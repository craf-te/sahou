<script setup lang="ts">
// One connection row in a node's sheet (send or receive direction). Collapsed = a one-line summary
// (destination · key · pattern · type); expanded = the connection's slot type editor(s) inline. Editing
// goes through the pure contract-edits functions; the core does every NO (§4).
import { computed } from "vue";
import type { Connection, Contract, Descriptor, Slot } from "../core-bridge";
import {
  deleteConnection, setPattern, toggleTarget, updateConnection,
} from "../edits/contract-edits";
import { slotNamesOf, typeSummary } from "../table/sheet-model";
import ShapeEditor from "./ShapeEditor.vue";

const props = defineProps<{
  contract: Contract;
  id: string;
  node: string; // the active node whose sheet this row belongs to
  direction: "send" | "receive";
  descriptor: Descriptor | null;
  expanded: boolean;
}>();
const emit = defineEmits<{ change: [c: Contract]; toggle: [id: string] }>();

const conn = computed<Connection>(() => props.contract.connections[props.id]);
const otherNodes = computed(() => Object.keys(props.contract.nodes).filter((n) => n !== props.node));
/** Effective keyexpr: the descriptor's resolved key when available, else the override or the id. */
const effectiveKey = computed(
  () => props.descriptor?.connections[props.id]?.key ?? conn.value.key ?? props.id,
);
const slots = computed(() => slotNamesOf(conn.value));
const summary = computed(() => slots.value.map((s) => `${s}: ${typeSummary(conn.value[s])}`).join("  ·  "));

const val = (e: Event) => (e.target as HTMLInputElement).value;
const patch = (p: Partial<Connection>) => emit("change", updateConnection(props.contract, props.id, p));
function setKey(v: string) {
  const k = v.trim();
  // Writing the derived value back would freeze it as an override, so only write when it actually differs.
  patch({ key: k === "" || k === effectiveKey.value ? undefined : k });
}
function setSlot(slot: "payload" | "request" | "response", next: Slot) {
  patch({ [slot]: next } as Partial<Connection>);
}
</script>

<template>
  <tr :data-testid="`conn-row-${direction}`" class="conn-summary">
    <td class="dir-cell">
      <template v-if="direction === 'send'">
        <button
          v-for="n in otherNodes"
          :key="n"
          type="button"
          class="chip"
          :class="{ muted: !conn.to.includes(n) }"
          :aria-pressed="conn.to.includes(n)"
          :title="`toggle ${n} as a target`"
          @click="emit('change', toggleTarget(contract, id, n))"
        >{{ n }}</button>
        <span v-if="conn.to.length === 0" class="hint">no target</span>
      </template>
      <span v-else class="chip muted">{{ conn.from }}</span>
    </td>
    <td>
      <input :value="effectiveKey" aria-label="Key (keyexpr)" @change="setKey(val($event))" />
    </td>
    <td>
      <select
        :value="conn.pattern"
        aria-label="Pattern"
        @change="emit('change', setPattern(contract, id, val($event) as 'pub_sub' | 'query'))"
      >
        <option value="pub_sub">pub_sub</option>
        <option value="query">query</option>
      </select>
    </td>
    <td class="type-cell">
      <button
        type="button"
        class="btn-ghost expand"
        :aria-expanded="expanded"
        :data-testid="`expand-${id}`"
        @click="emit('toggle', id)"
      >{{ expanded ? "▾" : "▸" }} {{ summary }}</button>
    </td>
    <td class="ops">
      <button
        type="button"
        class="icon-btn del"
        :aria-label="direction === 'send' ? 'Delete connection' : 'Stop receiving (remove from targets)'"
        :title="direction === 'send' ? 'Delete connection' : 'Stop receiving (remove this node from the targets)'"
        @click="emit('change', direction === 'send' ? deleteConnection(contract, id) : toggleTarget(contract, id, node))"
      >✕</button>
    </td>
  </tr>
  <tr v-if="expanded" class="conn-detail">
    <td :colspan="5" class="nest">
      <ShapeEditor
        v-for="s in slots"
        :key="s"
        :shape="conn[s] ?? { typing: 'any' }"
        :label="s"
        @change="(slot: Slot) => setSlot(s, slot)"
      />
    </td>
  </tr>
</template>

<style scoped>
/* One-line cells so every summary row keeps the same height (spreadsheet grid) */
.dir-cell { display: flex; flex-wrap: nowrap; gap: 3px; align-items: center; overflow-x: auto; }
.dir-cell .chip { cursor: pointer; flex: none; }
.expand {
  display: block;
  width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: left;
  font-family: var(--font-mono);
  font-size: var(--text-sm);
}
</style>
