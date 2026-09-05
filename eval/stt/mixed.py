"""Build and check public Japanese/English switching fixtures; no hand labels."""

import argparse
from datetime import datetime, timezone
import json
import math
from pathlib import Path
import platform
import struct
import subprocess

from download import CACHE, sha256
from metrics import score
from run import FULL, MODEL_HASHES, ROOT, TURBO, VAD, invoke


CASES = {
    "ja-en-ja": [("ja", 0), ("en", 0), ("ja", 1)],
    "en-ja-en": [("en", 1), ("ja", 2), ("en", 2)],
    "long-ja-en-ja": [("ja", 0), ("ja", 1), ("en", 0), ("en", 1), ("ja", 2)],
    "long-en-ja-en": [("en", 0), ("en", 1), ("ja", 0), ("ja", 1), ("en", 2)],
    "ja-only": [("ja", 0), ("ja", 1), ("ja", 2)],
    "en-only": [("en", 0), ("en", 1), ("en", 2)],
}
FORMAT = struct.pack("<HHIIHH", 3, 1, 16000, 64000, 4, 32)


def read_float_wav(path: Path) -> bytes:
    """Read the pinned FLEURS mono 16 kHz float WAVs without changing samples."""
    blob = path.read_bytes()
    if blob[:4] != b"RIFF" or blob[8:12] != b"WAVE":
        raise ValueError("expected a RIFF WAV")
    chunks = {}
    offset = 12
    while offset + 8 <= len(blob):
        tag, size = struct.unpack_from("<4sI", blob, offset)
        end = offset + 8 + size
        if end > len(blob):
            raise ValueError("truncated WAV chunk")
        chunks[tag] = blob[offset + 8:end]
        offset = end + (size & 1)
    if chunks.get(b"fmt ", b"")[:16] != FORMAT:
        raise ValueError("expected mono 16 kHz float32 FLEURS input")
    pcm = chunks.get(b"data", b"")
    if not pcm or len(pcm) % 4:
        raise ValueError("invalid or empty float PCM")
    return pcm


def write_float_wav(path: Path, pcm: bytes) -> None:
    if len(pcm) % 4:
        raise ValueError("incomplete float sample")
    body = b"fmt " + struct.pack("<I", len(FORMAT)) + FORMAT
    body += b"fact" + struct.pack("<II", 4, len(pcm) // 4)
    body += b"data" + struct.pack("<I", len(pcm)) + pcm
    path.write_bytes(b"RIFF" + struct.pack("<I", len(body) + 4) + b"WAVE" + body)


def prepare(source: Path, output: Path) -> Path:
    data = json.loads(source.read_text(encoding="utf-8"))
    records = {lang: [r for r in data["records"] if r["language"] == lang][:3]
               for lang in ("ja", "en")}
    if any(len(rows) != 3 for rows in records.values()):
        raise ValueError("source manifest must include at least three recordings per language")
    samples = {}
    source_paths = {}
    for lang, rows in records.items():
        for index, row in enumerate(rows):
            audio = source.parent / row["audio"]
            if sha256(audio) != row["audio_sha256"]:
                raise ValueError(f"source checksum mismatch: {row['id']}")
            samples[lang, index] = read_float_wav(audio)
            source_paths[lang, index] = audio
    output.mkdir(parents=True, exist_ok=False)
    for (lang, index), path in source_paths.items():
        name = f"source-{lang}-{index}.wav"
        (output / name).write_bytes(path.read_bytes())
        records[lang][index] = {**records[lang][index], "audio": name}
    fixtures = []
    for name, sequence in CASES.items():
        pcm = bytearray()
        passages = []
        for key in sequence:
            if passages:
                pcm.extend(bytes(64000))  # One second of digital silence, 16 kHz float32.
            start = len(pcm) // 4
            pcm.extend(samples[key])
            row = records[key[0]][key[1]]
            passages.append({**row, "start_ms": start // 16, "end_ms": len(pcm) // 64})
        audio = output / f"{name}.wav"
        write_float_wav(audio, pcm)
        fixtures.append({"name": name, "audio": audio.name, "audio_sha256": sha256(audio),
                         "duration_ms": len(pcm) // 64, "passages": passages})
    manifest = output / "manifest.json"
    manifest.write_text(json.dumps({
        "construction": "mixed-pauses-v1", "source_manifest_sha256": sha256(source),
        "source": {k: v for k, v in data.items() if k != "records"},
        "sources": [r for rows in records.values() for r in rows],
        "fixtures": fixtures,
    }, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return manifest


def evaluate(fixture: dict, segments: list[dict]) -> dict:
    """Assign each segment by maximum time overlap; expose ambiguous language boundaries."""
    passages = []
    for source in fixture["passages"]:
        if passages and passages[-1]["language"] == source["language"]:
            previous = passages[-1]
            separator = " " if source["language"] == "en" else ""
            previous["reference"] += separator + source["reference"]
            previous["end_ms"] = source["end_ms"]
            previous["source_ids"].append(source["id"])
        else:
            passages.append({**source, "source_ids": [source["id"]]})
    texts = [[] for _ in passages]
    invalid = 0
    crossing = 0
    previous = -1
    for segment in segments:
        start, end = segment["start_ms"], segment["end_ms"]
        if not 0 <= start < end <= fixture["duration_ms"] or start < previous:
            invalid += 1
        previous = start
        overlaps = [max(0, min(end, p["end_ms"]) - max(start, p["start_ms"])) for p in passages]
        languages = {p["language"] for p, overlap in zip(passages, overlaps) if overlap > 0}
        crossing += len(languages) > 1
        if not any(overlaps):
            invalid += 1
            continue
        texts[max(range(len(passages)), key=overlaps.__getitem__)].append(segment["text"])
    results = []
    for passage, text in zip(passages, texts):
        hypothesis = (" " if passage["language"] == "en" else "").join(text)
        results.append({"language": passage["language"], "source_ids": passage["source_ids"],
                        "hypothesis": hypothesis, **score(passage["reference"], hypothesis, passage["language"])})
    return {"passages": results, "invalid_timestamps": invalid, "cross_language_segments": crossing}


def passes(result: dict, baselines: dict, extra_rate: float, extra_units: int) -> bool:
    return (result["invalid_timestamps"] == 0 and result["cross_language_segments"] == 0
            and bool(result["passages"])
            and all(p["hypothesis"].strip() and p["errors"] < p["reference_units"]
                    and p["errors"] <= sum(baselines[key]["errors"] for key in p["source_ids"])
                    + max(extra_units, math.ceil(p["reference_units"] * extra_rate))
                    for p in result["passages"]))


def run(args) -> None:
    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    fixtures = manifest["fixtures"]
    if not fixtures or any(not f["passages"] or f["duration_ms"] <= 0 for f in fixtures):
        raise ValueError("fixtures need nonempty language passages and positive durations")
    hashes = {name: sha256(args.models / name) for name in (args.model, VAD)}
    if any(hashes[name] != MODEL_HASHES[name] for name in hashes):
        raise ValueError("model checksum mismatch")
    for fixture in fixtures:
        if sha256(args.manifest.parent / fixture["audio"]) != fixture["audio_sha256"]:
            raise ValueError("fixture checksum mismatch")
    for source in manifest["sources"]:
        if sha256(args.manifest.parent / source["audio"]) != source["audio_sha256"]:
            raise ValueError("isolated source checksum mismatch")
    args.output.mkdir(parents=True, exist_ok=False)
    metadata = {"manifest_sha256": sha256(args.manifest), "binary_sha256": sha256(args.binary),
                "model_sha256": hashes, "platform": platform.platform(), "modes": args.modes,
                "git_commit": subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True).strip(),
                "git_dirty": bool(subprocess.check_output(["git", "-C", str(ROOT), "status", "--porcelain"], text=True)),
                "source_sha256": {str(p.relative_to(ROOT)): sha256(p) for p in sorted((ROOT / "crates/mojiroku-core").rglob("*.rs"))},
                "runner_sha256": sha256(Path(__file__)), "scorer_sha256": sha256(Path(__file__).with_name("metrics.py")),
                "maximum_extra_error_rate": args.extra_rate, "minimum_extra_error_units": args.extra_units,
                "gate_reference": "isolated source clips with automatic language detection and the same model",
                "warmup": "one excluded call per mode; measured order alternates"}
    (args.output / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
    baselines = {}
    for index, source in enumerate(manifest["sources"]):
        result = invoke(args.binary, args.manifest.parent / source["audio"], args.models, "auto",
                        "greedy", 300, args.output / f"isolated-{index}.log", args.model)
        separator = " " if source["language"] == "en" else ""
        hypothesis = separator.join(s["text"] for s in result["transcript"]["segments"])
        baselines[source["id"]] = {**source, **result, "hypothesis": hypothesis,
                                    **score(source["reference"], hypothesis, source["language"])}
    (args.output / "isolated.json").write_text(json.dumps(baselines, ensure_ascii=False, indent=2) + "\n")
    for mode in args.modes:
        invoke(args.binary, args.manifest.parent / fixtures[0]["audio"], args.models, mode,
               "greedy", 300, args.output / f"warmup-{mode}.log", args.model)
    rows = []
    for index, fixture in enumerate(fixtures):
        for mode in args.modes if index % 2 == 0 else reversed(args.modes):
            result = invoke(args.binary, args.manifest.parent / fixture["audio"], args.models, mode,
                            "greedy", 300, args.output / f"{fixture['name']}-{mode}.log", args.model)
            measured = evaluate(fixture, result["transcript"]["segments"])
            row = {"fixture": fixture["name"], "mode": mode, **result, **measured,
                   "passed": passes(measured, baselines, args.extra_rate, args.extra_units)}
            rows.append(row)
            (args.output / f"{fixture['name']}-{mode}.json").write_text(json.dumps(row, ensure_ascii=False, indent=2) + "\n")
            print(f"{fixture['name']} {mode}: {'PASS' if row['passed'] else 'FAIL'}; "
                  f"{result['pipeline_seconds']:.2f}s", flush=True)
    summary = [{k: row[k] for k in ("fixture", "mode", "passed", "pipeline_seconds", "invalid_timestamps", "cross_language_segments")}
               for row in rows]
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    if any(not row["passed"] for row in rows if row["mode"] == args.require_mode):
        raise SystemExit(f"{args.require_mode} failed the passage-retention gate")


def compare_auto(args) -> None:
    """Check that the default path is unchanged on the existing single-language corpus."""
    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    records = manifest["records"]
    if not records or {r["language"] for r in records} != {"ja", "en"}:
        raise ValueError("the corpus must include both Japanese and English")
    model_hashes = {name: sha256(args.models / name) for name in (TURBO, VAD)}
    if any(model_hashes[name] != MODEL_HASHES[name] for name in model_hashes):
        raise ValueError("model checksum mismatch")
    for record in records:
        if sha256(args.manifest.parent / record["audio"]) != record["audio_sha256"]:
            raise ValueError("corpus checksum mismatch")
    args.output.mkdir(parents=True, exist_ok=False)
    metadata = {"baseline_binary_sha256": sha256(args.baseline_binary),
                "candidate_binary_sha256": sha256(args.binary), "manifest_sha256": sha256(args.manifest),
                "model_sha256": model_hashes, "runner_sha256": sha256(Path(__file__)),
                "language_mode": "auto", "decoding": "greedy", "platform": platform.platform()}
    (args.output / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
    binaries = {"baseline": args.baseline_binary, "candidate": args.binary}
    for label, binary in binaries.items():
        invoke(binary, args.manifest.parent / records[0]["audio"], args.models, "auto", "greedy", 300,
               args.output / f"warmup-{label}.log")
    rows = []
    for index, record in enumerate(records):
        results = {}
        labels = list(binaries) if index % 2 == 0 else list(reversed(binaries))
        for label in labels:
            result = invoke(binaries[label], args.manifest.parent / record["audio"], args.models,
                            "auto", "greedy", 300, args.output / f"{index:03}-{label}.log")
            hypothesis = (" " if record["language"] == "en" else "").join(s["text"] for s in result["transcript"]["segments"])
            results[label] = {**result, **score(record["reference"], hypothesis, record["language"])}
        row = {"id": record["id"], "language": record["language"], "results": results,
               "identical": results["baseline"]["transcript"] == results["candidate"]["transcript"]}
        rows.append(row)
        (args.output / f"{index:03}.json").write_text(json.dumps(row, ensure_ascii=False, indent=2) + "\n")
        if (index + 1) % 10 == 0 or index + 1 == len(records):
            print(f"Checked {index + 1}/{len(records)} single-language recordings", flush=True)
    summary = {"recordings": len(rows), "identical": sum(row["identical"] for row in rows),
               "differences": [row["id"] for row in rows if not row["identical"]]}
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary), flush=True)
    if summary["differences"]:
        raise SystemExit("default-path transcripts changed")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    prep = sub.add_parser("prepare")
    prep.add_argument("--source-manifest", type=Path, default=CACHE / "fleurs-test-20.json")
    prep.add_argument("--output", type=Path, default=CACHE / "mixed-v1")
    check = sub.add_parser("run")
    check.add_argument("--manifest", type=Path, default=CACHE / "mixed-v1/manifest.json")
    check.add_argument("--binary", type=Path, default=ROOT / "target/release/examples/transcribe_cli")
    check.add_argument("--models", type=Path, required=True)
    check.add_argument("--model", choices=(TURBO, FULL), default=TURBO)
    check.add_argument("--modes", nargs="+", choices=("auto", "mixed"), default=["auto", "mixed"])
    check.add_argument("--require-mode", choices=("auto", "mixed"), default="mixed")
    check.add_argument("--extra-rate", type=float, default=0.05)
    check.add_argument("--extra-units", type=int, default=2)
    check.add_argument("--output", type=Path, default=Path(__file__).parent / "results" / datetime.now(timezone.utc).strftime("mixed-%Y%m%dT%H%M%S.%fZ"))
    compare = sub.add_parser("compare-auto")
    compare.add_argument("--manifest", type=Path, default=CACHE / "fleurs-test-20.json")
    compare.add_argument("--baseline-binary", type=Path, required=True)
    compare.add_argument("--binary", type=Path, default=ROOT / "target/release/examples/transcribe_cli")
    compare.add_argument("--models", type=Path, required=True)
    compare.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "prepare":
        print(prepare(args.source_manifest.resolve(), args.output.resolve()))
    elif args.command == "compare-auto":
        compare_auto(args)
    else:
        if (not 0 <= args.extra_rate <= 1 or args.extra_units < 0 or args.require_mode not in args.modes
                or len(set(args.modes)) != len(args.modes)):
            parser.error("use unique modes including --require-mode, an extra rate in [0,1], and nonnegative extra units")
        run(args)


if __name__ == "__main__":
    main()
