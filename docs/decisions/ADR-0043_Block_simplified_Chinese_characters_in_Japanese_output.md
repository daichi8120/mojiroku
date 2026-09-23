# ADR-0043: Block simplified Chinese characters in Japanese output

- Date: 2026-09-23
- Status: Accepted
- Related: ADR-0030 (summary model tiers), Issue #30

## Cause

Qwen3.5-4B was not adopted for the small tier on 2026-08-30 partly because a title
contained simplified Chinese (报汇). A wider rerun on 2026-09-23, 28 generations over
titles, minutes, summaries and action items from local meetings, reproduced it in
3 of 28 outputs, now also inside minutes bodies (户, 进). Qwen models can write a
simplified form in the middle of Japanese text. The prompt already asks for natural
Japanese, and prompting is not a reliable guard.

## Decision

When the sidecar writes Japanese (`--lang ja`, the default), the sampler removes every
vocabulary token whose text contains a CJK ideograph outside the Japanese character
set, before any other sampling step (`llama_sampler_init_logit_bias` with negative
infinity).

"Outside the Japanese character set" means not encodable in Windows-31J (JIS X 0208
plus the common vendor extensions), plus 个 and 价, which that encoding happens to
include. Kanji used in Japanese writing encode; simplified-only forms such as 进, 报,
户, 这 and 们 do not. Tokens that are only part of a UTF-8 character are left alone,
because Japanese characters share them.

The list is built once per sidecar run from the vocabulary: about 25,000 tokens for
Qwen3.5 and 12,600 for Qwen2.5, in about 0.1 s.

English output (`--lang en`) and the live translation path are unchanged.

## Trade-offs

- A few rare Japanese kanji outside Windows-31J (剝, 頰, 塡) can no longer be written.
  Each has a common form (剥, 頬, 填). A name that uses such a character in the
  transcript cannot be copied verbatim.
- A simplified character could still be produced from byte-level tokens. It did not
  happen in the gate below.
- Removing tokens changes the probability of the remaining candidates, so an output
  can take a different but valid path wherever a blocked token was a candidate.

## Verification

Same 28 inputs per model, fixed sampler seed, production token limit (2048).

| Model | Identical outputs | Simplified Chinese | Outputs with any automatic flag | Total time |
|---|---:|---:|---:|---:|
| Qwen3.5-4B | 23/28 | 3 → 0 | 3 → 1 | 941 → 949 s |
| Qwen3.5-9B | 26/28 | 0 → 0 | 2 → 2 (same two) | 1,622 → 1,641 s |
| Qwen2.5-7B | 24/28 | 0 → 0 | 2 → 1 | 1,687 → 1,582 s |

In the one new 4B flag, the model had started to write 适 (simplified 適); after the
block it wrote a different word and later ended the minutes without the ToDo heading.
Dropping a heading also happened with 7B (1/8) and with 4B in the earlier gate, with or
without this change. The other diverging outputs are rewordings. One 7B title gained
an emoji. The evaluation harness and its outputs contain meeting content and stay
outside the repository.

Live translation to Japanese uses the same model family and could adopt the same
block; it is not part of this change because its latency and quality gate are separate
(ADR-0037).
