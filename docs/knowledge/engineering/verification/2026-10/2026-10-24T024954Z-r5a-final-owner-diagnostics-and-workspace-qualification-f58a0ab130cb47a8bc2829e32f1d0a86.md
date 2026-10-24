---
type: "Verification Evidence"
title: "R5a final owner diagnostics and workspace qualification"
description: "Qualified Schema-relative addressing, owner edits, v3 fixtures and safe diagnostic recovery; broader source identity and panel workflows remain open."
timestamp: 2026-10-24T02:49:54Z
record_id: "f58a0ab130cb47a8bc2829e32f1d0a86"
producer_id: "codex"
run_id: "session-01a1629b-r5a-final"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "bddb0509947e2e40c9cf161c299b5c96ba821f5f"
verified_by: "serial nextest; all-feature all-target workspace check; focused Clippy correctness; scoped static reviews"
---

# Verification

All Cargo commands ran serially with `CARGO_BUILD_JOBS=1`.

- `cargo nextest run --locked -p nara_reflect -p nara_scene --features serde --test-threads=1`:
  532 passed, 1 skipped (`b87f2ab2`).
- Independent `nara-scene-module-consumer --test property_authoring`: 4 passed (`de8f7987`).
- Independent `nara-editor-panel-module-consumer --test prepared_authoring`: 3 passed
  (`0d96509f`). These exercise the public Application/session API, not a registered panel's
  `show()` prepare/preview/commit workflow.
- `cargo nextest run --locked -p nara_tooling -p nara_ui --all-features --test-threads=1`:
  151 passed (`2b592f39`). The cohort includes the separate public source-identity characterization.
- `cargo nextest run --locked -p nara-editor --lib -E 'test(value_draft)' --test-threads=1`:
  4 passed, 204 filtered/skipped (`1662e76e`). Keyed Record lists remain explicitly unsupported by
  the existing scalar collection controls.
- `cargo check --workspace --all-features --all-targets --locked`: passed in 4m27s with existing
  warnings. This is build coverage, not execution of every workspace test.
- `cargo clippy --locked -p nara_reflect -p nara_scene --all-targets --all-features --
  -D clippy::correctness`: passed with warnings. This is not a workspace warning-clean claim.

# Result

The Schema-relative R5a slice is implemented and qualified: sibling-scoped member IDs, rooted
declaration-path retirement, explicit keyed-list identity, exact current-Schema/version resolution,
and Scene/Prefab-owned ordered edits that reuse one prepared candidate/inverse/history path.

The expanded UI cohort initially failed on the old generation-two catalog fingerprint. The
catalog semantics and per-field evolution assertions were retained; only the frozen digest changed
to the canonical v3 domain. The complete 151-test rerun passed. A separate structural comparison of
all 15 rewritten JSON fixtures confirms that their component metadata/histories are unchanged
apart from v2-to-v3 envelopes and predecessor fingerprint strings; predecessor catalog objects
were not excluded from comparison.

# Evidence

Independent static reviews `01a16bab-9f62-7412-bd6c-41c714e91931` and
`01a16bab-eb23-71c1-97fa-75d3a453e2e2` found no reason to replace the scoped identity model. The
API review found loss of safe property rejection classification. Scene now retains static
`property-error-kind` and known `property-step-index` alongside `operation-index`, without
publishing keys or replacement values. The strengthened missing-member test failed on the absent
classification (`ad0cdd58`); the six-case focused cohort then passed (`236f3f69`). A test-construction
compile error was corrected before that red observation and is not counted as behavioral evidence.

The existing ECS `compile_fail` import of `nara_ecs::upstream` is intentional negative coverage,
not a stale positive example. The full substrate name remains `nara_ecs::bevy_ecs`; no alias was
added. The separate derive provenance README correction is outside the R5a implementation unit.

Code review: skipped (ce-code-review unavailable) - the dedicated full-review controller
`20261024-023026-7986eba1` terminated after scope discovery because its child session could not
discover native dispatch/collection tools. It performed no substantive source review and its
empty findings are not approval. The two completed static reviews above and the root's complete
owning-diff scan remain the actual independent/manual review coverage. All review-owned agents
were explicitly closed. No external coding/review provider was launched.

# Follow-up

R5a does not complete source/document/occurrence identity, declarative Prefab overlays, mixed
construction/property intent, complete field queries, registered-panel preparation, continuous
gesture semantics or preparation-cost comparison. The source-identity characterization confirms
a separate same-World UI collision; it is not a source-identity fix or an ADR 0102 refreeze. The
foundation goal remains Active. Native Editor/product qualification is not newly claimed by these
window-free owner tests and compilation checks.

The prepared owner no longer asks tools to repeat resolver logic or maintain Undo. Broader R2/R5b
work must continue through those same owners rather than add a compatibility wrapper or a second
transaction system. Preserve the existing untracked vendor directory; do not publish remotely.

# Citations

- [Prior R5a verification](2026-10-24T021414Z-r5a-property-addressing-current-qualification.md)
- [Property contract](../../../../architecture/durable-property-addressing-design.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Property diagnostics and limits](../../../../../crates/nara_scene/tests/property_edit_limits.rs)
- [Independent source consumer](../../../../../tests/fixtures/module-consumer/tests/property_authoring.rs)
