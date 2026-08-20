---
type: "Verification Evidence"
title: "EIR-U3 bounded Editor image import and reimport transaction"
description: "Verifies the bounded Host catalog, atomic metadata publication, task-owned image import, exact retained-pixel leases, retryable close, and failure-atomic reimport workflow."
timestamp: 2026-08-20T09:45:24Z
record_id: "e6cb2cf25cdf4321a15368421154678d"
status: "verified"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-20-001-feat-editor-image-import-and-reimport-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "2376a3a"
verified_by: "codex-root"
---

# Verification

Verified commits `a0141e6`, `cb2e6c1`, `c53b189`, and `2376a3a` against EIR-U3. The unit adds one
bounded Host-owned image catalog and Import/Reimport transaction, one shared Editor authoring
runtime, receipt-bound metadata publication, retryable physical task retirement, exact
retained-pixel leases, no-presentation lifecycle drive, replacement handoff, and concrete
session-local last-good image retention. It does not add Inspector, Canvas publication, reopen
hydration, or a native product trajectory.

# Result

The Host derives image identity from the accepted Project Browser/source-index facts, validates the
exact source and sidecar generations, publishes canonical metadata with atomic expected-missing or
expected-existing semantics, and only then schedules bounded decode. Completion revalidates source,
metadata, attempt, settings, stable ID, and expected asset version before committing pixels.

A failed, cancelled, stale, or superseded attempt cannot replace the accepted image. A failed
Reimport retains the concrete last-good pixels while exposing source/settings staleness separately
from metadata and attempt state. Successful metadata publication queues one exact successor source
index refresh. Uncertain publication enters `ReconcileRequired` and blocks a new identity until a
normal Refresh re-establishes catalog truth.

# Evidence

- Atomic missing-target publication passed its owner-level contract test:

  ```text
  cargo nextest run -p nara_fs -E 'test(missing_target_publication_uses_atomic_no_replace)' --test-threads=1
  ```

  The implementation uses native no-replace rename on Windows and Linux and fails closed on
  unsupported platforms; a receipt cannot claim strong compare-and-swap without that proof.
- The longitudinal Host transaction test passed 1/1 serially:

  ```text
  CARGO_BUILD_JOBS=1 cargo nextest run -p nara --lib --features tooling,runtime-2d,serde -E 'test(generic_editor_image_import_publishes_metadata_and_preserves_last_good_on_failure)' --test-threads=1
  ```

  It covers stale-frame rejection, initial Import, canonical metadata, successful Reimport,
  source and metadata replacement after accepted receipt, decode failure with exact last-good
  retention, uncertain publication/reconciliation, cancellation/draining, bounded reservation
  release, and project-wide stable-ID conflict rejection.
- Retained pixel accounting and lease identity passed 3/3:

  ```text
  CARGO_BUILD_JOBS=1 cargo nextest run -p nara_image -E 'test(same_host_retained_pixel_lease_is_exact_and_not_transferable) or test(arbitrary_retention_owner_does_not_bypass_publication_overlap_limit) or test(retained_commit_accounts_for_pixels_until_the_final_shared_image_drops)' --test-threads=1
  ```
- Retryable task-owner close passed 2/2 in `nara_tasks` and 1/1 through the Editor Host. The Host
  test holds one image worker across the first close deadline, proves the session and pixel budget
  remain owned, releases the worker, retries to a complete receipt, and observes the IO worker join:

  ```text
  CARGO_BUILD_JOBS=1 cargo nextest run -p nara_tasks -E 'test(shutdown_retains_an_uncooperative_worker_and_completes_on_retry) or test(runtime_close_retains_timed_out_task_owners_until_retry_completes)' --test-threads=1
  CARGO_BUILD_JOBS=1 cargo nextest run -p nara --lib --features tooling,runtime-2d,serde -E 'test(editor_project_close_retries_timed_out_image_task_owner_before_releasing_session)' --test-threads=1
  ```
- Four focused Editor application tests passed for dirty exit, Close Project, replacement failure
  atomicity, and save-before-replacement. `cargo check -p nara-editor` also passed after native
  close-retry deadlines were composed into both presented and zero-sized-window drive paths.
- The shared Runtime lifecycle drive predicate passed its focused platform test after both
  presented and zero-sized-window paths were bound to the same Preparing, Starting, Running,
  Stopping, and Retiring states:

  ```text
  CARGO_BUILD_JOBS=1 cargo nextest run -p nara-editor -E 'test(runtime_lifecycle_drive_is_independent_of_presentation) or test(transitional_runtime_lifecycle_schedules_bounded_follow_up_drive)' --test-threads=1
  ```
- Focused compile and owner tests also passed for the coalesced source-index refresh, shared file
  publication Host, and exclusive metadata lock path. `cargo check -p nara --features
  tooling,runtime-2d,serde --test editor_persistence` passed during integration.
- `cargo fmt --all` and `git diff --check` passed. Boundary searches found one intended Editor task
  pool owner and no ambient filesystem access, raw erased metadata, generic importer registry, or
  second image runtime in the EIR-U3 Host modules.
- The security recheck found no remaining P0/P1 defect after its missing-target replacement race was
  closed by the owner-level no-replace primitive and focused test above. Simplification removed
  post-commit fact re-query, duplicated metadata binding state, conflated refresh requests, and dead
  published catalog facts.
- A workspace Clippy run was not treated as an EIR-U3 gate: the installed Rust 1.97 toolchain
  stopped on unrelated pre-existing `nara_app` lints while the repository contract targets Rust
  1.95. No unrelated lint churn was included.

# Follow-up

EIR-U4 is the sole active successor. It may project the closed UI-neutral image settings and state,
add exact-frame Import/Reimport actions, render the private egui Inspector, and compose bounded
native polling deadlines. EIR-U5/EIR-U6 remain inactive; this record does not authorize Canvas
hydration, referenced-image reopen, or the native end-to-end trajectory.

# Citations

- `a0141e6` (`feat(editor): implement image import transaction`)
- `cb2e6c1` (`fix(editor): close image import resource lifetimes`)
- `c53b189` (`fix(editor): drive runtime retirement without presentation`)
- `2376a3a` (`fix(editor): keep runtime retirement progressing`)
- `docs/plans/2026-08-20-001-feat-editor-image-import-and-reimport-v1-plan.md`
- `docs/architecture/adr/0115-editor-image-import-metadata-and-authoring-publication.md`
- `src/project_host/runtime/editor/image_import/catalog.rs`
- `src/project_host/runtime/editor/image_import/transaction.rs`
- `src/project_host/runtime/editor/authoring_runtime.rs`
- `src/project_host/persistence/file_publication.rs`
- `src/project_host/authoring/source_index.rs`
- `crates/nara_image/src/import.rs`
- `crates/nara_fs/tests/filesystem_contract.rs`
- `src/project_host/runtime/tests.rs`
