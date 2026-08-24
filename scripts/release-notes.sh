#!/bin/bash
# 指定した git range の非マージコミットから、リリースノートの本文を組む。
#
# なぜスクリプトに出すか: リリース用 Release は別 repo（daichi8120/mojiroku-releases）に
# 作られるため、`gh release create --generate-notes` は使えない（生成元がリリース用 repo の
# 履歴になる）。ソース repo 側で組む必要がある。
# ワークフローの yml に直接書くと手元で試せず、壊れたときの再試行が高くつくので分離した。
#
# 使い方:
#   scripts/release-notes.sh <git-range>
#   例: scripts/release-notes.sh HEAD^..HEAD      # main への merge commit で「今回入った分」
#       scripts/release-notes.sh origin/main..origin/develop  # 事前プレビュー
#
# 分類は Conventional Commits の型で行う（docs/CONTRIBUTING.md）。
set -euo pipefail

RANGE="${1:?usage: release-notes.sh <git-range>}"

subjects=$(git log --no-merges --pretty=format:'%s' "$RANGE")

if [ -z "$subjects" ]; then
  # 空を黙って返すと「変更なし」に見えるが、実際は range の取り違えであることが多い。
  echo "（このリリースに含まれる変更を取得できませんでした）"
  exit 0
fi

# `feat:` `feat(core):` は拾い、`feature-flag` のような別語は拾わないよう区切り文字まで見る。
feats=$(printf '%s\n' "$subjects" | grep -E '^feat[(:]' || true)
fixes=$(printf '%s\n' "$subjects" | grep -E '^fix[(:]' || true)
others=$(printf '%s\n' "$subjects" | grep -Ev '^(feat|fix)[(:]' || true)

emit_section() {
  local heading="$1" lines="$2"
  [ -z "$lines" ] && return 0
  printf '### %s\n' "$heading"
  printf '%s\n' "$lines" | sed 's/^/- /'
  printf '\n'
}

emit_section "新機能" "$feats"
emit_section "修正" "$fixes"
emit_section "その他" "$others"
