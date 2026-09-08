"""Compare automatic language selection on pinned public speech and short live tails."""

import argparse
import json
from pathlib import Path
import statistics

from download import sha256
from mixed import read_float_wav, write_float_wav
from run import MODEL_HASHES, TURBO, VAD, combined_text, invoke
from metrics import score


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--models", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--per-language", type=int, default=4)
    args = parser.parse_args()
    if args.per_language < 1:
        parser.error("--per-language must be positive")
    for name in [TURBO, VAD]:
        if sha256(args.models / name) != MODEL_HASHES[name]:
            raise ValueError(f"unexpected model checksum: {name}")
    source = json.loads(args.manifest.read_text())
    selected = [row for lang in ["ja", "en"]
                for row in [r for r in source["records"] if r["language"] == lang][:args.per_language]]
    if {r["language"] for r in selected} != {"ja", "en"}:
        raise ValueError("manifest must contain both Japanese and English")
    args.output.mkdir(parents=True, exist_ok=False)
    fixtures = []
    for index, row in enumerate(selected):
        path = args.manifest.parent / row["audio"]
        if sha256(path) != row["audio_sha256"]:
            raise ValueError("unexpected source checksum")
        pcm = read_float_wav(path)
        for kind, samples in [("full", pcm), ("live-3.5s", pcm[:int(3.5 * 64000)])]:
            audio = args.output / f"{index}-{kind}.wav"
            write_float_wav(audio, samples)
            fixtures.append({"audio": audio, "kind": kind, "language": row["language"],
                             "reference": row["reference"], "id": row["id"]})
    silence = args.output / "silence.wav"
    write_float_wav(silence, bytes(14 * 64000))
    fixtures.append({"audio": silence, "kind": "silence", "language": None})
    rows = []
    for index, fixture in enumerate(fixtures):
        # Alternate ordering to reduce warm-cache/order bias; never run ML concurrently.
        variants = [("baseline", args.baseline), ("candidate", args.candidate)]
        if index % 2:
            variants.reverse()
        for variant, binary in variants:
            result = invoke(binary, fixture["audio"], args.models, "auto", "greedy", 180,
                            args.output / f"{index}-{variant}.log")
            transcript = result["transcript"]
            if fixture["kind"] == "silence":
                assert not transcript["segments"], "silence reached text decoding"
            elif variant == "candidate" and transcript["segments"]:
                assert transcript["language"] in ["ja", "en"], "unsupported decoder language"
            row = {**{k: v for k, v in fixture.items() if k != "audio"},
                   "variant": variant, **result}
            if fixture["kind"] == "full":
                row["score"] = score(fixture["reference"],
                                     combined_text(transcript["segments"], fixture["language"]),
                                     fixture["language"])
            rows.append(row)
        (args.output / "results.json").write_text(json.dumps(rows, ensure_ascii=False, indent=2) + "\n")
    summaries = []
    for kind in ["full", "live-3.5s"]:
        for variant in ["baseline", "candidate"]:
            group = [r for r in rows if r["kind"] == kind and r["variant"] == variant]
            summaries.append({"kind": kind, "variant": variant, "count": len(group),
                              "median_pipeline_seconds": statistics.median(r["pipeline_seconds"] for r in group),
                              "detected_language_matches": sum(r["transcript"]["language"] == r["language"] for r in group)
                              if variant == "candidate" else None})
    comparisons = [{"kind": rows[index]["kind"],
                    "segments_identical": rows[index]["transcript"]["segments"]
                    == rows[index + 1]["transcript"]["segments"]}
                   for index in range(0, len(rows), 2)]
    summary = {"manifest_sha256": sha256(args.manifest), "baseline_sha256": sha256(args.baseline),
               "candidate_sha256": sha256(args.candidate), "measurements": summaries,
               "comparisons": comparisons,
               "limits": "Short tails lack partial reference labels. Decoder language does not guarantee output accuracy."}
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
