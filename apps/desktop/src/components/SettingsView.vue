<script setup lang="ts">
// 设置：接入 / 偏好 / 关于。接入命令对所有人都一样；只有点击「接入」时才会修改本机配置。
import { computed, onMounted, onUnmounted, reactive, ref, toRaw, watch } from 'vue'
import {
  api,
  errorText,
  onUpdateProgress,
  type Host,
  type Prefs,
  type Setup,
  type UpdateProgress,
} from '../lib/api'
import { store, toast } from '../lib/store'

const INSTALL_LINE = 'curl -fsSL https://github.com/sylearn/TurnDock/releases/latest/download/install.sh | bash'

type Tab = 'connect' | 'prefs' | 'about'
const tab = ref<Tab>('connect')
const setup = ref<Setup | null>(null)
const error = ref<string | null>(null)
const ready = ref(false)
/** 正在执行的操作：某个客户端的接入或更新。 */
const busy = ref<string | null>(null)

const form = reactive<Prefs>({
  quick_replies: [],
  shortcut: '',
  notify: true,
  popup: false,
  theme: 'system',
  check_updates: true,
})
const editingShortcut = ref(false)
const shortcutDraft = ref('')
const rulesOpen = ref(false)


async function loadSetup() {
  try {
    setup.value = await api.setupStatus()
  } catch (e) {
    error.value = errorText(e)
  }
}

let stopProgress: (() => void) | undefined
onMounted(async () => {
  if (store.prefs) {
    const p = toRaw(store.prefs)
    Object.assign(form, { ...p, quick_replies: [...p.quick_replies] })
  }
  ready.value = true
  stopProgress = await onUpdateProgress((p) => (progress.value = p))
  await loadSetup()
})
onUnmounted(() => stopProgress?.())

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

function hostState(h: Host): { text: string; tone: 'ok' | 'idle' | 'off' } | null {
  if (h.connected) return { text: h.id === 'json' ? 'Cursor 已接入' : '已接入', tone: 'ok' }
  if (h.installed === false) return { text: '本机未安装', tone: 'off' }
  if (h.connected === false && h.id !== 'json') return { text: '未接入', tone: 'idle' }
  return null
}

function actionLabel(h: Host): string | null {
  if (h.connected || h.installed === false) return null
  if (h.action === 'run') return '接入'
  if (h.action === 'open') return '在 Cursor 中接入'
  return null
}

async function connect(h: Host) {
  busy.value = h.id
  error.value = null
  try {
    await api.connectHost(h.id)
    if (h.action === 'open') {
      toast('已打开 Cursor，在它的窗口里确认')
    } else {
      toast(`已接入 ${h.name}，新开的会话即可使用`)
    }
    await loadSetup()
  } catch (e) {
    error.value = errorText(e)
  } finally {
    busy.value = null
  }
}

// ---------- 更新 ----------
const checking = ref(false)
const checked = ref(false)
const progress = ref<UpdateProgress | null>(null)
const percent = computed(() => {
  const p = progress.value
  return p?.total ? Math.min(100, Math.round((p.downloaded / p.total) * 100)) : null
})

async function checkUpdate() {
  checking.value = true
  error.value = null
  try {
    store.update = await api.checkUpdate()
    checked.value = true
  } catch (e) {
    error.value = errorText(e)
  } finally {
    checking.value = false
  }
}

async function installUpdate() {
  busy.value = 'update'
  error.value = null
  progress.value = { downloaded: 0, total: null }
  try {
    // 成功时应用会重启，这里不会返回。
    await api.installUpdate()
  } catch (e) {
    error.value = errorText(e)
    busy.value = null
    progress.value = null
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
          v-for="t in ([['connect', '接入'], ['prefs', '偏好'], ['about', '关于']] as const)"
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
          <ol class="steps">
            <li>
              <h2>接入你的 AI 客户端</h2>
              <p class="hint">点「接入」由回合替你运行；也可以复制命令到终端，在任何一台 Mac 上都一样。</p>
              <ul class="hosts">
                <li v-for="h in setup?.hosts ?? []" :key="h.id" class="host">
                  <div class="host-head">
                    <span class="host-name">{{ h.name }}</span>
                    <span v-if="hostState(h)" class="pill" :class="hostState(h)!.tone">{{ hostState(h)!.text }}</span>
                    <span class="spacer" />
                    <button
                      v-if="actionLabel(h)"
                      type="button"
                      class="btn primary small"
                      :disabled="busy !== null"
                      @click="connect(h)"
                    >
                      {{ busy === h.id ? '正在接入…' : actionLabel(h) }}
                    </button>
                  </div>
                  <div class="code-line">
                    <!-- html 由 Rust 渲染并净化 -->
                    <div class="prose code-body" v-html="h.html" />
                    <button
                      type="button"
                      class="btn quiet small copy"
                      :aria-label="`复制 ${h.name} 的接入${h.language === 'json' ? '配置' : '命令'}`"
                      @click="copy(h.snippet, h.language === 'json' ? '配置' : '命令')"
                    >
                      复制
                    </button>
                  </div>
                  <p class="host-note">{{ h.note }}</p>
                </li>
              </ul>
            </li>

            <li>
              <h2>交接规则 <span class="optional">可选</span></h2>
              <p class="hint">
                回合已经通过 MCP 把交接规则告诉 AI。想让它每轮都稳定交接，可以把这段加进 CLAUDE.md 或 AGENTS.md。
              </p>
              <div class="code-card">
                <div class="code-head">
                  <button type="button" class="disclose" :aria-expanded="rulesOpen" @click="rulesOpen = !rulesOpen">
                    {{ rulesOpen ? '收起规则' : '查看规则' }}
                  </button>
                  <button type="button" class="btn quiet small" @click="copy(setup?.rules ?? '', '规则')">复制</button>
                </div>
                <div v-if="rulesOpen" class="prose rules" v-html="setup?.rules_html ?? ''" />
              </div>
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

        <!-- 关于 -->
        <template v-else>
          <div class="about">
            <span class="wordmark">回合</span>
            <span class="version">TurnDock {{ setup?.version ?? '' }}</span>
          </div>

          <h2 class="group">更新</h2>
          <div class="panel">
            <div class="row">
              <div class="row-text">
                <template v-if="store.update">
                  <span class="row-title">可以更新到 {{ store.update.version }}</span>
                  <span class="row-desc">下载后会验证签名，然后重启回合；等待中的 AI 会自动重连</span>
                </template>
                <template v-else>
                  <span class="row-title">{{ checked ? '已是最新版本' : '检查新版本' }}</span>
                  <span class="row-desc">只请求 GitHub Releases 上的版本信息，不发送任何数据</span>
                </template>
              </div>
              <button
                v-if="store.update"
                type="button"
                class="btn primary small"
                :disabled="busy !== null"
                @click="installUpdate"
              >
                {{ busy === 'update' ? (percent !== null ? `正在下载 ${percent}%` : '正在下载…') : '更新并重启' }}
              </button>
              <button v-else type="button" class="btn small" :disabled="checking" @click="checkUpdate">
                {{ checking ? '正在检查…' : '检查更新' }}
              </button>
            </div>
            <div v-if="store.update?.notes" class="row notes">
              <p>{{ store.update.notes }}</p>
            </div>
            <div class="row">
              <div class="row-text">
                <span class="row-title">启动时检查更新</span>
                <span class="row-desc">有新版本时在「设置与接入」旁提示，不会自动安装</span>
              </div>
              <button
                type="button"
                role="switch"
                class="switch"
                :aria-checked="form.check_updates"
                aria-label="启动时检查更新"
                @click="form.check_updates = !form.check_updates"
              />
            </div>
          </div>

          <h2 class="group">在另一台电脑上安装</h2>
          <div class="code-line single">
            <code class="install">{{ INSTALL_LINE }}</code>
            <button type="button" class="btn quiet small copy" @click="copy(INSTALL_LINE, '安装命令')">复制</button>
          </div>
          <p class="hint">在终端运行。再次运行即更新到最新版本；加 <code>-s -- --uninstall</code> 卸载。</p>

          <h2 class="group">数据</h2>
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

/* 客户端列表：一行名称与状态，一行命令，一行说明 */
.hosts {
  margin: 12px 0 0;
  padding: 0;
  list-style: none;
  border: 1px solid var(--rule);
  border-radius: 10px;
  overflow: hidden;
}
.host {
  padding: 12px 14px 12px;
  background: var(--sheet);
}
.host + .host {
  border-top: 1px solid var(--rule);
}
.host-head {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 28px;
}
.host-name {
  font-size: 14px;
  font-weight: 600;
}
.spacer {
  flex: 1;
}
.pill {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 20px;
  padding: 0 8px;
  border-radius: 10px;
  font-size: 11.5px;
  color: var(--ink-2);
  background: var(--stone);
}
.pill::before {
  content: '';
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: currentColor;
}
.pill.ok {
  color: var(--ok);
  background: color-mix(in srgb, var(--ok) 12%, transparent);
}
.pill.idle::before {
  background: transparent;
  box-shadow: inset 0 0 0 1px currentColor;
}
.pill.off {
  opacity: 0.75;
}
.pill.off::before {
  display: none;
}
.code-line {
  position: relative;
  margin-top: 8px;
  border-radius: 8px;
  background: var(--code-bg);
}
.code-line .copy {
  position: absolute;
  top: 6px;
  right: 6px;
}
.code-line.single {
  padding: 10px 70px 10px 14px;
  border: 1px solid var(--rule);
}
.install {
  display: block;
  font-family: var(--font-code);
  font-size: 12px;
  line-height: 20px;
  word-break: break-all;
  user-select: all;
}
.host-note {
  margin: 8px 0 0;
  font-size: 12px;
  line-height: 18px;
  color: var(--ink-2);
}
.optional {
  margin-left: 6px;
  font-size: 12px;
  font-weight: 500;
  color: var(--ink-2);
}
.disclose {
  padding: 0;
  border: 0;
  background: none;
  color: var(--signal-fg);
  font-size: 12.5px;
}

/* 关于 */
.about {
  display: flex;
  align-items: baseline;
  gap: 12px;
  margin-bottom: 4px;
}
.wordmark {
  font-family: var(--font-display);
  font-size: 28px;
  font-weight: 700;
  letter-spacing: 0.02em;
}
.version {
  font-family: var(--font-code);
  font-size: 12px;
  color: var(--ink-2);
}
.notes p {
  margin: 0;
  font-size: 12.5px;
  line-height: 20px;
  color: var(--ink-2);
  white-space: pre-wrap;
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
  padding: 10px 70px 10px 14px;
  background: transparent;
  white-space: pre;
  overflow-x: auto;
  border-radius: 0;
  max-height: none;
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
