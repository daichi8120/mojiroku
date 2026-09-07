import { useEffect, useState } from "react";
import { useI18n } from "@/i18n";
import { listLiveTranslations } from "@/lib/tauri";
import type { SavedLiveTranslation } from "@/lib/liveTranslation";

export function SavedTranslationRows({ rows }: { rows: SavedLiveTranslation[] }) {
  const { t } = useI18n();
  const copy = t.meeting.translation;
  return <div>
    <p className="mb-4 text-[11px] text-muted">{copy.savedHint}</p>
    {rows.length === 0 && <p className="text-[13px] text-muted">{copy.savedEmpty}</p>}
    {rows.map((row) => <div key={`${row.source_id}:${row.target}`} className="border-b border-line py-3">
      <span className="text-[10px] text-faint">{row.target === "ja" ? copy.japanese : copy.english}</span>
      <p className="mt-1 whitespace-pre-wrap text-[12px] text-muted">{row.source_text}</p>
      <p className="mt-1 whitespace-pre-wrap text-[14px] leading-relaxed text-speech">{row.translation}</p>
    </div>)}
  </div>;
}

export function SavedTranslations({ id }: { id: string }) {
  const { t } = useI18n();
  const [rows, setRows] = useState<SavedLiveTranslation[] | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let active = true;
    setRows(null);
    setFailed(false);
    listLiveTranslations(id).then((value) => { if (active) setRows(value); })
      .catch(() => { if (active) setFailed(true); });
    return () => { active = false; };
  }, [id]);
  if (failed) return <p role="alert" className="text-[12px] text-amber">{t.meeting.translation.savedFailed}</p>;
  if (rows === null) return <p role="status" className="text-[12px] text-muted">{t.meeting.translation.preparing}</p>;
  return <SavedTranslationRows rows={rows} />;
}
