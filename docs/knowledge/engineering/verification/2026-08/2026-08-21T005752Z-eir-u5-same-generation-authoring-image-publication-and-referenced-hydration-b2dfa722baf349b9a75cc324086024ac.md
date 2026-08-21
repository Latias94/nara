---
type: "Verification Evidence"
title: "EIR-U5 same-generation authoring image publication and referenced hydration"
description: "Verifies atomic image choices and prepared resources, referenced-only hydration, generic Save/Reopen, retirement, and exact source-proof boundaries."
timestamp: 2026-08-21T00:57:52Z
record_id: "b2dfa722baf349b9a75cc324086024ac"
status: "verified"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-20-001-feat-editor-image-import-and-reimport-v1-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "94157a1"
verified_by: "codex-root"
---

# Verification

Verified EIR-U5 across `a404a41` and source-proof correction `94157a1`. The unit publishes one
immutable authoring-image receipt containing matching UI image choices and prepared resources,
hydrates only images referenced by the accepted generic-authoring closure, and preserves the
ordinary Save/Close/Reopen boundary without compiling project Rust.

# Result

Accepted explicit imports and closure-backed hydration now enter the same session-local image
store. A successor authoring-image receipt is published only after the complete selected store can
be captured; UI choices and Canvas prepared resources therefore advance under one generation and
older field-edit frames cannot authorize a successor image.

Generic reopen hydrates only referenced canonical images on the existing bounded serial task lane.
Unreferenced images remain lazy, invalid referenced images publish a safe warning without blocking
the workspace, and invalid or identity-drifted metadata retires unsafe authoring choices. Saving an
imported `AssetRef`, closing the session, and reopening the same project restores the same typed
recipe and Canvas image from canonical source and metadata.

The final correction separates catalog eligibility from source-byte proof. A catalog refresh may
retain a last-good image but cannot renew its source digest. Ordinary refresh therefore projects
`SourceUnverified`; exact identity drift remains `SourceChanged`; a durable desired-settings
divergence remains `SettingsChanged`. Only a receipt-bound import path may advance source proof.

# Evidence

- Four focused authoring-publication tests passed 4/4. They cover referenced-only hydration,
  invalid metadata retirement, bounded diagnostics, and generic Save/Reopen with the same image
  recipe and Canvas resource:

  ```text
  CARGO_BUILD_JOBS=1 cargo nextest run -p nara --lib --features 'runtime-2d,runtime-ui,presentation-ui,tooling,serde' -E 'test(generic_editor_hydrates_only_referenced_images_and_retains_a_complete_canvas_on_failure) | test(generic_editor_retires_an_image_when_its_metadata_becomes_invalid) | test(authoring_image_warning_omits_an_unbounded_project_relative_locator) | test(generic_editor_saves_an_imported_asset_ref_and_hydrates_the_same_recipe_on_reopen)' --test-threads=1
  ```

- The post-review source-proof correction passed 4/4 focused Host tests. They distinguish catalog
  retention from source verification, preserve failed-Reimport last-good pixels, reject invalid
  metadata from authoring publication, and keep target-bound fallback facts exact:

  ```text
  CARGO_BUILD_JOBS=1 cargo nextest run -p nara --lib --features 'runtime-2d,runtime-ui,presentation-ui,tooling,serde' -E 'test(generic_editor_image_import_publishes_metadata_and_preserves_last_good_on_failure) | test(generic_editor_image_import_action_submits_the_exact_selected_detail) | test(generic_editor_image_import_fallback_binds_freshness_and_attempt_to_the_selected_target) | test(generic_editor_retires_an_image_when_its_metadata_becomes_invalid)' --test-threads=1
  ```

- `CARGO_BUILD_JOBS=1 cargo check -p nara-editor --tests`, `cargo fmt --all`, and
  `git diff --check` passed. Only pre-existing dead-code warnings were emitted by the focused root
  test build.
- Simplification split the U5 Runtime tests into `runtime/tests/authoring_images.rs`, removed
  duplicate image/catalog state, kept hydration and explicit imports on one runtime/store owner,
  and preserved one atomic receipt instead of adding another public registry or task pool.
- Independent review found the catalog proof-renewal and stale invalid-metadata regression. Both
  were corrected in `94157a1`; the focused regression batches above are the observed verification.

# Follow-up

EIR-U6 is the sole active successor under registration `0a63693fff3d415fadd50f3370d154e0`.
It may add only the standalone `nara-editor` product trajectory, representative native Canvas
evidence, public usage documentation, and final verification/ledger closure. Watchers, batch
import, durable artifact caches, other formats, project Rust execution, and unrelated Editor
successors remain inactive.

# Citations

- `a404a41` (`feat(editor): publish authoring image resources`)
- `94157a1` (`fix(editor): preserve image source proof boundaries`)
- `docs/plans/2026-08-20-001-feat-editor-image-import-and-reimport-v1-plan.md`
- `docs/architecture/adr/0115-editor-image-import-metadata-and-authoring-publication.md`
- `src/project_host/runtime/editor/authoring_images.rs`
- `src/project_host/runtime/editor/image_import/{catalog,hydration,projection,store,transaction}.rs`
- `src/project_host/runtime/tests/authoring_images.rs`
