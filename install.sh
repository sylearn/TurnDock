#!/usr/bin/env bash
# 回合（TurnDock）安装与更新。再次运行即更新到最新版本。
#
#   curl -fsSL https://github.com/sylearn/TurnDock/releases/latest/download/install.sh | bash
#
# 把 TurnDock.app 装进「应用程序」，并在 /usr/local/bin 放一个指向应用内的 turndock 命令：
# 终端输入 turndock 打开窗口；AI 客户端用 `turndock mcp` 连接。最后询问是否接入本机的 AI 客户端（确认后才执行）。
#
# 环境变量：
#   TURNDOCK_VERSION=0.1.0     安装指定版本（默认最新）
#   TURNDOCK_BIN_DIR=DIR       命令安装位置（默认 /usr/local/bin）
#   TURNDOCK_APP_DIR=DIR       应用安装位置（默认 /Applications）
#   TURNDOCK_FROM=DIR          从本地目录安装（发布前测试用）
#   TURNDOCK_CONNECT=ask|all|none   接入 AI 客户端：询问（默认）/ 全部 / 跳过
#   TURNDOCK_NO_OPEN=1         安装后不打开应用
# 卸载：curl -fsSL …/install.sh | bash -s -- --uninstall
set -euo pipefail

REPO="${TURNDOCK_REPO:-sylearn/TurnDock}"
VERSION="${TURNDOCK_VERSION:-latest}"
APP_NAME="TurnDock.app"
COMMAND="turndock"
SERVER="turndock"
BUNDLE_ID="app.turndock.desktop"

bold=$'\033[1m'; dim=$'\033[2m'; green=$'\033[32m'; red=$'\033[31m'; reset=$'\033[0m'
[[ -t 1 ]] || { bold=""; dim=""; green=""; red=""; reset=""; }
say()  { printf '%s\n' "$*"; }
step() { printf '%s›%s %s\n' "$bold" "$reset" "$*"; }
ok()   { printf '%s✓%s %s\n' "$green" "$reset" "$*"; }
die()  { printf '%s✗ %s%s\n' "$red" "$*" "$reset" >&2; exit 1; }

# 通过管道运行时 stdin 是脚本本身，交互只能走 /dev/tty。
has_tty() { [[ -r /dev/tty && -w /dev/tty ]] && { : </dev/tty; } 2>/dev/null; }
confirm() {
  local answer
  has_tty || return 1
  printf '%s [Y/n] ' "$1" >/dev/tty
  read -r answer </dev/tty || return 1
  [[ -z "$answer" || "$answer" =~ ^[Yy] ]]
}

[[ "$(uname -s)" == "Darwin" ]] || die "回合目前只支持 macOS"
major="$(sw_vers -productVersion | cut -d. -f1)"
(( major >= 13 )) || die "需要 macOS 13 或更高版本（当前 $(sw_vers -productVersion)）"

app_dir() {
  if [[ -n "${TURNDOCK_APP_DIR:-}" ]]; then echo "$TURNDOCK_APP_DIR"
  elif [[ -d "/Applications/$APP_NAME" || -w /Applications ]]; then echo /Applications
  else echo "$HOME/Applications"
  fi
}
APP_DIR="$(app_dir)"
APP="$APP_DIR/$APP_NAME"
BRIDGE="$APP/Contents/MacOS/$COMMAND"
BIN_DIR="${TURNDOCK_BIN_DIR:-/usr/local/bin}"

quit_app() {
  if pgrep -f "$APP_NAME/Contents/MacOS/turndock-desktop" >/dev/null 2>&1; then
    step "退出正在运行的回合（等待中的 AI 会自动重连）"
    osascript -e "tell application id \"$BUNDLE_ID\" to quit" >/dev/null 2>&1 || true
    for _ in $(seq 1 50); do
      pgrep -f "$APP_NAME/Contents/MacOS/turndock-desktop" >/dev/null 2>&1 || return 0
      sleep 0.1
    done
    pkill -f "$APP_NAME/Contents/MacOS/turndock-desktop" 2>/dev/null || true
  fi
}

# 需要时用 sudo；没有终端可交互时返回失败，由调用方改用用户目录。
as_needed() {
  if "$@" 2>/dev/null; then return 0; fi
  has_tty || return 1
  say "${dim}需要管理员权限：$*${reset}"
  sudo "$@" </dev/tty
}

uninstall() {
  quit_app
  for dir in "$BIN_DIR" "$HOME/.local/bin"; do
    if [[ -L "$dir/$COMMAND" ]]; then as_needed rm -f "$dir/$COMMAND" && ok "已删除命令 $dir/$COMMAND"; fi
  done
  if [[ -d "$APP" ]]; then rm -rf "$APP" && ok "已删除 $APP"; fi
  say ""
  say "你的数据仍在 ~/Library/Application Support/TurnDock，需要时手动删除。"
  say "AI 客户端里的接入配置不会被改动，可以分别运行："
  say "  claude mcp remove --scope user $SERVER"
  say "  codex mcp remove $SERVER"
  exit 0
}
[[ "${1:-}" == "--uninstall" ]] && uninstall

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fetch() { # fetch <资源名>
  local name="$1"
  if [[ -n "${TURNDOCK_FROM:-}" ]]; then
    cp "$TURNDOCK_FROM/$name" "$TMP/$name"
    return
  fi
  local url
  if [[ "$VERSION" == "latest" ]]; then
    url="https://github.com/$REPO/releases/latest/download/$name"
  else
    url="https://github.com/$REPO/releases/download/v${VERSION#v}/$name"
  fi
  if curl -fsSL --retry 3 -o "$TMP/$name" "$url"; then return; fi
  # 私有仓库：用已登录的 gh 下载。
  if command -v gh >/dev/null 2>&1; then
    local tag=()
    [[ "$VERSION" == "latest" ]] || tag=("v${VERSION#v}")
    gh release download ${tag[@]+"${tag[@]}"} -R "$REPO" -p "$name" -D "$TMP" --clobber >/dev/null 2>&1 && return
  fi
  die "下载失败：$url"
}

if [[ "$VERSION" == "latest" ]]; then step "下载最新版回合"; else step "下载回合 $VERSION"; fi
fetch TurnDock.app.tar.gz
fetch SHA256SUMS
(cd "$TMP" && grep ' TurnDock.app.tar.gz$' SHA256SUMS | shasum -a 256 -c - >/dev/null) \
  || die "校验和不一致，已停止安装"
ok "已下载并校验"

mkdir -p "$TMP/x"
tar -xzf "$TMP/TurnDock.app.tar.gz" -C "$TMP/x"
[[ -x "$TMP/x/$APP_NAME/Contents/MacOS/$COMMAND" ]] || die "安装包不完整：缺少 $COMMAND"
NEW_VERSION="$("$TMP/x/$APP_NAME/Contents/MacOS/$COMMAND" --version | awk '{print $2}')"

quit_app
step "安装到 $APP"
mkdir -p "$APP_DIR"
if [[ -d "$APP" ]]; then
  OLD_VERSION="$("$BRIDGE" --version 2>/dev/null | awk '{print $2}' || true)"
  rm -rf "$APP"
fi
ditto "$TMP/x/$APP_NAME" "$APP"
# 通过 curl 下载的文件没有隔离标记；保险起见再清一次，避免「已损坏」提示。
xattr -dr com.apple.quarantine "$APP" 2>/dev/null || true
if [[ -n "${OLD_VERSION:-}" && "$OLD_VERSION" != "$NEW_VERSION" ]]; then
  ok "已从 $OLD_VERSION 更新到 $NEW_VERSION"
else
  ok "已安装 $NEW_VERSION"
fi

step "安装终端命令 $COMMAND"
link_into() {
  local dir="$1"
  { [[ -d "$dir" ]] || as_needed mkdir -p "$dir"; } && as_needed ln -sfn "$BRIDGE" "$dir/$COMMAND"
}
if link_into "$BIN_DIR"; then
  LINK="$BIN_DIR/$COMMAND"
elif [[ -z "${TURNDOCK_BIN_DIR:-}" ]] && link_into "$HOME/.local/bin"; then
  LINK="$HOME/.local/bin/$COMMAND"
  case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *)
    say "${dim}~/.local/bin 不在 PATH 中。把这一行加到 ~/.zshrc：export PATH=\"\$HOME/.local/bin:\$PATH\"${reset}" ;;
  esac
else
  die "无法安装命令到 $BIN_DIR"
fi
ok "${LINK}（指向应用内，随应用更新）"

[[ -n "${TURNDOCK_NO_OPEN:-}" ]] || open -g "$APP" || true

# ---------- 接入 AI 客户端 ----------
CONNECT="${TURNDOCK_CONNECT:-ask}"
connected_any=0
offer() { # offer <名称> <检查命令…> -- <接入命令…>
  local name="$1"; shift
  local check=() add=()
  while [[ "$1" != "--" ]]; do check+=("$1"); shift; done; shift
  add=("$@")
  command -v "${add[0]}" >/dev/null 2>&1 || return 0
  if "${check[@]}" >/dev/null 2>&1; then ok "$name 已接入"; connected_any=1; return 0; fi
  case "$CONNECT" in
    none) return 0 ;;
    ask) confirm "接入 ${name}？将运行：${add[*]}" || { say "${dim}已跳过 $name${reset}"; return 0; } ;;
  esac
  if "${add[@]}" >/dev/null; then ok "已接入 $name"; connected_any=1; else say "${red}接入 $name 失败，可以稍后手动运行：${add[*]}${reset}"; fi
}
say ""
offer "Claude Code" claude mcp get "$SERVER" -- claude mcp add --scope user "$SERVER" -- "$COMMAND" mcp
offer "Codex" codex mcp get "$SERVER" -- codex mcp add "$SERVER" -- "$COMMAND" mcp

say ""
say "${bold}回合已就绪。${reset}在终端输入 ${bold}turndock${reset} 即可打开。"
say ""
if (( connected_any == 0 )); then
  say "把它接入你的 AI 客户端（任选其一）："
else
  say "接入其他 AI 客户端："
fi
say "  Claude Code   claude mcp add --scope user $SERVER -- $COMMAND mcp"
say "  Codex         codex mcp add $SERVER -- $COMMAND mcp"
say "  其他客户端    {\"mcpServers\": {\"$SERVER\": {\"command\": \"$COMMAND\", \"args\": [\"mcp\"]}}}"
say ""
say "${dim}接入后重启 AI 会话即可使用。以后再次运行本命令即可更新；应用内也会提示新版本。${reset}"
