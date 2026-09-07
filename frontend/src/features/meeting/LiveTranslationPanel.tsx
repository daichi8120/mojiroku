import { useEffect, useRef } from "react";
import { useI18n } from "@/i18n";
import { Button } from "@/components/ui";
import { cx } from "@/lib/cx";
import type { TranslationTarget } from "@/lib/liveTranslation";
import type { useLiveTranslation } from "@/lib/useLiveTranslation";

export function LiveTranslationPanel({ translation, target, setTarget, capturing }: {
  translation: ReturnType<typeof useLiveTranslation>;
  target: TranslationTarget;
  setTarget: (target: TranslationTarget) => void;
  capturing: boolean;
}) {
  const { t } = useI18n();
  const copy = t.meeting.translation;
  const scroll = useRef<HTMLDivElement>(null);
  useEffect(() => { if (scroll.current) scroll.current.scrollTop = scroll.current.scrollHeight; }, [translation.rows]);
  const progress = translation.progress;
  const status = translation.starting ? copy.preparing
    : progress?.stage === "download" ? `${copy.downloading} ${progress.total ? Math.min(100, Math.floor(100 * progress.done / progress.total)) : 0}%`
    : progress?.stage === "queued" ? copy.waiting
    : translation.rows.some((row) => row.status === "translating") ? copy.translating
    : copy.listening;
  return (
    <aside aria-label={copy.title} className="flex min-w-0 flex-1 flex-col bg-surface">
      <div className="flex flex-wrap items-center gap-2 px-[18px] pb-2.5 pt-3.5">
        <h2 className="text-[12px] font-bold text-ink">{copy.title}</h2>
        <details className="text-[11px] text-muted">
          <summary className="cursor-pointer">{copy.about}</summary>
          <p className="max-w-64 py-2">{copy.downloadHint} {copy.retained}</p>
        </details>
        <label className="ml-auto text-[11px] text-muted">
          <span className="sr-only">{copy.target}</span>
          <select aria-label={copy.target} value={target} disabled={!capturing || translation.starting || translation.unavailable || translation.historyFull}
            onChange={(e) => {
              const next = e.target.value as TranslationTarget;
              setTarget(next);
              if (translation.enabled) translation.start(next);
            }}
            className="rounded-btn border border-border-2 bg-surface-2 px-2 py-1.5 text-ink">
            <option value="ja">{copy.japanese}</option>
            <option value="en">{copy.english}</option>
          </select>
        </label>
        <Button size="sm" variant={translation.enabled ? "secondary" : "primary"}
          disabled={!capturing || translation.historyFull} onClick={() => translation.enabled ? translation.stop() : translation.start(target)}>
          {translation.enabled ? copy.disable : copy.enable}
        </Button>
      </div>
      {!translation.enabled && translation.rows.length === 0 && (
        <p className="px-[18px] py-3 text-[12px] text-muted">{copy.description}</p>
      )}
      {translation.historyFull && <p role="status" className="px-[18px] pb-2 text-[11px] text-amber">{copy.historyFull}</p>}
      {translation.enabled && <>
          <div role="status" className="px-[18px] pb-2 text-[11px] text-muted">
            {translation.unavailable ? copy.unavailable : translation.failed ? copy.failed : status}
            {!translation.failed && translation.pending > 0 && ` · ${translation.pending} ${copy.pending}`}
          </div>
          {translation.failed && !translation.unavailable && <div className="px-[18px] pb-3">
            <Button size="sm" variant="secondary" disabled={!capturing} onClick={() => translation.retry(target)}>{copy.retry}</Button>
          </div>}
          {translation.skipped > 0 && <p className="mx-[18px] mb-2 rounded border border-amber/30 bg-amber/10 px-2 py-1.5 text-[11px] text-amber">{copy.skipped}</p>}
      </>}
          <div ref={scroll} className="min-h-0 flex-1 overflow-auto px-[18px] pb-4" aria-label={copy.results}>
            {translation.rows.map((row) => <div key={`${row.sourceId}:${row.target}`} className="border-b border-line py-2.5">
              <span className="text-[10px] text-faint">{row.target === "en" ? copy.english : copy.japanese}</span>
              <p className="mb-1 text-[10.5px] leading-relaxed text-faint">{row.sourceText}</p>
              <p className={cx("text-[13.5px] leading-[1.7]", row.committed ? "text-speech" : "text-muted")}>
                {row.translation ?? (row.status === "error" ? row.error === "input_too_long" ? copy.tooLong : copy.rowFailed
                  : row.status === "skipped" ? copy.rowSkipped
                  : row.status === "translating" ? copy.translating : copy.rowQueued)}
              </p>
            </div>)}
          </div>
    </aside>
  );
}
