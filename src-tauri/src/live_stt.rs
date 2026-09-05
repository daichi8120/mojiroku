//! 会議モードのリアルタイム文字起こし（増分C・ADR-0017）。
//!
//! キャプチャ中の mic＋system 音声を周期的に whisper で文字起こしし、`meeting://live` イベントで
//! UI のライブ表示へ流す。**使い捨てプレビュー**であり、保存される権威データは停止時の
//! デュアルトラック文字起こし（per-track STT＋ソース合成 = ドリフト免疫）の方。
//!
//! 設計（advisor 確定）:
//! - **完全 best-effort・隔離（ブロッカー要件）**: モデルロード失敗 / whisper エラー / panic は
//!   「ライブ表示なし・録音は継続」へ縮退する。検証済みの録音経路を絶対に巻き込まない。
//! - **A: mix・ラベル無し**: mic＋system を 16k mono に混ぜて 1 回/tick で起こす（ソース帰属＝
//!   あなた/相手 は停止時に確定するので、プレビューには付けない）。
//! - **スライディングウィンドウ**: 確定済みは音声バッファから drain し、未確定 tail だけ毎 tick
//!   起こす（長尺でも tick あたりの仕事とメモリが一定）。tail 末尾 - GUARD より前を確定。
//! - **VAD ＋ RMS ゲート**: VAD で無音を除去しつつ、tail 全体が無音なら RMS で事前スキップする
//!   （完全無音 tail は VAD が空を返し生 PCM にフォールバック → whisper がハルシネーションする。
//!   CLAUDE.md の「ご視聴ありがとうございました」反復）。
//! Since ADR-0035, a present VAD model handles quiet nonzero tails; the RMS gate
//! below is only a fallback when VAD is unavailable. Digital silence is always skipped.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use mojiroku_core::models::{DEFAULT_VAD_MODEL, DEFAULT_WHISPER_MODEL};
use mojiroku_core::stt::{SttEngine, WhisperStt};

use crate::audio::spool::SharedPcm;
use crate::system_audio::{mix_mono, rms, to_playback_mono};

/// ライブ用のマイク共有ハンドル（バッファ, native rate, channels）。
pub type MicHandle = (Arc<SharedPcm>, u32, u16);
/// ライブ用のシステム音声共有ハンドル（バッファ = mono, native rate）。
pub type SystemHandle = (Arc<SharedPcm>, u32);

/// ライブ文字起こしの内部レート（whisper 入力）。
const TARGET_RATE: u32 = 16_000;
/// 1 tick の目標周期。whisper 実行が長引いたら超過分を待たない（adaptive）。
const TICK: Duration = Duration::from_millis(3500);
/// これ未満の tail は起こさない（whisper に十分な文脈を与える）。
const MIN_TAIL_MS: u64 = 2000;
/// tail 末尾からこの分は「未確定」として残す（次 tick で書き換わりうる）。
const COMMIT_GUARD_MS: u64 = 1500;
/// tail がこれを超えたら（連続発話で確定点が出ない等）強制的に前進させる上限。
const MAX_TAIL_MS: u64 = 14000;
/// 表示保持する確定行の上限（古いものから捨てる）。
const MAX_LINES: usize = 80;
/// tail 全体がこの RMS 未満なら無音とみなし whisper に渡さない。
const SILENCE_RMS: f32 = 1e-3;
/// stop 応答性のためのスリープ刻み。
const SLEEP_STEP: Duration = Duration::from_millis(80);

/// Let VAD classify quiet speech; retain the old noise guard without a VAD model.
fn skip_silent_tail(tail: &[f32], vad_available: bool) -> bool {
    if vad_available {
        tail.iter().all(|sample| *sample == 0.0)
    } else {
        rms(tail) < SILENCE_RMS
    }
}

/// UI へ送るライブ行。committed=true は確定（以後書き換えない）、false は未確定 tail。
#[derive(Clone, Serialize)]
pub struct LiveLine {
    pub text: String,
    pub committed: bool,
}

#[derive(Clone, Serialize)]
struct LivePayload {
    lines: Vec<LiveLine>,
}

/// ライブ文字起こしセッション（停止フラグ＋ワーカー join ハンドル）。
pub struct LiveSttSession {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

/// managed state。会議録音の全 teardown 経路（stop/cancel/rollback/離脱）から `stop` を呼ぶ。
pub struct LiveSttState(pub Mutex<Option<LiveSttSession>>);

impl LiveSttState {
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }
}

impl Default for LiveSttState {
    fn default() -> Self {
        Self::new()
    }
}

/// ライブ文字起こしを開始する。**best-effort**: 失敗しても Err は返さず、ライブ表示が出ないだけ。
/// mic/system はキャプチャ中の共有バッファ・レート（`*::live_handle`）。
/// `language` は whisper への言語ヒント（None=自動判定）。呼び出し側がセッション開始時の
/// 設定をスナップショットして渡す＝セッション中の設定変更は次セッションから反映される。
pub fn start(
    state: &LiveSttState,
    app: AppHandle,
    models_dir: std::path::PathBuf,
    mic: Option<MicHandle>,
    system: Option<SystemHandle>,
    language: Option<String>,
) {
    // 既存ワーカーがあれば止める（多重起動防止）。
    stop(state);

    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_for_worker = Arc::clone(&stop_flag);
    let handle = std::thread::spawn(move || {
        // panic ガード: ワーカーの panic を呑み、録音経路へ波及させない。
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_worker(&app, &models_dir, mic, system, language.as_deref(), &stop_for_worker);
        }));
        if result.is_err() {
            eprintln!("ライブ文字起こしワーカーが panic（ライブ表示なし・録音は継続）");
        }
    });

    *state.0.lock().unwrap() = Some(LiveSttSession {
        stop: stop_flag,
        handle,
    });
}

/// ライブ文字起こしを停止して join する。全 teardown 経路から呼ぶ。冪等（未起動でも安全）。
pub fn stop(state: &LiveSttState) {
    let session = state.0.lock().unwrap().take();
    if let Some(s) = session {
        s.stop.store(true, Ordering::Relaxed);
        let _ = s.handle.join();
    }
}

/// 共有バッファから「まだ取り込んでいない」新規サンプルだけを複製して返す（O(new)）。
/// 返り値の bool は「先頭を spool flush に追い越された」印（consumed < base）。
/// true のときは時刻整合が壊れているため、呼び出し側はローカルバッファを捨てて再同期する。
fn take_new(buf: &SharedPcm, consumed: &mut u64) -> (Vec<f32>, bool) {
    let (new_consumed, samples, skipped) = buf.snapshot_from(*consumed);
    *consumed = new_consumed;
    (samples, skipped)
}

/// 再同期用: 2 バッファの**末尾**（=ほぼ現在時刻）を揃え、長い方の先頭を捨てて同長にする。
/// 「index 0 = 同時刻」の前提を作り直す（min 長ミックスは先頭合わせで動くため）。
fn tail_align(a: &mut Vec<f32>, b: &mut Vec<f32>) {
    let n = a.len().min(b.len());
    let da = a.len() - n;
    let db = b.len() - n;
    a.drain(..da);
    b.drain(..db);
}

fn ms_to_samples(ms: u64) -> usize {
    (ms * TARGET_RATE as u64 / 1000) as usize
}

/// 16k バッファ両方の先頭 n サンプルを捨てて前進する（確定済み音声の解放）。両者を同数捨てて
/// インデックス整合（時刻整合）を保つ。
fn drain_front(mic16k: &mut Vec<f32>, sys16k: &mut Vec<f32>, n: usize) {
    let m = n.min(mic16k.len());
    mic16k.drain(..m);
    let s = n.min(sys16k.len());
    sys16k.drain(..s);
}

/// TICK までの残りを stop を見ながら細かくスリープ（stop 応答 ~80ms）。
fn sleep_remainder(t0: Instant, stop: &AtomicBool) {
    while t0.elapsed() < TICK {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        std::thread::sleep(SLEEP_STEP);
    }
}

fn emit(app: &AppHandle, committed: &VecDeque<String>, live: &[String]) {
    let mut lines: Vec<LiveLine> = committed
        .iter()
        .map(|t| LiveLine {
            text: t.clone(),
            committed: true,
        })
        .collect();
    for t in live {
        lines.push(LiveLine {
            text: t.clone(),
            committed: false,
        });
    }
    let _ = app.emit("meeting://live", LivePayload { lines });
}

fn run_worker(
    app: &AppHandle,
    models_dir: &std::path::Path,
    mic: Option<MicHandle>,
    system: Option<SystemHandle>,
    language: Option<&str>,
    stop: &AtomicBool,
) {
    // whisper ロード（VAD 付き）。モデル未 DL / ロード失敗 = ライブ表示なしで静かに終了
    // （録音は継続。停止時の本番文字起こしが必要ならモデルを DL する）。
    let whisper_path = models_dir.join(DEFAULT_WHISPER_MODEL);
    if !whisper_path.exists() {
        return;
    }
    let vad_path = models_dir.join(DEFAULT_VAD_MODEL);
    let vad = if vad_path.exists() { Some(vad_path) } else { None };
    let vad_available = vad.is_some();
    let engine = match WhisperStt::load(&whisper_path, vad) {
        Ok(e) => e,
        Err(_) => return,
    };

    // 16k mono の「未確定」バッファ（確定分は drain 済み＝メモリ一定）。両者はインデックス＝
    // ほぼ同時刻で整合（mic/system とも 16k・キャプチャ開始ほぼ同時。δ は cosmetic）。
    let mut mic16k: Vec<f32> = Vec::new();
    let mut sys16k: Vec<f32> = Vec::new();
    let mut mic_consumed: u64 = 0;
    let mut sys_consumed: u64 = 0;
    let mut committed: VecDeque<String> = VecDeque::new();

    while !stop.load(Ordering::Relaxed) {
        let t0 = Instant::now();

        // 1) 各ソースの新規サンプルを取得。どちらかが spool flush に追い越されていたら
        //    （長い whisper 実行や重処理休止で 30 秒超読めなかった等）、時刻整合が壊れて
        //    いるのでローカルバッファを捨て、今回チャンクを末尾揃えで積み直して再同期する
        //    （プレビューは使い捨てなので数秒の欠落は許容）。
        let (new_mic, mic_skipped) = match &mic {
            Some((buf, _, _)) => take_new(buf, &mut mic_consumed),
            None => (Vec::new(), false),
        };
        let (new_sys, sys_skipped) = match &system {
            Some((buf, _)) => take_new(buf, &mut sys_consumed),
            None => (Vec::new(), false),
        };
        if mic_skipped || sys_skipped {
            mic16k.clear();
            sys16k.clear();
        }
        if let Some((_, rate, ch)) = &mic {
            if !new_mic.is_empty() {
                mic16k.extend_from_slice(&to_playback_mono(new_mic, *ch, *rate, TARGET_RATE));
            }
        }
        if let Some((_, rate)) = &system {
            if !new_sys.is_empty() {
                // system は downmix_to_mono 済みで常に mono なので channels=1。
                sys16k.extend_from_slice(&to_playback_mono(new_sys, 1, *rate, TARGET_RATE));
            }
        }
        if (mic_skipped || sys_skipped) && mic.is_some() && system.is_some() {
            tail_align(&mut mic16k, &mut sys16k);
        }

        // 2) 両者の min 長で tail を作る（揃っている範囲のみ。残りは次 tick へ）。
        let len = match (&mic, &system) {
            (Some(_), Some(_)) => mic16k.len().min(sys16k.len()),
            (Some(_), None) => mic16k.len(),
            (None, Some(_)) => sys16k.len(),
            (None, None) => 0,
        };
        let tail_ms = len as u64 * 1000 / TARGET_RATE as u64;
        if tail_ms < MIN_TAIL_MS {
            sleep_remainder(t0, stop);
            continue;
        }

        // 重い ML ジョブ（ファイル文字起こし・話者分離・ローカル要約）の実行中は
        // ライブ推論を休止して譲る（whisper 同時実行によるメモリピーク回避。16GB 機対策）。
        // プレビューは使い捨てなので、休止中に溜まりすぎた分は捨てて前進する（メモリ有界）。
        if crate::commands::heavy_job_busy() {
            if tail_ms > MAX_TAIL_MS {
                drain_front(
                    &mut mic16k,
                    &mut sys16k,
                    ms_to_samples(tail_ms - MAX_TAIL_MS),
                );
            }
            sleep_remainder(t0, stop);
            continue;
        }
        let tail = mix_mono(
            mic16k.get(..len).unwrap_or(&[]),
            sys16k.get(..len).unwrap_or(&[]),
        );

        // 3) 無音 tail は whisper に渡さない（ハルシネーション回避）。長すぎる無音は drain で前進。
        if skip_silent_tail(&tail, vad_available) {
            if tail_ms > MAX_TAIL_MS {
                drain_front(
                    &mut mic16k,
                    &mut sys16k,
                    ms_to_samples(tail_ms - COMMIT_GUARD_MS),
                );
            }
            emit(app, &committed, &[]);
            sleep_remainder(t0, stop);
            continue;
        }

        // 4) tail を文字起こし（エラーは呑む＝ライブはスキップ・録音継続）。
        let transcript = match engine.transcribe(&tail, language) {
            Ok(t) => t,
            Err(_) => {
                sleep_remainder(t0, stop);
                continue;
            }
        };

        // 5) tail 末尾 - GUARD より前に終わるセグメントを確定、残りは未確定 tail。
        let commit_ms = tail_ms.saturating_sub(COMMIT_GUARD_MS);
        let mut live: Vec<String> = Vec::new();
        let mut max_committed_end_ms: u64 = 0;
        for seg in &transcript.segments {
            let text = seg.text.trim().to_string();
            if text.is_empty() {
                continue;
            }
            if seg.end_ms <= commit_ms {
                committed.push_back(text);
                if committed.len() > MAX_LINES {
                    committed.pop_front();
                }
                max_committed_end_ms = max_committed_end_ms.max(seg.end_ms);
            } else {
                live.push(text);
            }
        }

        // 6) 確定分の音声を drain（メモリ/負荷を一定に）。確定が無くても長すぎなら強制前進。
        let drop_ms = if max_committed_end_ms > 0 {
            max_committed_end_ms
        } else if tail_ms > MAX_TAIL_MS {
            tail_ms - COMMIT_GUARD_MS
        } else {
            0
        };
        if drop_ms > 0 {
            drain_front(&mut mic16k, &mut sys16k, ms_to_samples(drop_ms));
        }

        // 7) 送信（確定 + 未確定）。
        emit(app, &committed, &live);

        sleep_remainder(t0, stop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_live_tail_reaches_vad_below_the_old_rms_gate() {
        let tail: Vec<f32> = (0..ms_to_samples(3500))
            .map(|i| (i as f32 * 0.1).sin() * 0.0003 * 2.0_f32.sqrt())
            .collect();
        assert!(rms(&tail) < SILENCE_RMS);
        assert!(!skip_silent_tail(&tail, true), "quiet speech must reach VAD");
        assert!(skip_silent_tail(&tail, false), "keep the guard without VAD");
    }

    #[test]
    fn live_silence_is_skipped_with_or_without_vad() {
        for vad_available in [false, true] {
            assert!(skip_silent_tail(&[], vad_available));
            assert!(skip_silent_tail(
                &vec![0.0; ms_to_samples(14000)],
                vad_available
            ));
            assert!(!skip_silent_tail(
                &vec![0.02; ms_to_samples(3500)],
                vad_available
            ));
        }
    }

    #[test]
    fn tail_align_trims_heads_to_equal_len() {
        // 長い方の先頭を捨て、両者の末尾（=現在時刻）を揃える。
        let mut a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let mut b = vec![10.0, 20.0, 30.0];
        tail_align(&mut a, &mut b);
        assert_eq!(a, vec![3.0, 4.0, 5.0]);
        assert_eq!(b, vec![10.0, 20.0, 30.0]);
        // 同長は無変化。空も安全。
        let mut c = vec![1.0];
        let mut d = Vec::new();
        tail_align(&mut c, &mut d);
        assert!(c.is_empty() && d.is_empty());
    }

    #[test]
    fn take_new_tracks_absolute_index_and_skip() {
        let buf = SharedPcm::new();
        buf.push(&[1.0, 2.0, 3.0, 4.0]);
        let mut consumed = 0u64;
        let (s, skipped) = take_new(&buf, &mut consumed);
        assert_eq!((s.len(), skipped, consumed), (4, false, 4));
        // flush で先頭が飛んでも、追い越されていなければ skip ではない。
        buf.take_flush_chunk(0);
        buf.push(&[5.0, 6.0]);
        let (s, skipped) = take_new(&buf, &mut consumed);
        assert_eq!((s, skipped, consumed), (vec![5.0, 6.0], false, 6));
        // consumed より先まで flush されたら skip 印。
        buf.push(&[7.0, 8.0]);
        buf.take_flush_chunk(0);
        buf.push(&[9.0]);
        let mut stale = 6u64; // 7.0, 8.0 を読む前に flush された想定
        let (s, skipped) = take_new(&buf, &mut stale);
        assert_eq!((s, skipped, stale), (vec![9.0], true, 9));
    }
}
