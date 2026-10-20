#!/usr/bin/env bash
# 构建发布产物到 release/：universal 应用包、dmg、更新包与签名、latest.json、install.sh、SHA256SUMS。
# 本地与 CI 共用。需要 TAURI_SIGNING_PRIVATE_KEY（内容或路径）；密码为空时可不设 TAURI_SIGNING_PRIVATE_KEY_PASSWORD。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
REPO="${TURNDOCK_REPO:-sylearn/TurnDock}"
TARGET=universal-apple-darwin

VERSION="$(node -p "require('./apps/desktop/src-tauri/tauri.conf.json').version")"
PKG_VERSION="$(node -p "require('./apps/desktop/package.json').version")"
CARGO_VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
if [[ "$VERSION" != "$PKG_VERSION" || "$VERSION" != "$CARGO_VERSION" ]]; then
  echo "版本号不一致：tauri.conf.json=$VERSION package.json=$PKG_VERSION Cargo.toml=$CARGO_VERSION" >&2
  exit 1
fi
if [[ -n "${GITHUB_REF_NAME:-}" && "$GITHUB_REF_NAME" != "v$VERSION" ]]; then
  echo "标签 $GITHUB_REF_NAME 与版本 v$VERSION 不一致" >&2
  exit 1
fi
if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
  echo "缺少 TAURI_SIGNING_PRIVATE_KEY，无法签名更新包" >&2
  exit 1
fi
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"

rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null

(
  cd apps/desktop
  pnpm install --frozen-lockfile
  pnpm tauri build --target "$TARGET" --config src-tauri/tauri.release.conf.json
)

BUNDLE="target/$TARGET/release/bundle"
OUT="release"
rm -rf "$OUT"
mkdir -p "$OUT"

APP_TGZ="$BUNDLE/macos/TurnDock.app.tar.gz"
cp "$APP_TGZ" "$OUT/TurnDock.app.tar.gz"
cp "$APP_TGZ.sig" "$OUT/TurnDock.app.tar.gz.sig"
cp "$BUNDLE"/dmg/*.dmg "$OUT/TurnDock_${VERSION}_universal.dmg"
cp install.sh "$OUT/install.sh"

# 包内必须带着 turndock 命令，否则命令链接会指向不存在的文件。
tar -tzf "$OUT/TurnDock.app.tar.gz" | grep -q 'TurnDock.app/Contents/MacOS/turndock$' || {
  echo "应用包里没有 turndock 命令" >&2
  exit 1
}

# 更新说明取 CHANGELOG.md 中本版本的一节。
NOTES="$(awk -v v="## v$VERSION" '$0==v{f=1;next} /^## v/{f=0} f' CHANGELOG.md | sed '/./,$!d')"
URL="https://github.com/$REPO/releases/download/v$VERSION/TurnDock.app.tar.gz"
SIG="$(cat "$OUT/TurnDock.app.tar.gz.sig")"
NOTES="$NOTES" URL="$URL" SIG="$SIG" VERSION="$VERSION" node -e '
  const { NOTES, URL, SIG, VERSION } = process.env
  const platform = { signature: SIG, url: URL }
  const manifest = {
    version: VERSION,
    notes: NOTES,
    pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"),
    platforms: { "darwin-aarch64": platform, "darwin-x86_64": platform },
  }
  require("fs").writeFileSync("release/latest.json", JSON.stringify(manifest, null, 2) + "\n")
'
printf '%s\n' "$NOTES" > "$OUT/NOTES.md"

scripts/cask.sh "$VERSION" "$OUT/TurnDock.app.tar.gz"

(cd "$OUT" && shasum -a 256 TurnDock.app.tar.gz "TurnDock_${VERSION}_universal.dmg" install.sh > SHA256SUMS)
ls -lh "$OUT"
