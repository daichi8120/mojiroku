// デザイン基盤（#104）の決まりが崩れていないかを、コンポーネントのソースから検査する。
// 文字サイズは 7 段、色はトークン経由。例外はここに理由つきで列挙する。
import { describe, expect, it } from "vitest";

const sources = import.meta.glob(["../features/**/*.tsx", "../components/*.tsx", "../App.tsx", "!../**/*.test.tsx"], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

const TYPE_SCALE = new Set(["11", "12", "13", "14", "15", "18", "22"]);
// 録音タイマーの大きな数字だけは表示用の特大サイズ。
const SIZE_EXCEPTIONS = new Set(["58"]);

// 色を直書きしてよいファイル: ブランドのロゴ（外観で変えない）と、未実装機能のモック画面。
const COLOR_EXCEPTIONS = [
  "components/icons.tsx",
  "features/digest/DigestView.tsx",
  "features/detail/AskDrawer.tsx",
];

describe("design tokens", () => {
  it("uses only the type scale", () => {
    const bad: string[] = [];
    for (const [file, src] of Object.entries(sources)) {
      for (const m of src.matchAll(/text-\[(\d+(?:\.\d+)?)px\]/g)) {
        if (!TYPE_SCALE.has(m[1]) && !SIZE_EXCEPTIONS.has(m[1])) bad.push(`${file}: ${m[0]}`);
      }
    }
    expect(bad).toEqual([]);
  });

  it("takes colours from tokens instead of hard-coding them", () => {
    const bad: string[] = [];
    for (const [file, src] of Object.entries(sources)) {
      if (COLOR_EXCEPTIONS.some((e) => file.endsWith(e))) continue;
      // コード中の説明コメントは対象外。
      const code = src.replace(/\/\/.*$/gm, "").replace(/\/\*[\s\S]*?\*\//g, "");
      for (const m of code.matchAll(/#[0-9a-fA-F]{6}\b|rgba?\(\s*\d/g)) {
        bad.push(`${file}: ${m[0]}`);
      }
    }
    expect(bad).toEqual([]);
  });

  it("uses the named radius tokens", () => {
    const bad: string[] = [];
    for (const [file, src] of Object.entries(sources)) {
      if (COLOR_EXCEPTIONS.some((e) => file.endsWith(e))) continue;
      for (const m of src.matchAll(/rounded-\[\d+px\]/g)) bad.push(`${file}: ${m[0]}`);
    }
    expect(bad).toEqual([]);
  });
});
