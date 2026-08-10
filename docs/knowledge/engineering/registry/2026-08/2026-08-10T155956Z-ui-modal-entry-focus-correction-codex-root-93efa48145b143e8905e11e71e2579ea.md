---
type: "Work Registration"
title: "UI modal-entry focus correction: UIFM-U2 active"
description: "Closes the owning modal-focus correction and activates successor baseline publication plus explicit RUP-U1 rebase."
timestamp: 2026-08-10T15:59:56Z
record_id: "93efa48145b143e8905e11e71e2579ea"
status: "active"
producer_id: "codex-root"
run_id: "ui-modal-entry-focus-correction"
related_plan: "docs/plans/2026-08-10-001-refactor-ui-modal-entry-focus-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "40d3f87"
supersedes: "e29de7d5dfaf44c4934407ff1e4a761a"
registration_id: "engine-foundation-contract-completion-codex-root"
latest_link: "docs/knowledge/engineering/verification/2026-08/2026-08-10T155955Z-uifm-u1-modal-entry-focus-correction-04e51278170c4a839dbeb0c9658741d8.md"
---

# Scope

UIFM-U2 only: publish a successor Foundation Conformance Baseline for the modal-entry-focus
correction, pin it from the Runtime UI Product Slice, and reactivate RUP-U1 without rewriting prior
UIF or RUP evidence.

# Current Claim

UIFM-U1 is complete at `40d3f87`. Focused kernel, adapter and Reference Game tests prove that the
Surface Slot owner seat receives initial modal focus before final paint and semantic publication,
while initial non-modal presentation remains unfocused.

# Latest Links

- `docs/knowledge/engineering/verification/2026-08/2026-08-10T155955Z-uifm-u1-modal-entry-focus-correction-04e51278170c4a839dbeb0c9658741d8.md`
- `docs/plans/2026-08-10-001-refactor-ui-modal-entry-focus-correction-plan.md`
- `docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md`

# Handoff

Publish an immutable successor baseline that supersedes UIF-U5 only for this corrected behavior,
update the RUP plan to pin it, retire this focused correction plan, validate deterministic
engineering-memory rendering, and register RUP-U1 as the sole active product unit.

# Citations

- `docs/architecture/adr/0108-ui-surface-presented-session-and-atomic-interaction.md`
- `docs/architecture/adr/implementation-status.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-10T145838Z-uif-u5-foundation-conformance-baseline-70b35505e8da4cb388c36d3ede602259.md`
