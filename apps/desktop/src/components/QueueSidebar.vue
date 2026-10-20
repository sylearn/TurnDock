<script setup lang="ts">
import { ref } from 'vue'
import { age, clientMark, statusOf } from '../lib/format'
import { doneList, openList, select, store, visibleList } from '../lib/store'
import StoneMark from './StoneMark.vue'

defineProps<{ settingsOpen: boolean }>()
const emit = defineEmits<{ settings: [] }>()
const search = ref<HTMLInputElement | null>(null)

defineExpose({ focusSearch: () => search.value?.focus() })
</script>

<template>
  <nav class="sidebar" aria-label="请求队列">
    <div class="top" data-tauri-drag-region>
      <div class="filters" role="tablist">
        <button
          type="button"
          role="tab"
          :aria-selected="store.filter === 'open'"
          @click="store.filter = 'open'"
        >
          待回复 <span class="n">{{ openList.length }}</span>
        </button>
        <button
          type="button"
          role="tab"
          :aria-selected="store.filter === 'done'"
          @click="store.filter = 'done'"
        >
          已处理 <span class="n">{{ doneList.length }}</span>
        </button>
      </div>
    </div>

    <label class="search">
      <span class="visually-hidden">搜索请求</span>
      <input
        ref="search"
        v-model="store.query"
        type="search"
        placeholder="搜索标题、项目、来源　/"
        @keydown.escape="(store.query = ''), search?.blur()"
      />
    </label>

    <ul class="list" role="listbox" :aria-label="store.filter === 'open' ? '待回复' : '已处理'">
      <li
        v-for="r in visibleList"
        :key="r.id"
        role="option"
        :aria-selected="r.id === store.selectedId && !settingsOpen"
        class="item"
        :class="{ current: r.id === store.selectedId && !settingsOpen, later: store.later.has(r.id) }"
        @click="select(r.id), settingsOpen && emit('settings')"
      >
        <StoneMark class="mark" :stone="statusOf(r).stone" />
        <span class="title">{{ r.title }}</span>
        <span class="age">{{ age(r.created_at, store.now) }}</span>
        <span class="meta">
          <span class="client">{{ clientMark(r.client_name).glyph }} {{ clientMark(r.client_name).label }}</span>
          <span v-if="r.project" class="project">{{ r.project }}</span>
        </span>
        <span v-if="r.phase === 'waiting' && r.has_draft" class="flag">有草稿</span>
        <span v-else-if="!(r.phase === 'waiting' && r.attached)" class="flag" :class="statusOf(r).tone">
          {{ statusOf(r).label }}
        </span>
      </li>
      <li v-if="store.ready && visibleList.length === 0" class="empty">
        {{
          store.query
            ? '没有匹配的请求。'
            : store.filter === 'open'
              ? '没有等你回复的请求。'
              : '最近 7 天没有已处理的请求。'
        }}
      </li>
    </ul>

    <div class="bottom">
      <button type="button" class="btn quiet" :aria-pressed="settingsOpen" @click="emit('settings')">
        设置与接入
      </button>
    </div>
  </nav>
</template>

<style scoped>
.sidebar {
  display: flex;
  flex-direction: column;
  min-height: 0;
  background: var(--stone);
  border-right: 1px solid var(--rule);
}
.top {
  flex: none;
  height: var(--bar);
  display: flex;
  align-items: center;
  justify-content: flex-end;
  padding: 0 12px 0 84px; /* 让出 macOS 窗口按钮 */
}
.filters {
  display: flex;
  gap: 2px;
  padding: 2px;
  border-radius: 8px;
  background: color-mix(in srgb, var(--ink) 6%, transparent);
}
.filters button {
  height: 26px;
  padding: 0 10px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  font-size: 12.5px;
  color: var(--ink-2);
}
.filters button[aria-selected='true'] {
  background: var(--sheet);
  color: var(--ink);
  font-weight: 600;
  box-shadow: 0 1px 2px #16191a1a;
}
.n {
  font-variant-numeric: tabular-nums;
  margin-left: 2px;
}
.search {
  flex: none;
  padding: 10px 12px 8px;
}
.search input {
  width: 100%;
  height: 30px;
  padding: 0 10px;
  border: 1px solid transparent;
  border-radius: 7px;
  background: color-mix(in srgb, var(--ink) 5%, transparent);
  font-size: 12.5px;
  outline: none;
  user-select: text;
}
.search input:focus {
  border-color: var(--signal);
  background: var(--sheet);
}
.list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  margin: 0;
  padding: 2px 8px 8px;
  list-style: none;
}
.item {
  display: grid;
  grid-template-columns: 12px minmax(0, 1fr) auto;
  grid-template-rows: 22px 18px;
  column-gap: 10px;
  align-items: center;
  height: 62px;
  padding: 10px 10px 10px 12px;
  border-radius: 8px;
}
.item:hover {
  background: color-mix(in srgb, var(--ink) 4%, transparent);
}
.item.current {
  background: var(--sheet);
  box-shadow: 0 0 0 1px var(--rule);
}
.item.later {
  opacity: 0.72;
}
.mark {
  grid-row: 1;
}
.title {
  grid-column: 2;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.age {
  grid-column: 3;
  font-size: 12px;
  color: var(--ink-2);
  font-variant-numeric: tabular-nums;
}
.meta {
  grid-column: 2;
  grid-row: 2;
  display: flex;
  gap: 8px;
  min-width: 0;
  font-size: 12px;
  line-height: 18px;
  color: var(--ink-2);
}
.client {
  white-space: nowrap;
}
.project {
  font-family: var(--font-code);
  font-size: 11.5px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.flag {
  grid-column: 3;
  grid-row: 2;
  justify-self: end;
  font-size: 12px;
  line-height: 18px;
  color: var(--ink-2);
}
.flag.warn {
  color: var(--warn);
}
.flag.danger {
  color: var(--danger);
}
.flag.signal {
  color: var(--signal-fg);
}
.empty {
  padding: 16px 12px;
  color: var(--ink-2);
  font-size: 13px;
}
.bottom {
  flex: none;
  padding: 8px 12px 12px;
  border-top: 1px solid var(--rule);
}
.bottom .btn {
  margin-left: -4px;
}
.bottom .btn[aria-pressed='true'] {
  color: var(--ink);
  font-weight: 600;
}
</style>
