<script setup lang="ts">
// Fields editor (recursive; features derived from the spike). Editing emits a new array immutably
// produced by the pure functions in edits/contract-edits. It performs no validation — the type
// consistency of a default is delegated to the core's validatePayload, and a NO is displayed inline
// as {code, path, message} verbatim (§4/§5.1 · the GUI authors no error text).
// Layout: each field is a flex-wrap row of min-width controls, so on a narrow pane the controls drop to
// the next line (grow vertically) instead of crushing or forcing horizontal scroll (responsive §pane).
import { computed, ref } from "vue";
import type { Field, SahouDiag, TypeNameStr } from "../core-bridge";
import { validatePayload } from "../core-bridge";
import {
  addFieldAt, groupFieldsAt, removeFieldAt, ungroupFieldAt, updateFieldAt,
} from "../edits/contract-edits";

const props = defineProps<{ fields: Field[]; suggestion?: Record<string, unknown> }>();
const emit = defineEmits<{ change: [fields: Field[]] }>();

const SCALARS: TypeNameStr[] = ["int", "float", "string", "bool", "bytes", "timestamp"];
const FIELD_TYPES: TypeNameStr[] = [...SCALARS, "enum", "array", "map", "group"];
const sel = ref<Set<number>>(new Set());

function toggleSel(i: number) {
  const s = new Set(sel.value);
  if (s.has(i)) s.delete(i);
  else s.add(i);
  sel.value = s;
}

const change = (next: Field[]) => {
  sel.value = new Set();
  emit("change", next);
};
const patch = (i: number, p: Partial<Field>) => change(updateFieldAt(props.fields, [i], p));

const setType = (i: number, t: TypeNameStr) =>
  patch(i, { type: t, ...(t === "group" ? { fields: props.fields[i].fields ?? [] } : {}) });
const setEnum = (i: number, v: string) =>
  patch(i, { values: v.split(",").map((s) => s.trim()).filter(Boolean) });

function setDefault(i: number, raw: string) {
  if (raw.trim() === "") {
    patch(i, { default: undefined }); // empty = delete the default (§5.1)
    return;
  }
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    value = raw; // if not JSON, treat as a plain string (the core decides type consistency §5.1)
  }
  patch(i, { default: value });
}

/** Type consistency of the default = delegated to the core's validatePayload. We just pass the same
 *  input as the core's check_default ("wrap a single field into a 1-field Slot"); the decision is 100% the core's. */
const defaultDiags = computed<SahouDiag[][]>(() =>
  props.fields.map((f) =>
    f.default === undefined
      ? []
      : validatePayload({ typing: "typed", fields: [f] }, { [f.name]: f.default }),
  ),
);

/** A suggested value from wasm_sample (shown as an "e.g." chip when present · one-click adopt). */
const sugFor = (f: Field): string | undefined => {
  const v = props.suggestion?.[f.name];
  return v === undefined ? undefined : JSON.stringify(v);
};
const adopt = (i: number, f: Field) => {
  const v = props.suggestion?.[f.name];
  if (v !== undefined) patch(i, { default: v });
};

const defaultText = (f: Field) => (f.default === undefined ? "" : JSON.stringify(f.default));
const val = (e: Event) => (e.target as HTMLInputElement).value;
const checked = (e: Event) => (e.target as HTMLInputElement).checked;
</script>

<template>
  <div class="fields">
    <div v-for="(f, i) in fields" :key="i" class="field-block">
      <div class="field-row" :class="{ picked: sel.has(i) }" data-testid="field-row">
        <input
          type="checkbox"
          class="f-pick"
          title="Select for grouping"
          :aria-label="`select ${f.name} for grouping`"
          :checked="sel.has(i)"
          @change="toggleSel(i)"
        />
        <input
          class="f-name"
          :value="f.name"
          placeholder="name"
          aria-label="Field name"
          @change="patch(i, { name: val($event) })"
        />
        <div class="f-type">
          <select :value="f.type" aria-label="Type" @change="setType(i, val($event) as TypeNameStr)">
            <option v-for="t in FIELD_TYPES" :key="t" :value="t">{{ t }}</option>
          </select>
          <input
            v-if="f.type === 'enum'"
            class="sub"
            :value="(f.values ?? []).join(', ')"
            placeholder="a, b, c"
            aria-label="enum values (comma-separated)"
            @change="setEnum(i, val($event))"
          />
          <select
            v-if="f.type === 'array' || f.type === 'map'"
            class="sub"
            :value="typeof f.items === 'string' ? f.items : 'float'"
            aria-label="Element type"
            @change="patch(i, { items: val($event) as TypeNameStr })"
          >
            <option v-for="t in SCALARS" :key="t" :value="t">{{ t }}</option>
          </select>
        </div>
        <label class="f-req">
          <input
            type="checkbox"
            title="required (default on)"
            aria-label="required"
            :checked="f.required !== false"
            @change="patch(i, { required: checked($event) ? undefined : false })"
          />
          req
        </label>
        <div class="f-default default-cell">
          <input
            class="default"
            :value="defaultText(f)"
            placeholder="default (JSON)"
            title="default value (the core checks type consistency §5.1)"
            aria-label="default value (JSON)"
            :aria-invalid="defaultDiags[i].length > 0 ? 'true' : undefined"
            @change="setDefault(i, val($event))"
          />
          <!-- Type-mismatch NO: shows the core's {code, path, message} inline verbatim -->
          <div
            v-for="(d, di) in defaultDiags[i]"
            :key="di"
            class="field-diag"
            data-testid="field-default-diag"
            role="alert"
          >
            <span class="code">[{{ d.code }}]</span>
            <span class="path">@{{ d.path }}</span>
            {{ d.message }}
          </div>
          <!-- wasm_sample suggestion: show the value and adopt it with one click -->
          <a
            v-if="sugFor(f) !== undefined"
            class="adopt"
            role="button"
            tabindex="0"
            title="Adopt the sample value as the default (wasm_sample)"
            @click="adopt(i, f)"
            @keydown.enter.prevent="adopt(i, f)"
            @keydown.space.prevent="adopt(i, f)"
          >e.g. {{ sugFor(f) }}</a>
        </div>
        <div class="f-ops">
          <button
            v-if="f.type === 'group'"
            type="button"
            class="icon-btn ungrp"
            title="Ungroup (expand)"
            aria-label="Ungroup"
            @click="change(ungroupFieldAt(fields, [i]))"
          >⇤</button>
          <button
            type="button"
            class="icon-btn del"
            title="Delete field"
            aria-label="Delete field"
            @click="change(removeFieldAt(fields, [i]))"
          >✕</button>
        </div>
      </div>
      <!-- group: recursively render sub-fields (nesting). suggestion descends by the same key -->
      <div v-if="f.type === 'group'" class="nest">
        <FieldsTable
          :fields="f.fields ?? []"
          :suggestion="suggestion?.[f.name] as Record<string, unknown> | undefined"
          @change="(sub: Field[]) => patch(i, { fields: sub })"
        />
      </div>
    </div>
    <div class="row-btns">
      <button type="button" class="btn-ghost" data-testid="add-field" @click="change(addFieldAt(fields, []))">
        ＋ field
      </button>
      <button
        v-if="sel.size"
        type="button"
        class="btn-ghost grp"
        data-testid="group-fields"
        @click="change(groupFieldsAt(fields, [], [...sel]))"
      >
        ▸ Group selected ({{ sel.size }})
      </button>
    </div>
  </div>
</template>
