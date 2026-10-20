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
    checks: [],
    risks: [],
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
      { title: '导出模块完成，CSV 与 JSON 均可用', tool: 'hand_off', project: 'atlas-export', created_at: now - 3 * min },
      {
        message_html: "<p>新增导出模块，支持 CSV 与 JSON 两种格式，入口在 <code>src/export.rs</code>。</p>\n<ul>\n<li>大于 50MB 的数据改为流式写出，内存占用不随数据量增长</li>\n<li>列名映射改为读取 <code>export.toml</code>，旧的硬编码映射已删除</li>\n<li><code>cargo test</code> 新增 12 个用例</li>\n</ul>\n<pre data-lang=\"rust\"><code class=\"language-rust\"><span class=\"hl-source hl-rust\"><span class=\"hl-meta hl-function hl-rust\"><span class=\"hl-meta hl-function hl-rust\"><span class=\"hl-storage hl-modifier hl-rust\">pub</span> <span class=\"hl-storage hl-type hl-function hl-rust\">fn</span> </span><span class=\"hl-entity hl-name hl-function hl-rust\">export</span></span><span class=\"hl-meta hl-function hl-rust\"><span class=\"hl-meta hl-function hl-parameters hl-rust\"><span class=\"hl-punctuation hl-section hl-parameters hl-begin hl-rust\">(</span><span class=\"hl-variable hl-parameter hl-rust\">rows</span><span class=\"hl-punctuation hl-separator hl-rust\">:</span> impl <span class=\"hl-meta hl-generic hl-rust\">Iterator<span class=\"hl-punctuation hl-definition hl-generic hl-begin hl-rust\">&lt;</span>Item = Row<span class=\"hl-punctuation hl-definition hl-generic hl-end hl-rust\">&gt;</span></span>, <span class=\"hl-variable hl-parameter hl-rust\">format</span><span class=\"hl-punctuation hl-separator hl-rust\">:</span> Format, <span class=\"hl-variable hl-parameter hl-rust\">out</span><span class=\"hl-punctuation hl-separator hl-rust\">:</span> <span class=\"hl-keyword hl-operator hl-rust\">&amp;</span><span class=\"hl-storage hl-modifier hl-rust\">mut</span> dyn Write</span><span class=\"hl-meta hl-function hl-rust\"><span class=\"hl-meta hl-function hl-parameters hl-rust\"><span class=\"hl-punctuation hl-section hl-parameters hl-end hl-rust\">)</span></span></span></span><span class=\"hl-meta hl-function hl-rust\"> <span class=\"hl-meta hl-function hl-return-type hl-rust\"><span class=\"hl-punctuation hl-separator hl-rust\">-&gt;</span> <span class=\"hl-meta hl-generic hl-rust\">Result<span class=\"hl-punctuation hl-definition hl-generic hl-begin hl-rust\">&lt;</span><span class=\"hl-storage hl-type hl-rust\">usize</span><span class=\"hl-punctuation hl-definition hl-generic hl-end hl-rust\">&gt;</span></span></span> </span><span class=\"hl-meta hl-function hl-rust\"><span class=\"hl-meta hl-block hl-rust\"><span class=\"hl-punctuation hl-section hl-block hl-begin hl-rust\">{</span>\n    <span class=\"hl-storage hl-type hl-rust\">let</span> <span class=\"hl-storage hl-modifier hl-rust\">mut</span> n <span class=\"hl-keyword hl-operator hl-rust\">=</span> <span class=\"hl-constant hl-numeric hl-integer hl-decimal hl-rust\">0</span><span class=\"hl-punctuation hl-terminator hl-rust\">;</span>\n    <span class=\"hl-keyword hl-control hl-rust\">for</span> row <span class=\"hl-keyword hl-operator hl-rust\">in</span> rows <span class=\"hl-meta hl-block hl-rust\"><span class=\"hl-punctuation hl-section hl-block hl-begin hl-rust\">{</span>\n        format.<span class=\"hl-support hl-function hl-rust\">write_row</span><span class=\"hl-meta hl-group hl-rust\"><span class=\"hl-punctuation hl-section hl-group hl-begin hl-rust\">(</span>out<span class=\"hl-punctuation hl-separator hl-rust\">,</span> <span class=\"hl-keyword hl-operator hl-rust\">&amp;</span>row</span><span class=\"hl-meta hl-group hl-rust\"><span class=\"hl-punctuation hl-section hl-group hl-end hl-rust\">)</span></span><span class=\"hl-keyword hl-operator hl-rust\">?</span><span class=\"hl-punctuation hl-terminator hl-rust\">;</span> <span class=\"hl-comment hl-line hl-double-slash hl-rust\"><span class=\"hl-punctuation hl-definition hl-comment hl-rust\">//</span> 逐行写出，不缓存全部数据\n</span>        n <span class=\"hl-keyword hl-operator hl-rust\">+</span><span class=\"hl-keyword hl-operator hl-rust\">=</span> <span class=\"hl-constant hl-numeric hl-integer hl-decimal hl-rust\">1</span><span class=\"hl-punctuation hl-terminator hl-rust\">;</span>\n    </span><span class=\"hl-meta hl-block hl-rust\"><span class=\"hl-punctuation hl-section hl-block hl-end hl-rust\">}</span></span>\n    <span class=\"hl-support hl-type hl-rust\">Ok</span><span class=\"hl-meta hl-group hl-rust\"><span class=\"hl-punctuation hl-section hl-group hl-begin hl-rust\">(</span>n</span><span class=\"hl-meta hl-group hl-rust\"><span class=\"hl-punctuation hl-section hl-group hl-end hl-rust\">)</span></span>\n</span><span class=\"hl-meta hl-block hl-rust\"><span class=\"hl-punctuation hl-section hl-block hl-end hl-rust\">}</span></span></span>\n</span></code></pre>",
        checks: [
          { label: '单元测试', status: 'passed', detail: '54/54 通过' },
          { label: '类型检查', status: 'passed' },
          { label: '1GB 以上大文件', status: 'not_run' },
          { label: 'Excel 打开中文 CSV', status: 'manual', detail: '需要你在 Windows 上确认' },
        ],
        risks: ['CSV 默认 UTF-8 无 BOM，旧版 Excel 可能乱码', '流式写出时中途失败会留下半个文件'],
        options: [
          { id: 'bom', label: '给 CSV 加 BOM 并补一个 Excel 兼容测试', description: '唯一需要人工确认的风险，改动小，十分钟内可完成', recommended: true },
          { id: 'atomic', label: '改为先写临时文件再原子重命名', description: '避免半个文件；需要改动写出流程，约半小时', recommended: false },
          { id: 'bigfile', label: '补 1GB 大文件压测', description: '验证流式写出的内存表现，运行较慢', recommended: false },
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
          { id: '1', label: '继续', recommended: false },
          { id: '2', label: '先写文档', recommended: false },
          { id: '3', label: '提交代码', recommended: false },
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
          { id: 'y', label: '合并', recommended: false },
          { id: 'n', label: '保留两份', recommended: false },
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
