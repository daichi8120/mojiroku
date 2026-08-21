// 履歴 + 検索（Studio 05）。実機能: 全文検索 / 一覧 / 削除 / ハイライト。
// trim 空 → listRecordings、それ以外 → searchRecordings（250ms デバウンス + レースガード）。
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { useApp } from "@/lib/app";
import { cx } from "@/lib/cx";
import { translateError, useI18n } from "@/i18n";
import { deleteRecording, listRecordings, renameRecording, searchRecordings } from "@/lib/tauri";
import type { SearchHit } from "@/lib/types";
import { formatDateShort, formatDurationHuman } from "@/lib/types";
import { Chip, ConfirmDialog, Spinner } from "@/components/ui";
import { EmptyState } from "@/components/composite";
import { CheckIcon, ClockIcon, PencilIcon, SearchIcon, TrashIcon, XIcon } from "@/components/icons";

type Filter = "all" | "week";
const WEEK_MS = 7 * 24 * 60 * 60 * 1000;

// FTS5 snippet の [..] マッチ部分を <mark> に。ダーク: 文字=brand-tint / 地=indigo 25%。
function highlight(snippet: string): ReactNode[] {
  const parts: ReactNode[] = [];
  const re = /\[([^\]]*)\]/g;
  let last = 0;
  let key = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(snippet)) !== null) {
    if (m.index > last) parts.push(snippet.slice(last, m.index));
    parts.push(
      <mark
        key={key++}
        className="rounded bg-[rgba(99,102,241,0.25)] px-1 text-brand-tint"
      >
        {m[1]}
      </mark>,
    );
    last = re.lastIndex;
  }
  if (last < snippet.length) parts.push(snippet.slice(last));
  return parts;
}

export function HistoryView() {
  const { navigate, toast, refreshRecents } = useApp();
  const { t, lang } = useI18n();
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  // null = 初回ロード中。再検索では null に戻さず前結果を残す（チラつき防止・旧実装踏襲）。
  const [items, setItems] = useState<SearchHit[] | null>(null);
  // 削除は取り消し不可なので確認を一段挟む。pending = 確認中の対象。
  const [pending, setPending] = useState<SearchHit | null>(null);
  const [deleting, setDeleting] = useState(false);
  // タイトルのインライン編集。editing = 編集中の録音 id。
  const [editing, setEditing] = useState<string | null>(null);
  const [editValue, setEditValue] = useState("");
  const [savingRename, setSavingRename] = useState(false);
  const reqId = useRef(0);

  const searching = query.trim().length > 0;

  const load = useCallback(
    async (q: string) => {
      const my = ++reqId.current;
      try {
        const trimmed = q.trim();
        const next: SearchHit[] = trimmed
          ? await searchRecordings(trimmed)
          : (await listRecordings()).map((r) => ({ recording: r, snippet: "" }));
        if (my === reqId.current) setItems(next);
      } catch (e) {
        if (my === reqId.current) toast(translateError(e, t), "error");
      }
    },
    [toast, t],
  );

  // 250ms デバウンス（マウント時の初回 load も兼ねる）。
  useEffect(() => {
    const t = setTimeout(() => load(query), 250);
    return () => clearTimeout(t);
  }, [query, load]);

  const confirmRemove = useCallback(async () => {
    if (!pending) return;
    setDeleting(true);
    try {
      await deleteRecording(pending.recording.id);
      await load(query);
      refreshRecents();
      toast(t.history.deleted, "success");
      setPending(null);
    } catch (e) {
      toast(translateError(e, t), "error");
    } finally {
      setDeleting(false);
    }
  }, [pending, load, query, refreshRecents, toast, t]);

  const beginEdit = useCallback((h: SearchHit) => {
    setEditing(h.recording.id);
    setEditValue(h.recording.title ?? "");
  }, []);

  const cancelEdit = useCallback(() => {
    setEditing(null);
    setEditValue("");
  }, []);

  const saveRename = useCallback(
    async (id: string) => {
      if (savingRename) return;
      setSavingRename(true);
      try {
        await renameRecording(id, editValue);
        setEditing(null);
        setEditValue("");
        await load(query);
        refreshRecents();
        toast(t.history.renamed, "success");
      } catch (e) {
        toast(translateError(e, t), "error");
      } finally {
        setSavingRename(false);
      }
    },
    [savingRename, editValue, load, query, refreshRecents, toast, t],
  );

  // 「今週」はクライアント側で created_at が直近 7 日以内に絞り込み。
  const filtered = useMemo(() => {
    const list = items ?? [];
    if (filter !== "week") return list;
    const since = Date.now() - WEEK_MS;
    return list.filter((h) => new Date(h.recording.created_at).getTime() >= since);
  }, [items, filter]);

  const count = filtered.length;

  return (
    <div className="mx-auto flex max-w-[840px] flex-col gap-0 px-8 py-8">
      <h1 className="mb-4 text-[17px] font-bold text-ink">{t.history.title}</h1>

      {/* 検索バー */}
      <div className="flex items-center gap-2.5 rounded-[11px] border border-border-3 bg-surface-2 px-4 py-3">
        <SearchIcon size={16} className="shrink-0 text-faint" />
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={t.history.searchPlaceholder}
          className="min-w-0 flex-1 bg-transparent text-[14px] text-ink outline-none placeholder:text-faint"
        />
        {query && (
          <button
            onClick={() => setQuery("")}
            aria-label={t.history.clearSearch}
            className="shrink-0 text-dim transition-colors hover:text-body"
          >
            <XIcon size={16} />
          </button>
        )}
      </div>

      {/* フィルタ chip */}
      <div className="my-4 flex items-center gap-1.5">
        <Chip active={filter === "all"} onClick={() => setFilter("all")}>
          {t.history.filters.all}
        </Chip>
        <Chip
          className="text-dim"
          onClick={() => toast(t.history.filters.notReady, "info")}
        >
          {t.history.filters.withSummary}
          <span className="ml-1 text-[10px] text-faint">{t.history.filters.comingSoon}</span>
        </Chip>
        <Chip
          className="text-dim"
          onClick={() => toast(t.history.filters.notReady, "info")}
        >
          {t.history.filters.withSpeakers}
          <span className="ml-1 text-[10px] text-faint">{t.history.filters.comingSoon}</span>
        </Chip>
        <Chip active={filter === "week"} onClick={() => setFilter("week")}>
          {t.history.filters.week}
        </Chip>
        {items && count > 0 && (
          <span className="ml-auto self-center font-mono text-[11px] text-dim tnum">
            {searching ? t.history.countMatch(count) : t.history.count(count)}
          </span>
        )}
      </div>

      {/* 結果リスト */}
      {items === null ? (
        <div className="flex items-center justify-center gap-2 py-16 text-[13px] text-muted">
          <Spinner size={16} /> {t.common.loading}
        </div>
      ) : count === 0 ? (
        <EmptyState
          icon={<ClockIcon size={28} />}
          title={
            searching
              ? t.history.empty.noMatch(query.trim())
              : filter === "week" && (items?.length ?? 0) > 0
                ? t.history.empty.noneThisWeek
                : t.history.empty.none
          }
          hint={searching || filter === "week" ? undefined : t.history.empty.hint}
        />
      ) : (
        <div className="flex flex-col gap-2.5">
          {filtered.map((h) => {
            const r = h.recording;
            const isEditing = editing === r.id;
            return (
              <div
                key={r.id}
                onClick={() => {
                  if (!isEditing) navigate({ view: "detail", id: r.id });
                }}
                className={cx(
                  "group relative rounded-card border border-border bg-surface px-[17px] py-[15px] transition-colors",
                  isEditing ? "cursor-default" : "cursor-pointer hover:bg-hover",
                )}
              >
                <div className="flex items-center justify-between gap-3">
                  {isEditing ? (
                    <input
                      autoFocus
                      value={editValue}
                      onChange={(e) => setEditValue(e.target.value)}
                      onClick={(e) => e.stopPropagation()}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          e.preventDefault();
                          void saveRename(r.id);
                        } else if (e.key === "Escape") {
                          e.preventDefault();
                          cancelEdit();
                        }
                      }}
                      placeholder={t.common.untitledRecording}
                      className="min-w-0 flex-1 rounded-[7px] border border-border-3 bg-surface-2 px-2.5 py-1.5 text-[14px] font-semibold text-ink outline-none focus:border-brand"
                    />
                  ) : (
                    <div className="min-w-0 truncate text-[14px] font-semibold text-ink">
                      {r.title || t.common.untitled}
                    </div>
                  )}
                  <div className="flex shrink-0 items-center gap-2">
                    {isEditing ? (
                      <>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            void saveRename(r.id);
                          }}
                          disabled={savingRename}
                          aria-label={t.common.save}
                          title={t.common.save}
                          className="flex h-7 w-7 items-center justify-center rounded-btn text-green transition-colors hover:bg-surface-2 disabled:opacity-50"
                        >
                          {savingRename ? <Spinner size={14} /> : <CheckIcon size={16} />}
                        </button>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            cancelEdit();
                          }}
                          disabled={savingRename}
                          aria-label={t.common.cancel}
                          title={t.common.cancel}
                          className="flex h-7 w-7 items-center justify-center rounded-btn text-dim transition-colors hover:bg-surface-2 hover:text-body disabled:opacity-50"
                        >
                          <XIcon size={15} />
                        </button>
                      </>
                    ) : (
                      <>
                        <span className="font-mono text-[11px] text-dim tnum">
                          {formatDateShort(r.created_at, lang)} ·{" "}
                          {formatDurationHuman(r.duration_ms, lang)}
                        </span>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            beginEdit(h);
                          }}
                          aria-label={t.history.renameTitle}
                          title={t.history.renameTitle}
                          className={cx(
                            "flex h-7 w-7 items-center justify-center rounded-btn text-dim opacity-55 transition-all",
                            "hover:bg-surface-2 hover:text-body hover:opacity-100 group-hover:opacity-100",
                          )}
                        >
                          <PencilIcon size={14} />
                        </button>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            setPending(h);
                          }}
                          aria-label={t.common.delete}
                          title={t.common.delete}
                          className={cx(
                            // 常時うっすら表示（見つかる）→ 行/ボタンホバーで濃く
                            "flex h-7 w-7 items-center justify-center rounded-btn text-dim opacity-55 transition-all",
                            "hover:bg-surface-2 hover:text-red-light hover:opacity-100 group-hover:opacity-100",
                          )}
                        >
                          <TrashIcon size={15} />
                        </button>
                      </>
                    )}
                  </div>
                </div>
                {h.snippet && !isEditing && (
                  <div className="mt-2 text-[12.5px] leading-[1.6] text-muted">
                    {highlight(h.snippet)}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      <ConfirmDialog
        open={pending !== null}
        title={t.history.deleteConfirmTitle}
        body={
          pending
            ? t.history.deleteConfirmBody(pending.recording.title || t.common.untitled)
            : undefined
        }
        busy={deleting}
        onConfirm={confirmRemove}
        onCancel={() => setPending(null)}
      />
    </div>
  );
}
