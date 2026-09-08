# ADR-0041: Control speaker fragmentation during cleanup

- Date: 2026-09-08
- Status: Accepted
- Supersedes: The short-cluster promotion and weak-turn fallback rules in ADR-0040

## Cause

The 0.6.1 cleanup used the same 0.65 cosine threshold for two different decisions:
merging a voice and declaring a new speaker. Every cluster below that threshold
became another speaker, even if it represented less than a second of speech.
Short or noisy speech can produce a weak voice match without representing a new person.

A second path revived discarded clusters. When an individual turn scored below
0.65 against every anchor, it retained its original raw label, even when its
aggregate cluster had already been matched to an established speaker.

## Decision

Keep duration-based anchors and the underlying segmentation and embedding models.
For additional short-cluster anchors, require at least one second of accumulated
speech and cosine similarity strictly below 0.40 to every existing anchor.
This separates the evidence needed to create a speaker from the evidence needed
to assign an individual utterance. Subsecond fragments do not establish new speakers.

For a weak individual turn, use its aggregate cluster's nearest anchor instead of
reviving the raw label. A cluster already retained as an anchor maps back to itself.
When the original cluster has no centroid, retain the existing fallback behavior;
this change does not invent voice evidence for failed embedding extraction.

The thresholds are conservative cleanup rules, not calibrated identity probabilities.
Very brief or acoustically similar participants can still merge; noisy speech can
still be assigned incorrectly. This change adds no neural inference passes and does
not change microphone/system routing, timestamps, speaker naming, or stored history.
Existing recordings require diarization to be rerun to obtain revised assignments.

## Verification

`eval/diarization/check_consistency.py` compares product CLI output against local
speaker/time annotations. It measures each annotated speaker's dominant-label share,
requires different speakers to have different dominant labels, and rejects a drop
in coverage relative to the baseline. Its negative controls reject both splitting
one voice and merging different voices; a matching total speaker count is insufficient.

Use the existing annotations in `eval/diarization/gt_adr0009.py` with the documented
600-second fixture, keeping the audio and checksum-bound reference JSON untracked.
The public brief-speaker suite remains a separate gate to prevent fixing splitting
by simply collapsing all short speakers.

The native CLI comparison against v0.6.1 on the existing annotated fixture produced:

| Measure | v0.6.1 | Candidate |
|---|---:|---:|
| Total labels | 11 | 3 |
| Speaker A dominant-label share | 86.1% | 96.3% |
| Speaker B dominant-label share | 90.6% | 99.1% |
| Speaker C dominant-label share | 99.6% | 99.6% |
| Different dominant labels for A/B/C | Yes | Yes |
| Assigned speech coverage | Baseline | Unchanged |

The final candidate also passed all six public retention cases: 4/2/2/2 speakers,
a single-voice control, and the brief second voice. The latter contains about 1.3
seconds of detected speech, motivating a one-second evidence floor rather than two.
The original newly reported meeting is not available on this Mac; this is a
reproduction of the same failure mode on existing annotated audio, not verification
of that particular recording. The coarse annotations and remaining misassignments
do not establish perfect speaker recognition.
