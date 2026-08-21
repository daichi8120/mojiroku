// 右ペインの話者リスト。SpeakerChip 表示 → クリックで inline 改名（録音内の表示名）。
// さらに話者ライブラリ（ADR-0018）への対応づけ: 声紋でのサジェスト確認・新規登録・選択・解除。
import { useEffect, useRef, useState } from "react";
import { speakerName, type Speaker } from "@/lib/types";
import {
  renameSpeaker,
  identifySpeakers,
  listSpeakerLibrary,
  linkSpeaker,
  unlinkSpeaker,
  addSpeakerToLibrary,
  type LibrarySpeaker,
  type SpeakerMatchSuggestion,
} from "@/lib/tauri";
import { useApp } from "@/lib/app";
import { translateError, useI18n } from "@/i18n";
import { SpeakerChip } from "@/components/composite";
import { CheckIcon, PlusIcon, XIcon } from "@/components/icons";

export function SpeakerPanel({
  speakers,
  recordingId,
  onRenamed,
}: {
  speakers: Speaker[];
  recordingId: string;
  /** 改名成功時に親 detail.speakers を更新するコールバック。 */
  onRenamed: (speakerId: string, displayName: string | null) => void;
}) {
  const { toast } = useApp();
  const { t, lang } = useI18n();
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  // Enter は blur 経由で commit するため、Escape のときだけ blur commit を抑止する。
  const skipBlur = useRef(false);

  // ── 話者ライブラリ照合（ADR-0018） ──
  const [suggestions, setSuggestions] = useState<Record<string, SpeakerMatchSuggestion>>({});
  const [library, setLibrary] = useState<LibrarySpeaker[]>([]);
  const [picker, setPicker] = useState<string | null>(null); // 対応づけ中の speakerId
  const [regName, setRegName] = useState("");

  const reloadIdentify = async () => {
    try {
      const [sug, lib] = await Promise.all([
        identifySpeakers(recordingId),
        listSpeakerLibrary(),
      ]);
      const map: Record<string, SpeakerMatchSuggestion> = {};
      for (const s of sug) map[s.speaker_id] = s;
      setSuggestions(map);
      setLibrary(lib);
    } catch (e) {
      toast(translateError(e, t), "error");
    }
  };
  useEffect(() => {
    void reloadIdentify();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [recordingId]);

  const begin = (sp: Speaker) => {
    setEditing(sp.id);
    setDraft(sp.display_name ?? sp.label);
  };

  const commit = async (sp: Speaker) => {
    const trimmed = draft.trim();
    const next = trimmed === "" ? null : trimmed;
    setEditing(null);
    if (next === sp.display_name) return; // 変化なし
    // 未命名のまま自動ラベル（「話者1」等）を編集せず確定 = 実質変化なし。
    if (sp.display_name === null && next === sp.label) return;
    try {
      await renameSpeaker(recordingId, sp.id, next);
      onRenamed(sp.id, next);
    } catch (e) {
      toast(translateError(e, t), "error");
    }
  };

  const link = async (speakerId: string, libraryId: string, confidence: number) => {
    try {
      await linkSpeaker(recordingId, speakerId, libraryId, confidence);
      setPicker(null);
      setRegName("");
      await reloadIdentify();
    } catch (e) {
      toast(translateError(e, t), "error");
    }
  };

  const unlink = async (speakerId: string) => {
    try {
      await unlinkSpeaker(recordingId, speakerId);
      await reloadIdentify();
    } catch (e) {
      toast(translateError(e, t), "error");
    }
  };

  const registerAndLink = async (speakerId: string) => {
    const name = regName.trim();
    if (!name) return;
    try {
      const id = await addSpeakerToLibrary(name);
      await link(speakerId, id, 1.0);
    } catch (e) {
      toast(translateError(e, t), "error");
    }
  };

  const libName = (id: string) =>
    library.find((l) => l.id === id)?.name ?? t.detail.speakerPanel.librarySpeaker;

  /** 話者 1 件のライブラリ対応づけ行を描画する。コンポーネントではなく描画関数として
   *  呼ぶ（{identifyRow(sp)}）: JSX 要素にすると毎 render で型が変わり subtree が remount し、
   *  登録 input が 1 文字ごとにフォーカスを失う。hooks を持たないので直接呼び出しで安全。 */
  const identifyRow = (sp: Speaker) => {
    const s = suggestions[sp.id];
    // 確定済み: リンク先を表示 + 解除。
    if (s?.linked_library_id) {
      return (
        <div className="mt-1 flex items-center gap-1.5 pl-0.5 text-[11px]">
          <CheckIcon size={12} className="shrink-0 text-green" />
          <span className="truncate text-green-light">{libName(s.linked_library_id)}</span>
          <button
            onClick={() => void unlink(sp.id)}
            className="ml-auto border-0 bg-transparent text-[10.5px] text-dim transition-colors hover:text-sub"
          >
            {t.detail.speakerPanel.unlink}
          </button>
        </div>
      );
    }
    // 声紋が無い/短い: 対象外（手動対応づけは picker から可能）。
    const noPrint = !s || s.below_enroll_gate;

    if (picker === sp.id) {
      const suggestedId = s?.top_library_id ?? null;
      return (
        <div className="mt-1.5 flex flex-col gap-1.5 rounded-md border border-border-3 bg-surface-2 p-2">
          {library.length > 0 && (
            <div className="flex flex-col gap-0.5">
              {[...library]
                .sort((a, b) => (a.id === suggestedId ? -1 : b.id === suggestedId ? 1 : 0))
                .map((l) => (
                  <button
                    key={l.id}
                    onClick={() =>
                      void link(sp.id, l.id, l.id === suggestedId ? (s?.confidence ?? 1) : 1)
                    }
                    className="flex items-center gap-1.5 rounded border-0 bg-transparent px-1.5 py-1 text-left text-[11.5px] text-ink transition-colors hover:bg-hover"
                  >
                    <span className="truncate">{l.name}</span>
                    {l.id === suggestedId && s?.confidence != null && (
                      <span className="ml-auto shrink-0 rounded bg-green/15 px-1.5 py-0.5 font-mono text-[10px] text-green tnum">
                        {Math.round(s.confidence * 100)}%
                      </span>
                    )}
                  </button>
                ))}
            </div>
          )}
          <div className="flex items-center gap-1">
            <input
              value={regName}
              onChange={(e) => setRegName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void registerAndLink(sp.id);
                else if (e.key === "Escape") setPicker(null);
              }}
              placeholder={t.detail.speakerPanel.registerPlaceholder}
              className="min-w-0 flex-1 rounded border border-border bg-surface px-2 py-1 text-[11.5px] text-ink outline-none placeholder:text-muted focus:border-brand"
            />
            <button
              onClick={() => void registerAndLink(sp.id)}
              disabled={!regName.trim()}
              title={t.detail.speakerPanel.registerAndLink}
              className="flex shrink-0 items-center rounded border-0 bg-brand/15 p-1 text-brand-light transition-colors hover:bg-brand/25 disabled:opacity-40"
            >
              <PlusIcon size={13} />
            </button>
            <button
              onClick={() => {
                setPicker(null);
                setRegName("");
              }}
              title={t.common.close}
              className="flex shrink-0 items-center rounded border-0 bg-transparent p-1 text-muted transition-colors hover:text-sub"
            >
              <XIcon size={13} />
            </button>
          </div>
        </div>
      );
    }

    // 既定行: サジェスト or 対応づけボタン。
    return (
      <div className="mt-1 flex items-center gap-1.5 pl-0.5 text-[11px]">
        {noPrint ? (
          <span className="text-dim">{t.detail.speakerPanel.noVoiceprint}</span>
        ) : s?.top_name ? (
          <button
            onClick={() => void link(sp.id, s.top_library_id!, s.confidence ?? 1)}
            className="flex items-center gap-1 rounded border-0 bg-green/10 px-1.5 py-0.5 text-[11px] text-green-light transition-colors hover:bg-green/20"
            title={t.detail.speakerPanel.confirmSuggestion}
          >
            <CheckIcon size={11} />
            <span className="truncate">{s.top_name}</span>
            {s.confidence != null && (
              <span className="font-mono text-[10px] text-green tnum">
                {Math.round(s.confidence * 100)}%
              </span>
            )}
          </button>
        ) : null}
        <button
          onClick={() => {
            setPicker(sp.id);
            setRegName("");
          }}
          className="ml-auto border-0 bg-transparent text-[10.5px] text-dim transition-colors hover:text-sub"
        >
          {s?.top_name ? t.detail.speakerPanel.someoneElse : t.detail.speakerPanel.link}
        </button>
      </div>
    );
  };

  return (
    <div>
      <div className="mb-3 text-[11px] font-bold uppercase tracking-[0.08em] text-dim">
        {t.detail.speakerPanel.title}
      </div>
      <ul className="flex flex-col gap-3">
        {speakers.map((sp) => (
          <li key={sp.id} className="flex min-w-0 flex-col">
            <div className="flex min-w-0 items-center">
              {editing === sp.id ? (
                <input
                  autoFocus
                  value={draft}
                  onChange={(e) => setDraft(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.currentTarget.blur();
                    } else if (e.key === "Escape") {
                      skipBlur.current = true;
                      setEditing(null);
                      e.currentTarget.blur();
                    }
                  }}
                  onBlur={() => {
                    if (skipBlur.current) {
                      skipBlur.current = false;
                      return;
                    }
                    void commit(sp);
                  }}
                  className="min-w-0 flex-1 rounded-md border border-border-3 bg-surface-2 px-2 py-1 text-[12.5px] text-ink outline-none focus:border-brand"
                />
              ) : (
                <SpeakerChip
                  id={sp.id}
                  name={speakerName(sp.id, speakers, lang)}
                  onClick={() => begin(sp)}
                  title={t.common.clickToRename}
                  block
                />
              )}
            </div>
            {identifyRow(sp)}
          </li>
        ))}
      </ul>
    </div>
  );
}
