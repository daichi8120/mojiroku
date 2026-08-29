// プライバシーポリシーの「端末の外へ出る場面」の表が、実装の通信先と一致しているかを確かめる。
//
// なぜ要るか:
//   ポリシーは「宛先を増やしたら表も直す」という運用に頼っていて、その手順は必ず忘れられる。
//   宛先が抜けたポリシーは、無いより悪い（事実と違うことを公開している状態になる）。
//   Google の OAuth 審査もこのページを根拠にするので、ズレると審査の前提が崩れる。
//
// 使い方: node scripts/check-privacy-hosts.mjs （landing ディレクトリから）
// 通信先を意図的に増やしたときは、privacy-policy.ts の rows に足してから通す。
//
// ⚠️ この検査が見るのは**ホスト名の集合だけ**。次のズレは捕まえられない。
//    - 宛先は表にあるが、「何が送られるか」の説明が実装と違う
//      （2026-08-29 に実際に起きた: mojiroku.com は更新確認としてだけ載っていたが、
//        実際は Slack/Notion の OAuth ブローカーでもあり、トークンが通っていた）
//    - 送信する中身が増えたが、宛先は変わっていない
//    そこは人と外部レビューで見るしかない。この検査を「網羅している」と読まないこと。
import { readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, "..", "..");

// 通信ではない https:// の出現。ここに足すときは理由を書く。
// ⚠️ 「ブラウザで開くだけだから」は除外の理由にならない。2026-08-29 に claude.ai と
//    chatgpt.com をその理由で外していたが、実際は文字起こしを URL に載せて開いていた
//    （frontend/src/lib/share.ts の openInAi）。宛先に中身が渡るなら表に載せる。
const NOT_EGRESS = new Set([
  "tauri.app", // ソースのコメントに書かれたドキュメント URL
  "evil.example.com", // URL 検証のテストで使う偽ホスト
]);

function die(msg) {
  console.error(msg);
  process.exit(1);
}

/**
 * 走査対象から https:// のホストを集める。
 *
 * **失敗を握り潰さない。** grep は「一致ゼロ」で終了コード 1 を返すが、grep 自体が無い・
 * パスが読めない場合も例外になる。両者を同じ扱いにすると、検査が空振りしたまま
 * 成功してしまい、ガードが黙って無効になる（2026-08-29 のレビュー指摘）。
 */
function hostsIn(paths) {
  const out = new Set();
  for (const p of paths) {
    let text = "";
    try {
      text = execFileSync("grep", ["-rhoE", "https://[a-zA-Z0-9._/-]+", p], {
        cwd: repo,
        encoding: "utf8",
      });
    } catch (err) {
      if (err && err.status === 1) continue; // 一致ゼロ。正常
      die(
        `通信先の走査に失敗した（${p}）: ${err?.message ?? err}\n` +
          "検査できていない状態でビルドを通さないため、ここで止める。",
      );
    }
    for (const line of text.split("\n")) {
      const m = /^https:\/\/([^/"]+)/.exec(line.trim());
      if (m) out.add(m[1]);
    }
  }
  return out;
}

const found = hostsIn(["crates", "src-tauri/src", "frontend/src"]);
for (const h of NOT_EGRESS) found.delete(h);

// 走査が成立していれば必ず何件か出る。ゼロは「見つからなかった」ではなく
// 「見に行けていない」の徴候なので、成功にしない。
if (found.size === 0) {
  die(
    "通信先が 1 件も見つからなかった。走査対象のパスか正規表現が壊れている疑いがある。\n" +
      "検査できていない状態でビルドを通さないため、ここで止める。",
  );
}

const policy = readFileSync(resolve(here, "..", "src/copy/privacy-policy.ts"), "utf8");

/**
 * 言語ブロックを切り出して、その中の `host:` 行だけを見る。
 *
 * ファイル全体への部分一致では、コメントに書いただけ・片方の言語にしか無い状態でも
 * 通ってしまい、検査が保証にならない（2026-08-29 のレビュー指摘）。
 */
function hostsInBlock(startMarker, endMarker) {
  const from = policy.indexOf(startMarker);
  const to = policy.indexOf(endMarker);
  if (from < 0 || to < 0 || to <= from) {
    console.error(`privacy-policy.ts の構造が変わっている（${startMarker} が見つからない）`);
    process.exit(1);
  }
  const block = policy.slice(from, to);
  const out = new Set();
  for (const m of block.matchAll(/^\s*host:\s*"([^"]+)"/gm)) {
    // "a.example / b.example" のように 1 行に複数書く形を許す
    for (const h of m[1].split("/")) out.add(h.trim());
  }
  return out;
}

const inJa = hostsInBlock("export const ja", "export const en");
const inEn = hostsInBlock("export const en", "const dict");

const missing = [...found]
  .map((h) => ({ h, ja: inJa.has(h), en: inEn.has(h) }))
  .filter((r) => !r.ja || !r.en)
  .sort((a, b) => a.h.localeCompare(b.h));

if (missing.length) {
  console.error(
    "プライバシーポリシーの表に載っていない通信先がある:\n" +
      missing
        .map((r) => `  - ${r.h}（ja=${r.ja ? "あり" : "なし"} / en=${r.en ? "あり" : "なし"}）`)
        .join("\n") +
      "\n\nlanding/src/copy/privacy-policy.ts の egress.rows（ja と en の両方）に足すこと。" +
      "\n通信ではない出現なら scripts/check-privacy-hosts.mjs の NOT_EGRESS に理由つきで足す。",
  );
  process.exit(1);
}

console.log(
  `ok  通信先 ${found.size} 件はすべて ja/en 両方の表に記載されている` +
    `（表の行: ja=${inJa.size} / en=${inEn.size}）`,
);
