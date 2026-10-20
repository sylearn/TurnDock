import type { RequestDetail } from '../generated/RequestDetail'

const min = 60_000

function make(
  now: number,
  id: string,
  over: Partial<RequestDetail['summary']>,
  rest: Partial<RequestDetail>,
): RequestDetail {
  return {
    summary: {
      id,
      client_name: 'claude-code',
      client_version: '2.3.0',
      project: 'turndock',
      project_path: '/Users/me/code/turndock',
      title: '',
      tool: 'ask_user',
      phase: 'waiting',
      delivery: 'none',
      attached: true,
      outcome: null,
      has_draft: false,
      created_at: now - 12 * min,
      updated_at: now - 12 * min,
      deadline_at: now + 1000 * min,
      ...over,
    },
    heading: over.title ?? null,
    message_html: '',
    message_text: '',
    format: 'markdown',
    options: [],
    selection: 'none',
    allow_text: true,
    allow_images: true,
    decision: null,
    draft: null,
    ...rest,
  }
}

export function fixtures(now: number): RequestDetail[] {
  return [
    make(
      now,
      'a1',
      { title: '先确认第一版的实现范围', created_at: now - 12 * min },
      {
        message_html:
          '<p>协议内核与桌面外壳都已就绪。第一版先把<strong>回复闭环</strong>做稳，还是同时把多项目收件箱做完整？</p><ul><li>已完成：状态机、broker、MCP bridge，56 个测试通过</li><li>待定：草稿持久化、图片回复的体验细节</li></ul><pre><code class="language-rust">fn main() {\n    println!("回合");\n}\n</code></pre>',
        options: [
          { id: 'loop', label: '先做可靠的回复闭环', description: '接收、草稿、明确回复，每一步都能追踪' },
          { id: 'inbox', label: '同时把多项目收件箱做完整', description: '在闭环基础上加入队列、来源与快速面板' },
        ],
        selection: 'single',
      },
    ),
    make(
      now,
      'a2',
      {
        client_name: 'codex-mcp-client',
        project: 'atlas-export',
        title: '导出脚本已经写完，测试全部通过。接下来？',
        tool: 'zhi',
        attached: false,
        has_draft: true,
        created_at: now - 23 * min,
      },
      {
        message_html: '',
        options: [
          { id: '1', label: '继续' },
          { id: '2', label: '先写文档' },
          { id: '3', label: '提交代码' },
        ],
        selection: 'multiple',
        draft: { text: '顺便把 README 的示例更新一下', selected_ids: ['2'] },
      },
    ),
    make(
      now,
      'a3',
      { client_name: 'cursor-vscode', project: 'notes', title: '两组笔记有 37 处重复，要合并吗？', created_at: now - 64 * min },
      {
        message_html: '<p>其中 12 处内容完全相同，25 处只有标点差异。合并后保留较新的版本。</p>',
        options: [
          { id: 'y', label: '合并' },
          { id: 'n', label: '保留两份' },
        ],
        selection: 'single',
      },
    ),
    make(
      now,
      'd1',
      {
        title: '数据库迁移脚本可以执行吗？',
        phase: 'decided',
        delivery: 'handed_off',
        attached: false,
        outcome: 'answered',
        created_at: now - 180 * min,
        updated_at: now - 170 * min,
      },
      {
        message_html: '<p>迁移会新增两张表，不修改现有数据。</p>',
        decision: { outcome: 'answered', selected_ids: [], text: '可以，先在备份上跑一遍。', image_count: 0, decided_at: now - 170 * min },
      },
    ),
    make(
      now,
      'd2',
      {
        client_name: 'codex-mcp-client',
        project: 'atlas-export',
        title: '要删除旧的导出目录吗？',
        phase: 'decided',
        delivery: 'unknown',
        attached: false,
        outcome: 'declined',
        created_at: now - 300 * min,
        updated_at: now - 290 * min,
      },
      {
        message_html: '<p>旧目录占用 3.2GB。</p>',
        decision: { outcome: 'declined', selected_ids: [], text: '先别删，里面有客户的原始文件。', image_count: 0, decided_at: now - 290 * min },
      },
    ),
  ]
}
