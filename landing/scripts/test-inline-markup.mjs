// inline-markup の安全性テスト。set:html で出すので、エスケープが**先**であることが命綱。
import { inlineMarkup } from "../src/lib/inline-markup.js";

const cases = [
  ["<script>alert(1)</script>", (o) => !o.includes("<script"), "生タグを出さない"],
  ["**強調**", (o) => o.includes("<strong>強調</strong>"), "強調が HTML になる"],
  ["`code`", (o) => o.includes("<code"), "コードが HTML になる"],
  ["a & b < c", (o) => o.includes("&amp;") && o.includes("&lt;"), "記号をエスケープする"],
  ["**<b>x</b>**", (o) => o.includes("<strong>&lt;b&gt;") , "強調の中身もエスケープする"],
  ['" onmouseover="x', (o) => !o.includes('"'), "属性を抜ける引用符を潰す"],
];

let bad = 0;
for (const [input, ok, why] of cases) {
  const out = inlineMarkup(input);
  const pass = ok(out);
  if (!pass) bad++;
  console.log(`  ${pass ? "ok  " : "FAIL"} ${why}\n       ${JSON.stringify(out)}`);
}
process.exit(bad ? 1 : 0);
