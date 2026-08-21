// 複数ビューで再利用する複合コンポーネント。
import type { ReactNode } from "react";
import { useI18n } from "@/i18n";
import { cx } from "@/lib/cx";
import {
  formatTimestamp,
  speakerChipStyle,
  speakerName,
  type Segment,
  type Speaker,
} from "@/lib/types";
import { MOCK_PREVIEW } from "@/lib/mockData";
import { CheckIcon, CpuIcon } from "./icons";
import { Spinner } from "./ui";

// ── 話者チップ / ドット ───────────────────────────────────────────────────
export function SpeakerChip({
  id,
  name,
  onClick,
  title,
  block,
}: {
  id: string;
  name: string;
  onClick?: () => void;
  title?: string;
  /** 親幅まで縮めて名前を省略表示（狭い右ペイン等での横はみ出しを防ぐ）。 */
  block?: boolean;
}) {
  const Tag = onClick ? "button" : "span";
  return (
    <Tag
      onClick={onClick}
      title={title}
      style={speakerChipStyle(id)}
      className={cx(
        "inline-flex items-center rounded-md px-1.5 py-0.5 text-[11px] font-medium",
        block ? "max-w-full" : "shrink-0",
        onClick && "transition-opacity hover:opacity-80",
      )}
    >
      <span className={cx(block && "min-w-0 truncate")}>{name}</span>
    </Tag>
  );
}

// ── 話者つき文字起こしリスト ───────────────────────────────────────────────
export function TranscriptList({
  segments,
  speakers,
  showTimestamps = true,
  translate,
  className,
}: {
  segments: Segment[];
  speakers?: Speaker[];
  showTimestamps?: boolean;
  /** seg → 訳文（あれば原文の下に「訳」付きで表示）。翻訳プレビュー用。 */
  translate?: (seg: Segment) => string | null;
  className?: string;
}) {
  const { t, lang } = useI18n();
  return (
    <ol className={cx("divide-y divide-line", className)}>
      {segments.map((seg, i) => {
        const ja = translate?.(seg) ?? null;
        return (
          <li key={i} className="flex gap-3 px-1 py-2.5 text-[13.5px]">
            {showTimestamps && (
              <span className="shrink-0 pt-0.5 font-mono text-[11px] text-dim tnum">
                {formatTimestamp(seg.start_ms)}
              </span>
            )}
            {seg.speaker_id && (
              <span className="shrink-0 self-start">
                <SpeakerChip
                  id={seg.speaker_id}
                  name={speakerName(seg.speaker_id, speakers, lang)}
                />
              </span>
            )}
            <div className="min-w-0">
              <p className="text-speech break-words">{seg.text}</p>
              {ja && (
                <p className="mt-1 flex gap-1.5 text-[13px] text-sub">
                  <span className="mt-px shrink-0 rounded bg-[rgba(34,211,238,0.13)] px-1 text-[10px] font-medium text-cyan">
                    {t.composite.translated}
                  </span>
                  <span>{ja}</span>
                </p>
              )}
            </div>
          </li>
        );
      })}
    </ol>
  );
}

// ── ライブ波形（mjbar） ────────────────────────────────────────────────────
export function Waveform({
  active = true,
  bars = 48,
  height = 40,
  className,
}: {
  active?: boolean;
  bars?: number;
  height?: number;
  className?: string;
}) {
  return (
    <div
      className={cx("flex items-center justify-center gap-[3px]", className)}
      style={{ height }}
    >
      {Array.from({ length: bars }).map((_, i) => {
        const base = 0.2 + ((i * 37) % 100) / 140; // 疑似ランダムな基準高さ
        return (
          <span
            key={i}
            className="w-[3px] rounded-full"
            style={{
              height: height * (active ? 1 : base * 0.6),
              transformOrigin: "center",
              background: i % 2 === 0 ? "#818cf8" : "#22d3ee",
              animation: active
                ? `mjbar ${0.8 + (i % 5) * 0.16}s ease-in-out ${(i % 7) * 0.05}s infinite`
                : "none",
              transform: active ? undefined : `scaleY(${base})`,
            }}
          />
        );
      })}
    </div>
  );
}

// ── 処理パイプライン可視化 ─────────────────────────────────────────────────
export type PipeState = "done" | "active" | "pending";
export interface PipeStep {
  key: string;
  label: string;
  state: PipeState;
}

export function Pipeline({ steps }: { steps: PipeStep[] }) {
  return (
    <ol className="flex flex-col gap-1.5">
      {steps.map((s) => (
        <li
          key={s.key}
          className={cx(
            "flex items-center gap-3 rounded-[10px] border px-3 py-2.5 text-[13px]",
            s.state === "active"
              ? "border-brand/50 bg-selected text-ink shadow-[0_0_0_3px_rgba(99,102,241,0.12)]"
              : s.state === "done"
                ? "border-border bg-surface-2 text-sub"
                : "border-border bg-surface-2 text-dim",
          )}
        >
          <span className="flex h-5 w-5 shrink-0 items-center justify-center">
            {s.state === "done" ? (
              <span className="flex h-5 w-5 items-center justify-center rounded-full bg-[rgba(52,211,153,0.16)] text-green">
                <CheckIcon size={13} />
              </span>
            ) : s.state === "active" ? (
              <Spinner size={16} />
            ) : (
              <span className="h-2.5 w-2.5 rounded-full border border-border-3" />
            )}
          </span>
          <span>{s.label}</span>
        </li>
      ))}
    </ol>
  );
}

// ── 価値カード（ローカル / 無料 / プライバシー） ──────────────────────────────
export function ValueProps({ className }: { className?: string }) {
  const { t } = useI18n();
  const items = [
    { ...t.composite.valueProps.local, tone: "text-green" },
    { ...t.composite.valueProps.free, tone: "text-brand-lighter" },
    { ...t.composite.valueProps.speakers, tone: "text-cyan" },
  ];
  return (
    <div className={cx("grid grid-cols-3 gap-3", className)}>
      {items.map((it) => (
        <div key={it.title} className="rounded-card border border-border bg-surface-2 px-4 py-3">
          <div className={cx("text-[13px] font-bold", it.tone)}>{it.title}</div>
          <div className="mt-1 text-[12px] leading-relaxed text-muted">{it.body}</div>
        </div>
      ))}
    </div>
  );
}

// ── 「ローカル推論 · Metal · 無料」フッターバッジ ──────────────────────────────
export function LocalStatus({ className }: { className?: string }) {
  const { t } = useI18n();
  return (
    <div
      className={cx(
        "flex items-center gap-2 rounded-[10px] border border-border bg-surface-2 px-3 py-2 text-[11px] text-muted",
        className,
      )}
    >
      <CpuIcon size={14} className="text-green" />
      <span>{t.composite.localStatus}</span>
    </div>
  );
}

// ── 「送信なし」緑の安心バー ───────────────────────────────────────────────
export function PrivacyBar({ children }: { children: ReactNode }) {
  return (
    <div className="flex items-center gap-2 rounded-[10px] border border-[rgba(52,211,153,0.25)] bg-[rgba(16,26,22,0.6)] px-3.5 py-2 text-[12px] text-green-light">
      <span className="h-1.5 w-1.5 rounded-full bg-green" />
      {children}
    </div>
  );
}

// ── 空状態 ─────────────────────────────────────────────────────────────────
export function EmptyState({
  icon,
  title,
  hint,
}: {
  icon?: ReactNode;
  title: string;
  hint?: string;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 rounded-card border border-dashed border-border-2 px-6 py-16 text-center">
      {icon && <div className="text-dim">{icon}</div>}
      <div className="text-[14px] text-sub">{title}</div>
      {hint && <div className="max-w-sm text-[12px] text-muted">{hint}</div>}
    </div>
  );
}

// ── プレビュー（モック）マーカー ───────────────────────────────────────────
// ⚠️ 未実装機能のモック画面に付ける極小マーカー。配布前に判断（roadmap 参照）。
export function PreviewTag({ className }: { className?: string }) {
  const { t } = useI18n();
  if (!MOCK_PREVIEW) return null;
  return (
    <span
      className={cx(
        "inline-flex items-center gap-1 rounded-md border border-amber/30 bg-[rgba(245,158,11,0.12)] px-2 py-0.5 text-[10.5px] font-medium text-amber",
        className,
      )}
      title={t.composite.previewTagTitle}
    >
      {t.composite.previewTag}
    </span>
  );
}
