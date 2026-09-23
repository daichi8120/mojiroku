# ADR-0045: Follow the macOS appearance with one token set

- Date: 2026-09-23
- Status: Accepted
- Related: Issue #104, parent Issue #116

## Context

The UI was dark only (`color-scheme: dark`). A screen-by-screen audit found that the
design tokens in `frontend/src/index.css` were not holding the design together:

- 15 font sizes in half-pixel steps; 94 uses at 11 px or below.
- `dim` (2.9:1) and `faint` (3.6:1) failed WCAG AA on the app surfaces while carrying
  dates, durations and the edit/delete icons.
- Ten ad-hoc radii beside three tokens; about 80 hard-coded colours in components.
  The speaker palette was a fixed array of 6 hex colours, so speaker 7 looked like
  speaker 1.
- Red buttons with white text used `#ef4444` (3.8:1).

A light appearance cannot be added while colours live in components, so both problems
have one fix.

## Decision

1. **Every colour is a token.** `@theme` in `index.css` holds the dark values; an
   `@media (prefers-color-scheme: light)` block redefines the same `--color-*`
   variables. Components never branch on the appearance.
2. **The app follows macOS.** There is no in-app theme switch.
3. **Contrast is a token property.** Every text token except `disabled` meets 4.5:1 on
   every surface in both appearances. Accent colours used as text (green, amber, cyan,
   red, brand-light) get darker values in light mode rather than a second class.
   `danger` (`#dc2626`) is the red that carries white text; the brand gradient starts at
   `#5558ee` so white text on it is 5.2:1.
4. **Seven font sizes**: 11 / 12 / 13 / 14 / 15 / 18 / 22 px. Reading text (transcript,
   minutes) is 15 px. The recording timer (58 px) is the only exception.
5. **Named radii**: `tag` 6, `ctl` 8, `btn` 10, `card` 12, `win` 14.
6. **Eight speaker colours** as CSS variables `--spk-N-{text,bg,dot}`, each text
   colour 4.5:1 or more on its own tint.
7. A global `:focus-visible` ring, and `prefers-reduced-motion` stops animations.

`src/lib/designTokens.test.ts` fails when a component uses a size outside the scale,
an ad-hoc radius, or a hard-coded colour. Exceptions are listed in that test with a
reason (the logo, and mock-only preview screens).

## Rejected

- **An in-app light/dark switch.** One more setting for no clear gain; macOS already
  has it.
- **Light mode by inverting the dark palette.** The pale accents become unreadable on
  white; they need their own values.

## Consequences

- The window's `backgroundColor` in `tauri.conf.json` stays dark (`#0e1014`); in light
  mode there is a brief dark frame before the web view paints. Tauri has no
  per-appearance window colour.
- New UI must use tokens; the test enforces it.
- The mock-only screens (`DigestView`, `AskDrawer`) still hard-code colours and are
  not checked in light mode; they do not ship.
