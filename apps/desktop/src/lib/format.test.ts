import { describe, expect, it } from 'vitest'
import type { RequestSummary } from '../generated/RequestSummary'
import { age, clientMark, statusOf } from './format'

const base: RequestSummary = {
  id: 'x',
  client_name: 'claude-code',
  client_version: '1',
  project: 'turndock',
  project_path: '/w/turndock',
  title: 't',
  tool: 'ask_user',
  phase: 'waiting',
  delivery: 'none',
  attached: true,
  outcome: null,
  has_draft: false,
  created_at: 0,
  updated_at: 0,
  deadline_at: 0,
}

describe('format', () => {
  it('识别常见宿主', () => {
    expect(clientMark('codex-mcp-client').label).toBe('Codex')
    expect(clientMark('claude-code').glyph).toBe('◆')
    expect(clientMark('zed').label).toBe('zed')
  })

  it('等待时长', () => {
    expect(age(0, 10_000)).toBe('刚刚')
    expect(age(0, 12 * 60_000)).toBe('12 分钟')
    expect(age(0, 3 * 3600_000)).toBe('3 小时')
  })

  it('交付未知不会显示为成功', () => {
    const s = statusOf({ ...base, phase: 'decided', delivery: 'unknown', attached: false })
    expect(s.label).toBe('交付未知')
    expect(s.tone).toBe('warn')
  })

  it('AI 未在等待时如实说明', () => {
    expect(statusOf({ ...base, attached: false }).stone).toBe('idle')
  })
})
