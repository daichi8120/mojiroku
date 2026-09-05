"""Cache a deterministic FLEURS test subset. Python 3.10+, standard library only."""

import argparse
import csv
import hashlib
import io
import json
from pathlib import Path
import shutil
import tarfile
import urllib.request

REVISION = "70bb2e84b976b7e960aa89f1c648e09c59f894dd"
BASE = f"https://huggingface.co/datasets/google/fleurs/resolve/{REVISION}"
ARCHIVES = {
    "ja_jp": "5de465fa7aaafc4e2c13aba44771550b8cd2dd29bb9b265daeb6d92ca8e0c136",
    "en_us": "d9c2e37b41aacd41bc283554a0a82b5476b36887049774ecb2819dcaaa55a356",
}
METADATA_HASHES = {
    "ja_jp": "5dd9643511437414681ad3f23508596c621cdf78978724a09f1f06fefe9d300b",
    "en_us": "74c046239374deeb60fa63f258f907388093a32bcaa3140965f70ef05c79f7ca",
}
CACHE = Path(__file__).resolve().parent / "cache"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def download(url: str, path: Path, expected: str | None = None) -> None:
    if path.exists() and (expected is None or sha256(path) == expected):
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    partial = path.with_suffix(path.suffix + ".part")
    print(f"Downloading {url}", flush=True)
    with urllib.request.urlopen(url, timeout=120) as response, partial.open("wb") as output:
        shutil.copyfileobj(response, output)
    if expected is not None and sha256(partial) != expected:
        raise ValueError(f"checksum mismatch: {partial}")
    partial.replace(path)


def select_rows(tsv: str, limit: int) -> list[list[str]]:
    rows = list(csv.reader(io.StringIO(tsv), delimiter="\t", quoting=csv.QUOTE_NONE))
    if not rows or any(len(row) != 7 for row in rows):
        raise ValueError("unexpected FLEURS TSV schema (expected seven columns)")
    if not 1 <= limit <= len(rows):
        raise ValueError(f"limit must be between 1 and {len(rows)}")
    # Hash ordering is independent of upstream TSV order and Python's RNG version.
    # Filenames identify recordings; sentence IDs are repeated across speakers.
    return sorted(rows, key=lambda row: hashlib.sha256(
        ("mojiroku-stt-v1:" + row[1]).encode()).hexdigest())[:limit]


def prepare(cache: Path, limit: int) -> Path:
    records = []
    for locale, archive_hash in ARCHIVES.items():
        folder = cache / locale
        metadata = folder / "test.tsv"
        archive_path = folder / "test.tar.gz"
        download(f"{BASE}/data/{locale}/test.tsv", metadata, METADATA_HASHES[locale])
        rows = select_rows(metadata.read_text(encoding="utf-8"), limit)
        download(f"{BASE}/data/{locale}/audio/test.tar.gz", archive_path, archive_hash)
        wanted = {row[1] for row in rows}
        audio_dir = folder / "audio"
        audio_dir.mkdir(parents=True, exist_ok=True)
        found = set()
        with tarfile.open(archive_path, "r|gz") as archive:
            for member in archive:
                name = Path(member.name).name
                if name not in wanted or not member.isfile():
                    continue
                if name in found:
                    raise ValueError(f"duplicate audio member: {name}")
                # Never extract archive paths, symlinks, or arbitrary files.
                with archive.extractfile(member) as source, (audio_dir / name).open("wb") as dest:
                    shutil.copyfileobj(source, dest)
                found.add(name)
        if found != wanted:
            raise ValueError(f"missing audio: {sorted(wanted - found)}")
        for row in rows:
            sentence_id, filename, raw_text, _, _, num_samples, _ = row
            audio = audio_dir / filename
            records.append({
                "id": f"{locale}/{filename}", "sentence_id": sentence_id,
                "language": locale[:2], "audio": str(audio.relative_to(cache)),
                "audio_sha256": sha256(audio), "reference": raw_text,
                "duration_seconds": int(num_samples) / 16000,
            })
        print(f"Prepared {len(rows)} {locale} recordings", flush=True)
    manifest = cache / f"fleurs-test-{limit}.json"
    manifest.write_text(json.dumps({
        "dataset": "google/fleurs", "revision": REVISION, "split": "test",
        "license": "CC-BY-4.0", "source": "https://huggingface.co/datasets/google/fleurs",
        "selection": "sha256(mojiroku-stt-v1:<filename>), first N per language",
        "limit_per_language": limit, "archive_sha256": ARCHIVES,
        "metadata_sha256": METADATA_HASHES, "records": records,
    }, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return manifest


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, default=CACHE)
    parser.add_argument("--limit", type=int, default=20, help="recordings per language")
    args = parser.parse_args()
    if args.limit < 1:
        parser.error("--limit must be positive")
    print(prepare(args.cache.resolve(), args.limit))


if __name__ == "__main__":
    main()
