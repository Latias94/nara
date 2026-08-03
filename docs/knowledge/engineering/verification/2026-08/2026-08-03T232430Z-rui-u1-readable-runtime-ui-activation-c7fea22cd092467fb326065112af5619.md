---
type: "Verification Evidence"
title: "RUI-U1 readable runtime UI activation"
description: "Verifies SRT closure, the concrete readable-UI product gap, the bounded first runtime text owner, and truthful successor authority."
timestamp: 2026-08-03T23:24:30Z
record_id: "c7fea22cd092467fb326065112af5619"
tags: ["rui-u1", "runtime-ui", "text", "reference-game", "verification"]
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-08-04-001-feat-readable-runtime-ui-and-deterministic-text-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "8bcec9f"
verified_by: "current-source reconciliation plus independent reference-game UX and text/UI architecture audits"
---

# Verification

SRT-U6 is complete at `4f5fb6f` and its documentation closure is current at `8bcec9f`. The latest
external SRT-U5 review was reconciled against current source: unified fallible preparation, receipt
membership, runtime projectile ownership, authored Sprite preservation, generation-4 tombstones,
and root-private retention are already implemented and independently reviewed with no remaining
P0/P1. The one still-valid observation was a stale ADR 0011 ledger anchor pointing at removed
generation-3 reference-game types; RUI-U1 corrects it.

The remaining visible product failure is concrete: the desktop game has two unlabeled bars and a
terminal color rectangle, does not explain controls or auto-fire, omits score from the HUD, and has
no clickable Retry. Runtime UI also uses `Entity::to_bits()` as a painter/hit tie-break and has no
font, shaping, glyph cache, or text render path.

# Result

Passed for RUI activation at baseline `8bcec9f`.

- The SRT plan is reciprocally superseded after verified SRT-U1 through SRT-U6; immutable evidence
  remains authoritative and ADR 0089 stays Proposed.
- The successor is the sole implementation-ready active plan. It first establishes explicit UI
  source order, then one typed bundled font and private runtime-UI text path, existing-pass wgpu
  submission, the real HUD/Retry button, and a packaged product journey.
- Accepted ADR 0025, ADR 0031, ADR 0041, and ADR 0095 already define the required boundaries. The
  reversible private shaping implementation uses Parley/Swash without admitting `nara_text`, a
  backend Interface, system fonts, or a provider registry.
- The first font is bundled and typed. Project font import remains separate because Project Content
  currently classifies declared asset references through an image-only closure; widening that seam
  is not required to make this product readable.
- One UI stack will own painter and hit order. Generic hierarchy `Children` ordering is not promoted
  into a persistent product contract, and runtime entity allocation is removed as a tie-break.
- Button activation submits the existing reference-game Retry command. It does not add another
  reset path, input bus, widget callback, or gameplay-state framework.

# Evidence

- Current source, SRT verification, ADR/ledger, UI/input/render ownership, and reference-game
  presentation inspection: passed.
- Independent read-only reference-game UX audit: selected readable HUD, terminal overlay, button,
  and exact Retry reuse while rejecting title/menu and broad widget scope.
- Independent read-only text/UI architecture audit: selected existing UI crates, explicit sibling
  order, one ordered primitive stream, bundled typed font, private Parley/Swash/atlas state, and no
  project font importer in this slice.
- Engineering-memory validation/render and direct link/whitespace checks: recorded after this shard
  is published.
- Cargo and `tests/architecture_docs.rs`: intentionally not run for this documentation-only unit.

# Follow-up

RUI-U2 is the sole active implementation unit. It adds explicit persistent UI sibling order,
publishes a complete validated stack generation, removes entity-ID painter/hit fallbacks, and makes
panel extraction consume that exact stack before any font dependency is introduced.

# Citations

- `docs/plans/2026-08-04-001-feat-readable-runtime-ui-and-deterministic-text-plan.md`
- `docs/plans/2026-08-02-001-refactor-startup-scene-activation-and-atomic-retry-plan.md`
- `docs/architecture/adr/0025-runtime-ui-system.md`
- `docs/architecture/adr/0031-text-and-font-strategy.md`
- `docs/architecture/adr/0041-input-routing-actions-text-focus-and-accessibility.md`
- `docs/architecture/open-questions.md#oq-015-text-shaping-and-localization-stack`
- `crates/nara_ui/src/layout.rs`
- `crates/nara_ui/src/interaction.rs`
- `crates/nara_ui_render/src/queue.rs`
- `reference-game/src/ui.rs`
