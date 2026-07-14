<script setup lang="ts">
// A generic shape editor (shared by payload/request/response; features derived from the spike).
// Default suggestions come from the core's wasm_sample (§5.1). Type-consistency checking is left to
// the core (invalid_default / validatePayload).
// Note: the prop is named `shape` because `slot` is a reserved Vue attribute.
import { computed } from "vue";
import type { Field, Slot } from "../core-bridge";
import { sample } from "../core-bridge";
import FieldsTable from "./FieldsTable.vue";

const props = defineProps<{ shape: Slot; label: string }>();
const emit = defineEmits<{ change: [slot: Slot] }>();

const setTyping = (t: "any" | "typed") =>
  emit(
    "change",
    t === "any"
      ? { typing: "any" }
      : { typing: "typed", kind: props.shape.kind ?? "record", fields: props.shape.fields ?? [] },
  );

function setKind(k: "record" | "opaque") {
  if (k === "record") {
    const { encoding: _encoding, ...rest } = props.shape;
    emit("change", { ...rest, kind: "record", fields: props.shape.fields ?? [] });
  } else {
    const { fields: _fields, ...rest } = props.shape;
    emit("change", { ...rest, kind: "opaque", encoding: props.shape.encoding ?? "" });
  }
}

/** An "example of a valid value" from the core's sample. No suggestion while the type is broken or fields are empty (the core would say NO). */
const suggestion = computed<Record<string, unknown> | undefined>(() => {
  const s = props.shape;
  if (s.typing !== "typed" || (s.kind ?? "record") !== "record" || !(s.fields ?? []).length) {
    return undefined;
  }
  try {
    return sample(s) as Record<string, unknown>;
  } catch {
    return undefined;
  }
});
const val = (e: Event) => (e.target as HTMLInputElement).value;
</script>

<template>
  <div class="shape" data-testid="shape-editor">
    <div class="shape-head">
      <span class="slot-label">{{ label }}</span>
      <div class="seg" role="group" :aria-label="`toggle typing for ${label}`">
        <button
          type="button"
          :class="{ on: shape.typing === 'any' }"
          :aria-pressed="shape.typing === 'any'"
          title="any = untyped (not validated)"
          @click="setTyping('any')"
        >any</button>
        <button
          type="button"
          :class="{ on: shape.typing === 'typed' }"
          :aria-pressed="shape.typing === 'typed'"
          title="typed = typed and validated at the boundary"
          @click="setTyping('typed')"
        >typed</button>
      </div>
    </div>

    <!-- any = the state where the core intentionally skips validation. The GUI makes "unvalidated" explicit (the §4 division of labor) -->
    <div v-if="shape.typing === 'any'" class="callout warn" data-testid="any-warn">
      <strong>Unvalidated</strong>
      <span>anything passes — add a type and the boundary says NO. Shown as a red edge in the graph.</span>
    </div>

    <div v-else>
      <div class="kind-row">
        <span id="kind-label">kind</span>
        <div class="seg" role="group" aria-labelledby="kind-label">
          <button
            type="button"
            :class="{ on: (shape.kind ?? 'record') === 'record' }"
            :aria-pressed="(shape.kind ?? 'record') === 'record'"
            title="record = fields (a list of fields)"
            @click="setKind('record')"
          >record (fields)</button>
          <button
            type="button"
            :class="{ on: shape.kind === 'opaque' }"
            :aria-pressed="shape.kind === 'opaque'"
            title="opaque = bytes/media (contents not inspected)"
            @click="setKind('opaque')"
          >opaque (bytes/media)</button>
        </div>
      </div>

      <FieldsTable
        v-if="(shape.kind ?? 'record') === 'record'"
        :fields="shape.fields ?? []"
        :suggestion="suggestion"
        @change="(fields: Field[]) => emit('change', { ...shape, fields })"
      />

      <label v-else class="encoding-row">
        encoding
        <input
          :value="shape.encoding ?? ''"
          placeholder="video/raw, audio/pcm…"
          aria-label="encoding"
          @change="emit('change', { ...shape, encoding: val($event) })"
        />
      </label>
    </div>
  </div>
</template>
