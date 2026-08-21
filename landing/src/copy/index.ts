// コピーの入口。ja を「形の単一情報源」とし、en は Copy 型（= typeof ja）で
// キー構造の一致を tsc に強制させる。配列の要素数だけは型で守れないため、
// ビルド/開発時に assertSameShape で ja/en の配列長ズレを即エラーにする。
import ja from "./ja";
import en from "./en";

export const locales = ["ja", "en"] as const;
export type Locale = (typeof locales)[number];
export type Copy = typeof ja;

const dict: Record<Locale, Copy> = { ja, en };

/** Astro.currentLocale（string | undefined）を安全に解決する。未知値は ja。 */
export function getCopy(locale: string | undefined): Copy {
  return dict[(locale ?? "ja") as Locale] ?? ja;
}

// 型では守れない「配列の要素数一致」を再帰チェック（ビルド時にも throw させる）。
function assertSameShape(a: unknown, b: unknown, path: string): void {
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) {
      throw new Error(`copy: ja/en で配列長が不一致: ${path}（ja=${a.length}, en=${b.length}）`);
    }
    a.forEach((v, i) => assertSameShape(v, b[i], `${path}[${i}]`));
  } else if (a !== null && b !== null && typeof a === "object" && typeof b === "object") {
    for (const key of Object.keys(a as Record<string, unknown>)) {
      assertSameShape(
        (a as Record<string, unknown>)[key],
        (b as Record<string, unknown>)[key],
        `${path}.${key}`,
      );
    }
  }
}
assertSameShape(ja, en, "copy");
