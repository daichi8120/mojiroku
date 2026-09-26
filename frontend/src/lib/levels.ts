// 録音中の入力音量（#113）。バックエンドが前回以降の最大振幅（0〜1）を返すので、一定間隔で読み、
// メーター表示と「音を拾えていない」警告に使う。以前の波形は飾りで、実際の入力とは無関係だった。
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface RecordingLevels {
  mic: number | null;
  system: number | null;
}

export const recordingLevels = () => invoke<RecordingLevels>("recording_levels");

/** これを下回る振幅が続いたら「音を拾えていない」とみなす（約 -46 dBFS）。 */
export const SILENCE_PEAK = 0.005;
/** 無音がこの秒数続いたら警告する。 */
export const SILENCE_WARN_SEC = 5;
const POLL_MS = 100;

/** 最大振幅 → メーターの長さ（0〜1）。-60 dBFS を 0、0 dBFS を 1 とする対数目盛り。 */
export function peakToMeter(peak: number): number {
  if (!(peak > 0)) return 0;
  const db = 20 * Math.log10(Math.min(1, peak));
  return Math.max(0, Math.min(1, (db + 60) / 60));
}

/** 表示用のなめらかな値。上がるときは即座に、下がるときはゆっくり（針が暴れないように）。 */
export function smoothMeter(prev: number, next: number): number {
  return next >= prev ? next : Math.max(next, prev - 0.06);
}

export interface TrackLevel {
  /** メーターの長さ 0〜1。 */
  meter: number;
  /** 無音が続いている秒数。 */
  silentSec: number;
}

/**
 * 録音中だけ音量を読む。トラックが録音されていなければ null。
 * 呼び出しに失敗しても録音は止めない（メーターが動かないだけ）。
 */
export function useRecordingLevels(active: boolean): { mic: TrackLevel | null; system: TrackLevel | null } {
  const [state, setState] = useState<{ mic: TrackLevel | null; system: TrackLevel | null }>({
    mic: null,
    system: null,
  });
  useEffect(() => {
    if (!active) {
      setState({ mic: null, system: null });
      return;
    }
    let alive = true;
    const step = (prev: TrackLevel | null, peak: number | null): TrackLevel | null => {
      if (peak == null) return null;
      const meter = smoothMeter(prev?.meter ?? 0, peakToMeter(peak));
      const silentSec = peak < SILENCE_PEAK ? (prev?.silentSec ?? 0) + POLL_MS / 1000 : 0;
      return { meter, silentSec };
    };
    const h = window.setInterval(() => {
      recordingLevels()
        .then((l) => {
          if (alive) setState((s) => ({ mic: step(s.mic, l.mic), system: step(s.system, l.system) }));
        })
        .catch(() => {});
    }, POLL_MS);
    return () => {
      alive = false;
      window.clearInterval(h);
    };
  }, [active]);
  return state;
}
