// 仅用于设计审查的浏览器预览：用 Tauri 官方 mockIPC 注入示例数据。不参与生产构建。
// 打开 /preview.html；加 #quick 查看快速面板，加 ?theme=dark 查看深色。
import { mockIPC } from '@tauri-apps/api/mocks'
import { createApp } from 'vue'
import App from '../App.vue'
import '../styles/app.css'
import { fixtures } from './fixtures'
import setupJson from './setup.json'
import type { Setup } from '../lib/api'

const setup = setupJson as unknown as Setup

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
        check_updates: true,
      }
    case 'setup_status':
      return {
        ...setup,
        command: {
          ...setup.command,
          state: params.get('command') ?? 'ready',
          path: '/usr/local/bin/turndock',
          target: '/Applications/TurnDock.app/Contents/MacOS/turndock',
          bundled: '/Applications/TurnDock.app/Contents/MacOS/turndock',
          bundled_found: true,
        },
        hosts: setup.hosts.map((h) => (h.id === 'claude_code' ? { ...h, connected: true } : h)),
      }
    case 'check_update':
      return params.has('update')
        ? { current: '0.1.0', version: '0.2.0', notes: '- 决策记录：按项目回看每一轮选了什么方向\n- 修复快速面板偶尔不聚焦', date: null }
        : null
    case 'plugin:event|listen':
      return 1
    default:
      return null
  }
})

createApp(App).mount('#app')
