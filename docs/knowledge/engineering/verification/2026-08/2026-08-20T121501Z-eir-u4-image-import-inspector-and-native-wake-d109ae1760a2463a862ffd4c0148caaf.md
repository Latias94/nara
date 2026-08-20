---
type: "Verification Evidence"
title: "EIR-U4 image import Inspector and native wake"
description: "Verifies the UI-neutral image import projection, private egui Inspector, exact frame-bound action, last-good diagnostics, and bounded native polling wake."
timestamp: 2026-08-20T12:15:01Z
record_id: "d109ae1760a2463a862ffd4c0148caaf"
status: "verified"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-20-001-feat-editor-image-import-and-reimport-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "6797956"
verified_by: "codex-root"
---

# Verification

Verified EIR-U4 across `1620786` and correction `6797956`. The unit adds one closed UI-neutral image
import view/action algebra, one private egui Inspector, exact public-frame plus private-frame Host
admission, safe orthogonal metadata/availability/attempt projection, and bounded native task
polling. It does not publish images into UI authoring, hydrate referenced images on reopen, or
prove the native product trajectory.

# Result

Selecting an eligible PNG in the Project Browser exposes committed metadata separately from one
Adapter-local draft. Missing metadata seeds sRGB; Ready metadata seeds its committed setting;
Invalid and IdentityConflict remain read-only. Import/Reimport carries only the selected relative
path, exact detail revision, and typed settings through the existing frame-bound action batch.

The Host retains the concrete last-good image independently from desired metadata and attempt
state. Removing a sidecar after a successful import reuses the same stable ID instead of poisoning
the session asset identity. Source replacement projects `SourceChanged`. Failed source-index
reconciliation retains a non-submittable last-good detail and no longer counts as pending work, so
the native loop returns to `ControlFlow::Wait` instead of polling forever.

The correction binds latest attempt and fallback detail to the exact selected relative path. A
newer catalog generation without a fresh source digest projects `SourceUnverified`, while a newer
exact source-conflict observation overrides an older catalog generation and projects
`SourceChanged`; neither condition can be presented as `Fresh`.

# Evidence

- Four focused Host tests passed 4/4 with `runtime-2d,tooling,serde`. They cover degraded-authoring
  projection, exact action submission, stale detail rejection, sidecar deletion/recreation with
  stable-ID preservation, source replacement freshness, and refresh-lineage failure with
  last-good retention and no residual pending work:

  ```text
  CARGO_BUILD_JOBS=1 cargo nextest run -p nara --lib --features 'runtime-2d,tooling,serde' -E 'test(generic_editor_image_import_projection_remains_available_in_authoring_degraded) | test(generic_editor_image_import_action_submits_the_exact_selected_detail) | test(generic_editor_image_import_refresh_failure_retains_last_good_without_pending_work) | test(generic_editor_image_import_action_rejects_stale_detail_failure_atomically)' --test-threads=1
  ```

- The UI-neutral Tooling state/action contract passed 3/3:

  ```text
  CARGO_BUILD_JOBS=1 cargo nextest run -p nara_tooling -E 'test(image_import)' --test-threads=1
  ```

- The private egui Inspector passed 5/5 focused tests for its state table, local draft lifetime,
  safe last-good text, project-global duplicate-submit guard, and exact Project Browser routing:

  ```text
  CARGO_BUILD_JOBS=1 cargo nextest run -p nara_tooling_egui -E 'test(image_import) | test(last_good_status_labels_are_safe_and_specific) | test(another_target_pending_disables_an_idle_image_submit) | test(project_browser_uses_image_inspector_only_for_the_matching_asset_source)' --test-threads=1
  ```

- Native deadline composition passed 3/3. Pending work receives one bounded 16 ms deadline,
  terminal transition requests one successor frame, and an earlier wake source wins:

  ```text
  CARGO_BUILD_JOBS=1 cargo nextest run -p nara-editor --lib -E 'test(image_import_pending_work_uses_a_bounded_poll_deadline) | test(image_import_terminal_transition_requests_one_successor_frame) | test(earlier_wake_source_wins_over_image_import_polling)' --test-threads=1
  ```

- `cargo check -p nara-editor`, `cargo fmt --all --check`, and `git diff --check` passed. Boundary
  searches found no filesystem, image-domain, task, capability, or GPU authority in the new
  Tooling/egui modules.
- Correctness review identified stable-ID loss after sidecar deletion, refresh-failure state
  collapse/spin, and source-drift misprojection. All three were fixed with focused regression
  evidence. Simplification removed unused public accessors, stringly runtime-error recovery,
  duplicate Browser view construction, duplicate Adapter checks, and an unused catalog accessor.
  The public-frame, private-frame, and transaction checks remain intentionally separate trust
  boundaries.
- The post-review correction passed 4/4 focused Host tests for normal projection, target-bound
  fallback, refresh failure, and source replacement before Refresh:

  ```text
  CARGO_BUILD_JOBS=1 cargo nextest run -p nara --lib --features 'runtime-2d,tooling,serde' -E 'test(generic_editor_image_import_exact_source_conflict_overrides_stale_catalog_freshness) | test(generic_editor_image_import_fallback_binds_freshness_and_attempt_to_the_selected_target) | test(generic_editor_image_import_action_submits_the_exact_selected_detail) | test(generic_editor_image_import_refresh_failure_retains_last_good_without_pending_work)' --test-threads=1
  ```

  The final independent correctness review reported no findings, residual risks, or testing gaps.

# Follow-up

EIR-U5 is the sole active successor. It may unify accepted Editor images with strict/closure-backed
images, publish UI choices and prepared Canvas resources from one generation, and hydrate only
referenced images on generic reopen. EIR-U6 and unrelated Editor successors remain inactive.

# Citations

- `1620786` (`feat(editor): add image import inspector`)
- `6797956` (`fix(editor): bind image import fallback facts`)
- `docs/plans/2026-08-20-001-feat-editor-image-import-and-reimport-v1-plan.md`
- `docs/architecture/adr/0115-editor-image-import-metadata-and-authoring-publication.md`
- `crates/nara_tooling/src/image_import.rs`
- `crates/nara_tooling_egui/src/image_import.rs`
- `src/project_host/runtime/editor/image_import/projection.rs`
- `src/project_host/runtime/editor/image_import/transaction.rs`
- `editor/src/platform.rs`
- `src/project_host/runtime/tests.rs`
