#!/usr/bin/env bash
# mojiroku-llm（ローカル要約 sidecar, ADR-0007）をビルドし、
# Tauri externalBin が要求する triple 付き名で src-tauri/binaries/ に配置する。
# あわせて mojiroku-mcp（ローカル MCP サーバ, ADR-0010）もビルドする。
# `tauri dev` / `tauri build` の前に実行すること（justfile の dev/build から呼ばれる）。
set -euo pipefail

cd "$(dirname "$0")/.."
TRIPLE="$(rustc -vV | sed -n 's/host: //p')"

cargo build --release -p mojiroku-llm
mkdir -p src-tauri/binaries
cp "target/release/mojiroku-llm" "src-tauri/binaries/mojiroku-llm-${TRIPLE}"
chmod +x "src-tauri/binaries/mojiroku-llm-${TRIPLE}"
echo "placed: src-tauri/binaries/mojiroku-llm-${TRIPLE}"

# mojiroku-mcp は MCP クライアント（Claude Desktop/Code）が spawn するため、
# Tauri externalBin には登録しない（アプリは spawn しない）。ビルドだけ行い、
# 利用者は target/release/mojiroku-mcp を MCP 設定の command に指定する（docs/06_reference/mcp.md）。
cargo build --release -p mojiroku-mcp
echo "built:  target/release/mojiroku-mcp  (MCP 設定の command に指定。docs/06_reference/mcp.md 参照)"
