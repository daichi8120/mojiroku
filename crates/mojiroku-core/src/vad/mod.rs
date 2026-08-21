//! VAD（無音区間除去）。
//!
//! Phase 1 の文字起こしでは whisper.cpp 内蔵の Silero VAD（`WhisperVadContext`）を
//! `stt` モジュール内で直接用いて無音を除去する（ADR-0008）。下記の `Vad` トレイト /
//! `SpeechSpan` は現状未使用だが、Phase 2 の話者分離（sherpa-onnx）導入時に STT と
//! VAD の発話区間を共有する段抽象として再利用する想定で残している。

use crate::error::Result;

/// 音声区間（無音除去後）。
#[derive(Debug, Clone, Copy)]
pub struct SpeechSpan {
    pub start_ms: u64,
    pub end_ms: u64,
}

/// VAD の抽象。
pub trait Vad {
    /// 16kHz mono PCM(f32) から発話区間を検出する。
    fn detect(&self, pcm: &[f32], sample_rate: u32) -> Result<Vec<SpeechSpan>>;
}
