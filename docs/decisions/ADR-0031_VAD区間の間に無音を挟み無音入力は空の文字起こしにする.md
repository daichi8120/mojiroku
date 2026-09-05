# 0031. Separate VAD speech spans with silence before whisper; return an empty transcript when the VAD finds no speech

- Status: adopted (2026-09-05). Refines [ADR-0008](./ADR-0008_VADはwhisper内蔵Sileroを独立適用.md); the standalone-VAD topology is unchanged.
- Date: 2026-09-05
- Related: [ADR-0008](./ADR-0008_VADはwhisper内蔵Sileroを独立適用.md) (standalone Silero VAD and the filtered→original time map) / Issue #65 (missing text in transcripts) / [ADR-0021](./ADR-0021_FFI例外シールドと重処理直列化.md) (memory ceiling that the filtered buffer must respect)

## Context

Issue #65 reports two symptoms: turns shown in the wrong order, and speech missing from the transcript. The order swap was the meeting-mode start offset (fixed in v0.5.6, #74). This ADR is about the missing text.

The working hypothesis in the issue was that the Silero thresholds (`threshold = 0.5`, `min_speech_duration_ms = 250`) cut speech. Measuring on a real 257 s two-track Japanese meeting showed otherwise:

- The VAD kept 52 spans (27% of the mic track) and 80 spans (47% of the system track). The spans line up with actual speech; the quiet backchannels ("はい", −35 to −38 dB) were kept, not dropped. Lowering the thresholds has nothing to recover.
- The loss happens after the VAD. `vad_filter` glued the padded spans back to back into one gap-free stream. Whisper then saw dozens of unrelated utterances with no pause between them, merged them into long segments (one mic segment spanned 69–145 s), and dropped short replies such as "なるほど", "OKです", "お願いします", "ありがとうございます" in the process. Segment counts: 26 (mic) and 28 (system) for 4.3 minutes of two-person conversation.
- A separate defect: when the VAD succeeds but finds no speech at all, `transcribe_inner` fell back to the raw PCM. 60 s of digital silence came back as two segments of "ご視聴ありがとうございました", the very hallucination ADR-0008 was written to stop. A muted or silent mic track in meeting mode hits this path.

Experiments on the same recording (single `full()` call unless noted):

| variant | mic segments / chars | system segments / chars | wall (mic / system) |
|---|---|---|---|
| gap-free concatenation (before) | 26 / 517 | 28 / 752 | 2.0 s / 3.0 s |
| 500 ms silence between spans | 38 / 552 | 59 / 784 | 2.1 s / 3.4 s |
| **1000 ms silence between spans** | **39 / 535** | **71 / 708** | 2.4 s / 3.6 s |
| 1000 ms + one `full()` per chunk split at gaps > 3 s | 32 / 551 (11 calls) | 55 / 706 (5 calls) | 4.5 s / 3.8 s |
| whisper `no_context = true` | identical to the row above it | identical | – |

The recovered text is the short replies and fillers; the content words of the long utterances are unchanged across all variants, which is also how a suspected hallucinated paragraph on the mic track was shown to be real low-level audio (it reproduced word for word across independent whisper calls).

## Decision

1. **Insert 1 s of digital silence between padded VAD spans that were not adjacent in the original audio** (`stt::concat_ranges`, `VAD_GAP_MS = 1000`). Adjacent spans (padding clamped together) stay contiguous so no fake pause is inserted inside one utterance.
2. **Extend the filtered→original time map to gap-aware snapping**: a segment start that falls inside an inserted gap maps to the next span's start, a segment end to the previous span's end. The monotonicity guarantee of ADR-0008 holds (unit-tested over the gap).
3. **When the VAD runs and finds no speech, return an empty transcript without calling whisper.** The raw-PCM fallback remains only for VAD *errors* (model missing or failing), as before.
4. **Print one diagnostic line per transcription** (`stt vad: N spans, kept X% of Ys, whisper input Zs`), the same pattern as the meeting track offset line from #74, so a user on a dev build can report what the VAD kept on their machine.
5. Do not change the Silero thresholds. The evidence does not support it, and lowering them trades directly against the hallucination guard.

## Consequences

- ✅ Short replies and fillers survive; segments follow utterance boundaries. On the reference meeting the segment count rose from 26 → 39 (mic) and 28 → 71 (system) with the same content.
- ✅ Silent input produces no text. Fixture procedure: a 60 s digital-silence WAV and a 40 s room-silence slice cut from a real mic track both go through `transcribe_cli`; the silence file must yield 0 segments.
- ⚠️ The whisper input grows by ~1 s per span (mic 91 s → 127 s, system 153 s → 201 s on the reference). STT wall time rose ~20%. STT is a small share of the pipeline next to diarization, so this is acceptable; the buffer is reserved up front as before (ADR-0021).
- ⚠️ Whisper segments can still span an inserted gap when a span produced no tokens (observed once: a start snapped ~60 s early). That is the ADR-0008 "segment spanning silence" limitation and is unchanged here; splitting at span boundaries remains the follow-up if it hurts seeking.
- Dev tools kept under `crates/mojiroku-core/examples/`: `vad_spans_cli` (Silero spans with per-span RMS, parameters on the command line) and `vad_ab_cli` (same audio with and without VAD, transcripts written to files for diffing).

## Alternatives

- **Lower `threshold` / `min_speech_duration_ms`**: rejected. The spans already cover the quiet backchannels; the loss is downstream of the VAD.
- **One whisper call per chunk split at long gaps**: rejected. Worse text on the mic track (fragments, more decode errors), 2× wall time, and each short chunk still pays a full 30 s encoder window.
- **`no_context = true`**: no effect on this recording; not adopted.
- **Split whisper segments at span boundaries**: deferred (see Consequences).
