// 「同じページの別言語版 URL」を求める。ja=`/`（prefix なし）・en=`/en/` の対応。
//
// なぜ 1 箇所に集めるか（2026-08-29 のレビュー指摘）:
//   hreflang（Layout）と言語スイッチャーが別々にパスを組んでいると片方だけ直り、
//   「プライバシーページの英語版が LP」のような矛盾した宣言が出る。実際 Layout は
//   `/` と `/en/` 決め打ちで、/privacy でも LP を自分の別言語版として宣言していた。
//
// .js なのは inline-markup.js と同じ理由（Node から直接テストするため）。

/**
 * パスから ja/en 両方の URL パスを返す。末尾スラッシュは入力の形をそのまま保つ。
 * @param {string} pathname 現在のパス（例 "/privacy", "/en/", "/"）
 * @returns {{ ja: string, en: string }}
 */
export function localePaths(pathname) {
  const p = pathname || "/";
  const isEn = p === "/en" || p.startsWith("/en/");
  const ja = isEn ? p.replace(/^\/en(?=\/|$)/, "") || "/" : p;
  const en = isEn ? p : `/en${ja === "/" ? "/" : ja}`;
  return { ja, en };
}

/**
 * 「もう一方の言語」の同じページ。
 * @param {string} pathname
 * @param {"ja" | "en"} target
 * @returns {string}
 */
export function counterpartPath(pathname, target) {
  const { ja, en } = localePaths(pathname);
  return target === "en" ? en : ja;
}
