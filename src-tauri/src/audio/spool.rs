//! 録音 PCM のディスク逐次書き出し（spool 化・ADR-0023）。
//!
//! キャプチャ中の PCM を全量 RAM に貯めず（従来は 48kHz f32 で 2h ≈ 1.4GB/トラック）、
//! 共有バッファ（`SharedPcm`）からキャプチャワーカーが定期的に WAV（`WavSpoolWriter`）へ
//! 追記して先頭を解放する。常駐メモリは KEEP_TAIL ぶん（十数 MB）で一定になる。
//!
//! 役割分担:
//! - 音声コールバック: `SharedPcm::push` のみ（IO なし・ロック時間最小）
//! - キャプチャワーカー: `take_flush_chunk` → `WavSpoolWriter::append` + `flush`
//! - live_stt: `snapshot_from`（絶対 index 管理なので flush で位置を見失わない）

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// f32 [-1,1] → i16 量子化（クランプ込み）。ディスク書き出しの量子化の単一の正。
pub fn quantize_i16(s: f32) -> i16 {
    (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
}

/// i16 → f32 [-1,1]（`quantize_i16` の逆写像。ミックスの WAV 読み戻しで使う）。
pub fn dequantize_i16(v: i16) -> f32 {
    v as f32 / i16::MAX as f32
}

/// キャプチャ共有バッファ。**絶対サンプル index**（interleaved 単位）で管理し、
/// flush で先頭を捨てても読者（live_stt）が位置を見失わない。
pub struct SharedPcm {
    inner: Mutex<PcmInner>,
}

struct PcmInner {
    /// `data[0]` の絶対 index（= flush で捨て済みの総サンプル数）。
    base: u64,
    data: Vec<f32>,
}

impl SharedPcm {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(PcmInner {
                base: 0,
                data: Vec::new(),
            }),
        }
    }

    /// 音声コールバック用 append（IO なし）。
    pub fn push(&self, samples: &[f32]) {
        if let Ok(mut g) = self.inner.lock() {
            g.data.extend_from_slice(samples);
        }
    }

    /// 末尾 `keep_tail` サンプルを残して先頭を取り出し、base を進める（flush 用）。
    /// 残りが `keep_tail` 以下なら空を返す。`keep_tail=0` で全量 drain（停止時）。
    pub fn take_flush_chunk(&self, keep_tail: usize) -> Vec<f32> {
        let mut g = self.inner.lock().unwrap();
        if g.data.len() <= keep_tail {
            return Vec::new();
        }
        let n = g.data.len() - keep_tail;
        let out: Vec<f32> = g.data.drain(..n).collect();
        g.base += n as u64;
        out
    }

    /// 絶対 index `from_abs` 以降のサンプルを複製して返す（live_stt 用）。
    /// 返り値: `(新しい consumed 絶対 index, サンプル, 先頭を flush に追い越されたか)`。
    /// `skipped=true` は「from_abs..base が既にディスクへ行き読めなかった」＝呼び出し側は
    /// 時刻整合が壊れたものとして再同期する（プレビュー用途なので欠落は許容）。
    pub fn snapshot_from(&self, from_abs: u64) -> (u64, Vec<f32>, bool) {
        let g = self.inner.lock().unwrap();
        let skipped = from_abs < g.base;
        let start = from_abs.max(g.base) - g.base;
        let start = (start as usize).min(g.data.len());
        let out = g.data[start..].to_vec();
        (g.base + g.data.len() as u64, out, skipped)
    }
}

impl Default for SharedPcm {
    fn default() -> Self {
        Self::new()
    }
}

/// WAV の data チャンク（と RIFF file_size）は u32 バイト上限。16bit = 2byte/sample なので、
/// 書き込めるサンプル総数（interleaved 単位）の上限。ヘッダぶん（64byte 余裕）を引いて data 長・
/// file_size の両 u32 を妥当に保つ（16bit mono 48k で約 12.4h / 2ch で約 6.2h）。
const MAX_WAV_SAMPLES: u64 = (u32::MAX as u64 - 64) / 2;

/// `already` サンプル書き込み済みに `adding` サンプル追記すると u32 WAV 上限を超えるか。
fn exceeds_wav_limit(already: u64, adding: usize) -> bool {
    already + adding as u64 > MAX_WAV_SAMPLES
}

/// 追記型 WAV ライタ（16bit int, hound）。
///
/// `flush()` は hound がヘッダ（RIFF/data サイズ）を現在長で書き直して OS へ流すため、
/// SIGKILL/クラッシュでも**直近 flush 時点までは有効な WAV** が残る。正常終了は
/// `finalize()` で確定する（Drop の finalize は best-effort・エラー黙殺なので頼らない）。
///
/// 制約: WAV の data チャンクは u32 バイト上限（16bit mono 48k で約 12.4 時間、
/// 2ch で約 6.2 時間）。超過は `append` がエラーを返す（従来 write_wav と同一の制約）。
pub struct WavSpoolWriter {
    writer: hound::WavWriter<BufWriter<File>>,
    path: PathBuf,
    /// 書き込んだサンプル総数（interleaved 単位）。hound の len() は u32 なので自前 u64。
    samples_written: u64,
}

impl WavSpoolWriter {
    pub fn create(path: &Path, sample_rate: u32, channels: u16) -> Result<Self, String> {
        let spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let writer = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
        Ok(Self {
            writer,
            path: path.to_path_buf(),
            samples_written: 0,
        })
    }

    pub fn append(&mut self, samples: &[f32]) -> Result<(), String> {
        // u32 WAV 上限を超える追記は 1 サンプルも書かずに Err（doc の契約を実装で満たす）。
        // release では overflow-checks off で hound の u32 data 長が黙って wrap し、再デコードは
        // wrap ぶんしか読めず大半が無警告で欠落する＝spool の目的（録音を失わない）を自己否定
        // するため、超長時間録音（6h+）で上限に達したら以降を切って有効な WAV を残す。呼び出し側
        // （flush ループ / write_mixed_wav）の ? が spool_error 記録または伝播で処理する。
        if exceeds_wav_limit(self.samples_written, samples.len()) {
            return Err(format!(
                "spool: WAV data length limit ({MAX_WAV_SAMPLES} samples) exceeded"
            ));
        }
        for &s in samples {
            self.writer
                .write_sample(quantize_i16(s))
                .map_err(|e| e.to_string())?;
        }
        self.samples_written += samples.len() as u64;
        Ok(())
    }

    /// ヘッダを現在長で更新して OS へ flush（クラッシュ耐性の要）。
    pub fn flush(&mut self) -> Result<(), String> {
        self.writer.flush().map_err(|e| e.to_string())
    }

    /// 確定して `(path, 書き込んだサンプル総数)` を返す。
    pub fn finalize(self) -> Result<(PathBuf, u64), String> {
        let n = self.samples_written;
        self.writer.finalize().map_err(|e| e.to_string())?;
        Ok((self.path, n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mojiroku-spool-{}-{name}", std::process::id()))
    }

    #[test]
    fn shared_pcm_flush_keeps_tail_and_advances_base() {
        let p = SharedPcm::new();
        p.push(&[0.1; 100]);
        // keep_tail=30: 先頭 70 を取り出し base=70。
        assert_eq!(p.take_flush_chunk(30).len(), 70);
        // 残 30 <= keep_tail 30 → 空。
        assert!(p.take_flush_chunk(30).is_empty());
        // keep_tail=0 で全量 drain。
        assert_eq!(p.take_flush_chunk(0).len(), 30);
        let (consumed, rest, skipped) = p.snapshot_from(100);
        assert_eq!((consumed, rest.len(), skipped), (100, 0, false));
    }

    #[test]
    fn shared_pcm_snapshot_semantics() {
        let p = SharedPcm::new();
        p.push(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        // 途中から読む。
        let (c, s, skipped) = p.snapshot_from(2);
        assert_eq!((c, s, skipped), (5, vec![3.0, 4.0, 5.0], false));
        // flush で先頭 3 つが飛ぶ（base=3）。
        assert_eq!(p.take_flush_chunk(2), vec![1.0, 2.0, 3.0]);
        // consumed=2 < base=3 → skipped で base から読む。
        let (c, s, skipped) = p.snapshot_from(2);
        assert_eq!((c, s, skipped), (5, vec![4.0, 5.0], true));
        // ちょうど base は skip でない。
        let (_, s, skipped) = p.snapshot_from(3);
        assert_eq!((s, skipped), (vec![4.0, 5.0], false));
        // 末尾以降は空・非 skip。
        let (c, s, skipped) = p.snapshot_from(5);
        assert_eq!((c, s.len(), skipped), (5, 0, false));
    }

    #[test]
    fn spool_writer_split_append_matches_oneshot() {
        // 分割 append の結果が従来のワンショット write_wav とバイト一致する。
        let samples = [0.0f32, 1.0, -1.0, 2.0, -2.0, 0.5, -0.25, 0.125];
        let a = tmp("split.wav");
        let b = tmp("oneshot.wav");

        let mut w = WavSpoolWriter::create(&a, 16_000, 1).unwrap();
        w.append(&samples[..3]).unwrap();
        w.flush().unwrap();
        w.append(&samples[3..]).unwrap();
        let (path, n) = w.finalize().unwrap();
        assert_eq!((path.as_path(), n), (a.as_path(), samples.len() as u64));

        crate::commands::write_wav(&b, &samples, 16_000, 1).unwrap();
        assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
        let _ = std::fs::remove_file(&a);
        let _ = std::fs::remove_file(&b);
    }

    #[test]
    fn spool_flush_leaves_readable_wav() {
        // finalize 前でも flush 済みならヘッダが現在長で読める（クラッシュ耐性の近似検証）。
        let a = tmp("crash.wav");
        let copied = tmp("crash-copy.wav");
        let mut w = WavSpoolWriter::create(&a, 48_000, 1).unwrap();
        w.append(&[0.5; 480]).unwrap();
        w.flush().unwrap();
        std::fs::copy(&a, &copied).unwrap();

        let mut r = hound::WavReader::open(&copied).unwrap();
        assert_eq!(r.spec().sample_rate, 48_000);
        assert_eq!(r.samples::<i16>().count(), 480);

        drop(w); // Drop finalize は best-effort（テストでは結果を見ない）
        let _ = std::fs::remove_file(&a);
        let _ = std::fs::remove_file(&copied);
    }

    #[test]
    fn wav_limit_guard_boundary() {
        // 上限ちょうどまでは OK、1 でも超えると Err（append はこのとき 1 サンプルも書かず返す）。
        // ※ release 限定の実 wrap は ~2GB 書き込みが要り単体では再現できないため、append が
        //    参照する境界判定そのものを検証する。
        assert!(!exceeds_wav_limit(MAX_WAV_SAMPLES, 0)); // ちょうど上限は OK
        assert!(!exceeds_wav_limit(MAX_WAV_SAMPLES - 1, 1)); // 上限ちょうどに達する
        assert!(exceeds_wav_limit(MAX_WAV_SAMPLES, 1)); // 1 超過
        assert!(exceeds_wav_limit(MAX_WAV_SAMPLES - 1, 2)); // 2 で超過
        assert!(!exceeds_wav_limit(0, 48_000 * 60 * 60)); // 1h @48k mono は余裕
    }
}
