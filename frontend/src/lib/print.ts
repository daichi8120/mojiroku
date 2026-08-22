// PDF 書き出し（ブラウザ印刷 → 「PDF として保存」）。
//
// アプリ本体（#root）と全ポータル（オーバーレイ/ポップオーバー/トースト, いずれも body 直下）を
// `@media print` で display:none にし、body 直下に差し込んだ印刷専用ノード #mojiroku-print-root
// だけを表示して**トップレベルの** window.print() を呼ぶ（content-only 印刷）。
//
// ⚠️ 隠し iframe + iframe.contentWindow.print()（サブフレーム印刷）は macOS WKWebView/wry で
// ネイティブ印刷経路に到達せず無反応になる（実機で確認）。印刷は wry の WebViewExtMacOS::print →
// printOperationWithPrintInfo: が**メインフレームを描画**するため、必ず**トップレベル**の
// window.print() を使う（tauri#3066 / wry#713）。
//
// 日本語は WKWebView のテキストエンジンが Hiragino 等のシステムフォントで描画する（アプリ画面と
// 同一エンジン・同一フォント）。フォント同梱は不要（$0・ローカル・バイナリ肥大ゼロ）。出力は
// 画像化しないため PDF 内テキストは選択・検索可能。
// ⚠️ 直接ファイル保存ではなく **OS の印刷パネル経由**（macOS は「PDF として保存」）。UI で開示する。
import { dicts } from "@/i18n";
import { exportBaseName } from "./share";
import { speakerName, type Lang, type RecordingDetail } from "./types";
import { templateLabel } from "./templates";

const PRINT_ROOT_ID = "mojiroku-print-root";
const PRINT_STYLE_ID = "mojiroku-print-style";

/** HTML 特殊文字をエスケープ（`<` `>` `&`）。未エスケープだと角括弧表記でレイアウトが壊れる。 */
function esc(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/** インライン: エスケープ後に `**bold**`/`__bold__` を `<strong>` へ。 */
function inlineHtml(s: string): string {
  return esc(s)
    .replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>")
    .replace(/__(.+?)__/g, "<strong>$1</strong>");
}

/** 区切り線（`---`/`***`/`___` が 3 個以上のみの行）か。 */
function isThematicBreak(t: string): boolean {
  return /^(-{3,}|\*{3,}|_{3,})$/.test(t);
}

/** テーブル行 `| a | b |` をセル配列へ（前後の `|` を剥がしセル trim）。 */
function splitTableRow(line: string): string[] {
  let t = line.trim();
  if (t.startsWith("|")) t = t.slice(1);
  if (t.endsWith("|")) t = t.slice(0, -1);
  return t.split("|").map((c) => c.trim());
}

/** テーブル区切り行（`|---|:--:|` 等。各セルが任意コロン付きダッシュのみ）か。 */
function isTableSeparator(line: string): boolean {
  const t = line.trim();
  if (!t.includes("|") || !t.includes("-")) return false;
  return splitTableRow(t).every((c) => /^:?-+:?$/.test(c));
}

/** ヘッダ + 本文行から HTML テーブルを組む（セルはインライン変換）。 */
function renderTable(header: string[], rows: string[][]): string {
  const th = header.map((c) => `<th>${inlineHtml(c)}</th>`).join("");
  const body = rows
    .map((r) => `<tr>${r.map((c) => `<td>${inlineHtml(c)}</td>`).join("")}</tr>`)
    .join("");
  return `<table><thead><tr>${th}</tr></thead><tbody>${body}</tbody></table>`;
}

/**
 * 要約の Markdown を簡易 HTML へ。見出し `#…`（LLM 二重マーク `## # 議題` の先頭ハッシュ群も剥がす）、
 * 箇条書き `-`/`*`/`+` → `<ul><li>`、`| a | b |` 表 → `<table>`、`---` 区切り線は除去、その他は `<p>`。
 * インラインは inlineHtml。⚠️ 厳密な Markdown ではない（リンク等は未対応。MVP）。
 */
function mdToHtml(md: string): string {
  const out: string[] = [];
  const lines = md.split(/\r?\n/);
  let inList = false;
  const closeList = () => {
    if (inList) {
      out.push("</ul>");
      inList = false;
    }
  };
  for (let i = 0; i < lines.length; i++) {
    const t = lines[i].trim();
    if (t === "" || isThematicBreak(t)) {
      closeList();
      continue;
    }
    // 表: 現在行が `|` を含み次行が区切り行 → ヘッダ + 区切り + 連続本文行を 1 表に。
    if (t.includes("|") && i + 1 < lines.length && isTableSeparator(lines[i + 1])) {
      closeList();
      const header = splitTableRow(t);
      const rows: string[][] = [];
      i += 2; // ヘッダ + 区切りを消費
      while (i < lines.length && lines[i].trim() !== "" && lines[i].includes("|")) {
        rows.push(splitTableRow(lines[i]));
        i++;
      }
      i--; // for の ++ と相殺
      out.push(renderTable(header, rows));
      continue;
    }
    const h = /^(#{1,6})\s+(.*)$/.exec(t);
    if (h) {
      closeList();
      const text = h[2].replace(/^#+\s*/, ""); // 二重マーク見出しの内側 `#` を剥がす
      const level = Math.min(h[1].length + 1, 4); // `#` → h2（h1 は会議タイトル）
      out.push(`<h${level}>${inlineHtml(text)}</h${level}>`);
      continue;
    }
    const b = /^[-*+]\s+(.*)$/.exec(t);
    if (b) {
      if (!inList) {
        out.push("<ul>");
        inList = true;
      }
      out.push(`<li>${inlineHtml(b[1])}</li>`);
      continue;
    }
    closeList();
    out.push(`<p>${inlineHtml(t)}</p>`);
  }
  closeList();
  return out.join("\n");
}

/**
 * 要約本文の冒頭にセクションラベルと同じ見出し（例: 議事録 → 先頭 `# 議事録`）があれば落とす。
 * セクション h2 と二重表示になるため。一致したときだけ 1 見出しを除去する保守的な挙動。
 */
function stripRedundantTitle(md: string, label: string): string {
  const lines = md.split(/\r?\n/);
  let i = 0;
  while (i < lines.length && lines[i].trim() === "") i++;
  if (i < lines.length) {
    const h = /^#{1,6}\s+(.*)$/.exec(lines[i].trim());
    if (h && h[1].replace(/^#+\s*/, "").trim() === label) {
      return lines.slice(i + 1).join("\n");
    }
  }
  return md;
}

/** ページ余白（@page はセレクタ非依存なので prefix 外）。 */
const PAGE_CSS = `@page { margin: 18mm 16mm; }`;

/**
 * 印刷内容の見た目 CSS（ライト・改ページ制御）を、コンテナ root セレクタ前置きで生成する単一の正。
 * アプリ内印刷では root=`#mojiroku-print-root`、standalone 文書（meetingPrintHtml）では root=`body`。
 */
function contentCss(root: string): string {
  return `
${root} { font-family: -apple-system, "Hiragino Sans", "Hiragino Kaku Gothic ProN", "Yu Gothic", "Noto Sans JP", sans-serif; color: #1a1a1a; line-height: 1.7; font-size: 11pt; }
${root} h1 { font-size: 18pt; margin: 0 0 4pt; line-height: 1.4; }
${root} .meta { color: #666; font-size: 9.5pt; margin: 0 0 14pt; }
${root} h2 { font-size: 13pt; margin: 16pt 0 6pt; padding-bottom: 3pt; border-bottom: 1px solid #ddd; break-after: avoid; }
${root} h3, ${root} h4 { font-size: 11.5pt; margin: 10pt 0 4pt; break-after: avoid; }
${root} p { margin: 0 0 6pt; orphans: 2; widows: 2; }
${root} ul { margin: 0 0 8pt; padding-left: 20pt; list-style: disc; }
${root} li { margin: 0 0 3pt; }
${root} table { border-collapse: collapse; width: 100%; margin: 6pt 0 10pt; font-size: 10pt; }
${root} th, ${root} td { border: 1px solid #ccc; padding: 3pt 6pt; text-align: left; vertical-align: top; }
${root} th { background: #f3f4f6; font-weight: 600; }
${root} tr { break-inside: avoid; }
${root} .turn { break-inside: avoid; margin: 0 0 4pt; }
${root} .transcript { margin-top: 6pt; }
`;
}

/**
 * 会議（タイトル + メタ + 全要約 + 文字起こし）の HTML 本文フラグメントを作る純関数。
 * 全テキストをエスケープし、要約 Markdown は HTML 化する。アプリ内印刷ノードにも standalone 文書にも使う。
 */
export function meetingPrintBody(detail: RecordingDetail, lang: Lang): string {
  const { format: f, output: o } = dicts[lang];
  const r = detail.recording;
  const title = esc(r.title?.trim() || o.fallbackTitle);
  const date = r.created_at.slice(0, 10);
  const durMin = Math.round(r.duration_ms / 60000);
  // 実際に発言している話者だけ並べる。発言単位の訂正で最後の 1 件を移すと、
  // 話者行は残る（訂正を戻せるように意図してそうしている）ため、
  // detail.speakers をそのまま並べると 1 件も喋っていない人が載る。
  const spoke = new Set(
    detail.transcript.segments.map((x) => x.speaker_id).filter(Boolean) as string[],
  );
  const speakers = (detail.speakers ?? [])
    .filter((s) => spoke.has(s.id))
    .map((s) => s.display_name ?? s.label)
    .join(f.listSeparator);
  const meta = esc([date, f.durationMin(durMin), speakers].filter(Boolean).join(" · "));

  const summaries = detail.summaries
    .map((s) => {
      const label = templateLabel(s.template_id, lang);
      return `<section class="sum"><h2>${esc(label)}</h2>${mdToHtml(
        stripRedundantTitle(s.content, label),
      )}</section>`;
    })
    .join("\n");

  const turns = detail.transcript.segments
    .map((seg) => {
      const who = seg.speaker_id
        ? `<strong>${esc(speakerName(seg.speaker_id, detail.speakers, lang))}</strong>: `
        : "";
      return `<p class="turn">${who}${esc(seg.text)}</p>`;
    })
    .join("\n");

  return `<h1>${title}</h1>
<p class="meta">${meta}</p>
${summaries}
<section class="transcript"><h2>${dicts[lang].output.transcriptHeading}</h2>
${turns}
</section>`;
}

/**
 * 会議を 1 枚の standalone 印刷用 HTML 文書にする純関数（doctype + style + body）。
 * 現状の印刷経路（アプリ内 window.print）では未使用だが、将来の画像化(html2canvas)など
 * 別文書としてのレンダリング再利用のために本文 + CSS の単一の正を共有して残す。
 */
export function meetingPrintHtml(detail: RecordingDetail, lang: Lang): string {
  const title = esc(detail.recording.title?.trim() || dicts[lang].output.fallbackTitle);
  return `<!doctype html><html lang="${lang}"><head><meta charset="utf-8"><title>${title}</title><style>${PAGE_CSS}
html, body { margin: 0; padding: 0; }
${contentCss("body")}</style></head><body>
${meetingPrintBody(detail, lang)}
</body></html>`;
}

/** アプリ内印刷用 CSS: 画面では印刷ノードを隠し、印刷時は印刷ノード以外を隠す。 */
const PRINT_CSS = `
@media screen { #${PRINT_ROOT_ID} { display: none !important; } }
@media print {
  ${PAGE_CSS}
  /* アプリは html,body,#root に height:100% を敷く（内側スクロール構成）。印刷では body の
     高さ固定で print-root が 1 ページにクリップされ得るため、印刷既定値へ戻す（ゼロリスク）。 */
  html, body { background: #fff !important; margin: 0; padding: 0; height: auto !important; min-height: 0 !important; overflow: visible !important; }
  body > *:not(#${PRINT_ROOT_ID}) { display: none !important; }
  #${PRINT_ROOT_ID} { display: block !important; -webkit-print-color-adjust: exact; print-color-adjust: exact; }
  ${contentCss(`#${PRINT_ROOT_ID}`)}
}
`;

let printing = false;

/**
 * 会議を PDF（印刷）に。body 直下に印刷専用ノードを差し込み、トップレベル window.print() を呼ぶ。
 * OS の印刷パネルが開き、ユーザーが「PDF として保存」を選ぶ。afterprint かタイムアウトで撤去。
 * ユーザー操作起点で呼ぶ。
 */
export function printMeetingPdf(detail: RecordingDetail, lang: Lang): void {
  if (printing) return; // 印刷準備中の再入を無視
  printing = true;
  const prevTitle = document.title;

  try {
    // afterprint が来ず前回ノードが残っていた場合の保険（ID 重複を避ける）。
    document.getElementById(PRINT_ROOT_ID)?.remove();
    document.getElementById(PRINT_STYLE_ID)?.remove();

    // 「PDF として保存」の既定ファイル名は document.title 由来。印刷中だけ会議名に差し替える。
    document.title = exportBaseName(detail);

    const style = document.createElement("style");
    style.id = PRINT_STYLE_ID;
    style.textContent = PRINT_CSS;

    const root = document.createElement("div");
    root.id = PRINT_ROOT_ID;
    root.innerHTML = meetingPrintBody(detail, lang);

    document.head.appendChild(style);
    document.body.appendChild(root);

    let done = false;
    const cleanup = () => {
      if (done) return;
      done = true;
      window.removeEventListener("afterprint", cleanup);
      style.remove();
      root.remove();
      document.title = prevTitle;
      printing = false;
    };
    window.addEventListener("afterprint", cleanup);

    // レイアウト確定後にトップレベル印刷。afterprint が来ない環境向けに長めのフォールバック撤去。
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        try {
          window.print();
        } finally {
          setTimeout(cleanup, 60000);
        }
      }),
    );
  } catch (e) {
    // 同期セットアップで失敗しても再投入できるよう、注入物・タイトル・フラグを戻す。
    document.getElementById(PRINT_ROOT_ID)?.remove();
    document.getElementById(PRINT_STYLE_ID)?.remove();
    document.title = prevTitle;
    printing = false;
    throw e;
  }
}
