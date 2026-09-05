"""Compare greedy and beam-5 through transcribe_cli; run inference serially."""

import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import platform
import subprocess
import time

from download import CACHE, sha256
from metrics import NORMALIZATION, score

ROOT = Path(__file__).resolve().parents[2]
# Pinned to the product defaults in crates/mojiroku-core/src/models/mod.rs.
MODEL_HASHES = {
    "ggml-large-v3-turbo-q5_0.bin": "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
    "ggml-silero-v5.1.2.bin": "29940d98d42b91fbd05ce489f3ecf7c72f0a42f027e4875919a28fb4c04ea2cf",
}


def combined_text(segments: list[dict], language: str) -> str:
    separator = "" if language == "ja" else " "
    return separator.join(segment["text"].strip() for segment in segments)


def aggregate(rows: list[dict]) -> list[dict]:
    results = []
    for language in ("ja", "en"):
        for decoder in ("greedy", "beam5"):
            group = [row for row in rows if row["language"] == language and row["decoding"] == decoder]
            if not group:
                raise ValueError(f"no results for {language}/{decoder}")
            errors = sum(row["errors"] for row in group)
            units = sum(row["reference_units"] for row in group)
            duration = sum(row["duration_seconds"] for row in group)
            pipeline = sum(row["pipeline_seconds"] for row in group)
            results.append({
                "language": language, "decoding": decoder,
                "metric": "CER" if language == "ja" else "WER", "recordings": len(group),
                "errors": errors, "reference_units": units, "rate": errors / units,
                "audio_seconds": duration, "pipeline_seconds": pipeline,
                "process_seconds": sum(row["process_seconds"] for row in group),
                "real_time_factor": pipeline / duration,
            })
    return results


def invoke(binary: Path, audio: Path, models: Path, hint: str, decoder: str,
           timeout: float, log: Path) -> dict:
    started = time.perf_counter()
    # A subprocess per recording follows the product's file path including model
    # loading. No Python inference, shell interpolation, or concurrent ML jobs.
    with log.open("w", encoding="utf-8") as stderr:
        completed = subprocess.run(
            [str(binary), str(audio), str(models), hint, decoder, "--json"],
            capture_output=False, stdout=subprocess.PIPE, stderr=stderr,
            text=True, check=True, timeout=timeout,
        )
    process_seconds = time.perf_counter() - started
    result = json.loads(completed.stdout)
    if result["decoding"] != decoder:
        raise ValueError("CLI did not use the requested decoder")
    if (result["whisper_model"], result["vad_model"]) != tuple(MODEL_HASHES):
        raise ValueError("model defaults changed; update the harness model preflight")
    if "stt vad: filtering failed" in log.read_text(encoding="utf-8"):
        raise ValueError(f"VAD fell back to raw audio; see {log}")
    result["process_seconds"] = process_seconds
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=CACHE / "fleurs-test-20.json")
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/examples/transcribe_cli")
    parser.add_argument("--models", type=Path, required=True, help="existing app model cache; no downloads")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--language-mode", choices=("auto", "reference"), default="auto")
    parser.add_argument("--timeout", type=float, default=600, help="maximum seconds per recording")
    args = parser.parse_args()
    manifest_path, binary, models = args.manifest.resolve(), args.binary.resolve(), args.models.resolve()
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    records = manifest["records"]
    if not records or {row["language"] for row in records} != {"ja", "en"}:
        parser.error("manifest must contain both Japanese and English recordings")
    if len({row["id"] for row in records}) != len(records):
        parser.error("manifest contains duplicate recording IDs")
    if args.timeout <= 0:
        parser.error("--timeout must be positive")
    # Verify inputs before spending time on inference. Missing VAD must not silently
    # turn this into a different pipeline through the app's best-effort fallback.
    model_hashes = {name: sha256(models / name) for name in MODEL_HASHES}
    if model_hashes != MODEL_HASHES:
        raise ValueError("model checksum mismatch; use the product's default models")
    for row in records:
        if sha256(manifest_path.parent / row["audio"]) != row["audio_sha256"]:
            raise ValueError(f"audio checksum mismatch: {row['id']}")
        score(row["reference"], "", row["language"])
        if row["duration_seconds"] <= 0:
            raise ValueError(f"invalid duration: {row['id']}")

    output = args.output or Path(__file__).resolve().parent / "results" / datetime.now(
        timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    output.mkdir(parents=True, exist_ok=False)
    git = lambda *args: subprocess.check_output(["git", "-C", str(ROOT), *args], text=True).strip()
    metadata = {
        "dataset": {key: value for key, value in manifest.items() if key != "records"},
        "manifest_sha256": sha256(manifest_path), "binary_sha256": sha256(binary),
        "model_sha256": model_hashes, "normalization": NORMALIZATION,
        "scorer_sha256": sha256(Path(__file__).with_name("metrics.py")),
        "runner_sha256": sha256(Path(__file__)),
        "git_commit": git("rev-parse", "HEAD"), "git_dirty": bool(git("status", "--porcelain")),
        "source_sha256": {str(path.relative_to(ROOT)): sha256(path) for path in sorted(
            (ROOT / "crates/mojiroku-core").rglob("*.rs"))},
        "platform": platform.platform(), "machine": platform.machine(),
        "language_mode": args.language_mode, "decoder_order": "alternates per recording",
        "warmup": "one excluded call per decoder on the first recording",
        "timing": "pipeline includes audio decode, model load, VAD, and whisper; excludes build/download",
    }
    (output / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    print(f"Results: {output}", flush=True)
    first = records[0]
    hint = "auto" if args.language_mode == "auto" else first["language"]
    for decoder in ("greedy", "beam5"):
        invoke(binary, manifest_path.parent / first["audio"], models, hint, decoder,
               args.timeout, output / f"warmup-{decoder}.log")
    rows = []
    with (output / "recordings.jsonl").open("w", encoding="utf-8") as stream:
        for index, record in enumerate(records):
            hint = "auto" if args.language_mode == "auto" else record["language"]
            order = ("greedy", "beam5") if index % 2 == 0 else ("beam5", "greedy")
            for decoder in order:
                result = invoke(binary, manifest_path.parent / record["audio"], models, hint,
                                decoder, args.timeout, output / f"{index:04}-{decoder}.log")
                hypothesis = combined_text(result["transcript"]["segments"], record["language"])
                row = {**record, **result, **score(record["reference"], hypothesis, record["language"]),
                       "hypothesis": hypothesis}
                rows.append(row)
                stream.write(json.dumps(row, ensure_ascii=False) + "\n")
                stream.flush()
                print(f"{index + 1}/{len(records)} {record['language']} {decoder}: "
                      f"{row['rate']:.1%}, {row['pipeline_seconds']:.2f}s", flush=True)
    # Written only when every pair succeeds; failures never produce partial scores
    # that could be mistaken for a completed benchmark.
    summary = aggregate(rows)
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print("| Language | Decoder | Metric | Error rate | Pipeline (s) | RTF |")
    print("|---|---|---|---:|---:|---:|")
    for row in summary:
        print(f"| {row['language']} | {row['decoding']} | {row['metric']} | {row['rate']:.2%} | "
              f"{row['pipeline_seconds']:.2f} | {row['real_time_factor']:.3f} |")


if __name__ == "__main__":
    main()
