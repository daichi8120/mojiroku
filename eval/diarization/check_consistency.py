"""Check within-speaker consistency and separation against local time annotations.

Reference JSON: {"audio_sha256": "...", "intervals": [[start, end, "A"], ...]}.
Keep recordings, references identifying private recordings, and raw results untracked.
"""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import subprocess


def score(turns, intervals, minimum_purity=0.95):
    """Score 100 ms frames; silence is reported separately from identity purity."""
    counts = {}
    for start, end, speaker in intervals:
        count = counts.setdefault(speaker, Counter())
        for tick in range(round(start * 10), round(end * 10)):
            a, b = tick / 10, (tick + 1) / 10
            overlaps = [(max(0, min(b, y) - max(a, x)), label) for x, y, label in turns]
            best = max(overlaps, key=lambda item: item[0], default=(0, None))
            count[best[1] if best[0] > 0 else None] += 1
    rows = {}
    for speaker, count in counts.items():
        assigned = sum(n for label, n in count.items() if label is not None)
        labels = {label: n for label, n in count.items() if label is not None}
        dominant = max(labels, key=labels.get, default=None)
        rows[speaker] = {
            "dominant_label": dominant,
            "labels": len(labels),
            "purity": labels.get(dominant, 0) / assigned if assigned else 0,
            "coverage": assigned / sum(count.values()) if count else 0,
        }
    dominant = [row['dominant_label'] for row in rows.values()]
    separate = None not in dominant and len(set(dominant)) == len(rows)
    return {"speakers": rows, "separate": separate,
            "pass": bool(rows) and separate and all(r['purity'] >= minimum_purity for r in rows.values())}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--audio', type=Path, required=True)
    parser.add_argument('--reference', type=Path, required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--models', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    ref = json.loads(args.reference.read_text())
    digest = hashlib.sha256(args.audio.read_bytes()).hexdigest()
    if digest != ref['audio_sha256']:
        parser.error('audio checksum does not match the reference')
    intervals = ref['intervals']
    if not intervals or any(not (0 <= start < end) or not speaker for start, end, speaker in intervals):
        parser.error('reference intervals must be nonempty and have valid times and speakers')
    ordered = sorted(intervals)
    if any(a[1] > b[0] for a, b in zip(ordered, ordered[1:])):
        parser.error('reference intervals must not overlap')
    args.output.mkdir(parents=True, exist_ok=False)
    report = {'audio_sha256': digest, 'reference_sha256': hashlib.sha256(args.reference.read_bytes()).hexdigest()}
    for label, binary in [('baseline', args.baseline), ('candidate', args.binary)]:
        with (args.output / f'{label}.log').open('w') as log:
            output = subprocess.check_output([str(binary.resolve()), str(args.audio.resolve()), str(args.models.resolve())],
                                             stderr=log, text=True, timeout=1800)
        (args.output / f'{label}.txt').write_text(output)
        turns = [(float(a), float(b), c) for a, b, c in re.findall(r'^\s*(\d+\.\d+) --\s*(\d+\.\d+)\s+(S\d+)\s*$', output, re.MULTILINE)]
        report[label] = score(turns, intervals)
        report[label]['binary_sha256'] = hashlib.sha256(binary.read_bytes()).hexdigest()
    # A candidate must not appear more consistent by dropping difficult speech.
    report['coverage_preserved'] = all(row['coverage'] >= report['baseline']['speakers'][speaker]['coverage'] - 0.01
                                       for speaker, row in report['candidate']['speakers'].items())
    report['pass'] = report['candidate']['pass'] and report['coverage_preserved']
    (args.output / 'results.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))
    raise SystemExit(0 if report['pass'] else 1)


if __name__ == '__main__':
    main()
