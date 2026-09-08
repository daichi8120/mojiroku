# Release notes 0.6.2

mojiroku 0.6.2 fixes a speaker-label regression introduced in 0.6.1, where one person could be split into many speakers.

- **More stable speaker labels:** brief or unclear voice fragments need stronger evidence before becoming another speaker. Uncertain utterances use their established cluster's voice instead of reviving discarded labels.
- **Preserve distinct short contributions:** the fix retains the brief-speaker improvements covered by the public regression fixtures.

The fix applies to newly processed recordings. Existing saved speaker assignments do not change automatically.

Speaker recognition is still approximate. Very brief, similar, noisy, or overlapping voices may need manual correction. No new model download or database migration is required by this patch.
