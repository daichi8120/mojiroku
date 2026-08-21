//! 音声デコード: 任意フォーマット → **16kHz / mono / f32[-1,1]**（whisper 入力仕様）。
//!
//! symphonia でデコード（planar/interleaved・i16/i32/f32 を `SampleBuffer<f32>` で正規化）→
//! mono ダウンミックス → rubato で 16kHz にリサンプル。

use std::fs::File;
use std::path::Path;

use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::error::{CoreError, Result};

/// whisper が要求するサンプルレート。
pub const WHISPER_SAMPLE_RATE: u32 = 16_000;

/// 音声ファイルをデコードし、16kHz mono f32 PCM を返す。
pub fn decode_to_pcm16k_mono<P: AsRef<Path>>(path: P) -> Result<Vec<f32>> {
    let (mono, src_rate) = decode_to_mono_f32(path)?;
    Ok(resample_to_16k(&mono, src_rate))
}

/// デコード → mono f32（元のサンプルレートのまま）。
fn decode_to_mono_f32<P: AsRef<Path>>(path: P) -> Result<(Vec<f32>, u32)> {
    let file = File::open(path.as_ref()).map_err(|e| CoreError::Io(e.to_string()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.as_ref().extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| CoreError::Audio(format!("probe: {e}")))?;
    let mut format = probed.format;

    let track = format
        .default_track()
        .ok_or_else(|| CoreError::Audio("no default track".into()))?;
    let track_id = track.id;
    let src_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| CoreError::Audio("unknown sample rate".into()))?;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| CoreError::Audio(format!("make decoder: {e}")))?;

    let mut mono: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            // ストリーム終端
            Err(SymphoniaError::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(SymphoniaError::ResetRequired) => break,
            Err(e) => return Err(CoreError::Audio(format!("next_packet: {e}"))),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let channels = spec.channels.count().max(1);
                // 任意のサンプル形式を f32 interleaved [-1,1] に変換
                let mut sbuf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                sbuf.copy_interleaved_ref(decoded);
                for frame in sbuf.samples().chunks(channels) {
                    let sum: f32 = frame.iter().copied().sum();
                    mono.push(sum / channels as f32);
                }
            }
            // 破損パケットはスキップ
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(CoreError::Audio(format!("decode: {e}"))),
        }
    }

    Ok((mono, src_rate))
}

/// mono f32 を 16kHz にリサンプル（rubato, 高品質 sinc）。すでに 16k ならそのまま返す。
fn resample_to_16k(input: &[f32], src_rate: u32) -> Vec<f32> {
    if src_rate == WHISPER_SAMPLE_RATE || input.is_empty() {
        return input.to_vec();
    }

    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };
    let chunk = 1024usize;
    let mut resampler = match SincFixedIn::<f32>::new(
        WHISPER_SAMPLE_RATE as f64 / src_rate as f64,
        2.0,
        params,
        chunk,
        1,
    ) {
        Ok(r) => r,
        Err(_) => return input.to_vec(),
    };

    let mut out: Vec<f32> = Vec::with_capacity(input.len() * WHISPER_SAMPLE_RATE as usize / src_rate as usize + chunk);
    let mut pos = 0;
    while pos + chunk <= input.len() {
        let wave_in = [input[pos..pos + chunk].to_vec()];
        if let Ok(wave_out) = resampler.process(&wave_in, None) {
            out.extend_from_slice(&wave_out[0]);
        }
        pos += chunk;
    }
    // 端数はゼロ詰めして最後に流す（末尾に僅かな無音が付くが MVP では許容）
    if pos < input.len() {
        let mut last = input[pos..].to_vec();
        last.resize(chunk, 0.0);
        if let Ok(wave_out) = resampler.process(&[last], None) {
            out.extend_from_slice(&wave_out[0]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, secs: f32, freq: f32) -> Vec<f32> {
        let n = (rate as f32 * secs) as usize;
        (0..n)
            .map(|i| (i as f32 * freq * 2.0 * std::f32::consts::PI / rate as f32).sin() * 0.5)
            .collect()
    }

    #[test]
    fn passthrough_when_already_16k() {
        let input = sine(16_000, 1.0, 440.0);
        let out = resample_to_16k(&input, 16_000);
        assert_eq!(out.len(), input.len());
    }

    #[test]
    fn resample_48k_to_16k_length_and_range() {
        let input = sine(48_000, 1.0, 440.0); // 1 秒
        let out = resample_to_16k(&input, 48_000);
        // 期待長 ≈ 16000（チャンク境界の誤差を許容）
        assert!(
            (out.len() as i32 - 16_000).abs() < 2048,
            "resampled len = {}",
            out.len()
        );
        // 値域 [-1,1]・有限
        assert!(out.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
    }

    #[test]
    fn resample_44100_to_16k_is_finite() {
        let input = sine(44_100, 0.5, 1000.0);
        let out = resample_to_16k(&input, 44_100);
        assert!(!out.is_empty());
        assert!(out.iter().all(|x| x.is_finite()));
    }
}
