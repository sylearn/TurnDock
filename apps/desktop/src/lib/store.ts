// 窗口内的视图状态。请求的生命周期只来自 Rust 快照；这里不推断、不修改状态。
import { computed, reactive } from 'vue'
import type { RequestDetail } from '../generated/RequestDetail'
import type { RequestSummary } from '../generated/RequestSummary'
import { api, errorText, onBrokerEvent, onPrefs, type Prefs } from './api'
import { isOpen } from './format'

export type Filter = 'open' | 'done'

export const store = reactive({
  ready: false,
  error: null as string | null,
  revision: 0,
  requests: [] as RequestSummary[],
  selectedId: null as string | null,
  detail: null as RequestDetail | null,
  filter: 'open' as Filter,
  query: '',
  prefs: null as Prefs | null,
  now: Date.now(),
  /** 稍后处理：只影响本窗口内的排序，不改变请求状态。 */
  later: new Set<string>(),
  toast: null as string | null,
})

let toastTimer: number | undefined

export function toast(text: string) {
  store.toast = text
  window.clearTimeout(toastTimer)
  toastTimer = window.setTimeout(() => (store.toast = null), 3200)
}

/** 待回复：最早的在前（先到先答）；稍后处理的排到最后。 */
export const openList = computed(() =>
  store.requests
    .filter(isOpen)
    .sort((a, b) => {
      const la = store.later.has(a.id) ? 1 : 0
      const lb = store.later.has(b.id) ? 1 : 0
      return la - lb || a.created_at - b.created_at
    }),
)

export const doneList = computed(() =>
  store.requests.filter((r) => !isOpen(r)).sort((a, b) => b.updated_at - a.updated_at),
)

export const visibleList = computed(() => {
  const list = store.filter === 'open' ? openList.value : doneList.value
  const q = store.query.trim().toLowerCase()
  if (!q) return list
  return list.filter((r) =>
    [r.title, r.client_name, r.project ?? ''].some((s) => s.toLowerCase().includes(q)),
  )
})

export const selected = computed(
  () => store.requests.find((r) => r.id === store.selectedId) ?? null,
)

let detailToken = 0

export async function loadDetail(id: string | null) {
  const token = ++detailToken
  if (!id) {
    store.detail = null
    return
  }
  try {
    const d = await api.detail(id)
    if (token === detailToken) store.detail = d
  } catch (e) {
    if (token === detailToken) store.error = errorText(e)
  }
}

export async function refresh() {
  try {
    const snap = await api.snapshot()
    store.revision = snap.revision
    store.requests = snap.requests
    store.error = null
    store.ready = true
    if (store.selectedId && !store.requests.some((r) => r.id === store.selectedId)) {
      store.selectedId = null
    }
    if (!store.selectedId) {
      store.selectedId = openList.value[0]?.id ?? null
    }
    await loadDetail(store.selectedId)
  } catch (e) {
    store.error = errorText(e)
  }
}

export function select(id: string | null) {
  if (id === store.selectedId) return
  store.selectedId = id
  void loadDetail(id)
}

/** 在当前可见列表中移动选择。 */
export function move(step: 1 | -1) {
  const list = visibleList.value
  if (list.length === 0) return
  const i = list.findIndex((r) => r.id === store.selectedId)
  const next = list[Math.min(list.length - 1, Math.max(0, i + step))]
  if (next) select(next.id)
}

/** 下一条待回复（跳过当前）。 */
export function nextOpenAfter(id: string): string | null {
  const list = openList.value.filter((r) => r.id !== id)
  return list[0]?.id ?? null
}

export function applyTheme(theme: Prefs['theme']) {
  const root = document.documentElement
  if (theme === 'system') delete root.dataset.theme
  else root.dataset.theme = theme
}

let started = false

export async function start() {
  if (started) return
  started = true
  let timer: number | undefined
  await onBrokerEvent(() => {
    // 合并短时间内的多个事件；丢事件也没关系，总是重新拉取快照。
    window.clearTimeout(timer)
    timer = window.setTimeout(() => void refresh(), 30)
  })
  await onPrefs((p) => {
    store.prefs = p
    applyTheme(p.theme)
  })
  window.setInterval(() => (store.now = Date.now()), 30_000)
  try {
    store.prefs = await api.getPrefs()
    applyTheme(store.prefs.theme)
  } catch (e) {
    store.error = errorText(e)
  }
  await refresh()
}
