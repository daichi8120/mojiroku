#!/usr/bin/env bash
# cargo の runner。tauri dev（= cargo run）がビルドした dev バイナリを、安定した
# identity で署名してから実行する。これにより再ビルド毎に macOS の TCC 許可
# （会議モードのマイク/画面収録など）がリセットされ再許可を求められる問題を防ぐ
# （理由とセットアップは scripts/dev-codesign-setup.sh）。
#
# identity の選び方（ADR-0022）:
#   1. 環境変数 MOJIROKU_DEV_SIGN_IDENTITY（最優先）
#   2. repo ルートの .mojiroku-dev-sign.env（gitignore 済みのマシンローカル設定。
#      中身は MOJIROKU_DEV_SIGN_IDENTITY="Apple Development: ... (XXXXXXXXXX)" の1行）
#   3. 既定の自己署名 mojiroku-dev（dev-codesign-setup.sh で作成。フォールバック）
#   Apple Development 証明書は Team ID 由来の安定した designated requirement を持ち、
#   identity 切替時・年次失効での再作成時に TCC を一度だけ再許可すれば以後は安定する。
#
# 配線: .cargo/config.toml の [target.aarch64-apple-darwin] runner に登録。cargo は
#       バイナリ実行の直前に `dev-sign-run.sh <binary> <args...>` の形で呼ぶ。runner なので
#       launcher（just dev / npm run dev / cargo run）に関係なく効く。
set -euo pipefail

BIN="$1"; shift

# マシンローカル設定（あれば）。env 直接指定が優先。
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ -z "${MOJIROKU_DEV_SIGN_IDENTITY:-}" ] && [ -f "$REPO_ROOT/.mojiroku-dev-sign.env" ]; then
  # shellcheck disable=SC1091
  source "$REPO_ROOT/.mojiroku-dev-sign.env"
fi

IDENTITY="${MOJIROKU_DEV_SIGN_IDENTITY:-mojiroku-dev}"
BUNDLE_ID="com.daichi0812.mojiroku"

# 証明書が無ければ署名せず素通し（証明書未作成のマシンや CI で壊さない）。
# Apple 発行の identity は find-identity に出る。自己署名（未信頼）は出ないので
# find-certificate にフォールバックして判定する。
if security find-identity -v -p codesigning 2>/dev/null | grep -Fq "$IDENTITY" \
   || security find-certificate -c "$IDENTITY" >/dev/null 2>&1; then
  codesign --force --sign "$IDENTITY" --identifier "$BUNDLE_ID" "$BIN" >/dev/null 2>&1 \
    || echo "[dev-sign-run] 署名に失敗（未署名のまま続行）: $BIN" >&2
fi

exec "$BIN" "$@"
