---
type: "Verification Evidence"
title: "EPB-U3 indexed Authoring document admission"
description: "Verifies exact indexed Scene and Prefab admission, independent persistence lanes, and dirty replacement arbitration."
timestamp: 2026-08-19T19:20:16Z
record_id: "684770f357d14a0184e3cdbec4751355"
status: "verified"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-19-001-feat-editor-project-browser-and-source-index-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "fe0fa55"
verified_by: "codex-root"
---

# Verification

Verified implementation commit `fe0fa55` against EPB-U3 of the Editor Project Browser and Source
Index v1 plan. The proving scope is closure-external Scene and Prefab admission from one accepted
source-index snapshot, exact source revalidation, and the two path-bound Editor persistence lanes.

# Result

EPB-U3 passes. The generic Authoring session retains one immutable admission basis containing the
accepted project-root, manifest, schema-bundle, Adapter, and file-limit authority. An indexed Open
must resolve an exact member of the accepted snapshot, match its document kind and scoped identity,
reopen it through the retained project-root capability, and complete a fresh bounded Authoring
decode/publication before a workspace document or persistence lane can be installed.

Scene and Prefab now use independent path-bound persistence lanes. Retargeting an occupied lane
reuses the existing Save/Discard/Cancel arbitration: Cancel, failed persistence, or uncertain
persistence retains the exact old document and source owner; Save or Discard may publish the
prepared successor. Closing a document releases that lane and removes the obsolete hidden-source
Reopen behavior. A later Open must return through the source index and repeat admission.

Malformed, oversized, missing, wrong-kind, unindexed, identity-replaced, or stale-snapshot requests
publish nothing and leave active document, selection, dirty state, persistence operation, and the
other document lane unchanged. No Runtime plan, frozen native registry, ECS World, ambient path,
asset importer, or project Rust execution enters this path.

# Evidence

- `CARGO_BUILD_JOBS=1 cargo check -p nara --features runtime-2d,serde,tooling` passed.
- `CARGO_BUILD_JOBS=1 cargo check -p nara-editor --tests` passed.
- `CARGO_BUILD_JOBS=1 cargo nextest run -p nara --lib --features
  runtime-2d,serde,tooling -E 'test(generic_editor_opens_closure_external_scene_and_prefab_with_independent_lanes)
  | test(indexed_document_admission_rejections_leave_the_workspace_unchanged) |
  test(dirty_scene_retarget_uses_cancel_discard_and_save_before_publication) |
  test(dirty_prefab_retarget_preserves_the_scene_lane_and_honors_cancel_or_discard)'
  --test-threads=1` passed 4/4 tests.
- `CARGO_BUILD_JOBS=1 cargo nextest run -p nara-editor --test generic_open_recovery
  -E 'test(ordinary_generic_open_preserves_degraded_source_through_reopen_and_recovery)'
  --test-threads=1` passed 1/1 test.
- `CARGO_BUILD_JOBS=1 cargo nextest run -p nara --test editor_persistence
  -E 'test(save_advances_only_the_captured_revision_then_close_releases_the_lane)'
  --features tooling,runtime-2d,serde --test-threads=1` passed 1/1 test after the fixture was
  brought under the existing Runtime schema-agreement contract.
- `cargo fmt --all -- --check` and `git diff --check` passed. The focused source boundary contains
  no ambient filesystem access, Runtime publication, async task, watcher, importer, or project-code
  execution.

# Follow-up

EPB-U4 is the sole next active unit. It must project the immutable Host facts through one owned
UI-neutral browser model, keep selection/filter/expansion/query state in `nara_tooling`, send only
Refresh and Open through the exact `EditorFrameRevision`, render a bounded egui panel, and compose
scan/query pending work without moving browser state into the private platform shell.

# Citations

- `fe0fa55` (`feat(editor): admit indexed authoring documents (U3)`)
- `docs/plans/2026-08-19-001-feat-editor-project-browser-and-source-index-v1-plan.md`
- `docs/architecture/adr/0114-standalone-generic-editor-authoring-and-execution-boundary.md`
