<script setup lang="ts">
// Diagnostics pane: shows the core's {code, path, message} verbatim (the GUI authors no error text §4/§6).
// Clicking / pressing Enter on a row jumps to the matching pane via the unified path grammar (diag-target).
import type { SahouDiag } from "../core-bridge";
import { targetOf } from "./diag-target";

defineProps<{ diags: SahouDiag[] }>();
const emit = defineEmits<{ jump: [target: { kind: "edge" | "node"; id: string }] }>();

function jump(d: SahouDiag) {
  const t = targetOf(d.path);
  if (t) emit("jump", t);
}
</script>

<template>
  <div class="diag-pane">
    <h3>Diagnostics (NO at the boundary · the core is authoritative)</h3>
    <div v-if="!diags.length" class="callout ok" data-testid="no-diags">
      <strong>✓ No problems</strong>
      <span>the contract passes the boundary</span>
    </div>
    <ul v-else class="diag-list">
      <li
        v-for="(d, i) in diags"
        :key="i"
        class="diag"
        data-testid="diag-row"
        :role="targetOf(d.path) ? 'button' : undefined"
        :tabindex="targetOf(d.path) ? 0 : undefined"
        :title="targetOf(d.path) ? 'Jump to the location' : undefined"
        @click="jump(d)"
        @keydown.enter.prevent="jump(d)"
      >
        <span class="code">[{{ d.code }}]</span>
        <span class="path">@{{ d.path }}</span>
        <span class="msg">{{ d.message }}</span>
        <span v-if="targetOf(d.path)" class="jump-mark" aria-hidden="true">→</span>
      </li>
    </ul>
  </div>
</template>
