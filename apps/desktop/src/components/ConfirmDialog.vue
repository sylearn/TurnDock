<script setup lang="ts">
// 原生 <dialog>：焦点限制、Esc 关闭与焦点还原由浏览器处理。
import { nextTick, ref } from 'vue'

const props = defineProps<{
  title: string
  body: string
  confirmLabel: string
  danger?: boolean
  withReason?: boolean
}>()
const emit = defineEmits<{ confirm: [reason: string] }>()

const dialog = ref<HTMLDialogElement | null>(null)
const reason = ref('')
const confirmBtn = ref<HTMLButtonElement | null>(null)
const reasonBox = ref<HTMLTextAreaElement | null>(null)

async function open() {
  reason.value = ''
  dialog.value?.showModal()
  await nextTick()
  ;(props.withReason ? reasonBox.value : confirmBtn.value)?.focus()
}

function close() {
  dialog.value?.close()
}

function confirm() {
  emit('confirm', reason.value.trim())
  close()
}

function onKey(e: KeyboardEvent) {
  if (e.isComposing || e.keyCode === 229) return
  if (e.key === 'Enter' && (e.metaKey || e.ctrlKey || !props.withReason)) {
    e.preventDefault()
    confirm()
  }
}

defineExpose({ open, close })
</script>

<template>
  <dialog ref="dialog" class="confirm" @keydown="onKey" @click.self="close">
    <h2>{{ title }}</h2>
    <p>{{ body }}</p>
    <label v-if="withReason" class="reason">
      <span>理由（可选，会一并告诉 AI）</span>
      <textarea ref="reasonBox" v-model="reason" rows="3" />
    </label>
    <div class="actions">
      <button type="button" class="btn quiet" @click="close">取消</button>
      <button
        ref="confirmBtn"
        type="button"
        class="btn"
        :class="danger ? 'danger-solid' : 'primary'"
        @click="confirm"
      >
        {{ confirmLabel }}
      </button>
    </div>
  </dialog>
</template>

<style scoped>
.confirm {
  width: min(420px, calc(100vw - 48px));
  padding: 24px;
  border: 1px solid var(--rule);
  border-radius: 12px;
  background: var(--sheet);
  color: var(--ink);
  box-shadow: var(--shade);
}
.confirm::backdrop {
  background: #16191a40;
}
h2 {
  margin: 0 0 8px;
  font-size: 16px;
  line-height: 24px;
  font-weight: 650;
}
p {
  margin: 0;
  color: var(--ink-2);
}
.reason {
  display: grid;
  gap: 6px;
  margin-top: 16px;
  font-size: 12px;
  color: var(--ink-2);
}
.reason textarea {
  resize: vertical;
  padding: 8px 10px;
  border: 1px solid var(--edge);
  border-radius: 8px;
  background: var(--sheet);
  color: var(--ink);
  font-size: 14px;
  line-height: 22px;
  user-select: text;
}
.actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 20px;
}
.danger-solid {
  background: var(--danger);
  border-color: var(--danger);
  color: var(--sheet);
  font-weight: 600;
}
</style>
