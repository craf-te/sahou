<script setup lang="ts">
// Deploy tab (separate from the contract, DZ3 §6). Editing goes through the pure endpoints-edits
// functions → emitted via change. empty / auto = "don't write it" = LAN auto (progressive disclosure
// — explicit settings only when needed).
import type { Endpoints } from "../core-bridge";
import {
  setEnv, setNamespace, setNodeConnect, setNodeMode, setPlugins, setRouter,
} from "../edits/endpoints-edits";

const props = defineProps<{ endpoints: Endpoints; nodes: string[] }>();
const emit = defineEmits<{ change: [e: Endpoints] }>();

const ch = (e: Endpoints) => emit("change", e);
const val = (e: Event) => (e.target as HTMLInputElement).value;
const checked = (e: Event) => (e.target as HTMLInputElement).checked;
const modeOf = (id: string) => props.endpoints.nodes[id]?.mode ?? "auto";
const connectOf = (id: string) => (props.endpoints.nodes[id]?.connect ?? []).join(", ");
</script>

<template>
  <div data-testid="deploy-pane">
    <h3>Deploy (placement · separate from the contract)</h3>
    <div class="callout ok">
      <strong>✓ The same LAN connects automatically</strong>
      <span>peer + multicast scouting · no IP/port setup needed. The below is only for browser / Node, a different subnet, or when multicast is unavailable.</span>
    </div>

    <div class="grid2">
      <label>env
        <input :value="endpoints.env ?? ''" aria-label="env" @change="ch(setEnv(endpoints, val($event)))" />
      </label>
      <label>namespace (default keyexpr prefix)
        <input :value="endpoints.namespace" aria-label="namespace" @change="ch(setNamespace(endpoints, val($event)))" />
      </label>
    </div>

    <details class="adv">
      <summary>router / explicit endpoints (only when needed)</summary>
      <label class="chkline">
        <input
          type="checkbox"
          :checked="!!endpoints.router?.enabled"
          @change="ch(setRouter(endpoints, checked($event), endpoints.router?.endpoint))"
        />
        Use a router (required for browser / Node)
      </label>
      <label v-if="endpoints.router?.enabled">router endpoint
        <input
          :value="endpoints.router?.endpoint ?? ''"
          placeholder="tcp/host:7447"
          aria-label="router endpoint"
          @change="ch(setRouter(endpoints, true, val($event)))"
        />
      </label>

      <h4>Per-node overrides (default auto = LAN auto)</h4>
      <table class="dep">
        <thead><tr><th>node</th><th>mode</th><th>connect (empty = auto)</th></tr></thead>
        <tbody>
          <tr v-for="id in nodes" :key="id">
            <td>{{ id }}</td>
            <td>
              <select
                :value="modeOf(id)"
                :aria-label="`mode for ${id}`"
                @change="ch(setNodeMode(endpoints, id, val($event) as 'auto' | 'peer' | 'client'))"
              >
                <option value="auto">auto (LAN auto)</option>
                <option value="peer">peer</option>
                <option value="client">client</option>
              </select>
            </td>
            <td>
              <input
                :value="connectOf(id)"
                placeholder="tcp/host:7447 …"
                :aria-label="`connect for ${id}`"
                @change="ch(setNodeConnect(endpoints, id, val($event)))"
              />
            </td>
          </tr>
        </tbody>
      </table>

      <label>plugins (infra: rest / mqtt / …)
        <input
          :value="(endpoints.plugins ?? []).join(', ')"
          aria-label="plugins"
          @change="ch(setPlugins(endpoints, val($event)))"
        />
      </label>
    </details>
  </div>
</template>
