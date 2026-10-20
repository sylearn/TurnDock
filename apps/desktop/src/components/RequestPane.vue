<script setup lang="ts">
// 一条请求的阅读与回复。收件箱与快速面板共用；compact 为快速面板排版。
import { computed, nextTick, onBeforeUnmount, reactive, ref, watch } from 'vue'
import type { RequestDetail } from '../generated/RequestDetail'
import { api, errorText } from '../lib/api'
import { age, clientMark, statusOf } from '../lib/format'
import { store, toast } from '../lib/store'
import ConfirmDialog from './ConfirmDialog.vue'
import StoneMark from './StoneMark.vue'

const props = defineProps<{ detail: RequestDetail; compact?: boolean; remaining?: number }>()
const emit = defineEmits<{ done: [id: string]; later: [id: string]; expand: [id: string] }>()

interface Image {
  id: string
  name: string
  url: string
  data: string
  bytes: number
}

const MAX_IMAGES = 4
const MAX_IMAGE_BYTES = 10 * 1024 * 1024

// 图片只在本次运行内按请求保留；切换请求不会丢。
const imageCache = new Map<string, Image[]>()

const draft = reactive({ text: '', selected: [] as string[] })
const images = ref<Image[]>([])
const saveState = ref<'idle' | 'saving' | 'saved' | 'error'>('idle')
const sending = ref(false)
const notice = ref<{ tone: 'error' | 'info'; text: string } | null>(null)
const textarea = ref<HTMLTextAreaElement | null>(null)
const prose = ref<HTMLElement | null>(null)
const fileInput = ref<HTMLInputElement | null>(null)
const declineDialog = ref<InstanceType<typeof ConfirmDialog> | null>(null)
const dismissDialog = ref<InstanceType<typeof ConfirmDialog> | null>(null)
let submissionId: string | null = null
let loadedId = ''
let saveTimer: number | undefined
/** 载入草稿时的赋值不算编辑，不触发保存。 */
let hydrating = false

const summary = computed(() => props.detail.summary)
const status = computed(() => statusOf(summary.value))
const mark = computed(() => clientMark(summary.value.client_name))
const waiting = computed(() => summary.value.phase === 'waiting')
const multiple = computed(() => props.detail.selection === 'multiple')
const quickReplies = computed(() => store.prefs?.quick_replies ?? [])
const canSend = computed(
  () =>
    waiting.value &&
    !sending.value &&
    (draft.selected.length > 0 ||
      (props.detail.allow_text && draft.text.trim().length > 0) ||
      images.value.length > 0),
)
const optionLabels = computed(() =>
  (props.detail.decision?.selected_ids ?? []).map(
    (id) => props.detail.options.find((o) => o.id === id)?.label ?? id,
  ),
)

watch(
  () => props.detail,
  (d) => {
    if (d.summary.id === loadedId) return
    flushDraft()
    loadedId = d.summary.id
    const text = d.draft?.text ?? ''
    const selected = (d.draft?.selected_ids ?? []).filter((id) => d.options.some((o) => o.id === id))
    hydrating = text !== draft.text || selected.join('\u0000') !== draft.selected.join('\u0000')
    draft.text = text
    draft.selected = selected
    images.value = imageCache.get(d.summary.id) ?? []
    saveState.value = 'idle'
    notice.value = null
    submissionId = null
    void nextTick(() => {
      if (d.summary.phase === 'waiting' && (props.compact || d.options.length === 0)) {
        textarea.value?.focus()
      }
    })
  },
  { immediate: true },
)

watch(
  [() => draft.text, () => draft.selected.join('\u0000')],
  () => {
    if (hydrating) {
      hydrating = false
      return
    }
    if (!waiting.value) return
    submissionId = null
    saveState.value = 'saving'
    window.clearTimeout(saveTimer)
    saveTimer = window.setTimeout(saveDraft, 500)
  },
)

watch(
  () => props.detail.message_html,
  () => void nextTick(decorateCode),
  { immediate: true },
)

watch(images, (list) => {
  imageCache.set(loadedId, list)
  submissionId = null
})

onBeforeUnmount(flushDraft)

async function saveDraft() {
  const id = loadedId
  if (!id) return
  try {
    await api.saveDraft(id, { text: draft.text, selected_ids: [...draft.selected] })
    if (id === loadedId) saveState.value = 'saved'
  } catch {
    if (id === loadedId) saveState.value = 'error'
  }
}

function flushDraft() {
  if (saveTimer !== undefined && saveState.value === 'saving') {
    window.clearTimeout(saveTimer)
    void saveDraft()
  }
}

function toggleOption(id: string) {
  if (!waiting.value) return
  if (multiple.value) {
    draft.selected = draft.selected.includes(id)
      ? draft.selected.filter((s) => s !== id)
      : [...draft.selected, id]
  } else {
    draft.selected = draft.selected[0] === id ? [] : [id]
  }
}

function pickOption(index: number) {
  const o = props.detail.options[index]
  if (o) toggleOption(o.id)
}

async function send(
  outcome: 'answered' | 'declined' = 'answered',
  textOverride?: string,
) {
  if (sending.value || !waiting.value) return
  const id = loadedId
  const reply = {
    request_id: id,
    submission_id: (submissionId ??= crypto.randomUUID()),
    outcome,
    selected_ids: outcome === 'answered' ? [...draft.selected] : [],
    text: textOverride ?? (props.detail.allow_text || outcome === 'declined' ? draft.text : ''),
    images: outcome === 'answered' ? images.value.map((i) => i.data) : [],
  }
  sending.value = true
  notice.value = null
  try {
    const result = await api.submit(reply)
    imageCache.delete(id)
    for (const i of images.value) URL.revokeObjectURL(i.url)
    window.clearTimeout(saveTimer)
    saveState.value = 'idle'
    store.later.delete(id)
    toast(
      outcome === 'declined'
        ? '已拒绝。AI 会收到明确的否定。'
        : result === 'stored'
          ? '回复已保存。AI 下次来取时会收到。'
          : '已交给 AI。',
    )
    emit('done', id)
  } catch (e) {
    notice.value = { tone: 'error', text: `没有发出：${errorText(e)}。草稿还在。` }
  } finally {
    sending.value = false
  }
}

function sendQuick(index: number) {
  const text = quickReplies.value[index]
  if (!text || !waiting.value || !props.detail.allow_text) return
  if (draft.text.trim()) {
    // 输入框里已有内容时不直接发送，避免丢掉正在写的文字：追加到末尾供确认。
    draft.text = `${draft.text.trimEnd()}\n${text}`
    textarea.value?.focus()
    return
  }
  void send('answered', text)
}

async function dismiss() {
  try {
    await api.dismiss(loadedId)
    toast('已关闭这条请求。')
    emit('done', loadedId)
  } catch (e) {
    notice.value = { tone: 'error', text: errorText(e) }
  }
}

function later() {
  if (waiting.value) emit('later', loadedId)
}

// ---------- 图片 ----------

function readAsBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader()
    r.onload = () => resolve(String(r.result).replace(/^data:[^,]*,/, ''))
    r.onerror = () => reject(r.error)
    r.readAsDataURL(file)
  })
}

async function addFiles(files: Iterable<File>) {
  if (!waiting.value || !props.detail.allow_images) return
  for (const f of files) {
    if (!f.type.startsWith('image/')) continue
    if (images.value.length >= MAX_IMAGES) {
      notice.value = { tone: 'error', text: `最多 ${MAX_IMAGES} 张图片。` }
      return
    }
    if (f.size > MAX_IMAGE_BYTES) {
      notice.value = { tone: 'error', text: `${f.name || '图片'} 超过 10 MiB，没有添加。` }
      continue
    }
    const data = await readAsBase64(f)
    images.value = [
      ...images.value,
      {
        id: crypto.randomUUID(),
        name: f.name || '粘贴的图片',
        url: URL.createObjectURL(f),
        data,
        bytes: f.size,
      },
    ]
  }
}

function removeImage(id: string) {
  const target = images.value.find((i) => i.id === id)
  if (target) URL.revokeObjectURL(target.url)
  images.value = images.value.filter((i) => i.id !== id)
}

function onPaste(e: ClipboardEvent) {
  const files = [...(e.clipboardData?.files ?? [])]
  if (files.some((f) => f.type.startsWith('image/'))) {
    e.preventDefault()
    void addFiles(files)
  }
}

function onDrop(e: DragEvent) {
  const files = [...(e.dataTransfer?.files ?? [])]
  if (files.length) void addFiles(files)
}

function onPick(e: Event) {
  const input = e.target as HTMLInputElement
  void addFiles([...(input.files ?? [])])
  input.value = ''
}

// ---------- 其他 ----------

/** 给代码块加复制按钮（按钮是界面元素，不来自调用方内容）。 */
function decorateCode() {
  for (const pre of prose.value?.querySelectorAll('pre') ?? []) {
    if (pre.querySelector(':scope > .copy-code')) continue
    const b = document.createElement('button')
    b.type = 'button'
    b.className = 'copy-code'
    b.textContent = '复制'
    b.setAttribute('aria-label', '复制代码')
    pre.appendChild(b)
  }
}

function onProseClick(e: MouseEvent) {
  const btn = (e.target as HTMLElement | null)?.closest('.copy-code')
  if (btn) {
    const code = btn.parentElement?.querySelector('code')?.textContent ?? ''
    void copy(code.replace(/\n$/, ''))
    return
  }
  const a = (e.target as HTMLElement | null)?.closest('a[href]')
  if (!a) return
  e.preventDefault()
  void api.openExternal(a.getAttribute('href') ?? '')
}

async function copy(text: string) {
  try {
    await navigator.clipboard.writeText(text)
    notice.value = { tone: 'info', text: '已复制。' }
  } catch {
    notice.value = { tone: 'error', text: '复制失败。' }
  }
}

function kb(n: number) {
  return n < 1024 * 1024 ? `${Math.max(1, Math.round(n / 1024))} KB` : `${(n / 1048576).toFixed(1)} MB`
}

defineExpose({
  send: () => canSend.value && send(),
  sendQuick,
  pickOption,
  later,
  decline: () => waiting.value && declineDialog.value?.open(),
  focusInput: () => textarea.value?.focus(),
})
</script>

<template>
  <article class="pane" :class="{ compact }" @dragover.prevent @drop.prevent="onDrop">
    <header class="bar" data-tauri-drag-region>
      <span class="source" data-tauri-drag-region>
        <span class="glyph" aria-hidden="true">{{ mark.glyph }}</span>
        <span class="client">{{ mark.label }}</span>
        <span v-if="summary.project" class="project" :title="summary.project_path ?? ''">{{
          summary.project
        }}</span>
      </span>
      <span class="state" :class="status.tone" data-tauri-drag-region>
        <StoneMark :stone="status.stone" :size="10" />
        {{ status.label }}
        <span class="age">{{ age(summary.created_at, store.now) }}</span>
      </span>
      <template v-if="compact">
        <span v-if="remaining" class="remaining">还有 {{ remaining }} 条</span>
        <button type="button" class="btn quiet" title="在收件箱中打开（⌘E）" @click="emit('expand', summary.id)">
          展开 <span class="kbd">⌘E</span>
        </button>
      </template>
    </header>

    <div class="reading">
      <div class="column">
        <h1 v-if="detail.heading" class="question">{{ detail.heading }}</h1>
        <!-- message_html 已在 Rust 中渲染并经 ammonia 白名单净化 -->
        <div v-if="detail.message_html" ref="prose" class="prose" @click="onProseClick" v-html="detail.message_html" />

        <fieldset v-if="detail.options.length" class="options" :disabled="!waiting">
          <legend class="visually-hidden">{{ multiple ? '可多选' : '选择一项' }}</legend>
          <p class="hint">{{ multiple ? '可多选' : '选一项' }}，按数字键选择</p>
          <label
            v-for="(o, i) in detail.options"
            :key="o.id"
            class="option"
            :class="{ on: draft.selected.includes(o.id) }"
          >
            <input
              :type="multiple ? 'checkbox' : 'radio'"
              :name="`opt-${summary.id}`"
              :checked="draft.selected.includes(o.id)"
              @click.prevent="toggleOption(o.id)"
            />
            <span class="option-text">
              <span class="option-label">{{ o.label }}</span>
              <span v-if="o.description" class="option-desc">{{ o.description }}</span>
            </span>
            <span v-if="i < 9" class="kbd">{{ i + 1 }}</span>
          </label>
        </fieldset>

        <section v-if="!waiting" class="outcome" :class="status.tone">
          <p class="outcome-status">
            <StoneMark :stone="status.stone" :size="10" />
            {{ status.detail }}
          </p>
          <template v-if="detail.decision">
            <h2>{{ detail.decision.outcome === 'declined' ? '你拒绝了这个请求' : '你的回复' }}</h2>
            <p v-if="optionLabels.length" class="picked">{{ optionLabels.join('、') }}</p>
            <p v-if="detail.decision.text" class="said">{{ detail.decision.text }}</p>
            <p v-if="detail.decision.image_count" class="picked">附图 {{ detail.decision.image_count }} 张</p>
            <button
              v-if="detail.decision.text"
              type="button"
              class="btn quiet"
              @click="copy(detail.decision.text)"
            >
              复制回复
            </button>
          </template>
          <button v-else-if="detail.draft?.text" type="button" class="btn quiet" @click="copy(detail.draft.text)">
            复制草稿
          </button>
        </section>
      </div>
    </div>

    <footer v-if="waiting" class="composer">
      <div class="column">
        <p v-if="status.detail" class="idle-note">{{ status.detail }}</p>
        <div class="field" :class="{ disabled: !detail.allow_text }">
          <textarea
            v-if="detail.allow_text"
            ref="textarea"
            v-model="draft.text"
            :rows="compact ? 2 : 3"
            placeholder="写回复…（Enter 换行，⌘↵ 发送）"
            aria-label="回复"
            @paste="onPaste"
          />
          <ul v-if="images.length" class="images" aria-label="附图">
            <li v-for="img in images" :key="img.id">
              <img :src="img.url" :alt="img.name" />
              <span class="img-meta">{{ kb(img.bytes) }}</span>
              <button type="button" class="remove" :aria-label="`移除 ${img.name}`" @click="removeImage(img.id)">
                ×
              </button>
            </li>
          </ul>
        </div>

        <div v-if="quickReplies.length && detail.allow_text" class="quick" aria-label="快捷回复">
          <button
            v-for="(q, i) in quickReplies"
            :key="i"
            type="button"
            class="chip"
            :disabled="sending"
            :title="draft.text.trim() ? '追加到输入框' : '直接发送'"
            @click="sendQuick(i)"
          >
            <span class="kbd">⌥{{ i + 1 }}</span>{{ q }}
          </button>
        </div>

        <div class="actions">
          <div class="secondary">
            <button
              v-if="detail.allow_images"
              type="button"
              class="btn quiet"
              :disabled="images.length >= MAX_IMAGES"
              @click="fileInput?.click()"
            >
              添加图片
            </button>
            <input ref="fileInput" type="file" accept="image/png,image/jpeg,image/webp,image/gif" multiple hidden @change="onPick" />
            <button type="button" class="btn quiet" title="⌘L" @click="later">稍后</button>
            <button type="button" class="btn quiet danger" title="⌘⇧⌫" @click="declineDialog?.open()">拒绝</button>
            <button v-if="!summary.attached" type="button" class="btn quiet" @click="dismissDialog?.open()">
              关闭请求
            </button>
          </div>
          <span class="save" :class="saveState" aria-live="polite">
            {{
              saveState === 'saving'
                ? '保存中'
                : saveState === 'saved'
                  ? '草稿已保存'
                  : saveState === 'error'
                    ? '草稿未保存，请先复制'
                    : ''
            }}
          </span>
          <button type="button" class="btn primary" :disabled="!canSend" @click="send()">
            {{ sending ? '发送中' : '发送' }} <span class="kbd">⌘↵</span>
          </button>
        </div>
        <p v-if="notice" class="notice" :class="notice.tone" role="status">{{ notice.text }}</p>
      </div>
    </footer>
    <footer v-else-if="notice" class="composer">
      <div class="column">
        <p class="notice" :class="notice.tone" role="status">{{ notice.text }}</p>
      </div>
    </footer>

    <ConfirmDialog
      ref="declineDialog"
      title="拒绝这个请求？"
      body="AI 会收到明确的拒绝，不会执行这个方案。"
      confirm-label="拒绝"
      danger
      with-reason
      @confirm="(reason) => send('declined', reason)"
    />
    <ConfirmDialog
      ref="dismissDialog"
      title="关闭这条请求？"
      body="不会回复任何内容。AI 之后来取时会被告知“没有回复”，不会被当作同意。"
      confirm-label="关闭请求"
      @confirm="dismiss"
    />
  </article>
</template>

<style scoped>
.pane {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-width: 0;
  background: var(--sheet);
}
.column {
  max-width: var(--measure);
  padding: 0 40px;
}
.compact .column {
  padding: 0 24px;
}

.bar {
  flex: none;
  display: flex;
  align-items: center;
  gap: 16px;
  height: var(--bar);
  padding: 0 16px 0 40px;
  border-bottom: 1px solid var(--rule);
}
.compact .bar {
  padding-left: 84px; /* 让出 macOS 窗口按钮 */
  gap: 12px;
}
.source {
  display: flex;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
  font-size: 13px;
}
.glyph {
  font-size: 10px;
  color: var(--ink-2);
}
.client {
  font-weight: 600;
  white-space: nowrap;
}
.project {
  color: var(--ink-2);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.state {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  white-space: nowrap;
}
.state.signal {
  color: var(--signal);
}
.state.warn {
  color: var(--warn);
}
.state.danger {
  color: var(--danger);
}
.state.muted {
  color: var(--ink-2);
}
.age {
  color: var(--ink-2);
  margin-left: 6px;
  font-variant-numeric: tabular-nums;
}
.remaining {
  font-size: 12px;
  color: var(--ink-2);
  white-space: nowrap;
}

.reading {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 28px 0 32px;
  scrollbar-gutter: stable;
}
.compact .reading {
  padding: 18px 0 20px;
}
.question {
  margin: 0 0 14px;
  font-family: var(--font-note);
  font-size: 22px;
  line-height: 32px;
  font-weight: 600;
  letter-spacing: 0.01em;
  user-select: text;
}
.compact .question {
  font-size: 18px;
  line-height: 26px;
  margin-bottom: 10px;
}

.options {
  display: grid;
  gap: 8px;
  margin: 24px 0 0;
  padding: 0;
  border: 0;
  min-width: 0;
}
.hint {
  margin: 0 0 2px;
  font-size: 12px;
  color: var(--ink-2);
}
.option {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 11px 14px;
  border: 1px solid var(--edge);
  border-radius: 10px;
  transition: border-color 120ms var(--ease), background-color 120ms var(--ease);
}
.option:hover {
  background: var(--stone);
}
.option.on {
  border-color: var(--signal);
  background: var(--signal-wash);
  box-shadow: inset 0 0 0 1px var(--signal);
}
.option input {
  flex: none;
  margin: 4px 0 0;
  accent-color: var(--signal);
}
.option-text {
  flex: 1;
  display: grid;
  gap: 2px;
}
.option-desc {
  font-size: 12px;
  line-height: 18px;
  color: var(--ink-2);
}
.option .kbd {
  margin-top: 1px;
  color: var(--ink-2);
}
.options:disabled .option {
  opacity: 0.7;
}

.outcome {
  margin-top: 28px;
  padding-top: 20px;
  border-top: 1px solid var(--rule);
}
.outcome-status {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0 0 16px;
  font-size: 13px;
}
.outcome.warn .outcome-status {
  color: var(--warn);
}
.outcome.danger .outcome-status {
  color: var(--danger);
}
.outcome.muted .outcome-status,
.outcome.signal .outcome-status {
  color: var(--ink-2);
}
.outcome h2 {
  margin: 0 0 6px;
  font-size: 13px;
  font-weight: 600;
}
.picked {
  margin: 0 0 4px;
}
.outcome .btn.quiet {
  margin-left: -12px;
}
.said {
  margin: 0 0 8px;
  white-space: pre-wrap;
  user-select: text;
}

.composer {
  flex: none;
  padding: 14px 0 16px;
  border-top: 1px solid var(--rule);
  background: var(--sheet);
}
.idle-note {
  margin: 0 0 10px;
  font-size: 12px;
  line-height: 18px;
  color: var(--warn);
}
.field {
  border: 1px solid var(--edge);
  border-radius: 10px;
  background: var(--sheet);
}
.field:focus-within {
  border-color: var(--signal);
  box-shadow: 0 0 0 1px var(--signal);
}
.field.disabled {
  border-style: dashed;
}
.field textarea {
  display: block;
  width: 100%;
  min-height: 66px;
  max-height: 220px;
  padding: 10px 12px;
  border: 0;
  border-radius: 10px;
  resize: vertical;
  background: transparent;
  line-height: 22px;
  outline: none;
  user-select: text;
}
.compact .field textarea {
  min-height: 52px;
}
.images {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin: 0;
  padding: 0 10px 10px;
  list-style: none;
}
.images li {
  position: relative;
  display: grid;
  gap: 2px;
  justify-items: center;
}
.images img {
  width: 56px;
  height: 56px;
  object-fit: cover;
  border-radius: 6px;
  border: 1px solid var(--rule);
}
.img-meta {
  font-size: 11px;
  line-height: 14px;
  color: var(--ink-2);
}
.remove {
  position: absolute;
  top: -6px;
  right: -6px;
  width: 20px;
  height: 20px;
  border: 1px solid var(--edge);
  border-radius: 50%;
  background: var(--sheet);
  font-size: 13px;
  line-height: 16px;
  padding: 0;
}

.quick {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 10px;
}
.chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 100%;
  min-height: 28px;
  padding: 0 10px;
  border: 1px solid var(--rule);
  border-radius: 14px;
  background: var(--stone);
  font-size: 12.5px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.chip:hover:not(:disabled) {
  border-color: var(--signal);
}
.chip .kbd {
  color: var(--ink-2);
}

.actions {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 12px;
}
.secondary {
  display: flex;
  flex-wrap: wrap;
  gap: 2px;
  margin-left: -12px;
}
.save {
  margin-left: auto;
  font-size: 12px;
  color: var(--ink-2);
  white-space: nowrap;
}
.save.error {
  color: var(--danger);
}
.notice {
  margin: 10px 0 0;
  font-size: 12.5px;
}
.notice.error {
  color: var(--danger);
}
.notice.info {
  color: var(--ink-2);
}
</style>
