# Live translation evaluation

Run public FLEURS speech through the real transcription CLI, then translate the resulting
captions with the real sidecar. The original and parallel target references are retained
for inspection. This checks a small read-speech sample, not general meeting accuracy.

Data and attribution follow [`../stt/README.md`](../stt/README.md): Google FLEURS,
CC BY 4.0, pinned revision and archive/TSV checksums. Select the first eight common numeric
sentence IDs with English text 50–160 characters and Japanese text at most 140 characters,
using the first recording in each pinned TSV. Audio, reference text, and raw results must
remain local in ignored `cache/` or `results/`, or outside the repository.

```bash
cargo build --release -p mojiroku-core --example transcribe_cli
cargo build --release -p mojiroku-llm
python3 eval/translation/run.py \
  --stt target/release/examples/transcribe_cli \
  --sidecar target/release/mojiroku-llm \
  --models "$HOME/Library/Application Support/com.daichi0812.mojiroku/models" \
  --translation-model /path/to/Qwen3.5-4B-Q4_K_M.gguf \
  --translation-model /path/to/Qwen3.5-9B-Q4_K_M.gguf \
  --output eval/translation/results/comparison
```

Use existing model files, Metal access, and no concurrent ML/build workload. The script
records binary/model hashes, public audio hashes, raw ASR/translation, process time, and
macOS `/usr/bin/time -l` peak RSS. Output directories must be new. This comparison uses
Qwen3.5 models with `--no-think`; it is not a general benchmark for arbitrary models.

For the real live-worker test, concatenate the selected English and Japanese float WAVs
with five seconds of silence after each passage and five more seconds at the end. The
`read_float_wav` / `write_float_wav` helpers in `eval/stt/mixed.py` preserve source samples.
Then run:

```bash
MOJIROKU_LIVE_AUDIO=/path/to/padded-en-ja.wav \
MOJIROKU_LIVE_MODELS=/path/to/models \
MOJIROKU_LIVE_SIDECAR=/path/to/mojiroku-llm \
MOJIROKU_LIVE_TRANSLATION_MODEL=/path/to/Qwen3.5-9B-Q4_K_M.gguf \
MOJIROKU_LIVE_OUTPUT=/tmp/live-translation-results.json \
cargo test -p mojiroku --lib real_audio_live_worker_with_translation -- --ignored --nocapture
```

The test feeds SharedPcm at capture speed and translates committed live captions while
Whisper and the sidecar share the product semaphore. It reports capture scheduling delay
and translated lines. Frontend queue tests separately cover draft revisions and stale
responses. Microphone/ScreenCaptureKit capture still needs a normal application smoke test.

See [ADR-0037](../../docs/decisions/ADR-0037_Optional_local_live_translation.md) for the
measured model comparison, selected model, memory floor, and known limitations.

Parent-death cleanup has a separate real-sidecar check. It requires the watcher marker
as well as process termination, so a model crash cannot be mistaken for success:

```bash
python3 eval/translation/check_parent_exit.py \
  --sidecar target/release/mojiroku-llm \
  --model /path/to/Qwen3.5-9B-Q4_K_M.gguf \
  --source /path/to/public-caption.txt \
  --output eval/translation/results/parent-exit
```

The supervisor owns its test process group and cleans up on failure. Emergency parent-death
termination uses `_exit`, bypassing C/C++ exit handlers that could race with active inference.

Check caption handling separately from translation accuracy. The regression uses eight
synthetic English greetings, questions, and instructions. The optional public-caption input
adds existing English/Japanese ASR text from this harness. Each caption is sent to its own
language, so the output must be an exact copy rather than a reply or a paraphrase:

```bash
python3 eval/translation/check_source_text.py \
  --binary target/release/mojiroku-llm \
  --model /path/to/Qwen3.5-9B-Q4_K_M.gguf \
  --captions-from eval/translation/results/comparison/results.json \
  --output eval/translation/results/source-text
```

The baseline preserved 9/24 captions exactly; the explicit response protocol preserved 24/24.
The model signals same-language input, and the host performs the copy. Normal translations
carry a grammar-constrained header that is removed before display; normal sampling resumes
for the body. Malformed responses are rejected.
The model can still misclassify a language or mistranslate text, so these are focused
regressions, not a guarantee of correctness for every caption.
