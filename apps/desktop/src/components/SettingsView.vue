<script setup lang="ts">
// 设置与接入。接入部分只生成配置文本，不修改任何外部配置文件。
import { onMounted, reactive, ref, toRaw } from 'vue'
import { api, errorText, type Integration, type Prefs } from '../lib/api'
import { store, toast } from '../lib/store'

const integration = ref<Integration | null>(null)
const form = reactive<Prefs>({
  quick_replies: [],
  shortcut: '',
  notify: true,
  popup: false,
  theme: 'system',
})
const repliesText = ref('')
const error = ref<string | null>(null)
const saving = ref(false)

onMounted(async () => {
  if (store.prefs) {
    const p = toRaw(store.prefs)
    Object.assign(form, { ...p, quick_replies: [...p.quick_replies] })
  }
  repliesText.value = form.quick_replies.join('\n')
  try {
    integration.value = await api.integration()
  } catch (e) {
    error.value = errorText(e)
  }
})

async function save() {
  saving.value = true
  error.value = null
  try {
    const saved = await api.setPrefs({
      ...form,
      quick_replies: repliesText.value.split('\n'),
    })
    store.prefs = saved
    Object.assign(form, saved)
    repliesText.value = saved.quick_replies.join('\n')
    toast('设置已保存。')
  } catch (e) {
    error.value = errorText(e)
  } finally {
    saving.value = false
  }
}

async function copy(text: string, what: string) {
  try {
    await navigator.clipboard.writeText(text)
    toast(`已复制${what}。`)
  } catch {
    error.value = '复制失败。'
  }
}
</script>

<template>
  <section class="settings">
    <header class="bar" data-tauri-drag-region><h1 data-tauri-drag-region>设置与接入</h1></header>
    <div class="scroll">
      <div class="column">
        <section>
          <h2>接入 AI 客户端</h2>
          <p class="lead">
            把下面的配置加到你使用的客户端里。回合不会修改这些文件；复制后由你自己粘贴。
          </p>
          <p v-if="integration && !integration.bridge_found" class="warn">
            没有在 {{ integration.bridge_path }} 找到 turndock-mcp。开发环境请先运行
            <code>cargo build -p turndock-bridge</code>。
          </p>
          <article v-for="s in integration?.snippets ?? []" :key="s.host" class="snippet">
            <div class="snippet-head">
              <h3>{{ s.host }}</h3>
              <span class="file">{{ s.file }}</span>
              <button type="button" class="btn quiet" @click="copy(s.content, ` ${s.host} 配置`)">复制</button>
            </div>
            <pre><code>{{ s.content }}</code></pre>
            <p class="note">{{ s.note }}</p>
          </article>
        </section>

        <section>
          <h2>给 AI 的规则</h2>
          <p class="lead">
            加到项目的 AGENTS.md、CLAUDE.md 或客户端的规则设置里。AI 只有被要求“结束前先确认”，才会在每轮结束前来问你。
          </p>
          <article class="snippet">
            <div class="snippet-head">
              <h3>规则片段</h3>
              <button type="button" class="btn quiet" @click="copy(integration?.rules ?? '', '规则片段')">复制</button>
            </div>
            <pre><code>{{ integration?.rules }}</code></pre>
          </article>
        </section>

        <form @submit.prevent="save">
          <h2>快捷回复</h2>
          <p class="lead">每行一条，最多 9 条，按 ⌥1–⌥9 发送。发给 AI 的就是这里写的文字。</p>
          <textarea v-model="repliesText" rows="4" aria-label="快捷回复，每行一条" />

          <h2>提醒</h2>
          <label class="check">
            <input v-model="form.notify" type="checkbox" />
            <span>新请求时发送系统通知（回合窗口在前台时不通知）</span>
          </label>
          <label class="check">
            <input v-model="form.popup" type="checkbox" />
            <span>新请求时直接弹出快速回复面板</span>
          </label>

          <h2>全局快捷键</h2>
          <p class="lead">唤出或收起快速回复面板。留空表示不使用。</p>
          <input v-model="form.shortcut" class="text" placeholder="Control+Alt+Space" aria-label="全局快捷键" />

          <h2>外观</h2>
          <div class="themes" role="radiogroup" aria-label="外观">
            <label v-for="t in (['system', 'light', 'dark'] as const)" :key="t" class="check">
              <input v-model="form.theme" type="radio" name="theme" :value="t" />
              <span>{{ t === 'system' ? '跟随系统' : t === 'light' ? '浅色' : '深色' }}</span>
            </label>
          </div>

          <p v-if="error" class="error" role="alert">{{ error }}</p>
          <div class="save-row">
            <button type="submit" class="btn primary" :disabled="saving">
              {{ saving ? '保存中' : '保存设置' }}
            </button>
          </div>
        </form>

        <section class="privacy">
          <h2>数据与隐私</h2>
          <p class="lead">
            所有请求与回复只保存在本机。已结束的请求保留 7 天后自动删除；没有账号、云同步或使用统计。
          </p>
        </section>
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
  height: var(--bar);
  padding: 0 40px;
  border-bottom: 1px solid var(--rule);
}
.bar h1 {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
}
.scroll {
  flex: 1;
  overflow-y: auto;
  padding: 8px 0 40px;
}
.column {
  max-width: var(--measure);
  padding: 0 40px;
}
h2 {
  margin: 28px 0 6px;
  font-size: 15px;
  font-weight: 650;
}
.lead {
  margin: 0 0 12px;
  color: var(--ink-2);
}
.warn {
  color: var(--warn);
}
.snippet {
  margin: 0 0 16px;
}
.snippet-head {
  display: flex;
  align-items: baseline;
  gap: 10px;
}
.snippet-head h3 {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
}
.file {
  font-size: 12px;
  color: var(--ink-2);
}
.snippet-head .btn {
  margin-left: auto;
}
pre {
  margin: 6px 0 6px;
  padding: 10px 12px;
  border-radius: 8px;
  background: var(--stone);
  font-family: var(--font-code);
  font-size: 12px;
  line-height: 18px;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  user-select: text;
}
code {
  font-family: var(--font-code);
  font-size: 12px;
}
.note {
  margin: 0;
  font-size: 12px;
  line-height: 18px;
  color: var(--ink-2);
}
textarea,
.text {
  width: 100%;
  padding: 8px 10px;
  border: 1px solid var(--edge);
  border-radius: 8px;
  background: var(--sheet);
  line-height: 22px;
  user-select: text;
}
.text {
  max-width: 280px;
}
textarea:focus,
.text:focus {
  outline: none;
  border-color: var(--signal);
  box-shadow: 0 0 0 1px var(--signal);
}
.check {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 30px;
}
.check input {
  accent-color: var(--signal);
}
.themes {
  display: flex;
  gap: 20px;
}
.save-row {
  margin-top: 24px;
}
.error {
  color: var(--danger);
}
.privacy {
  margin-top: 12px;
  padding-top: 4px;
  border-top: 1px solid var(--rule);
}
</style>
