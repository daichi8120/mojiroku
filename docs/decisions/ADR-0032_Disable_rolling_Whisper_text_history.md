# ADR-0032: Disable rolling Whisper text history

- Date: 2026-09-05
- Status: Accepted

## Context

Long recordings can enter a sustained decoding loop: the same short phrase replaces
continuing speech across many successive audio windows. The VAD spacing change in
ADR-0031 does not prevent this separate failure.

In the bundled `vendor/whisper-rs-sys/whisper.cpp/src/whisper.cpp`,
`whisper_full_with_state` clears `prompt_past0` and `prompt_past1` when a call starts
with `no_context=true`. It then rebuilds `prompt_past1` from decoded tokens and uses
that history in later windows whenever `n_max_text_ctx > 0` and the temperature is
below the history-conditioning cutoff. The default history budget is 16384.
Thus, `no_context=true` alone does not disable conditioning between windows inside
one transcription call.

## Decision

Set `FullParams::set_n_max_text_ctx(0)` in the shared decoder configuration. Each
window retains its audio context but receives no decoded text from earlier windows.
Keep language detection, the model, sampling, VAD, and timestamp mapping unchanged.
Apply this through the shared STT engine used by file, meeting, and live transcription.

## Alternatives

- **Only set `no_context=true`:** already the bundled default; it does not stop the
  rolling prompt within a single call.
- **Remove duplicate output lines:** hides the symptom and cannot recover speech
  that was replaced by a loop; it could also delete legitimate repeated speech.
- **Detect loops and retry with history disabled:** retains context for healthy
  recordings but adds detection thresholds, repeat inference, and recovery logic.
  Prefer the smaller decoder configuration change for this failure.

## Validation and consequences

A local A/B run on the same audio, model, language mode, and VAD configuration
reproduced sustained repetition with the default history and eliminated the
sustained loop with only the history budget disabled. This is evidence for the
repetition fix, not a word-accuracy benchmark. Evaluation content and identifying
metadata are excluded from the repository.

A regression test pins the history budget for automatic, Japanese, and English
language modes. It fails with the dependency default, even with `no_context=true`.

Losing earlier text may reduce terminology consistency across windows. It does not
prevent recognition errors or all hallucinations. Existing stored transcripts are
unchanged; affected recordings need transcription again with the updated decoder.
