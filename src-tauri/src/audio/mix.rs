//! ストリーミング結合ミックス（再生用 `<id>.wav` の生成・ADR-0023）。
//!
//! 会議停止時に per-track WAV（mic/system）をチャンク読み → mono 化 → 線形リサンプル →
//! 加算クランプ → 逐次書き込みし、全量を RAM に乗せない（従来はメモリ上の PCM を
//! 一括変換していて 2h 会議で数 GB 級のピークだった）。
//!
//! 粗いミックスである点は従来と同じ（δ/ドリフト未補正・視聴用途。ADR-0017。
//! 文字起こしは per-track の元 WAV を使うのでここの品質は影響しない）。
//!
//! Update (Issue #65): the start offset δ between the tracks is now applied as leading
//! silence on the later-starting track, so playback stays aligned with the offset-corrected
//! transcript (the detail view seeks by segment time). Clock drift is still uncorrected.

use std::path::Path;

use super::spool::{dequantize_i16, WavSpoolWriter};

/// チャンク境界をまたいで連続な線形リサンプラ（mono）。
///
/// 写像は `system_audio::resample_linear_mono` と**同一式の絶対 index 版**
/// （出力 n → 入力位置 `n / ratio` の線形補間）。位相の逐次累積ではなく絶対 index で
/// 計算するためドリフトが無く、同一入力ならバッチ版と**ビット一致**する
/// （`resampler_matches_batch_*` テストで固定）。
pub struct ChunkResampler {
    from_rate: u32,
    to_rate: u32,
    /// 次に生成する出力サンプルの絶対 index。
    next_out: u64,
    /// `pending[0]` の入力絶対 index。
    in_base: u64,
    /// 受領済み入力サンプル総数。
    in_total: u64,
    /// 未消費入力（補間に次サンプルが要る分の carry を含む）。
    pending: Vec<f32>,
}

impl ChunkResampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Self {
        Self {
            from_rate,
            to_rate,
            next_out: 0,
            in_base: 0,
            in_total: 0,
            pending: Vec::new(),
        }
    }

    fn ratio(&self) -> f64 {
        self.to_rate as f64 / self.from_rate as f64
    }

    /// 入力チャンクを与え、確定できる出力サンプルを返す（末尾の補間未確定分は carry）。
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if self.from_rate == self.to_rate || self.from_rate == 0 || self.to_rate == 0 {
            return input.to_vec(); // バッチ版の素通し条件と同一
        }
        self.pending.extend_from_slice(input);
        self.in_total += input.len() as u64;

        let ratio = self.ratio();
        let mut out = Vec::new();
        loop {
            let src = self.next_out as f64 / ratio;
            let i0 = src.floor() as u64;
            // s1（i0+1）が届くまで確定しない（バッチ版の「末尾は s1=s0」フォールバックは
            // 入力終端でのみ正しいので finish() 側で行う）。
            if i0 + 1 >= self.in_total {
                break;
            }
            let frac = (src - i0 as f64) as f32;
            let s0 = self.pending[(i0 - self.in_base) as usize];
            let s1 = self.pending[(i0 + 1 - self.in_base) as usize];
            out.push(s0 + (s1 - s0) * frac);
            self.next_out += 1;
        }
        // 消費済みプレフィックスを解放（次に必要な i0 以降だけ残す）。
        let need = (self.next_out as f64 / ratio).floor() as u64;
        let drop_n = need.saturating_sub(self.in_base).min(self.pending.len() as u64);
        self.pending.drain(..drop_n as usize);
        self.in_base += drop_n;
        out
    }

    /// 入力終端を確定し、残りの出力を吐き切る（バッチ版と同じ
    /// `out_len = round(in_total × ratio)`・終端フォールバック s1=s0）。
    pub fn finish(mut self) -> Vec<f32> {
        if self.from_rate == self.to_rate || self.from_rate == 0 || self.to_rate == 0 {
            return Vec::new();
        }
        let ratio = self.ratio();
        let out_total = (self.in_total as f64 * ratio).round() as u64;
        let mut out = Vec::new();
        while self.next_out < out_total {
            let src = self.next_out as f64 / ratio;
            let i0 = src.floor() as u64;
            let frac = (src - i0 as f64) as f32;
            let s0 = if i0 < self.in_total {
                self.pending[(i0 - self.in_base) as usize]
            } else {
                0.0
            };
            let s1 = if i0 + 1 < self.in_total {
                self.pending[(i0 + 1 - self.in_base) as usize]
            } else {
                s0
            };
            out.push(s0 + (s1 - s0) * frac);
            self.next_out += 1;
        }
        out
    }
}

/// WAV を mono・目標レートのチャンクとして逐次読むリーダ。
struct MonoTrackReader {
    reader: hound::WavReader<std::io::BufReader<std::fs::File>>,
    channels: usize,
    resampler: Option<ChunkResampler>, // None = 読み尽くして finish 済み
    /// resampler 出力の未返却分。
    buf: Vec<f32>,
}

/// 1 回の読み込みフレーム数（≒1 秒 @48k。ピークメモリを数 MB に抑える）。
const CHUNK_FRAMES: usize = 48_000;

impl MonoTrackReader {
    fn open(path: &Path, to_rate: u32) -> Result<Self, String> {
        let reader = hound::WavReader::open(path).map_err(|e| e.to_string())?;
        let spec = reader.spec();
        if spec.bits_per_sample != 16 || spec.sample_format != hound::SampleFormat::Int {
            return Err(format!(
                "unsupported wav format for mix: {:?}bit {:?}",
                spec.bits_per_sample, spec.sample_format
            ));
        }
        Ok(Self {
            reader,
            channels: spec.channels.max(1) as usize,
            resampler: Some(ChunkResampler::new(spec.sample_rate, to_rate)),
            buf: Vec::new(),
        })
    }

    /// 最大 `want` サンプル返す。空 = トラック終端。
    fn next_chunk(&mut self, want: usize) -> Result<Vec<f32>, String> {
        while self.buf.len() < want && self.resampler.is_some() {
            // フレーム単位で読み、mono 平均 → リサンプラへ。
            let n = CHUNK_FRAMES * self.channels;
            let mut raw: Vec<f32> = Vec::with_capacity(n);
            {
                let mut it = self.reader.samples::<i16>();
                for _ in 0..n {
                    match it.next() {
                        Some(v) => raw.push(dequantize_i16(v.map_err(|e| e.to_string())?)),
                        None => break,
                    }
                }
            }
            let eof = raw.len() < n;
            let mono: Vec<f32> = if self.channels == 1 {
                raw
            } else {
                raw.chunks_exact(self.channels)
                    .map(|fr| fr.iter().sum::<f32>() / self.channels as f32)
                    .collect()
            };
            let rs = self.resampler.as_mut().unwrap();
            self.buf.extend(rs.process(&mono));
            if eof {
                self.buf.extend(self.resampler.take().unwrap().finish());
            }
        }
        let take = want.min(self.buf.len());
        Ok(self.buf.drain(..take).collect())
    }
}

/// Serve `lead` frames of silence before the track's own audio (start-offset alignment).
/// The chunk is always filled up to `CHUNK_FRAMES` (silence, then audio): a short
/// silence-only chunk would let the other track advance a full chunk and shift this
/// track by the difference.
fn next_chunk_with_lead(r: &mut MonoTrackReader, lead: &mut usize) -> Result<Vec<f32>, String> {
    let n = (*lead).min(CHUNK_FRAMES);
    *lead -= n;
    let mut out = vec![0.0; n];
    if n < CHUNK_FRAMES {
        out.extend(r.next_chunk(CHUNK_FRAMES - n)?);
    }
    Ok(out)
}

/// per-track WAV（存在する側のみ）を読み、mono・`out_rate` で加算ミックスした WAV を書く。
/// どちらも None はエラー。ピークメモリはチャンクサイズ相当（数 MB）。
///
/// `mic_offset_ms`: how much later the mic track started than the system track (Issue #65).
/// The later-starting track is padded with that much leading silence; 0 = no padding.
pub fn write_mixed_wav(
    mic: Option<&Path>,
    system: Option<&Path>,
    out: &Path,
    out_rate: u32,
    mic_offset_ms: i64,
) -> Result<(), String> {
    let mut a = mic.map(|p| MonoTrackReader::open(p, out_rate)).transpose()?;
    let mut b = system.map(|p| MonoTrackReader::open(p, out_rate)).transpose()?;
    if a.is_none() && b.is_none() {
        return Err("mix: no input tracks".into());
    }
    let frames = |ms: u64| (ms * out_rate as u64 / 1000) as usize;
    let mut lead_a = if mic_offset_ms > 0 {
        frames(mic_offset_ms as u64)
    } else {
        0
    };
    let mut lead_b = if mic_offset_ms < 0 {
        frames(mic_offset_ms.unsigned_abs())
    } else {
        0
    };

    let mut writer = WavSpoolWriter::create(out, out_rate, 1)?;
    loop {
        let ca = match a.as_mut() {
            Some(r) => next_chunk_with_lead(r, &mut lead_a)?,
            None => Vec::new(),
        };
        let cb = match b.as_mut() {
            Some(r) => next_chunk_with_lead(r, &mut lead_b)?,
            None => Vec::new(),
        };
        if ca.is_empty() && cb.is_empty() {
            break;
        }
        // 短い側はゼロ詰め（mix_mono と同じ max 長・クランプ規則。クランプは quantize 内）。
        let n = ca.len().max(cb.len());
        let mut mixed = Vec::with_capacity(n);
        for i in 0..n {
            let s = ca.get(i).copied().unwrap_or(0.0) + cb.get(i).copied().unwrap_or(0.0);
            mixed.push(s.clamp(-1.0, 1.0));
        }
        writer.append(&mixed)?;
    }
    writer.finalize().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::spool::quantize_i16;
    use crate::system_audio::{mix_mono, resample_linear_mono, to_playback_mono};
    use std::path::PathBuf;

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mojiroku-mix-{}-{name}", std::process::id()))
    }

    /// 決定的な擬似ランダム波形（Date/rand 不使用）。
    fn wave(len: usize, seed: u32) -> Vec<f32> {
        (0..len)
            .map(|i| {
                let x = (i as f32 * 0.37 + seed as f32 * 1.7).sin() * 0.8
                    + (i as f32 * 0.011).cos() * 0.15;
                x.clamp(-1.0, 1.0)
            })
            .collect()
    }

    fn run_chunked(input: &[f32], from: u32, to: u32, chunks: &[usize]) -> Vec<f32> {
        let mut rs = ChunkResampler::new(from, to);
        let mut out = Vec::new();
        let mut pos = 0usize;
        let mut sizes = chunks.iter().cycle();
        while pos < input.len() {
            let n = (*sizes.next().unwrap()).max(1).min(input.len() - pos);
            out.extend(rs.process(&input[pos..pos + n]));
            pos += n;
        }
        out.extend(rs.finish());
        out
    }

    #[test]
    fn resampler_matches_batch_various_rates_and_chunks() {
        for (from, to) in [(48_000u32, 16_000u32), (44_100, 48_000), (48_000, 48_000), (16_000, 48_000)] {
            let input = wave(4321, from);
            let batch = resample_linear_mono(&input, from, to);
            for chunks in [&[1usize][..], &[7, 3][..], &[1000][..], &[4321][..]] {
                let streamed = run_chunked(&input, from, to, chunks);
                assert_eq!(streamed, batch, "from={from} to={to} chunks={chunks:?}");
            }
        }
    }

    #[test]
    fn resampler_empty_input() {
        let rs = ChunkResampler::new(48_000, 16_000);
        assert!(rs.finish().is_empty());
        let mut rs = ChunkResampler::new(48_000, 16_000);
        assert!(rs.process(&[]).is_empty());
        assert!(rs.finish().is_empty());
    }

    /// 旧パイプライン（全量メモリ: to_playback_mono ×2 + mix_mono + write_wav）と
    /// 新ストリーミング実装の出力 WAV が一致することを固定する。
    /// 入力はディスク経由（i16 量子化済み）に揃えるため、旧側も量子化後の値を使う。
    #[test]
    fn write_mixed_wav_matches_legacy_pipeline() {
        const OUT_RATE: u32 = 48_000;
        // mic: 44.1k 2ch / system: 48k mono（異レート・異ch の代表ケース）。
        let mic_f32 = wave(44_100 * 2, 1); // 1 秒ぶん interleaved 2ch
        let sys_f32 = wave(50_000, 2);

        let mic_wav = tmp("mic.wav");
        let sys_wav = tmp("sys.wav");
        crate::commands::write_wav(&mic_wav, &mic_f32, 44_100, 2).unwrap();
        crate::commands::write_wav(&sys_wav, &sys_f32, 48_000, 1).unwrap();

        // 旧パイプライン（ディスク上の i16 と同じ値になるよう量子化→逆量子化してから）。
        let deq = |v: &[f32]| -> Vec<f32> {
            v.iter().map(|&s| dequantize_i16(quantize_i16(s))).collect()
        };
        let mic_r = to_playback_mono(deq(&mic_f32), 2, 44_100, OUT_RATE);
        let sys_r = to_playback_mono(deq(&sys_f32), 1, 48_000, OUT_RATE);
        let legacy_mix = mix_mono(&mic_r, &sys_r);
        let legacy_wav = tmp("legacy.wav");
        crate::commands::write_wav(&legacy_wav, &legacy_mix, OUT_RATE, 1).unwrap();

        // 新実装。
        let out_wav = tmp("streamed.wav");
        write_mixed_wav(Some(&mic_wav), Some(&sys_wav), &out_wav, OUT_RATE, 0).unwrap();

        assert_eq!(
            std::fs::read(&out_wav).unwrap(),
            std::fs::read(&legacy_wav).unwrap(),
            "ストリーミングミックスは旧パイプラインとバイト一致する"
        );
        for p in [mic_wav, sys_wav, legacy_wav, out_wav] {
            let _ = std::fs::remove_file(&p);
        }
    }

    #[test]
    fn write_mixed_wav_single_track() {
        // 片側のみ（相手無音の会議）: そのトラックの mono/レート変換結果になる。
        let sys_f32 = wave(48_000, 3);
        let sys_wav = tmp("single-sys.wav");
        crate::commands::write_wav(&sys_wav, &sys_f32, 48_000, 1).unwrap();

        let out_wav = tmp("single-out.wav");
        write_mixed_wav(None, Some(&sys_wav), &out_wav, 48_000, 0).unwrap();

        let mut r = hound::WavReader::open(&out_wav).unwrap();
        assert_eq!(r.spec().channels, 1);
        assert_eq!(r.samples::<i16>().count(), 48_000);
        let _ = std::fs::remove_file(&sys_wav);
        let _ = std::fs::remove_file(&out_wav);
    }

    /// The later-starting track gets leading silence so playback lines up with the
    /// offset-corrected transcript (Issue #65). mic = 100 ms of 0.5, system = 100 ms of 0.25,
    /// mic started 50 ms later: system only, then both, then mic only.
    #[test]
    fn write_mixed_wav_applies_the_start_offset() {
        const RATE: u32 = 8_000;
        let mic_wav = tmp("off-mic.wav");
        let sys_wav = tmp("off-sys.wav");
        let out_wav = tmp("off-out.wav");
        crate::commands::write_wav(&mic_wav, &vec![0.5; 800], RATE, 1).unwrap();
        crate::commands::write_wav(&sys_wav, &vec![0.25; 800], RATE, 1).unwrap();
        let read = |p: &PathBuf| -> Vec<f32> {
            hound::WavReader::open(p)
                .unwrap()
                .samples::<i16>()
                .map(|s| dequantize_i16(s.unwrap()))
                .collect()
        };
        let near = |a: f32, b: f32| (a - b).abs() < 0.01;

        write_mixed_wav(Some(&mic_wav), Some(&sys_wav), &out_wav, RATE, 50).unwrap();
        let out = read(&out_wav);
        assert_eq!(out.len(), 1200);
        assert!(near(out[10], 0.25), "system only: {}", out[10]);
        assert!(near(out[600], 0.75), "both: {}", out[600]);
        assert!(near(out[1000], 0.5), "mic only: {}", out[1000]);

        // Negative offset (mic started first): the system track gets the lead instead.
        write_mixed_wav(Some(&mic_wav), Some(&sys_wav), &out_wav, RATE, -50).unwrap();
        let out = read(&out_wav);
        assert_eq!(out.len(), 1200);
        assert!(near(out[10], 0.5), "mic only: {}", out[10]);
        assert!(near(out[1000], 0.25), "system only: {}", out[1000]);

        for p in [mic_wav, sys_wav, out_wav] {
            let _ = std::fs::remove_file(&p);
        }
    }
}
