// 言語別パスの対応と、sitemap が出す対の整合を確かめる。
// hreflang と sitemap がずれると、検索エンジンは相互参照が成立しないとみなして両方を無効化する。
import { localePaths, counterpartPath } from "../src/lib/locale-path.js";
import { readFileSync } from "node:fs";

let bad = 0;
const eq = (got, want, why) => {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  if (!ok) bad++;
  console.log(`  ${ok ? "ok  " : "FAIL"} ${why}\n       got=${JSON.stringify(got)}`);
};

eq(localePaths("/"), { ja: "/", en: "/en/" }, "トップ");
eq(localePaths("/en/"), { ja: "/", en: "/en/" }, "英語トップ");
eq(localePaths("/privacy"), { ja: "/privacy", en: "/en/privacy" }, "ja の下層");
eq(localePaths("/en/privacy"), { ja: "/privacy", en: "/en/privacy" }, "en の下層");
eq(localePaths("/english-notes"), { ja: "/english-notes", en: "/en/english-notes" },
   "/en で始まる別の語を prefix と誤認しない");
eq(counterpartPath("/privacy", "en"), "/en/privacy", "切替 ja→en");
eq(counterpartPath("/en/privacy", "ja"), "/privacy", "切替 en→ja");

// sitemap の対がヘルパと一致すること（片方だけ直る事故を防ぐ）
const src = readFileSync(new URL("../src/pages/sitemap.xml.ts", import.meta.url), "utf8");
const pairs = [...src.matchAll(/ja:\s*"([^"]+)",\s*\n?\s*en:\s*"([^"]+)"/g)];
if (!pairs.length) {
  console.log("  FAIL sitemap の PAGE_PAIRS を読み取れない");
  bad++;
}
for (const [, ja, en] of pairs) {
  const got = localePaths(ja);
  const ok = got.ja === ja && got.en === en;
  if (!ok) bad++;
  console.log(`  ${ok ? "ok  " : "FAIL"} sitemap の対 ${ja} ⇔ ${en}`);
}

process.exit(bad ? 1 : 0);
