---
type: "Work Progress"
title: "Foundation U5 unifies ownership and freezes coordinated driver operations"
description: "Work Progress for Foundation U5 unifies ownership and freezes coordinated driver operations."
timestamp: 2026-10-15T11:22:50Z
record_id: "ae018c52e4384e059610f77afb30115b"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "5535a0d"
---

# Summary

U5 removes raw App execution and moves the configured state through the existing RuntimeOwner.
App owns configuration; RuntimeInstance owns startup, driving, faults, and close. No nested executable
App or independent App Drop close path survives. Consumer migration is implemented, with broad
verification still running; this is not U5 or foundation completion.

# Details

Ordinary start/finish perform one bounded close attempt and return the same unfinished owner.
Unwinding retains physical obligations in quarantine. Host obligations now transfer before validator
and observer initialization callbacks. Both new unwind counterexamples failed before production fixes.
Their correction passes in the 138-test nara_app/nara_tasks run
ce9d32df-7d3e-4e33-89ee-32be92fa82f5.

Independent source review also exposed a newly added trait World override that could be selected
without configuring a resource or operation. Its red result and correction are captured in
subagent record c8b70859ecb1443bb070b7c1bb4cb01b. Cross-resource operations now require App-time
registration in a private typed slot; ordinary ports stay resource-local. Move-only input refusal
retains the input. Same-owner fault and close checks are preserved.

Editor authoring now holds RuntimeInstance and lets TaskPlugin register its task close obligation,
instead of preinserting manually owned pools and running a second shutdown path. Desktop examples
use ordinary start and the Winit runner; finite consumers use finish. Obsolete capability/service
declarations and raw-run error variants are removed. Staging bridges needed across workspace crates
remain honestly public but hidden from ordinary docs and absent from the normal prelude.

The initial root run 1a505546-3685-40ee-9dfe-bcadffe09d1a passed 164 of 171 selected tests. It found
two fixture omissions, an obsolete first-load recipe assumption, two spelling-based boundary guards,
and two stale independent lockfiles. Corrected image/resize/schema fixtures pass all four focused
tests in run 56470422-089e-40ed-8a4c-54ade9fdf2bc. Boundary tests are being replaced with actual
compile/privacy consumers instead of extending Rust source inference. The post-correction full
workspace all-features/all-targets cargo check passes; full runtime/module behavior remains pending.

U6's read-only map reuses SchemaContribution/ProductRecipe/RuntimePlan and places strict tools in
the compiled headless process (record 040afbb34ab24947b5f1678d25d3e129). U12 investigation preserves
the carried SPL implementation while identifying close-retry, quiet-observation, and synchronous
preparation gaps (record df85ee87b18f42d88e257c480b5f83fb). Neither dependent unit has started code.

# Next Action

Finish compiled privacy fixtures, root Host/runtime integrations, affected module tests, and real
example/independent consumer execution. Review and commit only owned changes. Preserve the original
SPL staged/worktree inventory. Then execute U6 and U12 under their dependency gates, followed by
U7-U11/U13. Cargo remains serial with CARGO_BUILD_JOBS=1 and the shared target.

# Citations

- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md U5-U13
- crates/nara_app/src/runtime.rs
- src/project_host/runtime/editor/authoring_runtime.rs
- tests/runtime_instance.rs
- tests/runtime_driver_boundary.rs
- docs/knowledge/engineering/subagents/2026-10/2026-10-15T111906Z-runtime-driver-transactions-must-be-configured-before-execution-c8b70859ecb1443bb070b7c1bb4cb01b.md
