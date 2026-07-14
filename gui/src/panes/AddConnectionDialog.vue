<script setup lang="ts">
// Add-connection modal (UX redesign requirement 6/7/8): from (the selected node is pre-selected) /
// to / pattern are confirmed here before adding — the instant creation of placeholder connections is
// gone. `to` candidates are valid targets only (not `from`; declarative pane §9) = a self-loop is
// impossible to draw by construction. The add itself is contract-edits.addConnection (a pure
// function called on the App side).
import { computed, ref } from "vue";
import type { Contract } from "../core-bridge";
import type { AddConnectionInput } from "../edits/contract-edits";
import BaseDialog from "./BaseDialog.vue";

const props = defineProps<{ contract: Contract; initialFrom?: string | null }>();
const emit = defineEmits<{ close: []; add: [v: AddConnectionInput] }>();

const nodeIds = computed(() => Object.keys(props.contract.nodes));
const from = ref(
  props.initialFrom && props.initialFrom in props.contract.nodes
    ? props.initialFrom
    : (nodeIds.value[0] ?? ""),
);
const to = ref<string[]>([]);
const pattern = ref<"pub_sub" | "query">("pub_sub");

// Declarative to (§9): offer only nodes other than from
const targets = computed(() => nodeIds.value.filter((n) => n !== from.value));

function setFromValue(next: string) {
  from.value = next;
  to.value = to.value.filter((t) => t !== next); // drop the new from from the targets (immutable update)
}
function toggleTo(node: string) {
  to.value = to.value.includes(node) ? to.value.filter((t) => t !== node) : [...to.value, node];
}
function confirm() {
  if (!from.value) return;
  emit("add", { from: from.value, to: to.value, pattern: pattern.value });
}
const val = (e: Event) => (e.target as HTMLSelectElement).value;
</script>

<template>
  <BaseDialog title="Add connection" data-testid="add-conn-dialog" @close="emit('close')">
    <form @submit.prevent="confirm">
      <template v-if="nodeIds.length">
        <label>from (sender)
          <select data-autofocus :value="from" aria-label="from" @change="setFromValue(val($event))">
            <option v-for="n in nodeIds" :key="n" :value="n">{{ n }}</option>
          </select>
        </label>
        <div class="seg-row"><span>to (targets · multiple allowed · changeable later)</span></div>
        <div class="to-list">
          <label v-for="t in targets" :key="t" class="chk">
            <input type="checkbox" :checked="to.includes(t)" @change="toggleTo(t)" />{{ t }}
          </label>
        </div>
        <div class="seg-row">
          <span id="add-conn-pattern">pattern</span>
          <div class="seg" role="group" aria-labelledby="add-conn-pattern">
            <button
              type="button"
              :class="{ on: pattern === 'pub_sub' }"
              :aria-pressed="pattern === 'pub_sub'"
              title="pub_sub = one-way publishing (1→N)"
              @click="pattern = 'pub_sub'"
            >pub_sub</button>
            <button
              type="button"
              :class="{ on: pattern === 'query' }"
              :aria-pressed="pattern === 'query'"
              title="query = request→response (the response comes back reliable)"
              @click="pattern = 'query'"
            >query</button>
          </div>
        </div>
      </template>
      <div v-else class="hint">No nodes yet — add one first with "＋ Node" in the canvas.</div>
      <div class="modal-btns">
        <button type="button" data-testid="dialog-cancel" @click="emit('close')">Cancel</button>
        <button type="button" class="primary" data-testid="dialog-confirm" :disabled="!nodeIds.length" @click="confirm">
          Add
        </button>
      </div>
    </form>
  </BaseDialog>
</template>
