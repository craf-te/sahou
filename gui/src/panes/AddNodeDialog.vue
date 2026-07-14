<script setup lang="ts">
// Add-node modal (UX redesign requirement 8): the name / kind are confirmed here before adding —
// "press it and a node_N sprouts instantly" is gone. The add itself is contract-edits.addNode (a pure
// function called on the App side). A duplicate name becomes a NO (null) at add time, so we show the
// reason before confirming to prevent it (never a silent no-op).
import { computed, ref } from "vue";
import type { Contract } from "../core-bridge";
import { uniqueId, type AddNodeInput } from "../edits/contract-edits";
import BaseDialog from "./BaseDialog.vue";

const props = defineProps<{ contract: Contract }>();
const emit = defineEmits<{ close: []; add: [v: AddNodeInput] }>();

const name = ref(uniqueId("node", props.contract.nodes)); // start from a unique suggestion (confirmable right away)
const kind = ref<"sahou" | "external">("sahou");
const error = computed(() => {
  const n = name.value.trim();
  if (n === "") return "Enter a name";
  if (n in props.contract.nodes) return `"${n}" already exists (node names are unique)`;
  return null;
});

function confirm() {
  if (error.value) return;
  emit("add", { name: name.value.trim(), kind: kind.value });
}
const val = (e: Event) => (e.target as HTMLInputElement).value;
</script>

<template>
  <BaseDialog title="Add node" data-testid="add-node-dialog" @close="emit('close')">
    <form @submit.prevent="confirm">
      <label>Name
        <input
          data-autofocus
          :value="name"
          :aria-invalid="error ? 'true' : undefined"
          aria-label="new node name"
          placeholder="e.g. sensor / visuals"
          @input="name = val($event)"
        />
      </label>
      <div class="seg-row">
        <span id="add-node-kind">kind</span>
        <div class="seg" role="group" aria-labelledby="add-node-kind">
          <button
            type="button"
            :class="{ on: kind === 'sahou' }"
            :aria-pressed="kind === 'sahou'"
            title="A node that carries the sahou runtime"
            @click="kind = 'sahou'"
          >sahou</button>
          <button
            type="button"
            :class="{ on: kind === 'external' }"
            :aria-pressed="kind === 'external'"
            title="External gear (no runtime — OSC/MIDI etc.)"
            @click="kind = 'external'"
          >external</button>
        </div>
      </div>
      <div v-if="error" class="warn" role="alert" data-testid="dialog-error">{{ error }}</div>
      <div class="modal-btns">
        <button type="button" data-testid="dialog-cancel" @click="emit('close')">Cancel</button>
        <button type="button" class="primary" data-testid="dialog-confirm" :disabled="!!error" @click="confirm">
          Add
        </button>
      </div>
    </form>
  </BaseDialog>
</template>
