---
type: "Work Registration"
title: "Runtime UI product slice: RUP-U1 reactivated"
description: "Pins the UIFM successor baseline and resumes the exact gameplay-to-Retry product trajectory."
timestamp: 2026-08-10T15:59:58Z
record_id: "c561d92562464eadb2904a61f0c14465"
status: "active"
producer_id: "codex-root"
run_id: "runtime-ui-product-slice"
related_plan: "docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "81f0394"
supersedes: "93efa48145b143e8905e11e71e2579ea"
registration_id: "engine-foundation-contract-completion-codex-root"
latest_link: "docs/knowledge/engineering/verification/2026-08/2026-08-10T155957Z-uifm-u2-successor-foundation-conformance-baseline-19cfa496dc2346368764057367cc2163.md"
---

# Scope

RUP-U1 only: continue one managed runtime trajectory from the Scene/Prefab-authored HUD through
terminal state, exact Retry activation, pending/rejected/applied behavior and successor
presentation, now pinned to successor Foundation baseline
`19cfa496dc2346368764057367cc2163`.

# Current Claim

UIFM-U1 and UIFM-U2 are complete. The owning shared defect is corrected at `40d3f87`, verified by
record `04e51278170c4a839dbeb0c9658741d8`, and published as successor baseline
`19cfa496dc2346368764057367cc2163`. RUP-U1 is again the sole active UI unit.

# Latest Links

- `docs/knowledge/engineering/verification/2026-08/2026-08-10T155957Z-uifm-u2-successor-foundation-conformance-baseline-19cfa496dc2346368764057367cc2163.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-10T155955Z-uifm-u1-modal-entry-focus-correction-04e51278170c4a839dbeb0c9658741d8.md`
- `docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md`

# Handoff

Continue the real ordered product trajectory. Keep the component fixture supplementary; close the
first-party Winit/AccessKit ingress and backend-presented desktop path without reopening shared UI
contracts unless another neutral owning-module reproduction fails against this pinned baseline.

# Citations

- `docs/architecture/reference-game-ui-product-slice.md`
- `docs/architecture/adr/0108-ui-surface-presented-session-and-atomic-interaction.md`
- `docs/plans/2026-08-10-001-refactor-ui-modal-entry-focus-correction-plan.md`
