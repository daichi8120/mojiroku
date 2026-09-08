# ADR-0040: Preserve brief speakers during cleanup

- Date: 2026-09-07
- Status: Superseded in part by [ADR-0041](ADR-0041_Control_speaker_fragmentation.md)
- Context: v0.6.0 feedback reports distinct remote participants receiving one label.

## Cause and decision

The duration-based cleanup selected groups speaking at least 15 seconds or 6% of
total detected speech, whichever was larger. If none qualified, only the largest
group survived. Every turn was then assigned to its nearest surviving group without
a minimum similarity requirement. Even orthogonal synthetic voice vectors merged.

Duration now seeds anchor candidates rather than deciding which voices may exist.
Shorter groups with cosine similarity below 0.65 to every current anchor remain
separate candidates. Individual turns require similarity at least 0.65 before
reassignment; otherwise their original detected group is retained. Missing turn
embeddings fall back to the original group centroid and then the original group.
Invalid or zero embeddings are excluded, and nonfinite similarity scores cannot
enter ordering. Long-duration groups remain candidates; identical or confused
voice embeddings can still produce incorrect assignments.

This adds no neural inference passes. The threshold is a conservative initial
policy checked on the fixtures below, not a universally calibrated identity score.
Preserving uncertain groups can expose more split speakers; future changes should
measure both merging and splitting rather than optimize speaker count alone.

## Validation

The provider's public four-speaker recording yielded four raw groups of about
11.3, 5.9, 5.7 and 7.6 seconds. Pairwise centroid similarities were 0.078–0.298.
The previous cleanup collapsed them to one speaker; the revised cleanup retained
all four and matched the grouping in the provider's example output.

| Fixture | Previous speaker count | Revised count |
|---|---:|---:|
| Provider four-speaker sample | 1 | 4 |
| English two-speaker sample 1 | 1 | 2 |
| English two-speaker sample 2 | 1 | 2 |
| English two-speaker sample 3 | 2 | 2 |
| Derived single-voice control | 1 | 1 |
| Derived long first voice / brief second voice | 1 | 2 |

The derived controls use interior portions of the provider's documented turns,
with silence between clips. These are speaker-retention checks, not independently
human-labeled diarization error rates or proof about the reported meeting.
Unit regressions cover absolute and relative duration cutoffs, similar fragments,
unsupported turn reassignment, and invalid short centroids. The existing models,
initial clustering threshold, microphone/system routing, and text alignment stay
unchanged. Audio and raw diagnostic output are kept outside tracked files.

Sources: [provider fixtures and reference output](https://k2-fsa.github.io/sherpa/onnx/speaker-diarization/models.html),
[public audio assets](https://github.com/k2-fsa/sherpa-onnx/releases/tag/speaker-segmentation-models).
