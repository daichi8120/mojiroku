// ポリシー本文の最小限の装飾（`**強調**` と `` `コード` ``）を HTML にする。
//
// なぜ要るか: プライバシーポリシーは平文だと読みにくく、「議事録は通りません」のような
// 打ち消しの一文が埋もれる。かといって本文に生の HTML を書くと、コピーを直す人が
// タグを壊しやすい。記法は 2 つだけに絞り、それ以外は素通しする。
//
// ⚠️ **必ず先にエスケープしてから**変換する。順序を逆にすると本文に書いた `<script>` が
//    そのまま出る。呼び出し側は set:html で使うので、ここが唯一の防波堤になる。
//
// なぜ .ts ではなく .js なのか（2026-08-29 のレビュー指摘）:
//   同じ実装を scripts/test-inline-markup.mjs から Node で直接 import してテストする。
//   .ts のままだと TypeScript をネイティブに読めない Node では ERR_UNKNOWN_FILE_EXTENSION に
//   なり、build に繋いだ検査ごと落ちる。型は JSDoc で付ける。

/** @type {Record<string, string>} */
const ESCAPES = {
  "&": "&amp;",
  "<": "&lt;",
  ">": "&gt;",
  '"': "&quot;",
  "'": "&#39;",
};

/**
 * @param {string} s
 * @returns {string}
 */
function escapeHtml(s) {
  return s.replace(/[&<>"']/g, (c) => ESCAPES[c]);
}

/**
 * `**強調**` → <strong>、`` `コード` `` → <code>。他はエスケープ済みの平文。
 * @param {string} source
 * @returns {string}
 */
export function inlineMarkup(source) {
  return escapeHtml(source)
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/`([^`]+)`/g, '<code class="rounded bg-slate-800/70 px-1 py-0.5 text-[0.9em]">$1</code>');
}
