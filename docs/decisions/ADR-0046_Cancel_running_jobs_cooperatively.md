# ADR-0046: Cancel running jobs cooperatively

- Date: 2026-09-24
- Status: Accepted
- Updates: ADR-0024 (running jobs were not cancellable)
- Related: Issue #114, parent Issue #116

## Context

ADR-0024 made only pending jobs cancellable: a running job lives inside
`spawn_blocking` and "hard cancel is not provided". In practice a 95-minute import
could not be stopped once it started, and a misplaced re-run held the single
heavy-job slot until it finished.

## Decision

Cancellation is cooperative and flag-based.

1. The worker creates an `Arc<AtomicBool>` per job and registers it in `JobQueue`
   before waiting for the heavy-job permit. `cancel_job` cancels a pending job in the
   store as before, or sets the running job's flag.
2. The flag is bound to the worker thread with `mojiroku_core::cancel::scope`
   (thread-local). The pipeline does not spawn threads, so every stage sees it without
   new parameters.
3. The core checks it at every stage boundary (`report_stage`: decode → transcribe →
   diarization → merge) and inside Whisper through whisper.cpp's abort callback. The
   callback reads the `AtomicBool` through a raw pointer, like the existing progress
   callback. whisper-rs 0.16's `set_abort_callback_safe` is not used: its trampoline
   reads the stored `Box<Box<dyn FnMut>>` as `&mut F`.
4. A cancelled run returns `CoreError::Cancelled` → `error.job.canceled`; the worker
   records the job as `canceled`, not `failed`, and emits `job://update`. Results are
   written only at the end of a job, so the recording keeps its previous transcript
   and speakers.

Speaker separation (sherpa-onnx) cannot be interrupted mid-call; it stops at the next
boundary.

## Evidence

`large-v3 q5_0`, a 320 s English file built from a public test clip, Apple Silicon:
uninterrupted 32.7 s; flag set at 4.0 s → `Cancelled` returned at 4.02 s.

## Consequences

- The UI offers 中断 / Stop for running jobs (with a confirmation, since partial work
  is discarded) as well as pending ones.
- New heavy stages must call `report_stage` (or `cancel::check`) at their boundaries,
  or cancellation waits until the next one.
