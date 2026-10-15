---
type: "Verification Evidence"
title: "Stable source previews and revision transactions pass while remaining native fixtures expose owned integration gaps"
description: "Verification Evidence for Stable source previews and revision transactions pass while remaining native fixtures expose owned integration gaps."
timestamp: 2026-10-15T18:29:23Z
record_id: "070c3e4eb19f44668b779d6085592f82"
status: "partial"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "7d7f778"
---

# Verification

Commands were serialized with one Cargo build job in the shared target directory.

- U11 scene authoring decoding: `c7cebba8-9770-4290-ad6c-a63dbf642c40`, 11/11 passed, 0.525s.
- Tooling library: `699cba51-ac03-4837-81fe-d67cd6b8a2b5`, 45/45 passed, 2.051s.
- Resource/render/sprite/WGPU libraries: `59f17907-fe83-4c1a-bfdb-c3f673265aa0`, 207 run,
  206 passed and one new native-probe fixture failed, 23.683s. This is not an all-green run.
- Root libraries and seven content/runtime/persistence/transaction integration targets:
  `5bd91ec4-a474-436d-b2dd-b7d8ae629b63`, 289 run, 287 passed, two failed and one skipped,
  82.049s after 2m59s compilation.
- U8 independent domain characterization: `f9e2ed55-9716-4d8b-b6ea-a00adc9d2c4d`, six run,
  four passed and the two intended replacement/removal counterexamples failed, 1.272s.

# Result

Actual generic Scene previews resolve a stable image after source rename, and accepted Import
supports subsequent Sprite selection through both Path and StableId. Atomic multi-field edits,
Undo without another frame, stale-base conflicts and fresh-base commands without redraw pass
through the real Host. Existing save/reopen, dirty-close, source-race and content budget gates pass.

Core render tests establish two ordered views, clear/load, complete affine drawing/picking and
same-size requested-mode changes. Existing native palette and UI readback tests pass. The new
multi-view native probe did not reach pixel assertions in this run.

# Evidence

The first render rerun exposed a manual test's assumption that raw ECS entity numbers and opaque
entity bits have the same order. Both production extraction and packet validation already use
the same key. The test now captures actual extraction across consecutive frames; the ordering
guard remains. The new probe omitted the existing hierarchy placement required by sprite
extraction and panicked before physically closing Runtime. Its fixture now places actual roots
and retains retirement before assertions; a focused rerun is pending.

One root failure expected only the edited document's retained charge after failed startup. The
source now also retains its admitted stable-identity context. The test is being corrected to
require unchanged ownership through incomplete cleanup and complete release after physical close.
The other runtime-edit fixture still selects `enemy-anchor/enemy`, an obsolete concatenated
prefab locator. Its real instance target is being migrated; generation/safe-point checks remain.

U8 confirms that the strict lifecycle-free sync/clear path rejects a hook-owning transient
component on the same configuration entity. The two expected failures report the accepted
revision unavailable at PersistentProjection, before domain-native retirement. The smallest
correction is explicit domain retirement before strict replacement, then native reconstruction;
no core hook guard is relaxed. Four characterizations already prove initial native ownership,
invalid-document preservation, the strict guard and unpublished failed startup.

Focused read-only U7 correctness review returned no actionable P1/P2. It traced closure-only
identity retention, metadata revalidation, override references and validate-before-pixels-before-
identity ordering. Negative import-conflict/capacity and override-only direct tests remain coverage
opportunities, not demonstrated defects or a substitute for final review.

# Follow-up

Rerun the repaired native probe and the two root fixtures, then finish U8's concrete consumer
retirement. U10 presentation token/fake adapter/interpolation and explicit native offscreen-to-
window composition remain outstanding. Validate Reference selectors/bundle, maintained independent
consumers, CI and final review. Preserve the original user index; integrated worktree results do
not establish an independently buildable committed HEAD or completion of U1-U13.

# Citations

- docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md
- crates/nara_render_wgpu/src/view_probe.rs
- tests/workspace_play_runtime.rs
- src/project_host/runtime/tests.rs
- module-consumer/tests/domain_reconciliation.rs
- module-consumer/tests/support/domain_reconciliation.rs
