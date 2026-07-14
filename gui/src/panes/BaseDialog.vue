<script setup lang="ts">
// The shared frame for add-style modals (UX redesign): role=dialog + focus trap + close on Esc /
// backdrop click. Each dialog handles its own validation and confirmation — this is just the frame
// (accessibility and the close affordance).
import { onMounted, ref, useId } from "vue";

defineProps<{ title: string }>();
const emit = defineEmits<{ close: [] }>();

const titleId = useId();
const body = ref<HTMLElement | null>(null);

const focusables = (): HTMLElement[] =>
  Array.from(
    body.value?.querySelectorAll<HTMLElement>(
      "button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href]",
    ) ?? [],
  );

// Initial focus = data-autofocus (or the first focusable element if absent)
onMounted(() => {
  (body.value?.querySelector<HTMLElement>("[data-autofocus]") ?? focusables()[0])?.focus();
});

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") {
    e.stopPropagation();
    emit("close");
    return;
  }
  if (e.key !== "Tab") return; // Tab cycles within the modal (focus trap)
  const els = focusables();
  if (els.length === 0) return;
  const first = els[0];
  const last = els[els.length - 1];
  if (e.shiftKey && document.activeElement === first) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && document.activeElement === last) {
    e.preventDefault();
    first.focus();
  }
}
</script>

<template>
  <div class="modal-backdrop" @click.self="emit('close')" @keydown="onKeydown">
    <div ref="body" class="modal" role="dialog" aria-modal="true" :aria-labelledby="titleId">
      <h3 :id="titleId">{{ title }}</h3>
      <slot />
    </div>
  </div>
</template>
