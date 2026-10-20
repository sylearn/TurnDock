# TurnDock 工作约定

- 默认用中文沟通、注释和文档。简洁，区分已实现、已验证和计划。
- 先读 README 和当前阶段对应的文档；按 docs/05 的关卡推进，不擅自加入云同步、代码索引或模型调用。
- 请求生命周期只在 `turndock-broker` 的协调器 actor 里；前端只展示快照、发起动作，不维护第二套状态。
- 任何超时、沉默、断线、取消、关闭都不能被当作用户同意，文案和返回给 AI 的文字都要直说。
- 调用方内容只经 `turndock-broker::markdown` 渲染与净化；前端不得引入第二个 Markdown 渲染器，不得对未净化内容使用 `v-html`。
- 新增 Tauri 命令要同时加入 `build.rs` 的 COMMANDS 与 `capabilities/default.json`。
- Rust 类型变化后运行 `pnpm gen:types`；不要手改 `src/generated`。
- 依赖精确锁定，不用浮动 Git 依赖；不在日志里写正文、图片或密钥。
- 改动带测试：状态机 → core 单元测试；协调器 → broker `tests/ipc.rs`；工具行为 → bridge `tests/e2e.rs`；前端纯逻辑 → Vitest。
- 不修改用户正在使用的寸止、MCP 客户端配置或其他仓库，除非明确授权。
