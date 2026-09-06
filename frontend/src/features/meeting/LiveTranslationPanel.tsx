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
        <span className="rounded bg-brand/10 px-1.5 py-0.5 text-[10px] text-brand-light">{copy.preview}</span>
        <label className="ml-auto text-[11px] text-muted">
          <span className="sr-only">{copy.target}</span>
          <select aria-label={copy.target} value={target} disabled={!capturing || translation.starting || translation.unavailable}
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
          disabled={!capturing} onClick={() => translation.enabled ? translation.stop() : translation.start(target)}>
          {translation.enabled ? copy.disable : copy.enable}
        </Button>
      </div>
      {!translation.enabled ? (
        <div className="px-[18px] py-3 text-[12px] leading-relaxed text-muted">
          <p>{copy.description}</p>
          <p className="mt-3">{copy.downloadHint}</p>
          <p className="mt-3">{copy.temporary}</p>
        </div>
      ) : (
        <>
          <div role="status" className="px-[18px] pb-2 text-[11px] text-muted">
            {translation.unavailable ? copy.unavailable : translation.failed ? copy.failed : status}
            {!translation.failed && translation.pending > 0 && ` · ${translation.pending} ${copy.pending}`}
          </div>
          {translation.failed && !translation.unavailable && <div className="px-[18px] pb-3">
            <Button size="sm" variant="secondary" disabled={!capturing} onClick={() => translation.retry(target)}>{copy.retry}</Button>
          </div>}
          {translation.skipped > 0 && <p className="mx-[18px] mb-2 rounded border border-amber/30 bg-amber/10 px-2 py-1.5 text-[11px] text-amber">{copy.skipped}</p>}
          <div ref={scroll} className="min-h-0 flex-1 overflow-auto px-[18px] pb-4" aria-label={copy.results}>
            {translation.rows.map((row) => <div key={row.sourceId} className="border-b border-line py-2.5">
              <p className="mb-1 text-[10.5px] leading-relaxed text-faint">{row.sourceText}</p>
              <p className={cx("text-[13.5px] leading-[1.7]", row.committed ? "text-speech" : "text-muted")}>
                {row.translation ?? (row.status === "error" ? row.error === "input_too_long" ? copy.tooLong : copy.rowFailed
                  : row.status === "skipped" ? copy.rowSkipped
                  : row.status === "translating" ? copy.translating : copy.rowQueued)}
              </p>
            </div>)}
          </div>
          <p className="border-t border-line px-[18px] py-2 text-[11px] leading-relaxed text-faint">{copy.temporary}</p>
        </>
      )}
      <p className="border-t border-line px-[18px] py-2 text-[11px] text-faint">{t.meeting.live.aiNotesDetail}</p>
    </aside>
  );
}
