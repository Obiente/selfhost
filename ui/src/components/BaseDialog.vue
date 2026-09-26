<script setup lang="ts">
import {
  DialogRoot,
  DialogPortal,
  DialogOverlay,
  DialogContent,
  DialogTitle,
  DialogDescription,
  DialogClose,
} from 'reka-ui';
import { X } from 'lucide-vue-next';
withDefaults(
  defineProps<{
    open: boolean;
    title: string;
    description?: string;
    busy?: boolean;
    wide?: boolean;
  }>(),
  { busy: false, wide: false },
);
const emit = defineEmits<{ 'update:open': [value: boolean] }>();
</script>
<template>
  <DialogRoot
    :open="open"
    @update:open="
      (value) => {
        if (!busy) emit('update:open', value);
      }
    "
  >
    <DialogPortal>
      <DialogOverlay class="dialog-overlay" />
      <DialogContent
        class="base-dialog"
        :class="{ 'base-dialog-wide': wide }"
        :aria-busy="busy"
        @escape-key-down="
          (event) => {
            if (busy) event.preventDefault();
          }
        "
        @interact-outside="
          (event) => {
            if (busy) event.preventDefault();
          }
        "
      >
        <div class="modal-heading">
          <DialogTitle class="dialog-title">{{ title }}</DialogTitle
          ><DialogClose class="icon-button" :disabled="busy" aria-label="Close dialog"
            ><X :size="20"
          /></DialogClose>
        </div>
        <DialogDescription v-if="description" class="dialog-description">{{
          description
        }}</DialogDescription>
        <DialogDescription v-else class="sr-only">{{ title }}</DialogDescription>
        <slot />
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>
<style>
.dialog-overlay {
  position: fixed;
  inset: 0;
  background: #101510b3;
  backdrop-filter: blur(4px);
  z-index: 90;
}
.base-dialog {
  position: fixed;
  z-index: 91;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  width: min(600px, calc(100vw - 32px));
  max-height: calc(100dvh - 32px);
  overflow: auto;
  overscroll-behavior: contain;
  padding: 28px;
  background: #262c27;
  color: #f1f0e9;
  border: 1px solid #535b4e;
  border-radius: 16px;
  box-shadow: 0 24px 80px #0007;
}
.base-dialog-wide {
  width: min(940px, calc(100vw - 32px));
}
.dialog-title {
  font-family: 'Manrope Variable', sans-serif;
  font-size: 22px;
  line-height: 1.3;
  letter-spacing: -0.5px;
}
.dialog-description {
  color: var(--muted);
  font-size: 14px;
  line-height: 1.6;
  margin: 0 0 22px;
}
.base-dialog .modal-heading {
  gap: 16px;
  margin-bottom: 12px;
}
.base-dialog .modal-heading > .icon-button {
  margin-left: auto;
  flex-shrink: 0;
}
.base-dialog form > label {
  display: block;
  margin: 16px 0;
}
.base-dialog form > label input:not([type='checkbox']),
.base-dialog form > label select {
  display: block;
  width: 100%;
  margin-top: 7px;
}
.base-dialog .app-choice {
  display: flex;
}
.base-dialog:focus {
  outline: none;
}
@media (max-width: 640px) {
  .base-dialog {
    padding: 20px;
  }
}
</style>
