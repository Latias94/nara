---
type: "Work Registration"
title: "Runtime UI Product Slice: RUP-U0 active"
description: "Activates focused Reference Game product integration against the immutable UIF-U5 Foundation Conformance Baseline."
timestamp: 2026-08-10T14:58:38Z
record_id: "45fed67358834fc49f77d20c97b780a3"
status: "active"
producer_id: "codex-root"
run_id: "runtime-ui-product-slice"
related_plan: "docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "e307cb3"
supersedes: "e2588ef669d841afbb388f28ec65148a"
registration_id: "engine-foundation-contract-completion-codex-root"
latest_link: "docs/knowledge/engineering/verification/2026-08/2026-08-10T145838Z-uif-u5-foundation-conformance-baseline-70b35505e8da4cb388c36d3ede602259.md"
---

# Scope

RUP-U0 only: reconcile the current Reference Game UI product fixture, remove misleading
Cartesian/hash claims, and establish a compilable focused product evidence surface.

# Current Claim

UIF-U5 is complete at `e307cb3` with baseline record
`70b35505e8da4cb388c36d3ede602259`. The Runtime UI Product Slice is the sole active UI plan and
RUP-U0 is its sole active unit. The superseded NUI plan remains historical; its open NUI-U15B
product obligations are preserved by the active RUP requirements.

# Latest Links

- `docs/knowledge/engineering/verification/2026-08/2026-08-10T145838Z-uif-u5-foundation-conformance-baseline-70b35505e8da4cb388c36d3ede602259.md`
- `docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md`

# Handoff

Start with the untracked `reference-game/tests/ui_product.rs` and
`reference-game/tests/support/ui_product_fixture*` work. Preserve useful structured assertions,
separate component fixtures from Product-Validated backend claims, use ordinary cases at scale 1.0,
keep one fractional pointer case, and remove opaque hash or Cartesian coverage. Do not reopen UIF,
Editor, new widgets, dynamic assets or generalized authoring infrastructure without a neutral
owning-contract failure.

# Citations

- `docs/plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md`
- `docs/architecture/reference-game-ui-product-slice.md`
