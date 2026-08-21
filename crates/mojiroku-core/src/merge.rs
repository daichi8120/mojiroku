//! 話者マージ: 話者分離の turn を STT セグメントへ**時間重なり**で割り当て、
//! `Segment.speaker_id` を埋める（whisperX 流）。
//!
//! 前提（ADR-0008/0009）: STT セグメントのタイムスタンプは VAD 無音除去後に**原時刻へ
//! 再マップ済み**で、diarization は**原音声**に対して走らせている。よって両者は同じ
//! 原音声時間軸にあり、重なり計算が成立する。

use crate::diarization::DiarizationResult;
use crate::lang::Lang;
use crate::schemas::{Segment, Speaker, Transcript};

/// `[a0,a1)` と `[b0,b1)` の重なり長（ms）。交差しなければ 0。
fn overlap_ms(a0: u64, a1: u64, b0: u64, b1: u64) -> u64 {
    let lo = a0.max(b0);
    let hi = a1.min(b1);
    hi.saturating_sub(lo)
}

/// 各 STT セグメントに、最も時間が重なる話者 turn の `speaker_id` を割り当てる。
/// どの turn とも重ならないセグメントは `None` のまま（UI 側で未割当として扱う）。
pub fn assign_speakers(transcript: &mut Transcript, diar: &DiarizationResult) {
    if diar.turns.is_empty() {
        return;
    }
    for seg in &mut transcript.segments {
        let mut best: Option<(&str, u64)> = None;
        for turn in &diar.turns {
            let ov = overlap_ms(seg.start_ms, seg.end_ms, turn.start_ms, turn.end_ms);
            if ov == 0 {
                continue;
            }
            if best.map(|(_, bo)| ov > bo).unwrap_or(true) {
                best = Some((turn.speaker_id.as_str(), ov));
            }
        }
        if let Some((sid, _)) = best {
            seg.speaker_id = Some(sid.to_string());
        }
    }
}

/// 会議モードのデュアルトラック合成で、マイク（自分）に割り当てる予約話者 id。
pub const SELF_SPEAKER_ID: &str = "self";

/// デュアルトラック合成（会議モード・ADR-0017）: マイク（自分）と システム音声（相手）の
/// Transcript を 1 つの時系列 Transcript ＋ 話者リストにまとめる。
///
/// ソース帰属が構造上保証されるため、マイク側は単一話者 `SELF_SPEAKER_ID`（「あなた」/ "You"）に
/// 固定し、システム側は diarization の話者 id をそのまま保持する。セグメントは `start_ms` で安定
/// ソートし、同時刻はマイク→システムの順を保つ。話者リストは（マイクに発話があれば）自分を先頭に、
/// 続いてシステム側の話者を並べる。ラベルは DB に保存されるため生成時の `lang` で固定される
/// （過去データは変えない）。
///
/// 時刻注意: 各 Transcript の時刻は各トラックの録音開始基準。開始タイミングのズレ（δ）と
/// クロックドリフトは未補正。per-track STT なのでソース帰属は不変で、影響は近接する異トラック
/// 発話の並び順がわずかに乱れる cosmetic な範囲に留まる（δ 補正は将来の精緻化）。
pub fn merge_tracks(
    mic: Transcript,
    system: Transcript,
    system_speakers: Vec<Speaker>,
    lang: Lang,
) -> (Transcript, Vec<Speaker>) {
    let mic_has_speech = !mic.segments.is_empty();

    let mut segments: Vec<Segment> =
        Vec::with_capacity(mic.segments.len() + system.segments.len());
    // マイク = 自分（単一話者に固定）。
    for mut s in mic.segments {
        s.speaker_id = Some(SELF_SPEAKER_ID.to_string());
        segments.push(s);
    }
    // システム = 相手（diarization の話者 id を保持）。
    segments.extend(system.segments);
    // 安定ソート（同 start_ms は push 順＝マイク→システムを保つ）。
    segments.sort_by_key(|s| s.start_ms);

    let language = mic.language.or(system.language);

    let mut speakers = Vec::with_capacity(system_speakers.len() + 1);
    if mic_has_speech {
        speakers.push(Speaker {
            id: SELF_SPEAKER_ID.to_string(),
            label: match lang {
                Lang::Ja => "あなた",
                Lang::En => "You",
            }
            .to_string(),
            display_name: None,
        });
    }
    // システム側（相手）は会議文脈に合わせ既定ラベルを「相手N」/「Guest N」に再ラベルする
    // （diarization の既定は「話者N」/「Speaker N」。en は "Speaker N" のままだと通常録音の
    // 既定ラベルと区別が付かないため、会議の相手とわかる "Guest N" を使う）。id は不変
    // （セグメント帰属に使う）。N は system_speakers の順（S1=最も喋った話者）。
    // ユーザー改名（display_name）は別途尊重される。
    for (i, mut s) in system_speakers.into_iter().enumerate() {
        s.label = match lang {
            Lang::Ja => format!("相手{}", i + 1),
            Lang::En => format!("Guest {}", i + 1),
        };
        speakers.push(s);
    }

    (Transcript { language, segments }, speakers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diarization::{DiarizationResult, SpeakerTurn};
    use crate::schemas::{Segment, Speaker, Transcript};

    fn seg(start_ms: u64, end_ms: u64) -> Segment {
        Segment {
            start_ms,
            end_ms,
            text: "x".into(),
            speaker_id: None,
        }
    }
    fn turn(start_ms: u64, end_ms: u64, sid: &str) -> SpeakerTurn {
        SpeakerTurn {
            start_ms,
            end_ms,
            speaker_id: sid.into(),
        }
    }

    #[test]
    fn assigns_dominant_overlap_speaker() {
        let mut t = Transcript {
            language: None,
            // 0-1000ms はほぼ S1、1100-2000ms はほぼ S2
            segments: vec![seg(0, 1000), seg(1100, 2000)],
        };
        let diar = DiarizationResult {
            speakers: vec![],
            turns: vec![turn(0, 1050, "S1"), turn(1050, 3000, "S2")],
            ..Default::default()
        };
        assign_speakers(&mut t, &diar);
        assert_eq!(t.segments[0].speaker_id.as_deref(), Some("S1"));
        assert_eq!(t.segments[1].speaker_id.as_deref(), Some("S2"));
    }

    #[test]
    fn picks_max_overlap_when_segment_straddles_boundary() {
        // セグメント 800-1400 は S1 と 200ms、S2 と 350ms 重なる → S2。
        let mut t = Transcript {
            language: None,
            segments: vec![seg(800, 1400)],
        };
        let diar = DiarizationResult {
            speakers: vec![],
            turns: vec![turn(0, 1000, "S1"), turn(1050, 2000, "S2")],
            ..Default::default()
        };
        assign_speakers(&mut t, &diar);
        assert_eq!(t.segments[0].speaker_id.as_deref(), Some("S2"));
    }

    #[test]
    fn leaves_none_when_no_overlap() {
        let mut t = Transcript {
            language: None,
            segments: vec![seg(5000, 6000)],
        };
        let diar = DiarizationResult {
            speakers: vec![],
            turns: vec![turn(0, 1000, "S1")],
            ..Default::default()
        };
        assign_speakers(&mut t, &diar);
        assert_eq!(t.segments[0].speaker_id, None);
    }

    #[test]
    fn empty_diarization_is_noop() {
        let mut t = Transcript {
            language: None,
            segments: vec![seg(0, 1000)],
        };
        assign_speakers(&mut t, &DiarizationResult::default());
        assert_eq!(t.segments[0].speaker_id, None);
    }

    fn seg_t(start_ms: u64, end_ms: u64, text: &str) -> Segment {
        Segment {
            start_ms,
            end_ms,
            text: text.into(),
            speaker_id: None,
        }
    }
    fn spk(id: &str) -> Speaker {
        Speaker {
            id: id.into(),
            label: id.into(),
            display_name: None,
        }
    }

    #[test]
    fn merge_tracks_interleaves_by_time_and_labels_self() {
        let mic = Transcript {
            language: Some("ja".into()),
            segments: vec![seg_t(500, 1000, "mic-a"), seg_t(2500, 3000, "mic-b")],
        };
        let system = Transcript {
            language: Some("ja".into()),
            segments: vec![seg_t(0, 400, "sys-a"), seg_t(1500, 2000, "sys-b")],
        };
        let (merged, speakers) = merge_tracks(mic, system, vec![spk("S1")], Lang::Ja);
        // 時系列順に並ぶ。
        let texts: Vec<&str> = merged.segments.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, vec!["sys-a", "mic-a", "sys-b", "mic-b"]);
        // マイク由来は self。
        assert_eq!(merged.segments[1].speaker_id.as_deref(), Some(SELF_SPEAKER_ID)); // mic-a
        assert_eq!(merged.segments[3].speaker_id.as_deref(), Some(SELF_SPEAKER_ID)); // mic-b
        // 話者は「あなた」先頭 + システム話者（id 不変・ラベルは「相手N」に再ラベル）。
        assert_eq!(speakers.len(), 2);
        assert_eq!(speakers[0].id, SELF_SPEAKER_ID);
        assert_eq!(speakers[0].label, "あなた");
        assert_eq!(speakers[1].id, "S1");
        assert_eq!(speakers[1].label, "相手1");
    }

    #[test]
    fn merge_tracks_no_self_speaker_when_mic_empty() {
        let system = Transcript {
            language: Some("ja".into()),
            segments: vec![seg_t(0, 500, "sys")],
        };
        let (merged, speakers) =
            merge_tracks(Transcript::default(), system, vec![spk("S1")], Lang::Ja);
        assert_eq!(merged.segments.len(), 1);
        // マイク無発話なら self 話者は足さない。システム話者は「相手N」に再ラベル。
        assert_eq!(speakers.len(), 1);
        assert_eq!(speakers[0].id, "S1");
        assert_eq!(speakers[0].label, "相手1");
    }

    /// en の既定ラベルは「You」/「Guest N」（生成時の言語で DB に固定される）。
    #[test]
    fn merge_tracks_english_labels() {
        let mic = Transcript {
            language: Some("en".into()),
            segments: vec![seg_t(0, 500, "mic")],
        };
        let system = Transcript {
            language: Some("en".into()),
            segments: vec![seg_t(600, 900, "sys")],
        };
        let (_, speakers) = merge_tracks(mic, system, vec![spk("S1"), spk("S2")], Lang::En);
        assert_eq!(speakers[0].label, "You");
        assert_eq!(speakers[1].label, "Guest 1");
        assert_eq!(speakers[2].label, "Guest 2");
    }

    #[test]
    fn merge_tracks_stable_order_same_start() {
        // 同 start_ms はマイク → システムの順を保つ。
        let mic = Transcript {
            language: None,
            segments: vec![seg_t(1000, 1200, "mic")],
        };
        let system = Transcript {
            language: None,
            segments: vec![seg_t(1000, 1500, "sys")],
        };
        let (merged, _) = merge_tracks(mic, system, vec![], Lang::Ja);
        assert_eq!(merged.segments[0].text, "mic");
        assert_eq!(merged.segments[1].text, "sys");
    }
}
