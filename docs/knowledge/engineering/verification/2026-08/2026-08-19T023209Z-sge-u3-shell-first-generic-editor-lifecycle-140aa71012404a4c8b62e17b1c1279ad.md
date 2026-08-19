---
type: "Verification Evidence"
title: "SGE-U3 shell-first generic Editor lifecycle"
description: "Verifies native shell-first project admission, retained replacement handoff, strict ordinary filesystem trust, recovery path hygiene, exact degraded Reopen preservation, explicit Runtime unavailability, and the advanced trusted canary boundary."
timestamp: 2026-08-19T02:32:09Z
record_id: "140aa71012404a4c8b62e17b1c1279ad"
status: "verified"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-18-2158-feat-standalone-generic-editor-project-open-and-recovery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "3c59e98"
verified_by: "codex-root"
---

# Verification

Verified commit `3c59e98` against SGE-U3 of the Standalone Generic Editor plan. The proving scope
covers native-shell-first startup, ordinary Authoring Project admission, Project Manager and
Recovery interaction, retained replacement transactions, degraded Scene/Prefab persistence, and
the separately named trusted compiled Editor lane.

# Result

SGE-U3 passes. `nara-editor` now exists before project admission, ordinary Open consumes the
data-only Authoring Project receipt without constructing a Runtime plan, and rejected project facts
remain visible in the live Recovery surface. Replacement Open retains the exact last-good workspace
through preparation, refresh, dirty-close resolution, failure, and cancellation. Ordinary Play
reports the explicit external execution Host boundary, while the existing in-process Game View is
reachable only through the trusted compiled API.

# Evidence

- `cargo check -p nara-editor` passed.
- `cargo nextest run -p nara-editor --test application_contract --test-threads=1` passed 10 tests
  covering shell-first admission, Recovery, repair/Retry, retained replacement, failure-atomic dirty
  close, reachable Open/Close actions, explicit Runtime unavailability, and coherent workspace
  publication.
- `cargo nextest run -p nara_tooling_egui project_manager_and_recovery_focus_follow_entry_generation --test-threads=1`
  passed, proving deterministic Project Manager/Recovery focus across entry generations.
- `cargo test -p nara --lib --features 'desktop-winit,presentation-2d,presentation-ui,project-workflow,render-wgpu,runtime-ui,tooling-egui' degraded_scene_save_and_reopen_require_the_exact_unavailable_manifest -- --nocapture`
  passed, including hostile out-of-band unavailable-record drift rejection without advancing the
  observed file identity or digest.
- `cargo check --manifest-path reference-game/Cargo.toml --locked --features editor-game-view-canary --bin editor_game_view_canary`
  passed, proving the renamed trusted compiled canary remains reachable without becoming the
  ordinary Editor entry.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Structured code review accepted and resolved the replacement Exit orphan, Authoring Reopen
  preservation, strict ordinary-root trust, retained replacement workspace, bounded Recovery
  diagnostics, and surface-owned input ordering findings. The proposed line-count split and an
  SGE-U4/U5 Reference Game trajectory were rejected as out of scope.
- The final simplify pass applied five behavior-preserving state/projection reductions. Two small
  redraw-allocation optimizations were skipped because they would expand the Adapter Interface for
  negligible SGE-U3 benefit; no safety validation or ordering check was removed.

# Follow-up

SGE-U4 is the only next active unit. It must publish canonical first-party authoring bundles for the
generated starter, Reference Game, and one clean-room custom owner, then bind those data-only facts
back to strict provider-capable Runtime verification. SGE-U5 and all later Editor capabilities remain
inactive.

# Citations

- `3c59e98` (`feat(editor): establish shell-first generic project lifecycle`)
- `docs/plans/2026-08-18-2158-feat-standalone-generic-editor-project-open-and-recovery-plan.md`
- `docs/architecture/adr/0090-unavailable-schema-and-lossless-authoring.md`
- `docs/architecture/adr/0114-standalone-generic-editor-authoring-and-execution-boundary.md`
