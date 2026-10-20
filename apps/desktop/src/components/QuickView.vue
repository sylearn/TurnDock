<script setup lang="ts">
// 快速回复面板：只处理最早一条待回复（场景 A）。答完最后一条后自动收起。
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { api, onSelect } from '../lib/api'
import { isField, resolveKey } from '../lib/keys'
import { nextOpenAfter, openList, select, store } from '../lib/store'
import RequestPane from './RequestPane.vue'

const pane = ref<InstanceType<typeof RequestPane> | null>(null)
const current = computed(() =>
  store.detail && store.detail.summary.id === store.selectedId ? store.detail : null,
)
const remaining = computed(() =>
  Math.max(0, openList.value.filter((r) => r.id !== store.selectedId).length),
)

function showFirst() {
  select(openList.value[0]?.id ?? null)
}

function onDone(id: string) {
  const next = nextOpenAfter(id)
  if (next) {
    select(next)
  } else {
    window.setTimeout(() => void api.hideQuick(), 700)
  }
}

function onLater(id: string) {
  store.later.add(id)
  const next = nextOpenAfter(id)
  if (next) select(next)
  else void api.hideQuick()
}

function expand(id: string | null) {
  void api.showInbox(id)
  void api.hideQuick()
}

function onKey(e: KeyboardEvent) {
  if (document.querySelector('dialog[open]')) return
  const action = resolveKey(e, isField(e.target))
  if (!action) return
  e.preventDefault()
  switch (action.kind) {
    case 'send':
      pane.value?.send()
      break
    case 'quick':
      pane.value?.sendQuick(action.index)
      break
    case 'option':
      pane.value?.pickOption(action.index)
      break
    case 'later':
      pane.value?.later()
      break
    case 'decline':
      pane.value?.decline()
      break
    case 'toggleView':
      expand(store.selectedId)
      break
    case 'escape':
      void api.hideQuick()
      break
    default:
      break
  }
}

let unlisten: (() => void) | undefined
onMounted(async () => {
  window.addEventListener('keydown', onKey)
  unlisten = await onSelect(() => {
    // 每次唤出都回到最早的一条，并把焦点放到输入框。
    store.later.clear()
    showFirst()
    window.setTimeout(() => pane.value?.focusInput(), 50)
  })
})
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKey)
  unlisten?.()
})
</script>

<template>
  <div class="quick">
    <RequestPane
      v-if="current && current.summary.phase === 'waiting'"
      ref="pane"
      :detail="current"
      :remaining="remaining"
      compact
      @done="onDone"
      @later="onLater"
      @expand="expand"
    />
    <section v-else class="empty">
      <header class="bar" data-tauri-drag-region />
      <div class="body">
        <h1>现在没有轮到你</h1>
        <p>按 Esc 收起。</p>
        <button type="button" class="btn" @click="expand(null)">打开收件箱 <span class="kbd">⌘E</span></button>
      </div>
    </section>
  </div>
</template>

<style scoped>
.quick {
  height: 100%;
}
.empty {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--sheet);
}
.bar {
  flex: none;
  height: var(--bar);
}
.body {
  margin: auto 0;
  padding: 0 32px 12vh;
}
h1 {
  margin: 0 0 4px;
  font-family: var(--font-display);
  font-size: 18px;
  line-height: 26px;
  font-weight: 600;
}
p {
  margin: 0 0 14px;
  color: var(--ink-2);
}
</style>
