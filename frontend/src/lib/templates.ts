// テンプレ id ↔ 表示の共通ヘルパー。
//
// ⚠️ ここは**エクスポート/印刷の出力見出し**ラベル（Rust の `export::template_label` と揃える。
// 両言語とも一致必須 — ズレると Notion/Slack 追記との整合が壊れる。両側のテストで固定）。
// アプリ内の詳細ビューは "AI議事録" など別の UI ラベル＋未知 id を id のまま出す挙動なので、
// DetailView は意図的に別定義のままにする（ここへ寄せない）。

import { dicts } from "@/i18n";
import type { Lang, Summary } from "./types";

/** テンプレ id を出力見出しラベルへ。未知 id は「メモ」/ "Notes"。
 * 実体は辞書の output.templateLabels / output.templateFallback（Rust 側と一致必須）。 */
export function templateLabel(id: string, lang: Lang): string {
  const o = dicts[lang].output;
  return (o.templateLabels as Record<string, string>)[id] ?? o.templateFallback;
}

/** summaries から指定テンプレの要約を取り出す（無ければ undefined）。 */
export function findSummary(
  summaries: Summary[],
  templateId: string,
): Summary | undefined {
  return summaries.find((s) => s.template_id === templateId);
}
