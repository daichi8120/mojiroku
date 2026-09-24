// 複数ビューで再利用する複合コンポーネント。
import { memo, type ReactNode } from "react";
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
  /** null は「話者不明」。色を割り当てず控えめな中間色で描く。 */
  id: string | null;
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

/** 本文中の query（大文字小文字は区別しない）に印を付ける（#111）。 */
export function markMatches(text: string, query: string): ReactNode {
  const q = query.trim();
  if (!q) return text;
  const lower = text.toLowerCase();
  const needle = q.toLowerCase();
  const out: ReactNode[] = [];
  let from = 0;
  let at = lower.indexOf(needle, from);
  while (at >= 0) {
    if (at > from) out.push(text.slice(from, at));
    out.push(
      <mark key={at} className="rounded-sm bg-amber/30 px-px text-ink">
        {text.slice(at, at + needle.length)}
      </mark>,
    );
    from = at + needle.length;
    at = lower.indexOf(needle, from);
  }
  if (from < text.length) out.push(text.slice(from));
  return out;
}

// ── 話者つき文字起こしリスト ───────────────────────────────────────────────
export function TranscriptList({
  segments,
  speakers,
  showTimestamps = true,
  translate,
  onSpeakerClick,
  activeIdx,
  onSeek,
  query = "",
  currentMatchIdx = null,
  className,
}: {
  segments: Segment[];
  speakers?: Speaker[];
  showTimestamps?: boolean;
  /** seg → 訳文（あれば原文の下に「訳」付きで表示）。翻訳プレビュー用。 */
  translate?: (seg: Segment) => string | null;
  /**
   * 話者チップを押せるようにする（発言単位の訂正・Issue #19）。
   *
   * 渡すと **話者が付いていない発言にも「?」チップを描く**（クリック対象を作るため。
   * ついでに本文の左ずれも消える）。渡さないときの見た目は従来どおりで、
   * 他の利用箇所には影響しない。
   */
  onSpeakerClick?: (seg: Segment) => void;
  /** 再生中の発言（Segment.idx）。強調表示する（#110）。 */
  activeIdx?: number | null;
  /** 時刻を押したときにその位置から再生する（#110）。渡さなければ時刻はただの文字。 */
  onSeek?: (seg: Segment) => void;
  /** 文字起こし内検索の語（#111）。一致箇所に印を付ける。 */
  query?: string;
  /** 検索で今選んでいる発言（Segment.idx）。 */
  currentMatchIdx?: number | null;
  className?: string;
}) {
  return (
    <ol className={cx("divide-y divide-line", className)}>
      {segments.map((seg) => (
        <TranscriptRow
          key={seg.idx}
          seg={seg}
          speakers={speakers}
          showTimestamps={showTimestamps}
          translated={translate?.(seg) ?? null}
          onSpeakerClick={onSpeakerClick}
          onSeek={onSeek}
          active={activeIdx === seg.idx}
          query={query}
          currentMatch={currentMatchIdx === seg.idx}
        />
      ))}
    </ol>
  );
}

// 行ごとに memo する。再生中は位置が 1 秒に数回変わるが、描き直すのは強調が移った 2 行だけで済む。
const TranscriptRow = memo(function TranscriptRow({
  seg,
  speakers,
  showTimestamps,
  translated: ja,
  onSpeakerClick,
  onSeek,
  active,
  query,
  currentMatch,
}: {
  seg: Segment;
  speakers?: Speaker[];
  showTimestamps: boolean;
  translated: string | null;
  onSpeakerClick?: (seg: Segment) => void;
  onSeek?: (seg: Segment) => void;
  active: boolean;
  query: string;
  currentMatch: boolean;
}) {
  const { t, lang } = useI18n();
  return (
    <li
      data-seg-idx={seg.idx}
      aria-current={active ? "true" : undefined}
      className={cx(
        "flex gap-3 rounded-ctl px-1 py-2.5 text-[15px] leading-relaxed transition-colors",
        active && "bg-brand/10",
        currentMatch && "outline outline-2 outline-amber/60",
      )}
    >
      {showTimestamps &&
        (onSeek ? (
          <button
            data-seek
            onClick={() => onSeek(seg)}
            title={t.composite.playFromHere}
            aria-label={`${t.composite.playFromHere} ${formatTimestamp(seg.start_ms)}`}
            className={cx(
              "h-fit shrink-0 rounded-tag px-1 pt-1 font-mono text-[11px] tnum transition-colors hover:bg-hover hover:text-brand-light",
              active ? "text-brand-light" : "text-dim",
            )}
          >
            {formatTimestamp(seg.start_ms)}
          </button>
        ) : (
          <span className="shrink-0 pt-1 font-mono text-[11px] text-dim tnum">
            {formatTimestamp(seg.start_ms)}
          </span>
        ))}
      {(seg.speaker_id || onSpeakerClick) && (
        <span className="shrink-0 self-start">
          <SpeakerChip
            id={seg.speaker_id}
            name={
              seg.speaker_id
                ? speakerName(seg.speaker_id, speakers, lang)
                : t.composite.speakerUnknown
            }
            onClick={onSpeakerClick ? () => onSpeakerClick(seg) : undefined}
            title={onSpeakerClick ? t.composite.clickToFixSpeaker : undefined}
          />
        </span>
      )}
      <div className="min-w-0">
        <p className="text-speech break-words">{markMatches(seg.text, query)}</p>
        {ja && (
          <p className="mt-1 flex gap-1.5 text-[13px] text-sub">
            <span className="mt-px shrink-0 rounded bg-cyan/13 px-1 text-[11px] font-medium text-cyan">
              {t.composite.translated}
            </span>
            <span>{ja}</span>
          </p>
        )}
      </div>
    </li>
  );
});

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
              background: i % 2 === 0 ? "var(--color-brand-light)" : "var(--color-cyan)",
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
            "flex items-center gap-3 rounded-btn border px-3 py-2.5 text-[13px]",
            s.state === "active"
              ? "border-brand/50 bg-selected text-ink ring-[3px] ring-brand/12"
              : s.state === "done"
                ? "border-border bg-surface-2 text-sub"
                : "border-border bg-surface-2 text-dim",
          )}
        >
          <span className="flex h-5 w-5 shrink-0 items-center justify-center">
            {s.state === "done" ? (
              <span className="flex h-5 w-5 items-center justify-center rounded-full bg-green/16 text-green">
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
        "flex items-center gap-2 rounded-btn border border-border bg-surface-2 px-3 py-2 text-[11px] text-muted",
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
    <div className="flex items-center gap-2 rounded-btn border border-green/25 bg-green/10 px-3.5 py-2 text-[12px] text-green-light">
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
        "inline-flex items-center gap-1 rounded-md border border-amber/30 bg-amber/12 px-2 py-0.5 text-[11px] font-medium text-amber",
        className,
      )}
      title={t.composite.previewTagTitle}
    >
      {t.composite.previewTag}
    </span>
  );
}
