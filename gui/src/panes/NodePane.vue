<script setup lang="ts">
// node pane: rename / kind / delete. All edits go through the pure contract-edits functions
// (immutable updates) and only emit a new Contract — no validation (the core's job §4).
import { computed } from "vue";
import type { Contract } from "../core-bridge";
import { deleteNode, renameNode, setNodeKind } from "../edits/contract-edits";
import { nodeCaps } from "../graph/edge-style";

const props = defineProps<{ contract: Contract; id: string }>();
const emit = defineEmits<{
  change: [c: Contract];
  renamed: [id: string];
  deleted: [];
  connect: []; // contextual add-connection (with this node as from — the add itself is App's modal confirm)
}>();

const node = computed(() => props.contract.nodes[props.id]);
const kind = computed(() => node.value?.kind ?? "sahou");
const caps = computed(() => nodeCaps(props.contract, props.id));

function rename(next: string) {
  const c = renameNode(props.contract, props.id, next);
  if (c !== props.contract) {
    emit("change", c);
    emit("renamed", next);
  }
}
function pickKind(k: "sahou" | "external") {
  if (k !== kind.value) emit("change", setNodeKind(props.contract, props.id, k));
}
function remove() {
  emit("change", deleteNode(props.contract, props.id));
  emit("deleted");
}
const val = (e: Event) => (e.target as HTMLInputElement).value;
</script>

<template>
  <section v-if="node" data-testid="node-pane">
    <header class="pane-head">
      <h3>node</h3>
      <span class="chip" :class="{ muted: kind === 'sahou' }">{{ kind }}</span>
    </header>

    <label>name
      <input :value="id" aria-label="node name" @change="rename(val($event))" />
    </label>

    <div class="seg-row">
      <span id="node-kind-label">kind</span>
      <div class="seg" role="group" aria-labelledby="node-kind-label">
        <button
          type="button"
          :class="{ on: kind === 'sahou' }"
          :aria-pressed="kind === 'sahou'"
          title="A node that carries the sahou runtime"
          @click="pickKind('sahou')"
        >sahou</button>
        <button
          type="button"
          :class="{ on: kind === 'external' }"
          :aria-pressed="kind === 'external'"
          title="External gear (no runtime — OSC/MIDI etc.)"
          @click="pickKind('external')"
        >external</button>
      </div>
    </div>

    <div class="caps-row">
      <span>Capabilities (derived from wiring)</span>
      <template v-if="caps.length">
        <span v-for="c in caps" :key="c" class="chip muted">{{ c }}</span>
      </template>
      <span v-else class="hint">unwired</span>
    </div>

    <button type="button" class="primary pane-connect" data-testid="node-add-conn" @click="emit('connect')">
      ＋ Connection (from this node)
    </button>
    <button type="button" class="danger pane-del" @click="remove">Delete this node</button>
  </section>
</template>
