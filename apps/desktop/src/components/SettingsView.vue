<script setup lang="ts">
// 设置：接入（三步）/ 偏好 / 数据。接入只生成配置文本，不修改任何外部文件。
import { computed, onMounted, reactive, ref, toRaw, watch } from 'vue'
import { api, errorText, type Integration, type Prefs } from '../lib/api'
import { store, toast } from '../lib/store'

type Tab = 'connect' | 'prefs' | 'data'
const tab = ref<Tab>('connect')
const integration = ref<Integration | null>(null)
const host = ref(0)
const error = ref<string | null>(null)
const ready = ref(false)

const form = reactive<Prefs>({
  quick_replies: [],
  shortcut: '',
  notify: true,
  popup: false,
  theme: 'system',
})
const editingShortcut = ref(false)
const shortcutDraft = ref('')

const snippet = computed(() => integration.value?.snippets[host.value] ?? null)

onMounted(async () => {
  if (store.prefs) {
    const p = toRaw(store.prefs)
    Object.assign(form, { ...p, quick_replies: [...p.quick_replies] })
  }
  ready.value = true
  try {
    integration.value = await api.integration()
  } catch (e) {
    error.value = errorText(e)
  }
})

// 偏好即改即存（去抖），失败时就地说明原因。
let timer: number | undefined
watch(
  () => JSON.stringify(form),
  () => {
    if (!ready.value) return
    window.clearTimeout(timer)
    timer = window.setTimeout(save, 400)
  },
)

async function save() {
  error.value = null
  try {
    const saved = await api.setPrefs({ ...toRaw(form), quick_replies: [...form.quick_replies] })
    store.prefs = saved
  } catch (e) {
    error.value = errorText(e)
  }
}

async function copy(text: string, what: string) {
  try {
    await navigator.clipboard.writeText(text)
    toast(`已复制${what}`)
  } catch {
    error.value = '复制失败'
  }
}

const KEY_NAMES: Record<string, string> = {
  control: '⌃',
  ctrl: '⌃',
  alt: '⌥',
  option: '⌥',
  shift: '⇧',
  command: '⌘',
  cmd: '⌘',
  super: '⌘',
  commandorcontrol: '⌘',
  cmdorctrl: '⌘',
}

function keycaps(shortcut: string): string[] {
  return shortcut
    .split('+')
    .map((k) => k.trim())
    .filter(Boolean)
    .map((k) => KEY_NAMES[k.toLowerCase()] ?? (k.length === 1 ? k.toUpperCase() : k))
}

function startShortcut() {
  shortcutDraft.value = form.shortcut
  editingShortcut.value = true
}

function commitShortcut() {
  form.shortcut = shortcutDraft.value.trim()
  editingShortcut.value = false
}

function addReply() {
  if (form.quick_replies.length < 9) form.quick_replies.push('')
}

function removeReply(i: number) {
  form.quick_replies.splice(i, 1)
}
</script>

<template>
  <section class="settings">
    <header class="bar" data-tauri-drag-region>
      <h1 data-tauri-drag-region>设置</h1>
      <div class="tabs" role="tablist">
        <button
          v-for="t in ([['connect', '接入'], ['prefs', '偏好'], ['data', '数据']] as const)"
          :key="t[0]"
          type="button"
          role="tab"
          :aria-selected="tab === t[0]"
          @click="tab = t[0]"
        >
          {{ t[1] }}
        </button>
      </div>
    </header>

    <div class="scroll">
      <div class="column">
        <p v-if="error" class="error" role="alert">{{ error }}</p>

        <!-- 接入 -->
        <template v-if="tab === 'connect'">
          <div class="status" :class="{ bad: integration && !integration.bridge_found }">
            <span class="live" aria-hidden="true" />
            <span class="status-text">
              {{ !integration ? '正在检查…' : integration.bridge_found ? 'turndock-mcp 已就绪' : '没有找到 turndock-mcp' }}
            </span>
            <code class="path" :title="integration?.bridge_path">{{ integration?.bridge_path }}</code>
            <button
              v-if="integration"
              type="button"
              class="btn quiet small"
              @click="copy(integration.bridge_path, '路径')"
            >
              复制路径
            </button>
          </div>
          <p v-if="integration && !integration.bridge_found" class="hint warn">
            开发环境请先运行 <code>cargo build -p turndock-bridge</code>。
          </p>

          <ol class="steps">
            <li>
              <h2>添加到你的 AI 客户端</h2>
              <div class="segmented" role="tablist" aria-label="AI 客户端">
                <button
                  v-for="(s, i) in integration?.snippets ?? []"
                  :key="s.host"
                  type="button"
                  role="tab"
                  :aria-selected="host === i"
                  @click="host = i"
                >
                  {{ s.host }}
                </button>
              </div>
              <div v-if="snippet" class="code-card">
                <div class="code-head">
                  <code class="file">{{ snippet.file }}</code>
                  <button type="button" class="btn quiet small" @click="copy(snippet.content, '配置')">复制</button>
                </div>
                <!-- html 由 Rust 渲染并净化 -->
                <div class="prose code-body" v-html="snippet.html" />
              </div>
              <p v-if="snippet" class="hint">{{ snippet.note }}</p>
            </li>

            <li>
              <h2>告诉 AI 何时交接</h2>
              <p class="hint">
                加到项目的 AGENTS.md、CLAUDE.md 或客户端的规则设置里。AI 会在每轮工作结束时交给你一张回合卡。
              </p>
              <div class="code-card">
                <div class="code-head">
                  <code class="file">AGENTS.md / CLAUDE.md</code>
                  <button
                    type="button"
                    class="btn quiet small"
                    @click="copy(integration?.rules ?? '', '规则')"
                  >
                    复制
                  </button>
                </div>
                <div class="prose rules" v-html="integration?.rules_html ?? ''" />
              </div>
            </li>

            <li>
              <h2>试一下</h2>
              <p class="hint">让 AI 做一件小事，比如修一个拼写错误。它结束时，回合卡会出现在左侧队列里。</p>
            </li>
          </ol>
        </template>

        <!-- 偏好 -->
        <template v-else-if="tab === 'prefs'">
          <h2 class="group">提醒</h2>
          <div class="panel">
            <div class="row">
              <div class="row-text">
                <span class="row-title">新请求时通知</span>
                <span class="row-desc">回合窗口在前台时不通知</span>
              </div>
              <button
                type="button"
                role="switch"
                class="switch"
                :aria-checked="form.notify"
                aria-label="新请求时通知"
                @click="form.notify = !form.notify"
              />
            </div>
            <div class="row">
              <div class="row-text">
                <span class="row-title">新请求时弹出快速面板</span>
                <span class="row-desc">适合习惯寸止弹窗的用法</span>
              </div>
              <button
                type="button"
                role="switch"
                class="switch"
                :aria-checked="form.popup"
                aria-label="新请求时弹出快速面板"
                @click="form.popup = !form.popup"
              />
            </div>
            <div class="row">
              <div class="row-text">
                <span class="row-title">快速面板快捷键</span>
                <span class="row-desc">在任何应用里唤出或收起</span>
              </div>
              <form v-if="editingShortcut" class="shortcut-edit" @submit.prevent="commitShortcut">
                <input
                  v-model="shortcutDraft"
                  class="text mono"
                  placeholder="Control+Alt+Space"
                  aria-label="快捷键"
                  @keydown.escape="editingShortcut = false"
                  @blur="commitShortcut"
                />
              </form>
              <button v-else type="button" class="keys" title="点击修改" @click="startShortcut">
                <template v-if="form.shortcut">
                  <kbd v-for="(k, i) in keycaps(form.shortcut)" :key="i">{{ k }}</kbd>
                </template>
                <span v-else class="off">未设置</span>
              </button>
            </div>
          </div>

          <h2 class="group">外观</h2>
          <div class="panel">
            <div class="row">
              <span class="row-title">主题</span>
              <div class="segmented compact" role="radiogroup" aria-label="主题">
                <button
                  v-for="t in ([['system', '跟随系统'], ['light', '浅色'], ['dark', '深色']] as const)"
                  :key="t[0]"
                  type="button"
                  role="radio"
                  :aria-checked="form.theme === t[0]"
                  @click="form.theme = t[0]"
                >
                  {{ t[1] }}
                </button>
              </div>
            </div>
          </div>

          <h2 class="group">快捷回复</h2>
          <p class="hint">用于提问类请求，按 ⌥1–⌥9 发送。发给 AI 的就是这里写的文字。交接中推荐项已预选，直接按 ⌘↵。</p>
          <div class="panel">
            <div v-for="(_, i) in form.quick_replies" :key="i" class="row reply">
              <kbd>⌥{{ i + 1 }}</kbd>
              <input v-model="form.quick_replies[i]" class="text" :aria-label="`快捷回复 ${i + 1}`" />
              <button type="button" class="btn quiet small danger" :aria-label="`删除快捷回复 ${i + 1}`" @click="removeReply(i)">
                删除
              </button>
            </div>
            <div class="row">
              <button type="button" class="btn quiet small" :disabled="form.quick_replies.length >= 9" @click="addReply">
                添加一条
              </button>
            </div>
          </div>
        </template>

        <!-- 数据 -->
        <template v-else>
          <h2 class="group">保存在哪里</h2>
          <div class="panel">
            <div class="row">
              <div class="row-text">
                <span class="row-title">只在本机</span>
                <span class="row-desc">没有账号、云同步或使用统计；回合不调用任何模型</span>
              </div>
            </div>
            <div class="row">
              <div class="row-text">
                <span class="row-title">保留时长</span>
                <span class="row-desc">已结束的请求保留 7 天后自动删除；还在等待的不会被清理</span>
              </div>
            </div>
            <div class="row">
              <div class="row-text">
                <span class="row-title">日志</span>
                <span class="row-desc">只记录编号、状态与耗时，不记录内容和图片</span>
              </div>
            </div>
          </div>
        </template>
      </div>
    </div>
  </section>
</template>

<style scoped>
.settings {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--sheet);
}
.bar {
  flex: none;
  display: flex;
  align-items: center;
  gap: 20px;
  height: var(--bar);
  padding: 0 40px;
  border-bottom: 1px solid var(--rule);
}
.bar h1 {
  margin: 0;
  font-size: 13px;
  font-weight: 650;
}
.tabs {
  display: flex;
  gap: 2px;
}
.tabs button {
  height: 28px;
  padding: 0 12px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--ink-2);
  font-size: 13px;
}
.tabs button:hover {
  color: var(--ink);
}
.tabs button[aria-selected='true'] {
  background: var(--signal-wash);
  color: var(--signal-fg);
  font-weight: 600;
}
.scroll {
  flex: 1;
  overflow-y: auto;
  padding: 24px 0 48px;
}
.column {
  max-width: var(--measure);
  padding: 0 40px;
}

/* 状态条 */
.status {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 40px;
  padding: 0 6px 0 14px;
  border: 1px solid var(--rule);
  border-radius: 10px;
  background: var(--stone);
  font-size: 13px;
}
.live {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--ok);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--ok) 22%, transparent);
  flex: none;
}
.status.bad .live {
  background: var(--warn);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--warn) 22%, transparent);
}
.status-text {
  white-space: nowrap;
  font-weight: 500;
}
.path {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-code);
  font-size: 11.5px;
  color: var(--ink-2);
}

/* 步骤：这是真实的先后顺序，所以用编号 */
.steps {
  margin: 28px 0 0;
  padding: 0;
  list-style: none;
  counter-reset: step;
}
.steps > li {
  position: relative;
  padding: 0 0 28px 40px;
  counter-increment: step;
}
.steps > li::before {
  content: counter(step);
  position: absolute;
  left: 0;
  top: 0;
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  border-radius: 50%;
  border: 1px solid var(--signal);
  color: var(--signal-fg);
  font-size: 12px;
  font-weight: 650;
  font-variant-numeric: tabular-nums;
}
.steps > li:not(:last-child)::after {
  content: '';
  position: absolute;
  left: 12px;
  top: 30px;
  bottom: 6px;
  width: 1px;
  background: var(--rule);
}
.steps h2 {
  margin: 1px 0 10px;
  font-size: 15px;
  line-height: 22px;
  font-weight: 650;
}
.hint {
  margin: 8px 0 0;
  font-size: 12.5px;
  line-height: 20px;
  color: var(--ink-2);
}
.hint.warn {
  color: var(--warn);
}
.hint code {
  font-family: var(--font-code);
  font-size: 12px;
}

.segmented {
  display: inline-flex;
  gap: 2px;
  padding: 3px;
  margin-bottom: 10px;
  border-radius: 9px;
  background: var(--stone);
  border: 1px solid var(--rule);
}
.segmented.compact {
  margin: 0;
}
.segmented button {
  height: 26px;
  padding: 0 12px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--ink-2);
  font-size: 12.5px;
}
.segmented button[aria-selected='true'],
.segmented button[aria-checked='true'] {
  background: var(--sheet);
  color: var(--ink);
  font-weight: 600;
  box-shadow: 0 1px 2px #0000002e, 0 0 0 1px var(--rule);
}

.code-card {
  border: 1px solid var(--rule);
  border-radius: 10px;
  background: var(--code-bg);
  overflow: hidden;
}
.code-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 34px;
  padding: 0 6px 0 14px;
  border-bottom: 1px solid var(--rule);
}
.file {
  font-family: var(--font-code);
  font-size: 11.5px;
  color: var(--ink-2);
}
.code-body :deep(pre),
.code-body :deep(pre[data-lang]) {
  margin: 0;
  padding: 12px 14px;
  border-radius: 0;
  max-height: none;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
.code-body :deep(pre[data-lang])::before,
.code-body :deep(.copy-code) {
  display: none;
}
.rules {
  padding: 12px 16px 14px;
  font-size: 13px;
  line-height: 22px;
}
.rules :deep(ul) {
  list-style: disc;
}
.rules :deep(ul ul) {
  list-style: circle;
}
.rules :deep(h2) {
  margin: 0 0 6px;
  font-size: 13px;
}

/* 偏好：左标签、右控件 */
.group {
  margin: 28px 0 8px;
  font-size: 12px;
  font-weight: 600;
  color: var(--ink-2);
}
.group:first-child {
  margin-top: 0;
}
.panel {
  border: 1px solid var(--rule);
  border-radius: 10px;
  background: var(--stone);
}
.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  min-height: 52px;
  padding: 8px 14px;
}
.row + .row {
  border-top: 1px solid var(--rule);
}
.row-text {
  display: grid;
  gap: 1px;
}
.row-title {
  font-size: 13.5px;
}
.row-desc {
  font-size: 12px;
  line-height: 18px;
  color: var(--ink-2);
}

.switch {
  position: relative;
  flex: none;
  width: 36px;
  height: 22px;
  padding: 0;
  border: 0;
  border-radius: 11px;
  background: var(--edge);
  transition: background-color 140ms var(--ease);
}
.switch::after {
  content: '';
  position: absolute;
  top: 3px;
  left: 3px;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: #fff;
  box-shadow: 0 1px 2px #0000004d;
  transition: transform 140ms var(--ease);
}
.switch[aria-checked='true'] {
  background: var(--signal);
}
.switch[aria-checked='true']::after {
  transform: translateX(14px);
}

.keys {
  display: inline-flex;
  gap: 4px;
  padding: 4px;
  border: 0;
  border-radius: 7px;
  background: transparent;
}
.keys:hover {
  background: var(--sheet);
}
kbd {
  display: inline-grid;
  place-items: center;
  min-width: 24px;
  height: 24px;
  padding: 0 6px;
  border: 1px solid var(--edge);
  border-bottom-width: 2px;
  border-radius: 6px;
  background: var(--sheet);
  font-family: var(--font-ui);
  font-size: 12px;
  color: var(--ink);
}
.off {
  font-size: 12.5px;
  color: var(--ink-2);
}
.text {
  height: 30px;
  padding: 0 10px;
  border: 1px solid var(--edge);
  border-radius: 7px;
  background: var(--sheet);
  font-size: 13px;
  outline: none;
  user-select: text;
}
.text:focus {
  border-color: var(--signal);
  box-shadow: 0 0 0 1px var(--signal);
}
.mono {
  font-family: var(--font-code);
  font-size: 12px;
  width: 200px;
}
.reply {
  justify-content: flex-start;
}
.reply .text {
  flex: 1;
  min-width: 0;
}
.btn.small {
  min-height: 26px;
  padding: 0 10px;
  font-size: 12px;
}
.error {
  margin: 0 0 16px;
  color: var(--danger);
}
</style>
