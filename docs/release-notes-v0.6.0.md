# Release notes 0.6.0

mojiroku 0.6.0 adds optional Japanese/English language switching and live translation,
plus better handling of quiet speech.

- **Mixed-language transcription:** select **Japanese + English (slower)** to
  re-detect language at speech pauses during Japanese/English meetings.
- **Optional full Whisper large-v3:** choose the larger model for recordings and
  imported files. Live transcription continues using turbo.
- **Quiet-speech improvements:** quiet audio reaches speech detection more reliably,
  while the original recorded audio remains unchanged.
- **Live translation preview:** translate Japanese ↔ English beside the original
  meeting captions using local Qwen3.5-9B. Requires at least **16 GB RAM** and a
  separate **5.68 GB model download**, even if a summary model is already installed.
- **Playback duration:** retain the audio file's full duration after transcription finishes, so trailing silence and seeking remain accurate.

Turbo, greedy decoding, and automatic language detection remain the defaults.
Translation is off by default; summary model selection is unchanged.

Translations are temporary and are not saved or exported. They clear when disabled,
when the target language changes, when leaving the meeting screen, or when recording
stops. Slow processing can skip older pending captions. Recognition errors can carry
into translations, so check the original transcript when accuracy matters.
Mixed-language detection depends on pauses, and performance varies by hardware;
the 16 GB minimum is not a guarantee of responsiveness on every Mac.

New mixed-language and translation controls currently remain English in both
interface languages.
