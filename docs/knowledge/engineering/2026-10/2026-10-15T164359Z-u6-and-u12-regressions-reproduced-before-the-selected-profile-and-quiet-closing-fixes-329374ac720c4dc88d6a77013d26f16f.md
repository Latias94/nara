---
type: "Verification Record"
title: "U6 and U12 regressions reproduced before the selected-profile and quiet-Closing fixes"
description: "Verification Record for U6 and U12 regressions reproduced before the selected-profile and quiet-Closing fixes."
timestamp: 2026-10-15T16:43:59Z
record_id: "329374ac720c4dc88d6a77013d26f16f"
status: "verified"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "25c29de"
verified_by: "codex-root"
---

# Summary

Two source-confirmed defects were reproduced before their production fixes. This is red evidence,
not a green gate; the subsequent focused suite is still running.

# Details

Nextest `62f82fb3-969c-40b8-bd54-a138a3c72aaf`, session `40860`, ran the two selected tests with
`-p nara -p nara-editor --all-features --lib --locked --offline`, one test thread and one build
job. Both compiled and failed at their intended assertions in 0.357 seconds:

- `missing_startup_scene_diagnostic_preserves_the_selected_profile`: the Check request selected
  `tools`, but its repair hint omitted `--profile tools`.
- `quiet_supervised_wake_preserves_the_published_authoring_frame`: ordinary authoring preserved
  its frame, but replacement Closing silently replaced the displayed frame on a no-change wake.
  The test completed owned replacement/exit cleanup before asserting, so red evidence did not
  depend on an abandoned managed execution owner.

The fixes read the profile already retained in the startup issue through a crate-private accessor
and give Closing an effects-only branch that publishes only real observation changes.

Three read-only ce-simplify-code passes completed for U6/U12: reuse, quality, and efficiency.
Applied suggestions consolidate the existing target/profile mapping, unify the two identical
retained-process termination branches, and remove an unused lifecycle-poll argument. The
equivalent earliest-deadline expression change was skipped as low value. Efficiency found no
additional justified change; exact-generation checks and stable worker/process deadlines stay.

Generated starter dependency evidence: `cargo generate-lockfile --offline` followed by
`cargo tree --locked --offline -e normal --prefix none` succeeds for default headless features,
including `project-tooling`, with no WGPU, Winit, or egui normal dependency. Lock-file resolution
can still list optional desktop packages; the claimed dependency boundary is the active tree.

# Next Action

Collect session `63751` (project-tooling tests plus complete Editor library suite), then verify
Editor integrations and the native canary. The simplification verification/typecheck/lint gates
remain pending. No full ce-code-review receipt or complete U6/U12 claim is made here.

# Citations

- docs/knowledge/engineering/2026-10/2026-10-15T163819Z-compiled-project-tooling-and-host-integration-pass-all-198-focused-tests-6a8e83bd2e48414b9573ab094aa2240e.md
- src/project_tooling/export/trajectory.rs
- editor/src/app.rs
- starters/desktop-2d/v1/src/bin/headless.rs
