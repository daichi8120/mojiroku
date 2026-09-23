// 録音中（Studio 03）。マイクは Home で開始済み。タイマー + 実際の入力音量 + 停止 / 破棄。
// 停止 → stop_mic_recording（音声確定＋ジョブ投入で即返す） → 詳細へ（進捗は DetailView が job://update で表示）。
import { useEffect, useRef, useState } from "react";
import { translateError, useI18n } from "@/i18n";
import { useApp } from "@/lib/app";
import { cancelMicRecording, stopMicRecording } from "@/lib/tauri";
import { elapsedSeconds, formatDuration } from "@/lib/types";
import { SILENCE_WARN_SEC, useRecordingLevels } from "@/lib/levels";
import { LevelMeter } from "@/components/composite";
import { ConfirmDialog } from "@/components/ui";
import { StopIcon } from "@/components/icons";

export function RecordingView({
  diarize,
  title,
  recordOnly = false,
}: {
  diarize: boolean;
  title?: string;
  recordOnly?: boolean;
}) {
  const { navigate, toast, refreshRecents } = useApp();
  const { t } = useI18n();
  // 経過時間は開始時刻からの差分で出す（MeetingView と同方式）。setInterval は再描画の
  // トリガにのみ使い、発火回数は数えない（数えると誤差が過小方向へ累積する。Issue #6）。
  const [startedAt] = useState(() => Date.now());
  // 停止を押した時刻。以降は表示を止める（バックエンドの確定待ちの間もカウントし続けないため）。
  const [stoppedAt, setStoppedAt] = useState<number | null>(null);
  const [stopping, setStopping] = useState(false);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  // 実際のマイク入力（#113）。無音が続いたら、停止してから気づく前に知らせる。
  const levels = useRecordingLevels(!stopping);
  const silent = (levels.mic?.silentSec ?? 0) >= SILENCE_WARN_SEC;
  const [, forceTick] = useState(0);
  const timer = useRef<number | null>(null);

  useEffect(() => {
    timer.current = window.setInterval(() => forceTick((n) => n + 1), 1000);
    return () => {
      if (timer.current !== null) clearInterval(timer.current);
    };
  }, []);

  const elapsed = elapsedSeconds(startedAt, stoppedAt ?? Date.now());

  const stop = async () => {
    if (timer.current !== null) {
      clearInterval(timer.current);
      timer.current = null;
    }
    setStoppedAt(Date.now());
    setStopping(true);
    try {
      const res = await stopMicRecording(diarize, title, recordOnly);
      refreshRecents();
      navigate({ view: "detail", id: res.recording_id });
    } catch (e) {
      toast(translateError(e, t), "error");
      navigate({ view: "home" });
    }
  };

  const discard = async () => {
    setConfirmDiscard(false);
    setStopping(true);
    try {
      await cancelMicRecording();
      toast(t.recording.discarded, "info");
    } catch (e) {
      toast(translateError(e, t), "error");
    }
    navigate({ view: "home" });
  };

  return (
    <div className="flex min-h-full flex-col items-center justify-center gap-8 px-8 py-12">
      <div className="flex items-center gap-2.5 text-[14px] text-red-light">
        <span className="h-2.5 w-2.5 animate-mjpulse rounded-full bg-red" />
        {recordOnly ? t.recording.statusRecordOnly : t.recording.status}
      </div>

      <div className="font-mono text-[58px] font-medium leading-none text-ink tnum">
        {formatDuration(elapsed * 1000)}
      </div>

      <div className="flex w-full max-w-[520px] flex-col items-center gap-2">
        <LevelMeter value={levels.mic?.meter ?? 0} segments={32} height={14} label={t.recording.micLevel} className="w-full" />
        <p
          role={silent ? "alert" : undefined}
          className={silent ? "text-[13px] text-amber" : "text-[12px] text-muted"}
        >
          {silent ? t.recording.silentWarning : t.recording.micLevel}
        </p>
      </div>

      <div className="flex items-center gap-3">
        <button
          onClick={() => setConfirmDiscard(true)}
          disabled={stopping}
          className="h-12 rounded-full border border-border-2 px-5 text-[14px] text-sub transition-colors hover:bg-hover hover:text-ink disabled:opacity-60"
        >
          {t.recording.discard}
        </button>
        <button
          onClick={stop}
          disabled={stopping}
          className="inline-flex h-12 items-center gap-2.5 rounded-full bg-danger px-7 text-[14px] font-medium text-white ring-4 ring-red/20 transition-colors hover:bg-red-light disabled:opacity-60"
        >
          <StopIcon size={18} />
          {recordOnly ? t.recording.stopAndSaveOnly : t.recording.stopAndTranscribe}
        </button>
      </div>

      <p className="text-[12px] text-muted">
        {recordOnly
          ? t.recording.recordOnlyHint
          : `${diarize ? t.recording.diarizeOn : t.recording.diarizeOff} · ${t.recording.footer}`}
      </p>

      <ConfirmDialog
        open={confirmDiscard}
        title={t.recording.discardConfirmTitle}
        body={t.recording.discardConfirmBody}
        confirmLabel={t.recording.discard}
        onConfirm={() => void discard()}
        onCancel={() => setConfirmDiscard(false)}
      />
    </div>
  );
}
