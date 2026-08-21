// ホーム / 取り込み（Studio 02）。会議モードを主役に、その下にファイル取込・マイク録音。
// 会議録音はアプリ全体の状態（useApp().meeting）。録音中は二重録音を避けてここからは開始させない。
import { useCallback, useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useApp } from "@/lib/app";
import { cx } from "@/lib/cx";
import { translateError, useI18n } from "@/i18n";
import { startMicRecording, transcribeFile } from "@/lib/tauri";
import { Toggle } from "@/components/ui";
import { PrivacyBar, ValueProps } from "@/components/composite";
import { FileAudioIcon, MicIcon, VideoIcon } from "@/components/icons";

const AUDIO_EXT = ["mp3", "wav", "m4a", "aac", "aiff", "flac", "ogg"];
const isAudio = (p: string) => AUDIO_EXT.some((x) => p.toLowerCase().endsWith(`.${x}`));

export function HomeView() {
  const { navigate, toast, refreshRecents, meeting, startMeeting } = useApp();
  const { t } = useI18n();
  const [diarize, setDiarize] = useState(false);
  // 音声だけ保存（後から文字起こし・ADR-0024 増分5）。ON のとき停止/取込は録音行だけ作りジョブは積まない。
  const [recordOnly, setRecordOnly] = useState(false);
  const [busy, setBusy] = useState(false);
  const [dragOver, setDragOver] = useState(false);

  // drag&drop のクロージャから最新 diarize / recordOnly / busy / 会議状態を読むための ref
  const diarizeRef = useRef(diarize);
  diarizeRef.current = diarize;
  const recordOnlyRef = useRef(recordOnly);
  recordOnlyRef.current = recordOnly;
  const busyRef = useRef(busy);
  busyRef.current = busy;
  const meetingActiveRef = useRef(meeting.status !== "idle");
  meetingActiveRef.current = meeting.status !== "idle";

  // 会議モード（主役）: idle なら開始 → 会議画面へ。録音中なら会議画面へ戻る。
  const beginMeeting = useCallback(async () => {
    if (meeting.status !== "idle") {
      navigate({ view: "meeting" });
      return;
    }
    const r = await startMeeting();
    // started → 録音中ビュー、denied → 開始画面で許可を誘導（startMeeting がトースト済み）。
    if (r !== "error") navigate({ view: "meeting" });
  }, [meeting.status, startMeeting, navigate]);

  const runFile = useCallback(
    async (path: string) => {
      if (meetingActiveRef.current) {
        toast(t.home.meetingBusy, "info");
        return;
      }
      // ref を同期更新して二重起動を防ぐ（state 経由の再レンダー同期だけだと、
      // 同一イベント内に複数ハンドラが発火した場合に両方すり抜ける）。
      if (busyRef.current) return;
      busyRef.current = true;
      setBusy(true);
      try {
        const res = await transcribeFile(
          path,
          diarizeRef.current,
          recordOnlyRef.current,
        );
        refreshRecents();
        navigate({ view: "detail", id: res.recording_id });
      } catch (e) {
        toast(translateError(e, t), "error");
      } finally {
        busyRef.current = false;
        setBusy(false);
      }
    },
    [navigate, toast, refreshRecents, t],
  );

  const pickFile = useCallback(async () => {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: t.home.audioFilterName, extensions: AUDIO_EXT }],
    });
    if (typeof selected === "string") runFile(selected);
  }, [runFile, t]);

  // OS のドラッグ&ドロップ（Tauri webview イベント）。
  // unlisten は Promise で返るため、cleanup が解決前に走っても解除漏れしないよう
  // active フラグ + Promise 経由の解除にする（lib/tauri.ts の useTauriEvent と同じパターン）。
  useEffect(() => {
    let active = true;
    const unlistenP = getCurrentWebview().onDragDropEvent((e) => {
      if (!active) return;
      if (e.payload.type === "over" || e.payload.type === "enter") setDragOver(true);
      else if (e.payload.type === "leave") setDragOver(false);
      else if (e.payload.type === "drop") {
        setDragOver(false);
        if (busyRef.current) return; // 処理中はドロップを無視（二重起動防止）
        const p = e.payload.paths.find(isAudio);
        if (p) runFile(p);
        else if (e.payload.paths.length > 0)
          toast(t.home.unsupportedFile, "error");
      }
    });
    return () => {
      active = false;
      unlistenP.then((un) => un());
    };
  }, [runFile, toast, t]);

  const startMic = useCallback(async () => {
    if (meeting.status !== "idle") {
      toast(t.home.meetingBusy, "info");
      return;
    }
    try {
      await startMicRecording();
      navigate({ view: "recording", diarize, recordOnly });
    } catch (e) {
      toast(translateError(e, t), "error");
    }
  }, [navigate, toast, diarize, recordOnly, meeting.status, t]);

  const recording = meeting.status !== "idle";

  return (
    <div className="mx-auto flex max-w-[760px] flex-col gap-6 px-8 py-10">
      <header>
        <h1 className="text-[17px] font-bold text-ink">{t.home.title}</h1>
        <p className="mt-1 text-[13px] text-muted">{t.home.subtitle}</p>
      </header>

      {/* 会議モード（主役） */}
      <button
        onClick={beginMeeting}
        className="group relative flex items-center gap-4 overflow-hidden rounded-win border border-brand/30 px-6 py-6 text-left transition-[filter] hover:brightness-[1.06]"
        style={{ background: "linear-gradient(135deg,rgba(99,102,241,0.18),rgba(79,70,229,0.10))" }}
      >
        <span className="flex h-14 w-14 shrink-0 items-center justify-center rounded-2xl bg-brand/20 text-brand-light">
          <VideoIcon size={28} />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block text-[15px] font-bold text-ink">{t.home.meetingCard.title}</span>
          <span className="mt-1 block text-[12.5px] leading-relaxed text-muted">
            {t.home.meetingCard.desc}
          </span>
        </span>
        <span
          className="inline-flex h-10 shrink-0 items-center gap-2 rounded-btn px-5 text-[13px] font-medium text-white"
          style={{ background: "linear-gradient(180deg,#6366F1,#4F46E5)" }}
        >
          {recording ? (
            <>
              <span className="h-2 w-2 animate-mjpulse rounded-full bg-white/90" />
              {t.app.meetingBar.backToMeeting}
            </>
          ) : (
            <>
              <span className="h-2.5 w-2.5 rounded-full bg-white/90" />
              {t.home.meetingCard.start}
            </>
          )}
        </span>
      </button>

      {/* その他の取り込み */}
      <div className="flex items-center gap-3 pt-1">
        <span className="h-px flex-1 bg-border" />
        <span className="text-[11px] font-medium uppercase tracking-[0.08em] text-dim">
          {t.home.otherImports}
        </span>
        <span className="h-px flex-1 bg-border" />
      </div>

      {/* ドラッグ&ドロップ枠 */}
      <button
        onClick={pickFile}
        disabled={busy}
        className={cx(
          "flex flex-col items-center justify-center gap-4 rounded-win border-2 border-dashed px-6 py-12 transition-colors",
          dragOver
            ? "border-brand bg-selected"
            : "border-border-2 bg-surface hover:border-border-3 hover:bg-surface-2",
          busy && "opacity-60",
        )}
      >
        <span className="flex h-12 w-12 items-center justify-center rounded-2xl bg-[rgba(99,102,241,0.12)] text-brand-light">
          <FileAudioIcon size={24} />
        </span>
        <span className="text-center">
          <span className="block text-[14px] font-medium text-ink">
            {t.home.dropTitle}
          </span>
          <span className="mt-1 block font-mono text-[11.5px] text-dim">
            mp3 / wav / m4a / flac / aac / ogg
          </span>
        </span>
      </button>

      {/* アクション */}
      <div className="flex flex-wrap items-center gap-3">
        <button
          onClick={pickFile}
          disabled={busy}
          className="inline-flex h-10 items-center gap-2 rounded-btn border border-border-2 bg-surface-2 px-5 text-[13px] font-medium text-ink transition-colors hover:bg-hover disabled:opacity-45"
        >
          <FileAudioIcon size={17} className="text-brand-light" />
          {t.home.chooseFile}
        </button>
        <button
          onClick={startMic}
          disabled={busy}
          className="inline-flex h-10 items-center gap-2 rounded-btn border border-border-2 bg-surface-2 px-5 text-[13px] font-medium text-ink transition-colors hover:bg-hover disabled:opacity-45"
        >
          <MicIcon size={17} className="text-red-light" />
          {t.home.recordMic}
        </button>
      </div>

      {/* 話者分離（音声だけ保存 ON のときは処理を後回しにするので無効化・後で選び直す） */}
      <div className="flex items-start gap-3 rounded-card border border-border bg-surface-2 px-4 py-3.5">
        <Toggle
          checked={diarize}
          onChange={setDiarize}
          disabled={busy || recordOnly}
          label={t.home.diarize.label}
        />
        <div className="min-w-0">
          <div className="text-[13px] font-medium text-ink">{t.home.diarize.title}</div>
          <div className="mt-0.5 text-[12px] text-muted">{t.home.diarize.desc}</div>
        </div>
      </div>

      {/* 音声だけ保存（後から文字起こし・ADR-0024） */}
      <div className="flex items-start gap-3 rounded-card border border-border bg-surface-2 px-4 py-3.5">
        <Toggle
          checked={recordOnly}
          onChange={setRecordOnly}
          disabled={busy}
          label={t.home.recordOnly.label}
        />
        <div className="min-w-0">
          <div className="text-[13px] font-medium text-ink">{t.home.recordOnly.title}</div>
          <div className="mt-0.5 text-[12px] text-muted">{t.home.recordOnly.desc}</div>
        </div>
      </div>

      <ValueProps />

      <PrivacyBar>{t.home.privacy}</PrivacyBar>
    </div>
  );
}
