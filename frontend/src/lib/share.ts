// 「AI に送る / コピー / 書き出し」の整形と操作（フロント完結）。
// 送信はすべてユーザー操作起点。自動送信は行わない（北極星 "送信なし" と両立）。
// 出力の見出し・既定タイトルはアプリ言語（lang）に従う。
import { openUrl } from "@tauri-apps/plugin-opener";
import { dicts } from "@/i18n";
import {
  formatTimestamp,
  speakerName,
  type Lang,
  type RecordingDetail,
  type Summary,
} from "./types";
import { templateLabel } from "./templates";

/** 要約 1 件を Markdown に（content はそのまま Markdown 想定）。 */
export function summaryMarkdown(summary: Summary): string {
  return summary.content.trim();
}

/** 文字起こしを Markdown に。話者つき／時刻つきを切り替えられる。 */
export function transcriptMarkdown(
  detail: RecordingDetail,
  lang: Lang,
  opts: { withSpeakers?: boolean; withTimestamps?: boolean } = {},
): string {
  const { withSpeakers = true, withTimestamps = false } = opts;
  const lines = detail.transcript.segments.map((seg) => {
    const time = withTimestamps ? `\`${formatTimestamp(seg.start_ms)}\` ` : "";
    const who =
      withSpeakers && seg.speaker_id
        ? `**${speakerName(seg.speaker_id, detail.speakers, lang)}**: `
        : "";
    return `${time}${who}${seg.text}`;
  });
  return lines.join("\n");
}

/** プレーンテキスト（話者ラベルつき・記号なし）。 */
export function transcriptPlain(detail: RecordingDetail, lang: Lang): string {
  return detail.transcript.segments
    .map((seg) => {
      const who = seg.speaker_id
        ? `${speakerName(seg.speaker_id, detail.speakers, lang)}: `
        : "";
      return `${who}${seg.text}`;
    })
    .join("\n");
}

/** SRT 字幕。 */
export function transcriptSrt(detail: RecordingDetail, lang: Lang): string {
  const ts = (ms: number) => {
    const h = Math.floor(ms / 3600000);
    const m = Math.floor((ms % 3600000) / 60000);
    const s = Math.floor((ms % 60000) / 1000);
    const msPart = Math.floor(ms % 1000);
    const p = (n: number, w = 2) => n.toString().padStart(w, "0");
    return `${p(h)}:${p(m)}:${p(s)},${p(msPart, 3)}`;
  };
  return detail.transcript.segments
    .map((seg, i) => {
      const who = seg.speaker_id
        ? `${speakerName(seg.speaker_id, detail.speakers, lang)}: `
        : "";
      return `${i + 1}\n${ts(seg.start_ms)} --> ${ts(seg.end_ms)}\n${who}${seg.text}\n`;
    })
    .join("\n");
}

/** YAML 文字列値のエスケープ（" と改行を無害化）。 */
function yamlStr(s: string): string {
  return `"${s.replace(/["\\]/g, "\\$&").replace(/[\r\n]+/g, " ")}"`;
}

/**
 * Obsidian 向け Markdown。YAML frontmatter（title/date/duration/source/speakers/tags）+
 * 全要約セクション + 文字起こし。Obsidian の Vault にそのまま放り込めるノート 1 枚。
 */
export function obsidianMarkdown(detail: RecordingDetail, lang: Lang): string {
  const r = detail.recording;
  const title = r.title?.trim() || dicts[lang].output.fallbackTitle;
  const date = r.created_at.slice(0, 10); // RFC3339 → YYYY-MM-DD
  const durMin = Math.round(r.duration_ms / 60000);
  const speakers = (detail.speakers ?? []).map((s) => s.display_name ?? s.label);

  const frontmatter = [
    "---",
    `title: ${yamlStr(title)}`,
    `date: ${date}`,
    `duration_min: ${durMin}`,
    `source: ${r.source_type}`,
    `speakers: [${speakers.map(yamlStr).join(", ")}]`,
    "tags: [mojiroku, meeting]",
    "---",
    "",
  ].join("\n");

  const parts: string[] = [frontmatter, `# ${title}`, ""];
  for (const s of detail.summaries) {
    parts.push(`## ${templateLabel(s.template_id, lang)}`, "", s.content.trim(), "");
  }
  parts.push(
    `## ${dicts[lang].output.transcriptHeading}`,
    "",
    transcriptMarkdown(detail, lang, { withSpeakers: true }),
    "",
  );
  return parts.join("\n");
}

/** 書き出しファイル名のベース（拡張子なし）。title を安全化し日付を添える。 */
export function exportBaseName(detail: RecordingDetail): string {
  const raw = detail.recording.title?.trim() || "mojiroku";
  const safe = raw.replace(/[/\\:*?"<>|\n\r\t]/g, "_").slice(0, 80);
  const date = detail.recording.created_at.slice(0, 10);
  return `${safe}_${date}`;
}

/** 生成 AI に渡すプロンプト（要約があれば添える）。プロンプト言語もアプリ言語に従う。 */
export function aiPrompt(detail: RecordingDetail, lang: Lang): string {
  const o = dicts[lang].output;
  const title = detail.recording.title ?? o.fallbackTitle;
  return o.aiPromptHead(title) + transcriptMarkdown(detail, lang, { withSpeakers: true });
}

/** クリップボードへコピー（Tauri webview の navigator.clipboard を使用）。 */
export async function copyText(text: string): Promise<void> {
  await navigator.clipboard.writeText(text);
}

export type AiProvider = "chatgpt" | "claude";

/**
 * 生成 AI のチャットを開く。プロンプトはクリップボードにもコピーする
 * （長文は URL に載らないため、貼り付けで確実に渡せるように）。
 * 戻り値は「URL プレフィルが使えたか」。
 */
export async function openInAi(provider: AiProvider, prompt: string): Promise<boolean> {
  await copyText(prompt);
  const short = prompt.length <= 1500;
  const q = encodeURIComponent(prompt);
  const url =
    provider === "chatgpt"
      ? short
        ? `https://chatgpt.com/?q=${q}`
        : "https://chatgpt.com/"
      : short
        ? `https://claude.ai/new?q=${q}`
        : "https://claude.ai/new";
  await openUrl(url);
  return short;
}
