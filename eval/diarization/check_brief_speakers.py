"""Count-based speaker-retention gate using public provider fixtures; not a DER benchmark."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import wave

FIXTURES = {
    "0-four-speakers-zh.wav": ("bedf036caed208386c67b4ef4b11f83d74dd0d420b102163a1c33cd09cde7010", 4),
    "1-two-speakers-en.wav": ("f1c877dc01595e28be7147bf2fe38e5268147a868bf3fdb5c37b97f5940e21f3", 2),
    "2-two-speakers-en.wav": ("ee9c33d34e8f0fda4b78277f609944a1565aa16e6e2146f4cb8f0efb0d70030b", 2),
    "3-two-speakers-en.wav": ("dd3cf2344f8410ee9a2e271e96d1c3f9b530f113ae5b47defecbc0d741a468e9", 2),
}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--audio-dir", type=Path, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--models", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    for name, (digest, _) in FIXTURES.items():
        if sha(args.audio_dir / name) != digest:
            parser.error(f"fixture checksum mismatch: {name}")
    args.output.mkdir(parents=True, exist_ok=False)
    cases = [(args.audio_dir / name, expected) for name, (_, expected) in FIXTURES.items()]
    with wave.open(str(cases[0][0])) as source:
        params = source.getparams()
        assert params.nchannels == 1 and params.sampwidth == 2
        pcm = source.readframes(source.getnframes())
    rate = params.framerate
    def clip(start, end):
        return pcm[int(start * rate) * 2:int(end * rate) * 2]
    for name, pieces, expected in [
        ("single-voice", [clip(1, 6), clip(22.5, 24.5), clip(53, 54)], 1),
        ("brief-second-voice", [clip(1, 6)] * 6 + [clip(7.5, 10)], 2),
    ]:
        path = args.output / f"{name}.wav"
        with wave.open(str(path), "wb") as output:
            output.setparams(params)
            output.writeframes((b"\0" * rate * 2).join(pieces) + b"\0" * rate * 2)
        cases.append((path, expected))
    rows = []
    for audio, expected in cases:
        row = {"fixture": audio.name, "sha256": sha(audio), "expected_speakers": expected}
        for label, executable in [("baseline", args.baseline), ("candidate", args.binary)]:
            if executable is None:
                continue
            with (args.output / f"{audio.stem}-{label}.log").open("w") as log:
                result = subprocess.run([str(executable.resolve()), str(audio.resolve()), str(args.models.resolve())],
                    stdout=subprocess.PIPE, stderr=log, text=True, check=True, timeout=180)
            (args.output / f"{audio.stem}-{label}.txt").write_text(result.stdout)
            row[label] = len(re.findall(r"^  speaker S\d+ =", result.stdout, re.MULTILINE))
        row["pass"] = row["candidate"] == expected
        rows.append(row)
        print(json.dumps(row), flush=True)
    report = {"binary_sha256": sha(args.binary), "baseline_sha256": sha(args.baseline) if args.baseline else None,
              "runner_sha256": sha(Path(__file__)),
              "models_sha256": {name: sha(args.models / name) for name in
                  ["sherpa-pyannote-segmentation-3-0.onnx", "nemo_titanet_large.onnx"]},
              "source": "https://github.com/k2-fsa/sherpa-onnx/releases/tag/speaker-segmentation-models", "cases": rows}
    (args.output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
    raise SystemExit(0 if all(row["pass"] for row in rows) else 1)


if __name__ == "__main__":
    main()
