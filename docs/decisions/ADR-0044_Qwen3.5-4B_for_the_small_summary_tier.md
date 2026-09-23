# ADR-0044: Qwen3.5-4B for the small summary tier

- Date: 2026-09-23
- Status: Accepted
- Updates: ADR-0030 (small tier)
- Depends on: ADR-0043 (simplified Chinese blocked in Japanese output)
- Related: Issue #30

## Context

Macs with less than 16 GB of memory download Qwen2.5-7B for local summaries. ADR-0030
kept it there because the lighter candidate, Qwen3.5-4B, failed the 2026-08-30 gate
with two visible defects: a dropped minutes heading and simplified Chinese in a title.

7B is not light. Its peak memory (6.26 GB) is close to the 9B model given to 16 GB+
Macs (6.53 GB), against 3.75 GB for 4B. Its minutes also do not follow meeting length:
a 27,947-character meeting produced 410 characters of minutes.

## Evidence (2026-09-23)

28 generations per model on the same local meetings (titles 10, minutes 8, summaries 5,
action items 5), fixed seed, production token limit.

| | Qwen2.5-7B | Qwen3.5-4B | Qwen3.5-4B with ADR-0043 |
|---|---:|---:|---:|
| Total time | 1,687 s | 941 s | 949 s |
| Simplified Chinese | 0 | 3 | **0** |
| Minutes missing a heading | 1/8 | 0/8 | 1/8 |
| Minutes length, 27,947-char meeting | 410 chars | 1,100 chars | about the same |
| Action items answered only "なし" | 4/5 | 1/5 | 1/5 |

Reading the outputs side by side: 4B minutes are more specific and follow the meeting;
7B minutes stay at 300–400 characters and keep transcription errors in names. The heading
drop occurs with both models at a similar rate and is not the distinguishing defect.
The simplified Chinese was, and ADR-0043 removes it at generation time.

4B has one weakness 7B does not show as often: on a lecture recording it listed the
lecture's content as decisions and action items, where 7B's "なし" was the better answer.

## Decision

- Qwen3.5-4B becomes the automatic choice for the small tier and the fallback default
  (`DEFAULT_SUMMARY_MODEL`). It is a thinking model and receives `--no-think`.
- Qwen2.5-7B stays in the catalog as an adopted model: it is **not chosen automatically**
  but can be chosen in Settings. The catalog gains `tier_default`, separating "chosen
  automatically for its tier" (at most one per tier) from "offered in Settings" (`adopted`).
- Existing installs are not changed. A Mac that already has 7B keeps using it (the cached
  model wins, ADR-0030), so nobody is made to download 2.7 GB behind their back.
  New installs and Macs without a summary model get 4B.

## Not measured

- English summaries with 4B. The gate above is Japanese only; ADR-0043 does not apply
  to English output.
- Behaviour on an actual 8 GB Mac. Peak memory figures come from a larger machine.

## Verification

- Catalog tests pin the new mapping (small tier → 4B, medium/large → 9B), the 7B and
  4B download URLs and hashes, at most one automatic model per tier, and that a cached 7B
  is kept at every memory size. Removing the `tier_default` filter fails three tests.
