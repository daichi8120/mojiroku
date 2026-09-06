// 録音の音声プレーヤ（詳細ビュー）。バックエンドの recording_audio_src が返す asset:// URL を
// <audio> で再生する。File/Mic/会議（結合ミックス <id>.wav）で同一に扱う。
import { useRef, useState } from "react";
import { useI18n } from "@/i18n";
import { formatDuration } from "@/lib/types";
import { PauseIcon, PlayIcon } from "@/components/icons";

export function AudioPlayer({
  src,
  fallbackDurationMs,
}: {
  src: string;
  fallbackDurationMs: number;
}) {
  // src/録音が変わったら頭出しに戻す。
  return <SourceAudioPlayer key={src} src={src} fallbackDurationMs={fallbackDurationMs} />;
}

function SourceAudioPlayer({ src, fallbackDurationMs }: { src: string; fallbackDurationMs: number }) {
  const { t } = useI18n();
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [playing, setPlaying] = useState(false);
  const [currentMs, setCurrentMs] = useState(0);
  // 尺は <audio> のメタデータを優先し、未取得の間は Recording.duration_ms を仮表示。
  const [mediaDurationMs, setMediaDurationMs] = useState<number | null>(null);
  const fallback = Number.isFinite(fallbackDurationMs) && fallbackDurationMs > 0 ? fallbackDurationMs : 0;
  const durationMs = mediaDurationMs ?? fallback;

  const readDuration = (audio: HTMLAudioElement) => {
    const duration = audio.duration;
    setMediaDurationMs(Number.isFinite(duration) && duration > 0 ? duration * 1000 : null);
  };

  const resetMedia = () => {
    setMediaDurationMs(null);
    setPlaying(false);
    setCurrentMs(0);
  };

  const toggle = () => {
    const a = audioRef.current;
    if (!a) return;
    if (a.paused) a.play().catch(() => setPlaying(false));
    else a.pause();
  };

  // スクラバのクリック位置へシーク。
  const seekFromClick = (e: React.MouseEvent<HTMLDivElement>) => {
    const a = audioRef.current;
    if (!a || durationMs <= 0) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const ratio = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
    a.currentTime = (ratio * durationMs) / 1000;
    setCurrentMs(ratio * durationMs);
  };

  const pct = durationMs > 0 ? Math.min(100, (currentMs / durationMs) * 100) : 0;

  return (
    <div className="flex items-center gap-3">
      <audio
        ref={audioRef}
        src={src}
        preload="metadata"
        onLoadedMetadata={(e) => readDuration(e.currentTarget)}
        onDurationChange={(e) => readDuration(e.currentTarget)}
        onEmptied={resetMedia}
        onError={resetMedia}
        onTimeUpdate={(e) => setCurrentMs(e.currentTarget.currentTime * 1000)}
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onEnded={() => {
          setPlaying(false);
          setCurrentMs(0);
        }}
      />
      <button
        onClick={toggle}
        aria-label={playing ? t.detail.audio.pause : t.detail.audio.play}
        className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-border-2 bg-surface-2 text-ink transition-colors hover:bg-hover"
      >
        {playing ? <PauseIcon size={15} /> : <PlayIcon size={15} />}
      </button>
      <div
        onClick={seekFromClick}
        role="slider"
        aria-label={t.detail.audio.seek}
        aria-valuenow={Math.round(pct)}
        aria-valuemin={0}
        aria-valuemax={100}
        className="group relative h-2 flex-1 cursor-pointer rounded-full bg-border-2"
      >
        <div
          className="absolute inset-y-0 left-0 rounded-full bg-brand"
          style={{ width: `${pct}%` }}
        />
        <div
          className="absolute top-1/2 h-3 w-3 -translate-x-1/2 -translate-y-1/2 rounded-full bg-brand-light opacity-0 transition-opacity group-hover:opacity-100"
          style={{ left: `${pct}%` }}
        />
      </div>
      <span className="shrink-0 font-mono text-[11.5px] text-muted tnum">
        {formatDuration(currentMs)} / {formatDuration(durationMs)}
      </span>
    </div>
  );
}
