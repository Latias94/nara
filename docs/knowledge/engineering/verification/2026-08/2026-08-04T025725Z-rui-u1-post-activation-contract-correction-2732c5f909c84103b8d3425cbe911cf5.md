---
type: "Verification Evidence"
title: "RUI-U1 post-activation contract correction"
description: "Corrects the active ledger pointer and makes the readable-runtime-UI units independently executable after independent read-only plan audits."
timestamp: 2026-08-04T02:57:25Z
record_id: "2732c5f909c84103b8d3425cbe911cf5"
tags: ["rui-u1", "runtime-ui", "input-routing", "text", "correction", "verification"]
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-08-04-001-feat-readable-runtime-ui-and-deterministic-text-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "2fb81c8"
supersedes: "c7fea22cd092467fb326065112af5619"
verified_by: "independent reference-game UX, plan feasibility, plan coherence, and scope audits plus current-source reconciliation"
---

# Correction

The RUI successor remains the correct product slice, but post-activation review found that its
first draft was not yet an independently executable contract. The ADR implementation ledger still
named the superseded SRT plan, RUI-U2 did not include migration of the reference game's duplicate
root orders, and later units assumed input, font, and WGPU material capabilities that their owning
modules could not provide.

The active plan now makes those boundaries explicit:

- RUI-U2 migrates the current reference-game HUD in the same unit that starts rejecting duplicate
  root order.
- The first font remains a private validated typed asset owned by `nara_ui_render`; its controlled
  storage satisfies ADR 0031 while public handles, import, and runtime override wait for a real
  custom-font consumer.
- Glyph submission uses a private `Solid | Image | GlyphAtlas` material closure and packet-owned
  atlas snapshot without claiming to rewrite the existing Image submission path.
- A dedicated input unit adds an event-time pointer timeline, cancellation causes, viewport
  invalidation, and UI-before-gameplay consumption before button activation is used by the product.
- The reference-game adapter removes the old Retry command binding and becomes the sole mapper for
  Enter plus the semantic Retry UI action.
- Runtime Save/Reopen and Editor runtime-UI persistence are not claimed. The packaged product
  closure is owned only by the final unit.

# Result

Passed after correction. RUI-U1 remains complete and RUI-U2 remains the sole active implementation
unit, now with its existing product consumer and regression gate in scope. No Rust source was
changed by this correction and no Cargo command was required.

# Evidence

- Direct authority inspection: `docs/architecture/README.md`, the active/superseded plan
  frontmatter, and the ADR 0025/0031/0041 next-unit entries in
  `docs/architecture/adr/implementation-status.md` now agree on RUI.
- Current product inspection: `reference-game/src/ui.rs` has two primary-window roots at the old
  default order and is therefore explicitly part of RUI-U2 migration.
- Input inspection: `ButtonTransition` lacks event-time pointer facts and `PointerState` retains
  only the final coordinate, so the owning-contract unit precedes button product use.
- Render inspection: the current wgpu quad material distinguishes only Image versus fallback, so
  the private glyph-atlas material and cache path are named explicitly.
- Scope inspection: runtime font override, runtime HUD persistence, broad accessibility export,
  and pre-package closure claims were removed or deferred.

# Follow-up

Continue RUI-U2 proof-first. Publish one failure-atomic UI stack, migrate all current first-party
root and sibling order consumers in the same change, and do not introduce font or input work until
that unit is green and reviewed.

# Citations

- `docs/plans/2026-08-04-001-feat-readable-runtime-ui-and-deterministic-text-plan.md`
- `docs/architecture/adr/implementation-status.md`
- `docs/architecture/adr/0041-input-routing-actions-text-focus-and-accessibility.md`
- `crates/nara_input/src/lib.rs`
- `crates/nara_render_wgpu/src/quad.rs`
- `reference-game/src/ui.rs`
