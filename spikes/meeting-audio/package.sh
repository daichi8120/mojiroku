#!/usr/bin/env bash
# Phase 7 会議モード スパイクを ad-hoc 署名の .app バンドルにパッケージする。
#   - 毎回 build_tag を更新 → バイナリ(cdhash)が変わる（再ビルド→TCC失効サイクルの計測用）
#   - clean deep ad-hoc 署名（electron-builder 型の壊れた seal によるサイレント無効化を回避）
#   - 出力: dist/MojirokuSpike.app
# 使い方: bash package.sh
set -euo pipefail
cd "$(dirname "$0")"

APP="dist/MojirokuSpike.app"
BIN_NAME="MojirokuSpike"

# 1) cdhash を毎回変えるためビルドタグを更新（include_str! 依存なので cargo が再コンパイル）。
TAG="build-$(date +%Y%m%d-%H%M%S)"
echo "$TAG" > src/build_tag.txt
echo "[package] build_tag = $TAG"

# 2) release ビルド
cargo build --release
echo "[package] cargo build OK"

# 3) .app 構造を組み立て
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp Info.plist "$APP/Contents/Info.plist"
cp target/release/meeting-audio-spike "$APP/Contents/MacOS/$BIN_NAME"
chmod +x "$APP/Contents/MacOS/$BIN_NAME"

# 4) clean deep ad-hoc 署名（"-" = ad-hoc）。--force で既存 seal を作り直す。
codesign --remove-signature "$APP" 2>/dev/null || true
codesign --force --deep --sign - "$APP"
echo "[package] ad-hoc 署名 OK:"
codesign -dvv "$APP" 2>&1 | grep -E "Identifier|Signature|CDHash|flags" || true

echo ""
echo "==================================================================="
echo " 完成: $(pwd)/$APP"
echo "-------------------------------------------------------------------"
echo " 権威ある TCC 計測は bundle ID で起動するため 'open' を使う:"
echo ""
echo "   open '$(pwd)/$APP' --args 25"
echo ""
echo " → 25秒キャプチャ。何か音(音楽/通話)を鳴らしておく。"
echo "   結果ログ: ~/Desktop/mojiroku-spike-log.txt"
echo "   録音WAV : ~/Desktop/mojiroku-spike-capture.wav  (afplay で再生)"
echo ""
echo " ※ ターミナル直起動(下記)は責任プロセスが Terminal 側になり得るので"
echo "    TCC の帰属計測には使わない（コード動作の簡易確認用）:"
echo "   ./$APP/Contents/MacOS/$BIN_NAME 15"
echo "==================================================================="
