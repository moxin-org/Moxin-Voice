# TTS Segment Popover Visual Polish Design

## Goal

Make the TTS segment popover feel like a deliberate part of the player: every
action is immediately recognizable, inactive segment rows remain clearly
bounded, and the popover separates visually from the page below it.

## Scope

- Replace the bespoke SDF icons added for the segment entry and four segment
  actions with a cohesive, local SVG icon set.
- Add a visible one-pixel border to every segment card, with a stronger blue
  border for the currently playing card.
- Strengthen the popover elevation with a soft, layered drop shadow while
  retaining its existing rounded shape and theme-aware border.
- Keep existing action targets, playback behavior, retry behavior, and the
  120-character segmentation policy unchanged.

## Icon assets

Add five monochrome 20px SVG files under
`apps/moxin-voice/resources/icons/`:

- `segments.svg`: stacked audio segments; used by the player entry button.
- `segment-play.svg`: play triangle.
- `segment-download.svg`: down arrow into a tray.
- `segment-expand.svg`: downward chevron; the same glyph remains acceptable
  for collapse because the row's expanded text makes its state obvious.
- `segment-retry.svg`: circular retry arrow.

Makepad's existing `<Icon>` widget loads SVG through `dep(...)` and applies
theme colors through `draw_icon`, so the icons remain crisp at desktop scale
and adapt to light and dark modes without separate PNG variants. Buttons keep
their current 28px hit areas and receive hover/pressed backgrounds from the
button style; only the glyph rendering moves out of the SDF shader.

## Card and popover treatment

Each segment card has a 1px neutral blue-gray stroke in both themes. Its
playing state uses a slightly stronger primary-blue stroke together with the
existing translucent blue fill. The new border is visible even when the row is
not selected, addressing the lack of visual grouping in the current list.

The popover gains a two-layer shadow: a wide, low-opacity shadow for elevation
and a tighter, darker shadow near its lower edge. Its theme-aware surface and
one-pixel border remain intact, so the effect reads as a floating panel rather
than a dark outline.

## Verification

- Add focused source-contract tests that require SVG-backed segment icon
  widgets, the inactive card stroke, and the multi-layer popover shadow.
- Run `cargo test -p moxin-voice` for the focused UI tests and `cargo check -p
  moxin-voice`.
- Do not alter the known, unrelated Chinese voice-ID baseline failure in the
  complete `moxin-voice` library suite.
