// 録音中（Studio 03）。マイクは Home で開始済み。タイマー + ライブ波形 + 停止。
// 停止 → stop_mic_recording（音声確定＋ジョブ投入で即返す） → 詳細へ（進捗は DetailView が job://update で表示）。
import { useEffect, useRef, useState } from "react";
import { translateError, useI18n } from "@/i18n";
import { useApp } from "@/lib/app";
import { stopMicRecording } from "@/lib/tauri";
import { formatDuration } from "@/lib/types";
import { Waveform } from "@/components/composite";
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
  const [elapsed, setElapsed] = useState(0);
  const [stopping, setStopping] = useState(false);
  const timer = useRef<number | null>(null);

  useEffect(() => {
    timer.current = window.setInterval(() => setElapsed((e) => e + 1), 1000);
    return () => {
      if (timer.current !== null) clearInterval(timer.current);
    };
  }, []);

  const stop = async () => {
    if (timer.current !== null) {
      clearInterval(timer.current);
      timer.current = null;
    }
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

  return (
    <div className="flex min-h-full flex-col items-center justify-center gap-8 px-8 py-12">
      <div className="flex items-center gap-2.5 text-[14px] text-red-light">
        <span className="h-2.5 w-2.5 animate-mjpulse rounded-full bg-red" />
        {recordOnly ? t.recording.statusRecordOnly : t.recording.status}
      </div>

      <div className="font-mono text-[58px] font-medium leading-none text-ink tnum">
        {formatDuration(elapsed * 1000)}
      </div>

      <Waveform active bars={56} height={56} className="w-full max-w-[520px]" />

      <button
        onClick={stop}
        disabled={stopping}
        className="inline-flex h-12 items-center gap-2.5 rounded-full bg-red px-7 text-[14px] font-medium text-white shadow-[0_0_0_4px_rgba(239,68,68,0.18)] transition-colors hover:bg-red-light disabled:opacity-60"
      >
        <StopIcon size={18} />
        {recordOnly ? t.recording.stopAndSaveOnly : t.recording.stopAndTranscribe}
      </button>

      <p className="text-[12px] text-muted">
        {recordOnly
          ? t.recording.recordOnlyHint
          : `${diarize ? t.recording.diarizeOn : t.recording.diarizeOff} · ${t.recording.footer}`}
      </p>
    </div>
  );
}
