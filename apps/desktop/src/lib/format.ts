// 展示用的纯函数：来源标识、等待时长、状态文案。
import type { RequestSummary } from '../generated/RequestSummary'

export type Glyph = '◆' | '▲' | '■' | '●'

export function clientMark(name: string): { glyph: Glyph; label: string } {
  const n = name.toLowerCase()
  if (n.includes('claude')) return { glyph: '◆', label: 'Claude Code' }
  if (n.includes('codex')) return { glyph: '▲', label: 'Codex' }
  if (n.includes('cursor')) return { glyph: '■', label: 'Cursor' }
  return { glyph: '●', label: name || '未知来源' }
}

export function age(fromMs: number, nowMs: number): string {
  const s = Math.max(0, Math.round((nowMs - fromMs) / 1000))
  if (s < 45) return '刚刚'
  const m = Math.round(s / 60)
  if (m < 60) return `${m} 分钟`
  const h = Math.floor(m / 60)
  if (h < 24) return `${h} 小时`
  return `${Math.floor(h / 24)} 天`
}

/** 棋子：mine 轮到你；idle 轮到你但 AI 没在等；returned 已交还；closed 已结束。 */
export type Stone = 'mine' | 'idle' | 'returned' | 'closed'

export interface Status {
  stone: Stone
  label: string
  tone: 'signal' | 'warn' | 'muted' | 'danger'
  detail: string | null
}

export function statusOf(r: RequestSummary): Status {
  switch (r.phase) {
    case 'waiting':
      return r.attached
        ? { stone: 'mine', label: '在等你', tone: 'signal', detail: null }
        : {
            stone: 'idle',
            label: '未在等待',
            tone: 'warn',
            detail: 'AI 当前没有在等这条回复。你仍然可以回复，AI 下次来取时会收到。',
          }
    case 'decided':
      switch (r.delivery) {
        case 'pending':
          return { stone: 'returned', label: '待领取', tone: 'warn', detail: '回复已保存，等 AI 来取。' }
        case 'in_flight':
          return { stone: 'returned', label: '交付中', tone: 'muted', detail: '正在交给 AI 的连接。' }
        case 'unknown':
          return {
            stone: 'returned',
            label: '交付未知',
            tone: 'warn',
            detail: '回复发出时连接断开，无法确认 AI 是否收到。AI 再次来取时会重新交付。',
          }
        default:
          return r.outcome === 'declined'
            ? { stone: 'returned', label: '已拒绝', tone: 'danger', detail: 'AI 收到了明确的拒绝。' }
            : {
                stone: 'returned',
                label: '已交付',
                tone: 'muted',
                detail: '回复已交给 AI 的连接。这不代表 AI 已经执行。',
              }
      }
    case 'cancelled':
      return { stone: 'closed', label: '已取消', tone: 'muted', detail: 'AI 那边取消了这次提问，回复不会再发出。' }
    case 'expired':
      return { stone: 'closed', label: '已过期', tone: 'muted', detail: '超过等待上限，没有向 AI 表示同意。' }
    case 'dismissed':
      return {
        stone: 'closed',
        label: '已关闭',
        tone: 'muted',
        detail: '你关闭了这条请求。AI 来取时会被告知没有回复。',
      }
  }
}

export function isOpen(r: RequestSummary): boolean {
  return r.phase === 'waiting'
}
