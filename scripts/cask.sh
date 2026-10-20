#!/usr/bin/env bash
# 生成 Homebrew cask：scripts/cask.sh <版本> <TurnDock.app.tar.gz>
# 仓库本身就是 tap：brew tap sylearn/turndock https://github.com/sylearn/TurnDock
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$1"
SHA="$(shasum -a 256 "$2" | awk '{print $1}')"
REPO="${TURNDOCK_REPO:-sylearn/TurnDock}"
mkdir -p "$ROOT/Casks"
sed -e "s/@VERSION@/$VERSION/" -e "s/@SHA256@/$SHA/" -e "s#@REPO@#$REPO#g" \
  "$ROOT/scripts/turndock.rb.in" > "$ROOT/Casks/turndock.rb"
