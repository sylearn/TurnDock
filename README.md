# 回合 · TurnDock

你和 AI 之间的交接点。AI 每做完一轮工作，就把成果、验证情况和建议的下一步交给你：你一眼看懂，按一下 ⌘↵ 接受推荐，或者换个方向，然后顺着往下做。多个项目、多个 AI 客户端的交接汇在一处。

源于对 [寸止](https://github.com/imhuso/cunzhi) 的使用与审查，独立重写，兼容它的 `zhi` 工具。

## 安装

macOS 13 及以上，Apple 芯片与 Intel 通用。任选一种：

```sh
# 一行命令（再次运行即更新）
curl -fsSL https://github.com/sylearn/TurnDock/releases/latest/download/install.sh | bash

# 或 Homebrew
brew tap sylearn/turndock https://github.com/sylearn/TurnDock && brew install --cask turndock
```

装好之后，终端输入 `turndock` 就会打开回合。

> 预览期仓库尚未公开：已登录 `gh` 的协作者可以用 `gh release download -R sylearn/TurnDock -p install.sh -O - | bash` 安装。

## 接入 AI 客户端

每台电脑、每个用户都是同一行命令：

```sh
claude mcp add --scope user turndock -- turndock mcp   # Claude Code
codex mcp add turndock -- turndock mcp                  # Codex
```

Cursor 和其他客户端使用这段配置；回合「设置 → 接入」里也可以一键接入：

```json
{ "mcpServers": { "turndock": { "command": "turndock", "args": ["mcp"] } } }
```

## 更新与卸载

- 应用启动时检查新版本（可在「设置 → 关于」关闭），下载后验证签名再安装；`turndock` 命令在应用包里，随之更新。
- 也可以再次运行安装命令，或 `brew upgrade --cask turndock`。
- 卸载：`curl -fsSL https://github.com/sylearn/TurnDock/releases/latest/download/install.sh | bash -s -- --uninstall`。数据在 `~/Library/Application Support/TurnDock`，需要时手动删除。

## 当前状态

| 部分 | 状态 |
|---|---|
| 协议内核（状态机、broker、MCP bridge） | 已实现，有单元、集成与端到端测试 |
| 桌面应用（收件箱、快速回复面板、托盘、通知、全局快捷键） | 已实现，开发模式可运行 |
| 回合交接（`hand_off`）与回合卡界面 | 已实现 |
| 草稿持久化、图片回复、代码高亮 | 已实现（图片暂不随草稿持久化） |
| 一行安装、Homebrew、应用内更新（签名校验） | 已实现；尚未做 Apple 公证 |
| 迁移向导、真实宿主超时矩阵 | 未做，见路线 M0 第 6 项与 M4 |

## 它怎么工作

```text
AI 客户端 ──stdio── turndock mcp ──Unix socket── TurnDock.app（broker + 界面）
```

- 四个工具：`hand_off`（每轮结束的交接：标题、做了什么、验证、风险、建议的下一步）、`ask_user`（中途决策）、`await_reply`（续等）、`zhi`（寸止兼容）。
- 回合不调用任何模型；交接的质量来自工具协议和给 AI 的规则片段。
- 宿主会给工具调用设超时（Codex 默认 60 秒）。回合在超时前返回 `pending` 和 request_id，AI 用 `await_reply` 继续等。请求和你的回复都不会因此丢失。
- 桌面应用重启时，等待中的调用会自动重连续等。
- 沉默、超时、取消、关闭都不会被当作同意。

## 开发

需要 Rust 1.99、Node 24、pnpm 10。

```sh
cargo test --workspace                 # Rust 全部测试
cargo build -p turndock-bridge         # 构建 turndock 命令

cd apps/desktop
pnpm install
pnpm gen:types                         # 从 Rust 导出前端类型
pnpm test && pnpm typecheck
pnpm tauri dev                         # 运行桌面应用
```

- 不想动真实数据目录时，加 `TURNDOCK_HOME=/tmp/td-dev`。
- 设计审查：`pnpm dev` 后打开 `http://127.0.0.1:5199/preview.html`，可加 `#quick` 和 `?theme=dark`。用的是示例数据，不连接任何 AI。
- 无界面联调：`cargo run -p turndock-broker` 在终端里查看并回复请求。

## 发布

改版本号（`Cargo.toml`、`tauri.conf.json`、`package.json` 三处一致）并写 `CHANGELOG.md`，然后推送 `v<版本>` 标签。GitHub Actions 会构建 universal 应用、签名更新包、生成 `latest.json` 与 Homebrew cask，并创建 Release。本地可用 `TAURI_SIGNING_PRIVATE_KEY=~/.tauri/turndock.key scripts/release.sh` 产出同样的文件到 `release/`。

## 文档

1. [产品定义](docs/01-产品定义.md)：为什么定位为交接点、三类请求、与宿主自带提问的区别、原则
2. [体验设计](docs/02-体验设计.md)：三层入口、回合卡、键盘、版式、视觉、文案
3. [技术架构](docs/03-技术架构.md)：进程结构、选型、安全边界
4. [协议与状态](docs/04-协议与状态.md)：工具、续等、状态机、IPC、存储
5. [路线与验收](docs/05-路线与验收.md)：里程碑、性能目标、测试清单
6. [寸止审查](docs/06-寸止审查.md)：上游问题实测与对第一版方案的审查
7. [进度记录](docs/07-进度记录.md)：已实现、已验证、未完成

## 许可

MIT。
