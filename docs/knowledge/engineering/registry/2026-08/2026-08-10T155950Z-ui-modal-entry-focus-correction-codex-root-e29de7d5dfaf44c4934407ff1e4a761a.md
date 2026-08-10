---
type: "Work Registration"
title: "UI modal-entry focus correction: UIFM-U1 active"
description: "Pauses RUP-U1 after a renderer-neutral failure and activates the focused Surface-owner modal focus correction."
timestamp: 2026-08-10T15:59:50Z
record_id: "e29de7d5dfaf44c4934407ff1e4a761a"
status: "active"
producer_id: "codex-root"
run_id: "ui-modal-entry-focus-correction"
related_plan: "docs/plans/2026-08-10-001-refactor-ui-modal-entry-focus-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "3fe3c19"
supersedes: "4e811210cdb04c36ab586a3cb65b78a1"
registration_id: "engine-foundation-contract-completion-codex-root"
latest_link: "docs/knowledge/engineering/verification/2026-08/2026-08-10T155945Z-rup-u1-neutral-modal-focus-reproduction-cc8d66c49d134e9f9d18d680afe19f7a.md"
---

# Scope

UIFM-U1 only: seed the Surface Slot endpoint seat into staged candidate interaction, apply existing
modal first-focus reconciliation before final visual compilation, and prove exact paint/reducer/
semantic agreement without introducing product-specific policy.

# Current Claim

RUP-U1 landed the same-runtime Retry command trajectory at `f225643` and removed the unreceipted
desktop Retry fallback at `3fe3c19`, but product work is paused. Renderer-neutral nextest run
`650198a8-3023-4b3f-86a7-4c6691b3b719` proves the pinned UIF-U5 baseline does not establish initial
modal focus for the Surface Slot owner seat.

# Latest Links

- `docs/knowledge/engineering/verification/2026-08/2026-08-10T155945Z-rup-u1-neutral-modal-focus-reproduction-cc8d66c49d134e9f9d18d680afe19f7a.md`
- `docs/plans/2026-08-10-001-refactor-ui-modal-entry-focus-correction-plan.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-10T145838Z-uif-u5-foundation-conformance-baseline-70b35505e8da4cb388c36d3ede602259.md`

# Handoff

Implement the owning staged-session correction, keep semantic projection read-only, remove the
obsolete seat-less candidate-reconciliation signature, and verify initial modal, base-to-modal,
non-modal and endpoint replacement cases. Then publish a successor UIF baseline and explicitly
reactivate RUP-U1 without rewriting historical evidence.

# Citations

- `docs/plans/2026-08-10-001-refactor-ui-modal-entry-focus-correction-plan.md`
- `docs/architecture/adr/0108-ui-surface-presented-session-and-atomic-interaction.md`
- `crates/nara_ui_kernel/src/interaction/session.rs`
- `crates/nara_ui/src/runtime/state.rs`
