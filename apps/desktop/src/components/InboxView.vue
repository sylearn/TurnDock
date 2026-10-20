<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { api, onSelect } from '../lib/api'
import { isField, resolveKey } from '../lib/keys'
import { move, nextOpenAfter, select, selected, store } from '../lib/store'
import QueueSidebar from './QueueSidebar.vue'
import RequestPane from './RequestPane.vue'
import SettingsView from './SettingsView.vue'

const pane = ref<InstanceType<typeof RequestPane> | null>(null)
const sidebar = ref<InstanceType<typeof QueueSidebar> | null>(null)
const settingsOpen = ref(false)

function onDone(id: string) {
  const next = nextOpenAfter(id)
  if (next) select(next)
}

function onLater(id: string) {
  store.later.add(id)
  const next = nextOpenAfter(id)
  if (next) select(next)
}

function onKey(e: KeyboardEvent) {
  if (document.querySelector('dialog[open]')) return
  const action = resolveKey(e, isField(e.target))
  if (!action) return
  if (settingsOpen.value && action.kind !== 'escape' && action.kind !== 'search') return
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
    case 'next':
      move(1)
      break
    case 'prev':
      move(-1)
      break
    case 'search':
      sidebar.value?.focusSearch()
      break
    case 'toggleView':
      void api.showQuick()
      break
    case 'escape':
      if (settingsOpen.value) settingsOpen.value = false
      else (document.activeElement as HTMLElement | null)?.blur()
      break
  }
}

let unlisten: (() => void) | undefined
onMounted(async () => {
  window.addEventListener('keydown', onKey)
  unlisten = await onSelect((id) => {
    if (id) {
      settingsOpen.value = false
      store.filter = 'open'
      select(id)
    }
  })
})
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKey)
  unlisten?.()
})
</script>

<template>
  <div class="inbox">
    <QueueSidebar ref="sidebar" :settings-open="settingsOpen" @settings="settingsOpen = !settingsOpen" />
    <main class="main">
      <SettingsView v-if="settingsOpen" />
      <RequestPane
        v-else-if="store.detail && selected"
        ref="pane"
        :detail="store.detail"
        @done="onDone"
        @later="onLater"
      />
      <section v-else class="blank">
        <header class="bar" data-tauri-drag-region />
        <div class="blank-body">
          <svg width="40" height="40" viewBox="0 0 40 40" aria-hidden="true" class="stones">
            <circle cx="26" cy="14" r="9" class="hollow" />
            <circle cx="14" cy="26" r="9" class="solid" />
          </svg>
          <h1>{{ store.error ? '连接协调器失败' : '现在没有轮到你' }}</h1>
          <p v-if="store.error">{{ store.error }}</p>
          <p v-else>AI 需要你决定时，问题会出现在这里。</p>
          <button v-if="!store.error" type="button" class="btn" @click="settingsOpen = true">查看接入方式</button>
        </div>
      </section>
    </main>
  </div>
</template>

<style scoped>
.inbox {
  display: grid;
  grid-template-columns: 288px minmax(0, 1fr);
  height: 100%;
}
.main {
  min-width: 0;
  min-height: 0;
}
.blank {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--sheet);
}
.bar {
  flex: none;
  height: var(--bar);
  border-bottom: 1px solid var(--rule);
}
.blank-body {
  margin: auto 0;
  padding: 0 56px 10vh;
  max-width: 520px;
}
.stones {
  margin-bottom: 16px;
}
.solid {
  fill: var(--ink);
}
.hollow {
  fill: none;
  stroke: var(--signal);
  stroke-width: 2.5;
}
.blank-body h1 {
  margin: 0 0 6px;
  font-family: var(--font-note);
  font-size: 22px;
  line-height: 32px;
  font-weight: 600;
}
.blank-body p {
  margin: 0 0 16px;
  color: var(--ink-2);
}
</style>
