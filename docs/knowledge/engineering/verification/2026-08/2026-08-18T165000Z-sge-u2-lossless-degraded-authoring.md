---
type: "Verification Evidence"
title: "SGE-U2 lossless degraded authoring verification"
description: "Verifies exact-receipt-bound degraded Scene and Prefab authoring, bounded source closure, lossless persistence, and strict Runtime rejection."
timestamp: 2026-08-18T21:39:33Z
record_id: "cfbe9e2ce5d64a80aa946201f13fe8ce"
status: "verified"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-18-2158-feat-standalone-generic-editor-project-open-and-recovery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "52b834f"
verified_by: "codex-root"
---

# Verification

Verified commit `52b834f` against SGE-U2 of the Standalone Generic Editor plan. The scope covers
data-only authoring classification, exact unavailable-record preservation, bounded project-source
closure, typed persistence lanes, and the independent strict Runtime boundary.

# Result

SGE-U2 passes. Current-grammar Scene and Prefab documents can publish as `Complete` or `Degraded`
without loading project code. Available scalar fields remain editable; unavailable records remain
semantic, read-only source data and survive Save/Reopen with an identical manifest. Stable-ID
dependencies remain bounded unresolved facts instead of being guessed or scanned. Runtime rejects
degraded or owner-spoofed records before World mutation.

# Evidence

- `cargo fmt --all` and `git diff --check` passed.
- `cargo check -p nara --features tooling,project-workflow,serde` passed.
- `cargo check -p nara_tooling_egui` passed.
- `cargo check -p nara --tests --features runtime-2d,serde,tooling,presentation-2d,presentation-ui,desktop-winit,render-wgpu` passed on `52b834f`.
- `cargo nextest run -p nara_scene --features serde --test authoring_decode --test-threads=1`
  passed 6 tests covering Available/KnownUnbound editing, UnknownSchema round-trip,
  old/future classification, unknown value-tag rejection, exact reference retention, and strict
  pre-World Runtime rejection.
- `cargo nextest run -p nara_tooling degraded_authoring_scalar_gate_preserves_manifest_and_is_failure_atomic --test-threads=1`
  passed.
- One focused root `nextest` selection passed 4 tests covering path-asset source retention,
  stable-ID degraded dependency facts, shared closure-budget rejection without a receipt, and exact
  degraded Scene Save/Reopen preservation.

# Follow-up

SGE-U3 must be the only unit that installs the `AuthoringProjectReceipt` into the shell-first Editor
session. The receipt and persistence binding seams are intentionally root-private and are not yet an
ordinary product entry. Directory enumeration, a project-wide asset index, child-process Runtime,
and Editor extensions remain out of scope.

# Citations

- `52b834f` (`feat(editor): admit lossless degraded authoring projects`)
- `docs/plans/2026-08-18-2158-feat-standalone-generic-editor-project-open-and-recovery-plan.md`
- `docs/architecture/adr/0090-unavailable-schema-and-lossless-authoring.md`
- `docs/architecture/adr/0114-standalone-generic-editor-authoring-and-execution-boundary.md`
