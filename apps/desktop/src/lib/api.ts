// 与 Rust 外壳通信的唯一入口。业务状态只来自这里返回的快照。
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { BrokerEvent } from '../generated/BrokerEvent'
import type { Draft } from '../generated/Draft'
import type { RequestDetail } from '../generated/RequestDetail'
import type { Snapshot } from '../generated/Snapshot'
import type { SubmitReply } from '../generated/SubmitReply'
import type { SubmitStatus } from '../generated/SubmitStatus'

export interface Prefs {
  quick_replies: string[]
  shortcut: string
  notify: boolean
  popup: boolean
  theme: 'system' | 'light' | 'dark'
  check_updates: boolean
}

export interface CommandStatus {
  state: 'ready' | 'missing' | 'elsewhere'
  path: string
  target: string | null
  bundled: string
  bundled_found: boolean
}

export type HostId = 'claude_code' | 'codex' | 'json'

export interface Host {
  id: HostId
  name: string
  installed: boolean | null
  connected: boolean | null
  /** run：回合替你运行接入命令；open：交给客户端自己的确认；copy：只能复制 */
  action: 'run' | 'open' | 'copy'
  snippet: string
  language: string
  /** Rust 渲染并净化的高亮 HTML */
  html: string
  note: string
}

export interface Setup {
  version: string
  command: CommandStatus
  hosts: Host[]
  rules: string
  rules_html: string
}

export interface UpdateInfo {
  current: string
  version: string
  notes: string | null
  date: string | null
}

export interface UpdateProgress {
  downloaded: number
  total: number | null
}

/** Rust ActionError 的序列化形式。 */
export interface ActionError {
  kind: 'not_found' | 'invalid' | 'state' | 'storage' | 'stopped'
  message?: string
}

export function errorText(e: unknown): string {
  if (typeof e === 'string') return e
  if (e && typeof e === 'object') {
    const a = e as Partial<ActionError>
    if (a.message) return a.message
    if (a.kind === 'not_found') return '找不到这条请求'
    if (a.kind === 'stopped') return '协调器已停止，请重新打开回合'
  }
  return String(e)
}

export const api = {
  snapshot: () => invoke<Snapshot>('snapshot'),
  detail: (id: string) => invoke<RequestDetail | null>('detail', { id }),
  submit: (reply: SubmitReply) => invoke<SubmitStatus>('submit', { reply }),
  dismiss: (id: string) => invoke<void>('dismiss', { id }),
  saveDraft: (id: string, draft: Draft) => invoke<void>('save_draft', { id, draft }),
  getPrefs: () => invoke<Prefs>('get_prefs'),
  setPrefs: (prefs: Prefs) => invoke<Prefs>('set_prefs', { prefs }),
  setupStatus: () => invoke<Setup>('setup_status'),
  connectHost: (host: HostId) => invoke<void>('connect_host', { host }),
  checkUpdate: () => invoke<UpdateInfo | null>('check_update'),
  installUpdate: () => invoke<void>('install_update'),
  openExternal: (url: string) => invoke<void>('open_external', { url }),
  showInbox: (id: string | null) => invoke<void>('show_inbox', { id }),
  showQuick: () => invoke<void>('show_quick'),
  hideQuick: () => invoke<void>('hide_quick'),
}

export function onBrokerEvent(cb: (e: BrokerEvent) => void): Promise<UnlistenFn> {
  return listen<BrokerEvent>('turndock://event', (e) => cb(e.payload))
}

export function onSelect(cb: (id: string | null) => void): Promise<UnlistenFn> {
  return listen<string | null>('turndock://select', (e) => cb(e.payload))
}

export function onPrefs(cb: (p: Prefs) => void): Promise<UnlistenFn> {
  return listen<Prefs>('turndock://prefs', (e) => cb(e.payload))
}

export function onUpdateAvailable(cb: (u: UpdateInfo) => void): Promise<UnlistenFn> {
  return listen<UpdateInfo>('turndock://update-available', (e) => cb(e.payload))
}

export function onUpdateProgress(cb: (p: UpdateProgress) => void): Promise<UnlistenFn> {
  return listen<UpdateProgress>('turndock://update-progress', (e) => cb(e.payload))
}
