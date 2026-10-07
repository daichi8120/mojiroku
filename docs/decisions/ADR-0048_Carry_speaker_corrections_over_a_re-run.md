# ADR-0048: Carry speaker corrections over a re-run

- Date: 2026-10-07
- Status: Accepted
- Related: Issue #19, Issue #102, ADR-0024 (re-run with name carry-over), `docs/spec.md` §9.1

## Context

A user can change the speaker of a single line (`set_segment_speaker`, Issue #19). Until
v0.7.0 those corrections were safe only because the detail view hid speaker separation for
recordings that already had speakers. Issue #102 added 「話者分離をやり直す」 for exactly
those recordings, and the re-run rewrites every line's speaker
(`replace_speaker_assignments` deletes and re-inserts the segments). Every correction was
lost without a warning, and nothing recorded which lines had been corrected, so the loss
could not even be detected.

A correction is a stronger statement than a rename: it says the separation was wrong for
that line. Dropping it silently is not acceptable (§9.1).

## Decision

1. **Corrections are stored** in `speaker_corrections` (schema v8): one row per corrected
   line with the speaker the separation predicted and the speaker the user chose. A second
   correction of the same line keeps the first prediction. Choosing the predicted speaker
   again deletes the row (an undo). Re-transcribing deletes all rows, since `idx` may then
   point at different text.
2. **A re-run carries corrections over** (`correction::carry_corrections`). Line indexes do
   not change, because the text is not re-transcribed. Only the speaker ids change, so each
   chosen speaker X is mapped to a new id:
   1. the new speaker that most of X's **uncorrected** lines received (corrected lines do not
      vote; ties go to the smaller id),
   2. otherwise the voiceprint match used for name carry-over (`match_speaker_ids`, same
      threshold),
   3. the meeting's own speaker (`self`) maps to itself, as the mic track is never
      re-separated.
3. **A correction that cannot be mapped becomes unassigned ("?")**, and the job reports the
   count. The UI shows it as a notice after the re-run. The line is not left on the new
   prediction, which the user had already rejected once.
4. The stored rows are rewritten in the new ids. `predicted` becomes the re-run's
   assignment, so the table always compares the current separation with the user's choice.
   A row whose new prediction matches the user's choice is kept as a line confirmed by hand.
   Only the user choosing the predicted speaker again deletes a row.

## Consequences

- The table gives increment 2 of Issue #19 (corrections as evaluation labels) its data.
  Its rows are biased toward errors the user noticed (§9.1).
- Corrections made before v8 were never recorded and cannot be carried over.
- The unmapped notice is only shown while the app is open. Afterwards the "?" chips are
  the only sign.
- Any new path that rewrites `segments` must decide what happens to `speaker_corrections`.
