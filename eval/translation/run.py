"""Evaluate local translation on parallel FLEURS speech using the product CLIs."""
import argparse
import csv
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import statistics
import subprocess
import sys
import tarfile
import time

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "eval/stt"))
from download import ARCHIVES, BASE, METADATA_HASHES, REVISION, download, sha256

# Load the STT evaluation's catalog by path: both evaluation entrypoints are run.py.
_stt_spec = importlib.util.spec_from_file_location("mojiroku_stt_evaluation", ROOT / "eval/stt/run.py")
_stt_evaluation = importlib.util.module_from_spec(_stt_spec)
_stt_spec.loader.exec_module(_stt_evaluation)
TURBO, VAD = _stt_evaluation.TURBO, _stt_evaluation.VAD
MODEL_HASHES = _stt_evaluation.MODEL_HASHES


def run_checked(command: list[str], stderr, timeout: float) -> subprocess.CompletedProcess:
    # Own the wrapper and its descendants (notably /usr/bin/time's ML child).
    with subprocess.Popen(command, stdout=subprocess.PIPE, stderr=stderr, text=True,
                          start_new_session=True) as process:
        try:
            stdout, _ = process.communicate(timeout=timeout)
        except BaseException:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.communicate()
            raise
        result = subprocess.CompletedProcess(command, process.returncode, stdout)
        result.check_returncode()
        return result


def verify_stt_models(models: Path) -> dict[str, str]:
    hashes = {name: sha256(models / name) for name in (TURBO, VAD)}
    if any(value != MODEL_HASHES[name] for name, value in hashes.items()):
        raise ValueError("Whisper/VAD checksum mismatch; use the product's catalog models")
    return hashes


def validate_model_names(models: list[Path]) -> None:
    if len({model.name for model in models}) != len(models):
        raise ValueError("translation model basenames must be unique for unambiguous result grouping")


def stt_metadata(result: dict, log: str) -> dict:
    metadata = {key: result[key] for key in ("whisper_model", "vad_model", "decoding")}
    if metadata != {"whisper_model": TURBO, "vad_model": VAD, "decoding": "greedy"}:
        raise ValueError("CLI did not use the requested turbo/VAD/greedy pipeline")
    if "stt vad: filtering failed" in log:
        raise ValueError("VAD fell back to raw audio; the evaluation requires the product VAD pipeline")
    return {**metadata, "language_hint": "auto"}


def device_metadata() -> dict:
    device = {"platform": platform.platform(), "machine": platform.machine(),
              "cpu": None, "hardware_model": None, "physical_memory_bytes": None}
    if platform.system() == "Darwin":
        for field, key in (("cpu", "machdep.cpu.brand_string"), ("hardware_model", "hw.model"),
                           ("physical_memory_bytes", "hw.memsize")):
            try:
                value = run_checked(["/usr/sbin/sysctl", "-n", key], subprocess.DEVNULL, 3).stdout.strip()
                device[field] = int(value) if field == "physical_memory_bytes" else value
            except (OSError, ValueError, subprocess.SubprocessError):
                pass
    return device


def prepare(cache: Path, output: Path, limit: int) -> list[dict]:
    by_language = {}
    for locale in ARCHIVES:
        folder = cache / locale
        metadata = folder / "test.tsv"
        download(f"{BASE}/data/{locale}/test.tsv", metadata, METADATA_HASHES[locale])
        rows = {}
        for row in csv.reader(metadata.read_text().splitlines(), delimiter="\t", quoting=csv.QUOTE_NONE):
            if len(row) != 7:
                raise ValueError("unexpected FLEURS schema")
            # First recording in the pinned metadata, one per shared sentence ID.
            rows.setdefault(row[0], row)
        by_language[locale[:2]] = rows
    ids = sorted(set(by_language["ja"]) & set(by_language["en"]), key=int)
    ids = [id for id in ids if 50 <= len(by_language["en"][id][2]) <= 160
           and len(by_language["ja"][id][2]) <= 140][:limit]
    if len(ids) != limit:
        raise ValueError("not enough parallel short sentences")
    records = []
    for language, rows in by_language.items():
        locale = "ja_jp" if language == "ja" else "en_us"
        archive_path = cache / locale / "test.tar.gz"
        download(f"{BASE}/data/{locale}/audio/test.tar.gz", archive_path, ARCHIVES[locale])
        wanted = {rows[id][1]: id for id in ids}
        found = set()
        with tarfile.open(archive_path, "r|gz") as archive:
            for member in archive:
                name = Path(member.name).name
                if not member.isfile() or name not in wanted:
                    continue
                if name in found:
                    raise ValueError("duplicate selected audio member")
                id = wanted[name]
                audio = output / f"{language}-{id}.wav"
                with archive.extractfile(member) as source, audio.open("wb") as dest:
                    shutil.copyfileobj(source, dest)
                found.add(name)
                target = "en" if language == "ja" else "ja"
                records.append({"id": id, "language": language, "target": target,
                    "audio": str(audio), "audio_sha256": sha256(audio),
                    "reference": rows[id][2], "target_reference": by_language[target][id][2]})
        if found != set(wanted):
            raise ValueError("missing selected audio")
    return sorted(records, key=lambda row: (row["language"], int(row["id"])))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, default=ROOT / "eval/stt/cache")
    parser.add_argument("--stt", type=Path, required=True)
    parser.add_argument("--sidecar", type=Path, required=True)
    parser.add_argument("--models", type=Path, required=True)
    parser.add_argument("--translation-model", type=Path, action="append", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--limit", type=int, default=8)
    args = parser.parse_args()
    if args.limit < 1:
        parser.error("limit must be positive")
    validate_model_names(args.translation_model)
    stt_hashes = verify_stt_models(args.models.resolve())
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    records = prepare(args.cache.resolve(), args.output, args.limit)
    model_hashes = {str(p.resolve()): sha256(p) for p in args.translation_model}
    report = {"device": device_metadata(), "revision": REVISION, "dataset": "google/fleurs",
              "license": "CC-BY-4.0", "selection": "first short common sentence IDs; first pinned TSV recording",
              "stt_binary_sha256": sha256(args.stt), "sidecar_sha256": sha256(args.sidecar),
              "translation_models_sha256": model_hashes, "stt_models_sha256": stt_hashes,
              "metadata_sha256": METADATA_HASHES, "archive_sha256": ARCHIVES,
              "runner_sha256": sha256(Path(__file__)), "records": []}
    for row in records:
        name = f"{row['language']}-{row['id']}"
        with (args.output / f"{name}-stt.log").open("w") as log:
            stt = run_checked([str(args.stt.resolve()), row["audio"], str(args.models.resolve()),
                "auto", "greedy", "--json"], stderr=log, timeout=120)
        result = json.loads(stt.stdout)
        pipeline = stt_metadata(result, (args.output / f"{name}-stt.log").read_text())
        text = ("" if row["language"] == "ja" else " ").join(s["text"].strip() for s in result["transcript"]["segments"])
        if not text:
            raise ValueError("empty ASR output")
        source = args.output / f"{name}.txt"
        source.write_text(text)
        for model in args.translation_model:
            log_path = args.output / f"{name}-{model.stem}.log"
            started = time.perf_counter()
            with log_path.open("w") as log:
                translated = run_checked(["/usr/bin/time", "-l", str(args.sidecar.resolve()), "--translate",
                    str(model.resolve()), str(source), row["target"], "--no-think"], stderr=log, timeout=35)
            elapsed = time.perf_counter() - started
            log = log_path.read_text()
            rss = re.search(r"(\d+)\s+maximum resident set size", log)
            if rss is None:
                raise ValueError(f"missing macOS peak RSS measurement: {log_path}")
            report["records"].append({**row, "asr": text, "stt": pipeline, "model": model.name,
                "model_path": str(model.resolve()),
                "output": translated.stdout, "seconds": elapsed,
                "max_rss_bytes": int(rss[1])})
            (args.output / "results.json").write_text(json.dumps(report, ensure_ascii=False, indent=2))
            print(f"{name} {model.name}: {elapsed:.2f}s", flush=True)
    summary = []
    for model in args.translation_model:
        times = [r["seconds"] for r in report["records"] if r["model"] == model.name]
        rss = [r["max_rss_bytes"] for r in report["records"] if r["model"] == model.name]
        summary.append({"model": model.name, "samples": len(times), "median_seconds": statistics.median(times),
                        "max_seconds": max(times), "max_rss_bytes": max(rss)})
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
