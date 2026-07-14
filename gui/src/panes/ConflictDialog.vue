<script setup lang="ts">
// Conflict resolution (§5.3): the 3-way choice "keep / discard and reload / show diff".
// Safe default = keep local (nothing is auto-overwritten unless this dialog is dismissed).
// The decision and the overwrite both live in the store state machine — this only emits the choice.
import { computed, onMounted, ref } from "vue";
import { diffLines } from "../edits/diff";

const props = defineProps<{ kind: string; mine: string; theirs: string }>();
const emit = defineEmits<{ resolve: [choice: "keep" | "reload"] }>();

const showDiff = ref(false);
const lines = computed(() => diffLines(props.mine, props.theirs));
const keepBtn = ref<HTMLButtonElement | null>(null);
onMounted(() => keepBtn.value?.focus()); // initial focus = the safe side (keep local)
</script>

<template>
  <div class="conflict-backdrop">
    <div class="conflict" role="dialog" aria-modal="true" aria-labelledby="conflict-title">
      <h3 id="conflict-title">External edit conflict ({{ kind }})</h3>
      <p>
        The file was changed externally. Your local edits are not lost
        (safe default = keep local — nothing is auto-overwritten until you choose).
      </p>
      <div class="btns">
        <button ref="keepBtn" type="button" class="primary" @click="emit('resolve', 'keep')">
          Keep local edits (overwrite external)
        </button>
        <button type="button" class="danger" @click="emit('resolve', 'reload')">
          Discard and reload
        </button>
        <button type="button" :aria-expanded="showDiff" @click="showDiff = !showDiff">
          {{ showDiff ? "Hide diff" : "Show diff" }}
        </button>
      </div>
      <pre v-if="showDiff" class="diff" data-testid="conflict-diff"><span v-for="(l, i) in lines" :key="i" :class="l.kind">{{ l.kind === "add" ? "+" : l.kind === "del" ? "-" : " " }} {{ l.text }}</span></pre>
      <div v-if="showDiff" class="hint">- = local only / + = external only (raw-text diff, line-wise)</div>
    </div>
  </div>
</template>
