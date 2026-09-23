// 日本語 UI に英文が残っていないか（#105）。ja 辞書の文字列（関数はサンプル引数で呼ぶ）を走査し、
// 日本語を 1 文字も含まない英単語 3 語以上の文を拾う。製品名・書式・出力見出しなど英語が正しいものは除外。
import { describe, expect, it } from "vitest";
import ja from "./ja";
import en from "./en";

// 英語のままが正しいキー（パス前方一致）。追加するときは理由を添える。
const ALLOW: string[] = [];

function leaves(node: unknown, path: string, out: [string, string][]) {
  if (typeof node === "string") out.push([path, node]);
  else if (typeof node === "function") {
    try {
      const v = (node as (...a: unknown[]) => unknown)("x", "y", 1);
      if (typeof v === "string") out.push([path, v]);
    } catch {
      /* 引数の形が合わない関数は対象外 */
    }
  } else if (node && typeof node === "object") {
    for (const [k, v] of Object.entries(node)) leaves(v, path ? `${path}.${k}` : k, out);
  }
}

describe("ja dictionary", () => {
  it("has no untranslated English sentences", () => {
    const all: [string, string][] = [];
    leaves(ja, "", all);
    const japanese = /[぀-ヿ一-鿿]/;
    const englishWords = /\b[A-Za-z]{2,}\b(?:[\s,.'’-]+\b[A-Za-z]{2,}\b){2,}/;
    const bad = all.filter(
      ([path, v]) =>
        !japanese.test(v) && englishWords.test(v) && !ALLOW.some((a) => path.startsWith(a)),
    );
    expect(bad).toEqual([]);
  });
});

describe("dictionaries", () => {
  // 未実装の機能を「近日」「準備中」と見せて配らない（#108）。未実装なら UI ごと出さない。
  it("do not advertise unfinished features", () => {
    for (const dict of [ja, en]) {
      const all: [string, string][] = [];
      leaves(dict, "", all);
      const bad = all.filter(([, v]) => /近日|準備中|coming soon|\(soon\)/i.test(v));
      expect(bad).toEqual([]);
    }
  });
});
