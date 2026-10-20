// 仅用于设计审查的浏览器预览：用 Tauri 官方 mockIPC 注入示例数据。不参与生产构建。
// 打开 /preview.html；加 #quick 查看快速面板，加 ?theme=dark 查看深色。
import { mockIPC } from '@tauri-apps/api/mocks'
import { createApp } from 'vue'
import App from '../App.vue'
import '../styles/app.css'
import { fixtures } from './fixtures'

const params = new URLSearchParams(location.search)
const theme = params.get('theme')
if (theme === 'dark' || theme === 'light') document.documentElement.dataset.theme = theme

const data = fixtures(Date.now())
mockIPC((cmd, args) => {
  const a = (args ?? {}) as Record<string, unknown>
  switch (cmd) {
    case 'snapshot':
      return { revision: 1, requests: data.map((d) => d.summary) }
    case 'detail':
      return data.find((d) => d.summary.id === a.id) ?? null
    case 'get_prefs':
      return {
        quick_replies: ['继续', '按最佳实践继续', '先停下，总结目前的进展和下一步计划'],
        shortcut: 'Control+Alt+Space',
        notify: true,
        popup: false,
        theme: theme ?? 'system',
      }
    case 'integration':
      return {
        bridge_path: '/Applications/TurnDock.app/Contents/MacOS/turndock-mcp',
        bridge_found: true,
        rules: '## 与用户确认（TurnDock）\n\n- 需要用户决定时，调用 ask_user 并给出选项。',
        snippets: [
          {
            host: 'Claude Code',
            file: '终端',
            language: 'shell',
            content: "claude mcp add --scope user turndock -- '/Applications/TurnDock.app/Contents/MacOS/turndock-mcp'",
            note: 'TurnDock 每 25 分钟返回一次 pending，AI 会自动续等。',
          },
        ],
      }
    case 'plugin:event|listen':
      return 1
    default:
      return null
  }
})

createApp(App).mount('#app')
