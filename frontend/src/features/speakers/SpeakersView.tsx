// 話者ライブラリ（Studio 09 / ADR-0018）— 実機能。
// 端末内に登録した人物（声紋）の管理。録音ごとの照合は詳細画面の SpeakerPanel で行う。
import { useEffect, useState } from "react";
import { useApp } from "@/lib/app";
import { translateError, useI18n } from "@/i18n";
import { speakerInk } from "@/lib/types";
import { SectionLabel } from "@/components/ui";
import { PrivacyBar } from "@/components/composite";
import {
  UsersIcon,
  ShieldIcon,
  VideoIcon,
  PlusIcon,
  TrashIcon,
} from "@/components/icons";
import {
  listSpeakerLibrary,
  addSpeakerToLibrary,
  renameSpeakerLibrary,
  deleteSpeakerLibrary,
  type LibrarySpeaker,
} from "@/lib/tauri";

/** 30px のカラー丸アバター（地色は id 由来、文字は白）。 */
function Avatar({ id, initial }: { id: string; initial: string }) {
  return (
    <span
      className="flex h-[30px] w-[30px] shrink-0 items-center justify-center rounded-full text-[12px] font-semibold text-white"
      style={{ background: speakerInk(id).dot }}
    >
      {initial}
    </span>
  );
}

export function SpeakersView() {
  const { toast } = useApp();
  const { t } = useI18n();
  const [library, setLibrary] = useState<LibrarySpeaker[]>([]);
  const [loading, setLoading] = useState(true);
  const [newName, setNewName] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);

  const reload = async () => {
    try {
      setLibrary(await listSpeakerLibrary());
    } catch (e) {
      toast(translateError(e, t), "error");
    } finally {
      setLoading(false);
    }
  };
  useEffect(() => {
    void reload();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const add = async () => {
    const name = newName.trim();
    if (!name) return;
    try {
      await addSpeakerToLibrary(name);
      setNewName("");
      await reload();
    } catch (e) {
      toast(translateError(e, t), "error");
    }
  };

  const commitRename = async (v: LibrarySpeaker) => {
    const name = draft.trim();
    setEditing(null);
    if (!name || name === v.name) return;
    try {
      await renameSpeakerLibrary(v.id, name);
      await reload();
    } catch (e) {
      toast(translateError(e, t), "error");
    }
  };

  const remove = async (id: string) => {
    setConfirmDelete(null);
    try {
      await deleteSpeakerLibrary(id);
      await reload();
    } catch (e) {
      toast(translateError(e, t), "error");
    }
  };

  return (
    <div className="mx-auto flex max-w-[760px] flex-col gap-5 px-8 py-7">
      {/* ヘッダ */}
      <header className="flex items-center gap-2.5">
        <UsersIcon size={20} className="text-brand-light" />
        <h1 className="text-[17px] font-bold text-ink">{t.speakers.title}</h1>
      </header>

      {/* プライバシーカード */}
      <div className="flex items-start gap-2.5 rounded-card border border-green/20 bg-green/[0.07] px-4 py-3">
        <ShieldIcon size={16} className="mt-px shrink-0 text-green" />
        <p className="text-[12px] leading-relaxed text-green-light">{t.speakers.privacyNote}</p>
      </div>

      {/* 新規登録 */}
      <div className="flex items-center gap-2">
        <input
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void add();
          }}
          placeholder={t.speakers.addPlaceholder}
          className="min-w-0 flex-1 rounded-md border border-border bg-surface px-3 py-2 text-[13px] text-ink outline-none placeholder:text-muted focus:border-brand"
        />
        <button
          onClick={() => void add()}
          disabled={!newName.trim()}
          className="flex items-center gap-1.5 rounded-md border-0 bg-brand/[0.14] px-3 py-2 text-[12px] font-medium text-brand-light transition-colors hover:bg-brand/25 disabled:opacity-40"
        >
          <PlusIcon size={14} />
          {t.speakers.add}
        </button>
      </div>

      {/* 登録済みの話者 */}
      <section className="flex flex-col gap-3">
        <SectionLabel>{t.speakers.registered}</SectionLabel>
        {loading ? (
          <p className="text-[12px] text-muted">{t.common.loading}</p>
        ) : library.length === 0 ? (
          <p className="rounded-card border border-dashed border-border bg-surface/50 px-4 py-6 text-center text-[12px] leading-relaxed text-muted">
            {t.speakers.empty}
          </p>
        ) : (
          <div className="grid grid-cols-3 gap-3">
            {library.map((v) => (
              <div
                key={v.id}
                className="group relative rounded-card border border-border bg-surface p-4"
              >
                <div className="flex items-center gap-2.5">
                  <Avatar id={v.id} initial={v.name.slice(0, 1)} />
                  <div className="min-w-0 flex-1">
                    {editing === v.id ? (
                      <input
                        autoFocus
                        value={draft}
                        onChange={(e) => setDraft(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") e.currentTarget.blur();
                          else if (e.key === "Escape") {
                            setEditing(null);
                            e.currentTarget.blur();
                          }
                        }}
                        onBlur={() => void commitRename(v)}
                        className="w-full min-w-0 rounded-md border border-border-3 bg-surface-2 px-2 py-1 text-[13px] text-ink outline-none focus:border-brand"
                      />
                    ) : (
                      <button
                        onClick={() => {
                          setEditing(v.id);
                          setDraft(v.name);
                        }}
                        title={t.common.clickToRename}
                        className="block truncate border-0 bg-transparent p-0 text-left text-[13px] font-semibold text-ink"
                      >
                        {v.name}
                      </button>
                    )}
                  </div>
                </div>
                <div className="mt-2.5 font-mono text-[11px] text-muted tnum">
                  {t.speakers.identifiedCount(v.identified_count)}
                </div>
                {/* 削除 */}
                {confirmDelete === v.id ? (
                  <div className="absolute right-2 top-2 flex items-center gap-1">
                    <button
                      onClick={() => void remove(v.id)}
                      className="rounded border-0 bg-red/15 px-1.5 py-0.5 text-[10px] text-red transition-colors hover:bg-red/25"
                    >
                      {t.common.delete}
                    </button>
                    <button
                      onClick={() => setConfirmDelete(null)}
                      className="rounded border-0 bg-surface-2 px-1.5 py-0.5 text-[10px] text-sub"
                    >
                      {t.speakers.cancelDelete}
                    </button>
                  </div>
                ) : (
                  <button
                    onClick={() => setConfirmDelete(v.id)}
                    title={t.common.delete}
                    className="absolute right-2 top-2 rounded border-0 bg-transparent p-1 text-muted opacity-0 transition-opacity hover:text-red group-hover:opacity-100"
                  >
                    <TrashIcon size={13} />
                  </button>
                )}
              </div>
            ))}
          </div>
        )}
      </section>

      <PrivacyBar>
        <VideoIcon size={14} className="shrink-0 text-purple" />
        {t.speakers.footer}
      </PrivacyBar>
    </div>
  );
}
