// 録音の再生バー。文字起こしと連動させるため、親が位置を読んで（onTime）動かせる（ref の seek/toggle）。
import { useImperativeHandle, useRef, useState, type Ref } from "react";
import { useI18n } from "@/i18n";
import { formatDuration } from "@/lib/types";
import { PauseIcon, PlayIcon } from "@/components/icons";

/** 親（詳細画面）から再生を操作するためのハンドル（#110）。 */
export interface AudioPlayerHandle {
  /** ms へ移動する。play=true なら再生も始める。 */
  seek: (ms: number, play?: boolean) => void;
  toggle: () => void;
  isPlaying: () => boolean;
  /** 現在位置から deltaMs だけ進める（負で戻す・#111）。 */
  skip: (deltaMs: number) => void;
}

/** 再生速度の候補（#111）。押すたびに次へ回る。 */
export const PLAYBACK_RATES = [1, 1.25, 1.5, 2] as const;
export function nextRate(rate: number): number {
  const i = PLAYBACK_RATES.indexOf(rate as (typeof PLAYBACK_RATES)[number]);
  return PLAYBACK_RATES[(i + 1) % PLAYBACK_RATES.length];
}

export function AudioPlayer({
  src,
  fallbackDurationMs,
  onTime,
  onPlayingChange,
  ref,
}: {
  src: string;
  fallbackDurationMs: number;
  /** 再生位置（ms）が変わるたびに呼ぶ。文字起こしの現在行の強調に使う。 */
  onTime?: (ms: number) => void;
  onPlayingChange?: (playing: boolean) => void;
  ref?: Ref<AudioPlayerHandle>;
}) {
  return (
    <SourceAudioPlayer
      key={src}
      src={src}
      fallbackDurationMs={fallbackDurationMs}
      onTime={onTime}
      onPlayingChange={onPlayingChange}
      handleRef={ref}
    />
  );
}

function SourceAudioPlayer({
  src,
  fallbackDurationMs,
  onTime,
  onPlayingChange,
  handleRef,
}: {
  src: string;
  fallbackDurationMs: number;
  onTime?: (ms: number) => void;
  onPlayingChange?: (playing: boolean) => void;
  handleRef?: Ref<AudioPlayerHandle>;
}) {
  const { t } = useI18n();
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [playing, setPlayingState] = useState(false);
  const [currentMs, setCurrentMsState] = useState(0);
  const [mediaDurationMs, setMediaDurationMs] = useState<number | null>(null);
  const [rate, setRate] = useState<number>(1);
  const fallback = Number.isFinite(fallbackDurationMs) && fallbackDurationMs > 0 ? fallbackDurationMs : 0;
  const durationMs = mediaDurationMs ?? fallback;

  const setPlaying = (v: boolean) => {
    setPlayingState(v);
    onPlayingChange?.(v);
  };
  const setCurrentMs = (ms: number) => {
    setCurrentMsState(ms);
    onTime?.(ms);
  };

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

  const seekTo = (ms: number, play = false) => {
    const a = audioRef.current;
    if (!a) return;
    const clamped = Math.max(0, durationMs > 0 ? Math.min(ms, durationMs) : ms);
    a.currentTime = clamped / 1000;
    setCurrentMs(clamped);
    if (play && a.paused) a.play().catch(() => setPlaying(false));
  };

  const skip = (deltaMs: number) => {
    const a = audioRef.current;
    if (!a) return;
    seekTo(a.currentTime * 1000 + deltaMs);
  };

  const cycleRate = () => {
    const next = nextRate(rate);
    setRate(next);
    if (audioRef.current) audioRef.current.playbackRate = next;
  };

  useImperativeHandle(handleRef, () => ({
    seek: seekTo,
    toggle,
    isPlaying: () => !!audioRef.current && !audioRef.current.paused,
    skip,
  }));

  const seekFromClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (durationMs <= 0) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const ratio = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
    seekTo(ratio * durationMs);
  };

  const pct = durationMs > 0 ? Math.min(100, (currentMs / durationMs) * 100) : 0;

  return (
    <div className="flex items-center gap-3">
      <audio
        ref={audioRef}
        src={src}
        preload="metadata"
        onLoadedMetadata={(e) => {
          readDuration(e.currentTarget);
          e.currentTarget.playbackRate = rate;
        }}
        onDurationChange={(e) => readDuration(e.currentTarget)}
        onEmptied={resetMedia}
        onError={resetMedia}
        onTimeUpdate={(e) => setCurrentMs(e.currentTarget.currentTime * 1000)}
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onEnded={(e) => {
          // 表示だけでなく実際の再生位置も先頭へ戻す。戻さないと、次の再生や −5 が末尾から始まる。
          e.currentTarget.currentTime = 0;
          setPlaying(false);
          setCurrentMs(0);
        }}
      />
      <button
        onClick={toggle}
        aria-label={playing ? t.detail.audio.pause : t.detail.audio.play}
        title={t.detail.audio.spaceHint}
        className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-border-2 bg-surface-2 text-ink transition-colors hover:bg-hover"
      >
        {playing ? <PauseIcon size={15} /> : <PlayIcon size={15} />}
      </button>
      <button
        onClick={() => skip(-5000)}
        aria-label={t.detail.audio.back5}
        title={`${t.detail.audio.back5}（←）`}
        className="shrink-0 rounded-tag px-1.5 py-1 font-mono text-[11px] text-sub transition-colors hover:bg-hover hover:text-ink"
      >
        −5
      </button>
      <button
        onClick={() => skip(15000)}
        aria-label={t.detail.audio.forward15}
        title={`${t.detail.audio.forward15}（→）`}
        className="shrink-0 rounded-tag px-1.5 py-1 font-mono text-[11px] text-sub transition-colors hover:bg-hover hover:text-ink"
      >
        +15
      </button>
      <div
        onClick={seekFromClick}
        onKeyDown={(e) => {
          if (e.key === "ArrowLeft") skip(-5000);
          else if (e.key === "ArrowRight") skip(15000);
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
        tabIndex={0}
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
      <span className="shrink-0 font-mono text-[12px] text-muted tnum">
        {formatDuration(currentMs)} / {formatDuration(durationMs)}
      </span>
      <button
        onClick={cycleRate}
        aria-label={t.detail.audio.rate(rate)}
        title={t.detail.audio.rate(rate)}
        className="w-12 shrink-0 rounded-tag border border-border-2 py-0.5 font-mono text-[11px] text-sub transition-colors hover:bg-hover hover:text-ink"
      >
        {rate}×
      </button>
    </div>
  );
}
