# ADR-0039: Restrict automatic recognition to Japanese and English

- Status: Accepted
- Date: 2026-09-07
- Scope: live and offline transcription with Auto, including each Japanese/English mixed-mode window

## Context

Live transcription was reported to produce Korean or Chinese during Japanese/English meetings. Auto passed `None` to Whisper, allowing every language in the model. Mixed mode repeated that unrestricted detection at speech pauses. Neither mode expressed the product's Japanese/English language scope to the recognizer.

This is a recognition issue, before optional translation. Filtering characters from the resulting text would not correct language conditioning and could remove legitimate Japanese kanji or quoted text.

## Decision

After the existing VAD filtering, obtain Whisper's language probabilities and select the higher probability of Japanese (`ja`) and English (`en`). Pass that explicit choice to the normal transcription decoder. Apply this policy to Auto and each mixed-mode window; explicit language choices bypass detection.

Reject missing, non-finite, out-of-range, or entirely zero Japanese/English probability evidence. Do not recover from a detection error by returning to unrestricted Auto. Equal valid probabilities choose English deterministically; this is a tie rule, not an accuracy claim.

Implementation remains inside the existing C++ exception guard. VAD failures, required-VAD behavior, empty-speech handling, timestamp remapping, mixed-window offsets, progress, greedy decoding, and the live turbo model remain unchanged. Auto returns the selected language in `Transcript.language`; previously that field remained `None`. Mixed aggregate transcripts remain `None` because their windows may use different languages.

The installed whisper-rs 0.16.0 API provides `WhisperState::pcm_to_mel` and `lang_detect`, whose result includes language probabilities indexed by language ID. The bundled [`whisper_lang_auto_detect_with_state`](../../vendor/whisper-rs-sys/whisper.cpp/src/whisper.cpp) encodes the first audio window and calculates those probabilities. Native Auto already performs that encoder pass.

The Rust `full()` wrapper rejects empty PCM, although the underlying C++ function can reuse an existing mel spectrogram when no samples are passed. We retain the supported Rust API: prepare at most the first 30 seconds for language detection, then let `full()` process the original filtered PCM normally. This adds bounded spectrogram preparation instead of introducing an unsafe binding solely to avoid it.

## Alternatives

| Option | Reason |
|---|---|
| Keep unrestricted Auto and filter scripts afterward | Does not fix decoding; kanji cannot identify Chinese versus Japanese reliably. |
| Force one language for the entire meeting | Prevents automatic Japanese/English switching. Explicit Japanese and English remain available. |
| Restrict only the opt-in mixed mode | Leaves the reported default live Auto path exposed. |
| Add another language classifier | Adds a model and runtime path before evaluating the probabilities already available. |
| Tune on one ambiguous short excerpt | Risks overfitting and does not establish meeting accuracy. |

## Validation

The public corpus uses the existing FLEURS manifest with SHA-256 `222f4cc4b9e4f1cce0f53140b0401aa9611360ab4e1b24a6b4b2e8731ed6c154`. Model checksums are pinned in `eval/stt/run.py`. Measurements below used a local M4 Max with 128 GiB memory and turbo/greedy, serially, against the v0.6.0 release candidate binary.

| Check | Result |
|---|---|
| Four complete clips per language | All 8 selected the reference language; text and timestamps identical to baseline. |
| First 3.5 seconds of those clips | All 8 selected the reference language; text and timestamps identical to baseline. Partial transcripts were not scored against full references. |
| 14 seconds of digital silence | Empty output in both builds. |
| Median complete-clip pipeline time | 0.882 s baseline; 0.892 s candidate. |
| Median 3.5-second pipeline time | 0.831 s baseline; 0.830 s candidate. |
| Existing six mixed-language fixtures | All passed passage-retention and timestamp gates. |
| Existing public quiet-speech regression | Passed, including silence and required-VAD failure behavior. |
| Probability selection regression | Higher Korean/Chinese probabilities cannot win over the allowed candidates; explicit choices bypass detection; invalid evidence fails. |

These small latency measurements include model loading and are observations, not a general performance guarantee. The baseline binary SHA-256 was `072501b7f60c6ae81fd8369bc106af79feef5ee8f3496d4ce5636fffdd88c317`; candidate SHA-256 was `3584337553f60eb6cc2f8f0911fd30c1f0bcd4bb1b92220a684e264c19394783`. Later unrelated build changes can change the binary digest.

A search of short public windows also exposed a useful limit. In Japanese FLEURS `ja_jp/12193914003531280379.wav`, the one-second excerpt from 6.36 to 7.36 seconds survives VAD but unrestricted detection selects Chinese. Restricting candidates selects **English**, with observed probabilities `p(en)=0.0986` and `p(ja)=0.0769`. The selection remains within the supported languages, but this excerpt does not establish correct Japanese recognition. No unsupported winner appeared in the tested two-second or 3.5-second windows. The original reported meeting has not been reproduced.

The pinned optional test retains this counterexample, with source SHA-256 `9273720d8c41f5f49729aee40551d5d5a3643bd98fed5e4dea68d14c1f48f50c`. It asserts the supported decoder-language boundary, not perfect transcript accuracy.

## Reproduction

```sh
python3 eval/stt/check_languages.py \
  --manifest /path/to/fleurs-test-20.json \
  --baseline /path/to/v0.6.0-transcribe_cli \
  --candidate /path/to/candidate-transcribe_cli \
  --models /path/to/models \
  --output /path/to/new-results

MOJIROKU_TEST_LANGUAGE_WAV=/path/to/ja_jp/audio/12193914003531280379.wav \
MOJIROKU_TEST_MODELS=/path/to/models \
cargo test -p mojiroku-core auto_language_restricts_an_ambiguous_public_tail -- --ignored
```

Audio, raw output, and model files remain outside tracked source. Choosing a supported decoder language cannot guarantee the output's language or correctness, especially for brief, noisy, or overlapping speech. This change intentionally does not support automatic recognition of other languages.
