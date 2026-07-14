<script setup lang="ts">
// connection pane (features derived from the spike): pattern / from / to / shape slots /
// the 3-way delivery choice + advanced knobs / selector (query only §5.2) / effective keyexpr
// (from the descriptor; shown as stale §7). Editing goes through the pure contract-edits functions
// (immutable updates). `to` offers only nodes other than `from` = a self-loop is impossible to draw
// by construction (declarative pane §9).
import { computed, ref } from "vue";
import type { Connection, Contract, Descriptor, Slot } from "../core-bridge";
import {
  deleteConnection, renameConnection, setDelivery, setFrom, setPattern, toggleTarget, updateConnection,
} from "../edits/contract-edits";
import { slotsFor } from "../graph/edge-style";
import ShapeEditor from "./ShapeEditor.vue";

const props = defineProps<{
  contract: Contract;
  id: string;
  descriptor: Descriptor | null;
  descriptorStale: boolean;
}>();
const emit = defineEmits<{ change: [c: Contract]; renamed: [id: string]; deleted: [] }>();

const conn = computed<Connection | undefined>(() => props.contract.connections[props.id]);
const patch = (p: Partial<Connection>) => emit("change", updateConnection(props.contract, props.id, p));
const val = (e: Event) => (e.target as HTMLInputElement).value;
const checked = (e: Event) => (e.target as HTMLInputElement).checked;

function rename(next: string) {
  const c = renameConnection(props.contract, props.id, next);
  if (c !== props.contract) {
    emit("change", c);
    emit("renamed", next);
  }
}

// Delivery (a 3-way speed preset): Stream = best_effort+drop / Reliable = reliable+block /
// Custom = raw knobs (reliability / congestion individually)
type Delivery = "stream" | "reliable" | "custom";
const customOn = ref(false);
const delivery = computed<Delivery>(() => {
  const c = conn.value;
  if (!c) return "custom";
  const r = c.reliability ?? "best_effort";
  const g = c.congestion ?? "drop";
  if (r === "best_effort" && g === "drop") return "stream";
  if (r === "reliable" && g === "block") return "reliable";
  return "custom";
});
const isCustom = computed(() => customOn.value || delivery.value === "custom");
function pickDelivery(m: Delivery) {
  if (m === "custom") {
    customOn.value = true;
    return;
  }
  customOn.value = false;
  emit("change", setDelivery(props.contract, props.id, m));
}

// Declarative to (§9): offer only valid targets (nodes other than from)
const targets = computed(() =>
  conn.value ? Object.keys(props.contract.nodes).filter((n) => n !== conn.value!.from) : [],
);
const effectiveKey = computed(() => props.descriptor?.connections[props.id]?.key ?? null);
const slotOf = (k: "payload" | "request" | "response"): Slot => conn.value?.[k] ?? { typing: "any" };
</script>

<template>
  <section v-if="conn" data-testid="connection-pane">
    <header class="pane-head">
      <h3>connection</h3>
      <span class="chip">{{ conn.pattern }}</span>
    </header>

    <label>name
      <input :value="id" aria-label="connection name" @change="rename(val($event))" />
    </label>

    <div class="seg-row">
      <span id="conn-pattern-label">pattern</span>
      <div class="seg" role="group" aria-labelledby="conn-pattern-label">
        <button
          type="button"
          :class="{ on: conn.pattern === 'pub_sub' }"
          :aria-pressed="conn.pattern === 'pub_sub'"
          title="pub_sub = one-way publishing (1→N)"
          @click="emit('change', setPattern(contract, id, 'pub_sub'))"
        >pub_sub</button>
        <button
          type="button"
          :class="{ on: conn.pattern === 'query' }"
          :aria-pressed="conn.pattern === 'query'"
          title="query = request→response (the response comes back reliable)"
          @click="emit('change', setPattern(contract, id, 'query'))"
        >query</button>
      </div>
    </div>

    <label>from
      <select :value="conn.from" aria-label="from" @change="emit('change', setFrom(contract, id, val($event)))">
        <option v-for="(_n, nid) in contract.nodes" :key="nid" :value="nid">{{ nid }}</option>
      </select>
    </label>

    <div class="seg-row"><span>to (targets · multiple allowed)</span></div>
    <div class="to-list">
      <label v-for="t in targets" :key="t" class="chk">
        <input
          type="checkbox"
          :checked="conn.to.includes(t)"
          @change="emit('change', toggleTarget(contract, id, t))"
        />{{ t }}
      </label>
    </div>

    <!-- selector (query only · a contract attribute §5.2 — not a GUI-local decoration) -->
    <label v-if="conn.pattern === 'query'">selector (the query's request condition · lives in the contract)
      <input
        :value="conn.selector ?? ''"
        placeholder="?level=info"
        aria-label="selector"
        @change="patch({ selector: val($event) || undefined })"
      />
    </label>

    <!-- shape slots (1 or 2 depending on the pattern) -->
    <ShapeEditor
      v-for="k in slotsFor(conn)"
      :key="k"
      :shape="slotOf(k)"
      :label="k"
      @change="(s: Slot) => patch({ [k]: s })"
    />

    <template v-if="conn.pattern !== 'query'">
      <div class="seg-row">
        <span id="conn-delivery-label">Delivery</span>
        <div class="seg" role="group" aria-labelledby="conn-delivery-label">
          <button
            type="button"
            :class="{ on: !isCustom && delivery === 'stream' }"
            :aria-pressed="!isCustom && delivery === 'stream'"
            @click="pickDelivery('stream')"
          >Stream</button>
          <button
            type="button"
            :class="{ on: !isCustom && delivery === 'reliable' }"
            :aria-pressed="!isCustom && delivery === 'reliable'"
            @click="pickDelivery('reliable')"
          >Reliable</button>
          <button
            type="button"
            :class="{ on: isCustom }"
            :aria-pressed="isCustom"
            @click="pickDelivery('custom')"
          >Custom</button>
        </div>
      </div>
      <div v-if="!isCustom" class="hint">
        {{ delivery === "stream"
          ? "latest-wins · OK to drop · never blocks (video / audio / sensors)"
          : "never lose one · wait when congested (commands / queues / events) — effectively TCP-like on the same LAN" }}
      </div>
      <div v-else class="grid2">
        <label>reliability
          <select
            :value="conn.reliability ?? 'best_effort'"
            aria-label="reliability"
            @change="patch({ reliability: val($event) as 'best_effort' | 'reliable' })"
          >
            <option value="reliable">reliable (resend)</option>
            <option value="best_effort">best_effort (fire-and-forget)</option>
          </select>
        </label>
        <label>congestion
          <select
            :value="conn.congestion ?? 'drop'"
            aria-label="congestion"
            @change="patch({ congestion: val($event) as 'drop' | 'block' })"
          >
            <option value="drop">drop (discard when congested)</option>
            <option value="block">block (wait when congested)</option>
          </select>
        </label>
      </div>
    </template>
    <div v-else class="hint">query: the response comes back reliable</div>

    <!-- Advanced: priority is orthogonal to delivery (ordering only, not a delivery guarantee). Default data. -->
    <details class="adv">
      <summary>Advanced (priority / express / keyexpr)</summary>
      <label>priority (send order under congestion · default data)
        <select
          :value="conn.priority ?? 'data'"
          aria-label="priority"
          @change="patch({ priority: val($event) as Connection['priority'] })"
        >
          <option>realtime</option>
          <option>interactive_high</option>
          <option>interactive_low</option>
          <option>data_high</option>
          <option value="data">data (default)</option>
          <option>data_low</option>
          <option>background</option>
        </select>
      </label>
      <label class="chkline">
        <input
          type="checkbox"
          :checked="!!conn.express"
          @change="patch({ express: checked($event) || undefined })"
        />
        express (suppress batching · low latency)
      </label>
      <label>keyexpr (override)
        <input
          :value="conn.key ?? ''"
          placeholder="auto-derived"
          aria-label="keyexpr override"
          @change="patch({ key: val($event) || undefined })"
        />
      </label>
      <div class="hint keyexpr">effective keyexpr:
        <code>{{ effectiveKey ?? "—" }}</code>
        <span v-if="descriptorStale" class="stale" data-testid="keyexpr-stale">stale — updates once diagnostics clear (§7)</span>
      </div>
    </details>

    <button
      type="button"
      class="danger pane-del"
      @click="emit('change', deleteConnection(contract, id)); emit('deleted')"
    >Delete this connection</button>
  </section>
</template>
