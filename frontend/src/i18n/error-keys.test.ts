// Rust が返す i18n キーが、ja / en の辞書に存在することを走査で確かめる。
//
// なぜ必要か: 発言単位の話者訂正（Issue #19）で「辞書にキーはあるが到達しない」バグが
// **2 周のレビューで見逃された**。層ごとにテストが自己完結していて、継ぎ目を誰も見ていなかった。
//
//   Rust のテスト   … Result の中身しか見ない
//   フロントのテスト … Rust が実際に何を渡すか知らない
//
// キー名をハードコードしたテストは、Rust 側でキーを改名すると緑のまま素通りする。
// ここでは**ソースから実際のキーを抽出**するので、改名して辞書に足し忘れれば落ちる。
//
// 取り込みは Vite の import.meta.glob（`query: "?raw"`）。@types/node を足さずに済ませるため
// （node:fs を使うと tsc が落ちる。frontend に @types/node は入っていない）。
//
// 限界: 文字列リテラルしか拾えない。`format!("error.{}.{}", ..)` のような組み立ては漏れる
// （現状そういう箇所は無い）。
import { describe, expect, it } from "vitest";
import en from "./en";
import ja from "./ja";

const SOURCES = import.meta.glob("../../../{crates,src-tauri}/**/*.rs", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

const KEY_RE = /"(error\.[a-z0-9_]+(?:\.[a-z0-9_]+)+)"/g;

/** テストフィクスチャのキーは実在しなくてよい（本番経路を通らない）。 */
const TEST_ONLY = new Set(["error.job.boom"]);

function collectKeys(): Map<string, string> {
  const found = new Map<string, string>(); // key -> 最初に見つけたファイル
  for (const [file, src] of Object.entries(SOURCES)) {
    for (const m of src.matchAll(KEY_RE)) {
      if (!TEST_ONLY.has(m[1]) && !found.has(m[1])) found.set(m[1], file);
    }
  }
  return found;
}

describe("Rust が返す i18n キーが辞書にある", () => {
  const keys = collectKeys();

  it("そもそもキーを抽出できている（走査が空振りしていない）", () => {
    expect(keys.size).toBeGreaterThan(20);
  });

  it("ja / en の両方に存在する", () => {
    const missing: string[] = [];
    for (const [key, file] of keys) {
      const inJa = key in (ja.errors as Record<string, string>);
      const inEn = key in (en.errors as Record<string, string>);
      if (!inJa || !inEn) missing.push(`${key} (${file}) ja=${inJa} en=${inEn}`);
    }
    expect(missing).toEqual([]);
  });
});
