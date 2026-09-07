# ADR-0038: Background model downloads and saved live translations

- Date: 2026-09-07
- Status: Accepted
- Supersedes: The download ownership and temporary-only lifecycle in [ADR-0037](ADR-0037_Optional_local_live_translation.md)

## Context

The v0.6.0 translation preview coupled a multi-gigabyte model download and completed
translations to the meeting page. Navigating away could cancel the download, delete its
partial file, and clear translated captions. Users also could not prepare the selected
models before a meeting or read completed translations after saving the recording.

The final offline transcript can differ from live captions. A saved translation must
retain its exact original caption rather than appear to translate a later revised text.

## Decision

### Model preparation belongs to the application

Settings exposes an explicit Download action for the selected transcription and summary
models, the translation model, and missing live-caption prerequisites. Selecting a model
does not start a download. Show the model size, progress, readiness, and retryable errors.
Live captions continue to use turbo regardless of the offline transcription selection.

`ModelDownloadsState` owns native background tasks and their progress snapshots. A page
subscribes to events and obtains a current snapshot when mounted. Translation inference
joins the same transfer. Navigation does not cancel translation or its download. Revising
a caption, disabling translation, or stopping recording cancels the waiter or inference,
not the model transfer. Quitting the application ends its tasks; retained partial files
can be resumed on the next request.

There is at most one application transfer per catalog file. Core cache-path locks also
serialize direct model preparation from offline jobs with Settings downloads. The background
download manager does not acquire the heavy ML permit; existing job and inference callers
retain their scheduling rules.

For catalog transcription, summary, VAD, and translation models:

- Keep a stable partial file beside the destination and request the missing byte range.
- Accept partial responses only when the range and total match the expected model.
  If the server returns a complete response instead, truncate the partial before writing.
- Retain interrupted transfers. Discard oversized or checksum-invalid content.
- Verify the complete expected byte count and SHA-256 before atomically installing the
  model. A valid complete partial can be installed without another network transfer.
- Keep pinned source revisions and existing model choices. Translation still has a
  separate cache file and does not change summary selection.

### Completed translations belong to the meeting and recording

`App` owns the live translation controller for the entire meeting. Route changes no
longer dispose of it. Completed results are retained by `(source caption ID, target
language)`, with the exact source text attached. A successful revised result replaces the
previous completed value for that key. Turning translation off or changing the target
stops obsolete inference while preserving completed results. A new or discarded meeting
clears the meeting archive.

On Stop, freeze the completed archive and pass it to native recording persistence. Pending,
skipped, failed, and cancelled requests are not saved as translated text. Saving does not
wait for the model download or pending translation inference to finish.

SQLite schema v7 adds `live_translations`, containing the recording ID, stable row order,
source caption ID, exact source text, target language, and translated text. Insert the
recording, search row, and translations in one transaction. Validate unique source/target
keys, supported targets, nonempty text, row size, row count, and total text size. Reject an
invalid snapshot before stopping capture. A database write failure rolls back the database
transaction instead of leaving a recording without its submitted translation history.

The detail view reads saved translations separately from the final transcript. Replacing
or regenerating the final transcript does not overwrite this history. Deleting the recording
cascades to its translations. The existing read-only MCP binary must be rebuilt with the
schema-v7 core so its schema compatibility check accepts the updated database.

## Alternatives

| Alternative | Reason not selected |
|---|---|
| Start downloads automatically when selecting a model | A choice should not silently start a multi-gigabyte transfer. |
| Keep page-owned tasks and add more cancellation warnings | Does not preserve progress or completed work when navigating. |
| Recreate translations from the final transcript on Stop | Changes the source text, adds inference work, and cannot preserve what the user saw. |
| Attach translations directly to final transcript segment IDs | Live captions and final segments do not have a reliable one-to-one mapping. |
| Persist every live update immediately | Adds continuous writes and a separate crash-recovery protocol beyond saving a completed meeting. |

## Consequences and verification

Navigation is no longer a data-lifetime boundary. Completed translations now occupy local
storage alongside their recording. This does not change the local inference model, its
memory requirement, caption revision guards, child cancellation, or shared ML semaphore.

Only completed translations captured at Stop are durable. App termination before Stop can
still lose the in-memory translation archive. Historical v0.6.0 translations cannot be
recovered because that version did not save them. Stored rows are capped at 20,000 and
32 MiB of text per submitted recording; source and translated fields are separately bounded.
The controller pauses further translation before exceeding those limits, retaining a saveable
archive while audio recording continues. Only a new meeting or explicit discard resets this
limit. Native and frontend boundaries both reject blank or oversized translated output.

Tiny localhost HTTP fixtures exercise interrupted resume, ignored-range fallback, malformed
range rejection, whole-file integrity, retained partials, and cache-path serialization.
Native coordinator tests exercise view detachment, transfer joining, retry, and catalog
validation. Store tests cover migration/reopen, transcript replacement, deletion cascade,
and transaction rollback. Controller tests cover completed-result retention, revised
captions, and cancellation. Native UI and combined release validation
remain part of the release checks; these unit tests do not establish translation quality.
