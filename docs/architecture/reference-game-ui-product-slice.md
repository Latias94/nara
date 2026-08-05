# Reference-Game UI Product Slice

**Status**: Decision checkpoint for NUI-U9 through NUI-U13
**Date**: 2026-08-05
**Authority**: ADR 0102 and the NUI implementation plan
**Scope**: The first complete data-driven runtime UI proof for the reference game

This document is a product input for the renderer-neutral UI work. It is intentionally narrower
than an editor design system: it describes one shippable game surface, its states, and its ownership
boundaries. It does not introduce a general widget registry, docking model, or Editor object tree.

## Product Surface

The reference game uses one 16:9 game surface with a fixed logical design space of 960 x 540. The
adapter maps the logical space to the actual surface and preserves the authored safe-area insets.
The acceptance captures use these three physical targets:

| Target | Physical surface | Logical scale | Proof purpose |
|---|---:|---:|---|
| Compact desktop | 1280 x 720 | 1.333... | Minimum readable HUD and pointer hit targets |
| Standard desktop | 1920 x 1080 | 2.0 | Primary visual and interaction baseline |
| High-density desktop | 2560 x 1440 | 2.666... | Text raster, snapping, and packet residency proof |

The logical layout must remain unchanged across the three targets. Only the adapter's physical
mapping, pixel snapping, and bounded text raster revision may differ.

## Authoring Structure

The runtime UI is one ordered Scene/Prefab projection with these roots:

```text
reference-hud (layer: gameplay)
  health-group
    health-track
    health-fill
    health-label
  wave-status
  score-status

terminal-overlay (layer: modal)
  terminal-panel
    outcome-label
    run-summary
    retry-button
    dismiss-button
  input-hint
```

`reference-hud` remains visible while the wave is running. `terminal-overlay` is inert and absent
from hit/focus/semantics while hidden. When shown, it owns the modal scope and the gameplay HUD is
visually retained but input-inert. The structure, ordering, style roles, labels, and button names
are Scene/Prefab data; the Rust adapter supplies only typed facts and consumes typed intents.

## Visual Direction

The first visual pass is restrained and high-contrast so the game state is scannable during a wave:

- Health uses a dark neutral track, green fill, and a compact text value at the track edge.
- Wave and score use short text rows with a shared theme role, not hand-positioned draw calls.
- The terminal panel uses a solid dark surface with a thin warm border. It is a framed modal tool,
  not a page-level card and not a translucent full-screen decoration.
- Retry is the primary action. Dismiss is secondary and remains available only when the product
  facts permit leaving the terminal state.
- Focus is visible without relying on color alone: a two-pixel outline and a changed text role.

The initial palette is deliberately mixed: charcoal surfaces, cool cyan structural accents, green
health, amber progress, and warm red terminal failure. Exact color values belong to the immutable
`UiTheme` asset and may change without Rust recompilation.

## State Matrix

| Product state | HUD | Overlay | Primary intent | Input/semantics |
|---|---|---|---|---|
| Running | Visible and interactive only for admitted gameplay controls | Hidden, absent from hit/focus | None | Gameplay actions route normally; UI roots remain in the frame |
| Paused | Visible but inert | Optional pause treatment is out of this slice | None | Observation and UI cancellation remain pause-safe |
| Completed | Visible but inert | Visible with completion facts and Retry | `Activate(retry)` | Modal scope traps focus; Dismiss is enabled if admitted |
| Defeated | Visible but inert | Visible with defeat facts and Retry | `Activate(retry)` | Same modal contract, different theme/status role |
| Retry pending | Last submitted terminal frame remains authoritative | Retry disabled with pending status | None | Duplicate activation is rejected and cannot enqueue a second command |
| Retry rejected | Last-good terminal frame remains visible | Error text is data-driven; Retry re-enabled | `Activate(retry)` | Typed rejection is announced in semantics; no partial replacement is shown |

Every state is captured in four interaction variants: default, hover, pressed, and keyboard focus.
Disabled and hidden variants are also captured for Retry pending, stale roots, and modal teardown.
The variants change theme roles and semantic state, not the structural order or logical keys.

## Ownership Boundary

| Concern | Owner | Rust responsibility |
|---|---|---|
| Root/node order, layout, theme roles, labels, accessibility names | Scene/Prefab data and UI kernel | Validate and project typed authoring data |
| Health, wave, score, terminal outcome, retry status | Reference-game domain | Publish immutable facts for the current frame |
| Hover, focus, capture, modal scope, last-good frame | UI kernel/session | Reduce input against the exact retained frame/receipt pair |
| Retry and dismiss behavior | Reference-game command adapter | Map typed `UiIntent` to an existing domain command |
| Image/glyph residency and backend submission | UI/render presentation owners | Build an owned packet and return a submission receipt |

Changing HUD geometry, text, theme roles, modal structure, or labels must be possible by editing
Scene/Prefab data and assets. Rust changes are required only when a new typed fact, intent, or domain
rule is introduced.

## Acceptance Captures

For each physical target, the product proof captures `running`, `completed`, `defeated`, `retry
pending`, and `retry rejected`. Each capture records the logical frame revision, environment
revision, structural key sequence, semantic tree, packet result, and pixel/snapshot hash. A receipt
is recorded as backend acceptance and submission of the exact packet; it is never described as proof
that a human saw the frame.

The first implementation may use a bounded test presenter for headless captures. Desktop captures
must use the ordinary presentation recipe and the same authoring data. Any difference between the
three targets is limited to physical mapping, snapping, and admitted raster generations.

## Exit Criteria

NUI-U9/U10 may start after this document, ADR 0102's checkpoint amendment, and the identity rule
are referenced by their evidence records. NUI-U13 cannot close until the current Rust-built HUD is
replaced by this data boundary and all five product states pass the matrix above.
