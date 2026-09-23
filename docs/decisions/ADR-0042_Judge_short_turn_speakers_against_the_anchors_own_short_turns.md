# ADR-0042: Judge short-turn speakers against the anchor's own short turns

- Date: 2026-09-23
- Status: Accepted
- Refines: The additional-anchor rule in ADR-0041

## Cause

A 32-minute one-on-one recorded in meeting mode on v0.6.2 listed eight remote
speakers. The remote-participant (system) track holds one person. Re-running the
current diarization CLI on that track reproduced it: one speaker with 980 s of
speech and seven with 3 to 7 s each. Every turn of the seven was 0.3 to 1.9 s long,
mostly backchannels such as "なるほど" and "了解".

ADR-0041 creates an extra speaker when a cluster has at least one second of
accumulated speech and cosine similarity below 0.40 to every existing anchor.
Both conditions are unreliable for clusters built from short turns:

- Accumulated duration counts four 0.4 s fragments the same as one 1.6 s utterance.
  TitaNet embeddings of sub-second windows carry little voice identity.
- Short turns of the same person are far less similar to that person's centroid
  than 0.40 suggests. On this track, the dominant speaker's own turns shorter than
  2 s scored a median cosine of 0.40 against that speaker's centroid, a 25th
  percentile of 0.18, and a 5th percentile of 0.07. Half of that person's own
  short turns would pass as "another voice" under ADR-0041.

Once one fragment cluster became an anchor, the weak-turn fallback reassigned other
backchannels to it, so each promoted fragment grew to several seconds.

## Decision

Keep ADR-0041's duration anchors, merge threshold, and weak-turn fallback. Add two
conditions for promoting a short cluster to a new speaker:

1. The cluster must contain at least one turn of 1 s or longer. Accumulated
   sub-second fragments no longer count as evidence of a voice.
2. If every turn of the cluster is shorter than 2 s, its similarity to each duration
   anchor must be below that anchor's own short-turn 5th percentile (and still below
   0.40). The percentile is measured only when the anchor has at least ten short
   turns with embeddings; otherwise the fixed 0.40 rule from ADR-0041 applies.

The second rule compares like with like: a candidate built only from short turns is
judged against how short turns of the established speaker actually score, instead
of against a threshold tuned on longer speech. A candidate with a turn of 2 s or
longer keeps the fixed 0.40 rule.

This adds no inference passes; it reuses the per-turn embeddings consolidation
already computes.

## Trade-off

A real participant who speaks only in short turns, in a recording where an anchor
has many short turns, now needs a voice further from that anchor than before. A
brief participant whose short turns look like the dominant speaker's own short turns
is merged into that speaker. We accept this: with these embeddings the two cases are
not distinguishable, and eight phantom speakers in a two-person call is the more
visible failure. Speakers with at least one turn of 2 s or more are unaffected.

## Verification

| Case | v0.6.2 | This change |
|---|---:|---:|
| One-on-one, system track (1 person, ~1,006 s speech) | 8 speakers | 1 speaker |
| Annotated 600 s fixture (`check_consistency.py`, 3 people) | pass | pass, identical per-speaker purity and coverage |
| Public brief-speaker suite (`check_brief_speakers.py`, 6 cases) | 6/6 | 6/6 |

Neither rule is sufficient alone on the one-on-one track. Rule 1 alone would leave five
speakers: four of the promoted clusters had a turn of 1.2 to 1.9 s (derived from the
measured per-cluster statistics, not a separate run). Rule 2 alone leaves two:
the second, 17 s after reassignment, was seeded by a cluster of four turns totalling
1.6 s, none longer than 0.5 s.

The public "brief second voice" case has a single 1.32 s turn and passes both rules.
Its anchor has no short turns, so it exercises the fallback to 0.40, not the
percentile rule. The percentile rule is covered by unit tests and by the one-on-one
recording, which is private and not part of the repository.
Existing recordings need diarization to be rerun to get the new assignment.
