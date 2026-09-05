# Transcription evaluation

Compare greedy decoding and beam search (width 5) through the **real file pipeline**:
audio decode → Silero VAD → Whisper → original-time timestamp mapping. The runner
invokes `transcribe_cli`; there is no separate Python speech model.

This is a **sanity check on read speech, not a meeting-quality benchmark**. FLEURS
does not test overlapping speakers, conversational backchannels, long recordings,
or Japanese/English switching within a meeting. Real-use feedback after release
decides whether the app is better. No manual transcription or labelling is required.

Related: [#76](https://github.com/daichi8120/mojiroku/issues/76),
[#77](https://github.com/daichi8120/mojiroku/issues/77).

## Data and attribution

[FLEURS](https://huggingface.co/datasets/google/fleurs), by the FLEURS authors
([2022 paper](https://arxiv.org/abs/2205.12446)), is distributed under
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). The harness downloads
the Japanese `ja_jp` and English `en_us` **test** archives and their existing
transcripts from revision `70bb2e84b976b7e960aa89f1c648e09c59f894dd`.
Archive and TSV SHA-256 hashes are pinned in `download.py`.

The first download is about 739 MB; extracted samples need additional disk space.
Audio, reference text, and raw predictions stay in ignored `cache/` and `results/`
directories. They are not part of the app or this repository. The downloader copies
selected WAVs unchanged. Scoring normalises the text as described below.

Selection sorts recording filenames by SHA-256 of `mojiroku-stt-v1:<filename>` and
takes the first N **per language**. The default is 20. This is independent of TSV
row order and Python's random-number generator. Sentence IDs can repeat across
speakers, so recording filenames are the unique keys. A larger N extends the same
sample; it does not resample it. The manifest records audio hashes, references,
duration, revision, selection rule, and attribution.

## Run

Use Python 3.10+ (standard library only) and the normal Rust/Apple Silicon build
environment. Commands below run from the repository root. First let the app finish
downloading its default Whisper and Silero models. The runner verifies their hashes
and rejects missing or different models before inference.

```bash
python3 eval/stt/download.py --limit 20
cargo build --release -p mojiroku-core --example transcribe_cli
python3 eval/stt/run.py \
  --manifest eval/stt/cache/fleurs-test-20.json \
  --models "$HOME/Library/Application Support/com.daichi0812.mojiroku/models"
```

Run with Metal GPU access. An agent sandbox that denies the GPU can crash the
bundled Metal backend before transcription starts. Do not disable GPU acceleration
to obtain a nominally comparable timing. Stop other transcription/summary jobs and
avoid builds during the measurement; the harness runs one ML process at a time.

Optional arguments:

- `download.py --limit 100`: 100 recordings per language. Archive downloads are cached.
- `download.py --cache /path`: keep data elsewhere; pass its manifest to the runner.
- `run.py --language-mode reference`: force `ja`/`en` from the dataset. The default
  is **auto**, matching the app's automatic transcription setting. Do not compare
  runs with different language modes as if only the decoder changed.
- `run.py --binary /path/to/transcribe_cli`: use a separately built executable.
- `run.py --output /path/to/new-directory`: choose an output directory; existing
  directories are rejected so previous results cannot be mixed into a new run.
- `run.py --timeout 600`: seconds per recording; errors abort the run.

To inspect one file:

```bash
target/release/examples/transcribe_cli audio.wav /path/to/models auto greedy --json
target/release/examples/transcribe_cli audio.wav /path/to/models auto beam5 --json
```

The existing positional CLI and human-readable output still work. The added fourth
positional argument selects `default`, `greedy`, or `beam5`; an optional `--json`
follows it. JSON includes segments, selected decoder/model names, and pipeline time.

## Scoring and timing

- Japanese CER: Unicode NFKC width normalisation, remove Unicode punctuation and
  whitespace, then character edit distance divided by reference characters. The
  long-vowel mark is preserved. Numbers and kanji readings are not rewritten.
- English WER: NFKC, lowercase, remove straight/curly apostrophes within words,
  replace other Unicode punctuation with spaces, then whitespace-delimited word
  edit distance divided by reference words. Numbers are not expanded into words.
- Join English segments with a space and Japanese segments without one. Each
  recording is scored separately so errors cannot align across recordings.
- Aggregate by **total errors / total reference units**, not mean per-file rates.
  Empty predictions count as deletions. An empty normalised reference fails the
  run. Insertions can push CER/WER above 100%; rates are not clipped.
- Run one excluded warm-up per decoder, then alternate which decoder goes first
  for each recording. Both see exactly the same input and language setting.
- `pipeline_seconds` includes model loading, audio decode, VAD, and Whisper, with
  models already cached. `process_seconds` additionally includes executable launch
  and exit. Neither includes compilation or corpus download. RTF is total pipeline
  seconds / total audio seconds; lower is faster. This is not decoder-only timing.

`metadata.json` records model, binary, Rust source, manifest, scorer, and runner
hashes, Git revision/dirty state, platform, and language mode. `recordings.jsonl`
contains references, hypotheses, segments, errors, and timing for every call.
Per-call `.log` files keep stderr. `summary.json` is written **only if all pairs
succeed**. A VAD filtering failure rejects the run instead of silently scoring raw
audio. These outputs remain local; only aggregate numbers belong in public reports.

The normalisation identifier is `nfkc-punctuation-v1`; changing these rules requires
a new identifier and a fresh baseline. These scores are not directly comparable to
published FLEURS scores that use another normaliser, model, or language hint.

## Checks

```bash
python3 -m unittest discover -s eval/stt -p 'test_*.py' -v
cargo test -p mojiroku-core
```

Tests cover edit counts, width/punctuation handling, empty output, word boundaries,
weighted aggregation, and deterministic selection. Rust checks pin the effective
Whisper search width and the rolling-history guard for both decoder choices.

## Baseline

Measured 2026-09-05 on Apple M4 Max, 128 GiB RAM, macOS 26.6.2, release build with
Metal. Default large-v3-turbo q5_0, Silero v5.1.2, automatic language detection,
rolling text history disabled (ADR-0032), 20 recordings per language. Audio totals:
Japanese 260.40 s, English 199.66 s. One measured call per recording/decoder.

| Language | Decoder | Errors / reference units | CER / WER | Pipeline seconds | Process wall seconds |
|---|---|---:|---:|---:|---:|
| Japanese | Greedy | 29 / 1,058 characters | 2.74% | 17.78 | 20.91 |
| Japanese | Beam 5 | 33 / 1,058 characters | 3.12% | 19.80 | 23.40 |
| English | Greedy | 32 / 467 words | 6.85% | 18.23 | 20.96 |
| English | Beam 5 | 32 / 467 words | 6.85% | 19.39 | 21.41 |

Beam search improved / tied / worsened per-recording error counts on 1 / 16 / 3
Japanese recordings and 1 / 18 / 1 English recordings. It took 11.4% more Japanese
pipeline time and 6.3% more English pipeline time, without an aggregate accuracy
gain on this sample. **Keep greedy as the file default; live stays greedy.**
This small sample does not establish that beam search is worse in general, and
these absolute timings are not estimates for 16 GiB Macs or long meetings.

Baseline provenance: base commit `d026b91904c722437aabb1a31df85143863b8ce0` plus
this change; measured executable SHA-256
`39cab9cffd4ba2eba17f0c62b313fece4c452bbc86dcb07cb976764d84dc8b19`.
The decoder configuration retains whisper.cpp's existing temperature/fallback
defaults. Model checksums are in `run.py`; corpus checksums are in `download.py`.
The full default decision is in
[ADR-0033](../../docs/decisions/ADR-0033_Evaluate_beam_search_before_changing_the_default.md).
