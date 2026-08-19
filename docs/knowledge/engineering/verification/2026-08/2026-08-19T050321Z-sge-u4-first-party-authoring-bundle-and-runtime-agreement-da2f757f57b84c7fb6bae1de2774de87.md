---
type: "Verification Evidence"
title: "SGE-U4 first-party authoring bundle and runtime agreement"
description: "Verifies canonical starter and Reference Game bundles, clean-room generic Editor Open, and strict selected-native runtime agreement."
timestamp: 2026-08-19T05:03:21Z
record_id: "da2f757f57b84c7fb6bae1de2774de87"
status: "verified"
producer_id: "codex-root"
run_id: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-18-2158-feat-standalone-generic-editor-project-open-and-recovery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "2d42516"
verified_by: "codex-root"
---

# Verification

Verified commit `2d42516` against SGE-U4 of the Standalone Generic Editor plan. The proving scope
covers canonical first-party bundle publication, Reference Game bundle generation from its normal
product composition, one unrelated custom-owner project, and strict agreement between selected
native schema owners and the declared data-only bundle before Runtime or Export publication.

# Result

SGE-U4 passes. The `desktop-2d/v1` starter now publishes and declares a canonical built-in
authoring bundle in the same atomic project transaction. Reference Game checks in a canonical bundle
generated from its ordinary `project_recipe()` rather than a project-specific Editor target or
parallel provider list. The standalone Editor opens the starter, Reference Game, and a clean-room
KnownUnbound project through the same ordinary Authoring path without compiling project code.

Provider-capable Check, Run, trusted compiled Editor refresh, and Export now require a bounded
bundle/native agreement. Compatible bundle supersets remain valid; missing or divergent selected
owners and unbound reachable records fail before Runtime/output publication. The startup closure
retains the exact bundle source for source-carrying Export revalidation while excluding bundle-only
metadata from Runtime content identity.

# Evidence

- `cargo nextest run -p nara_reflect --features serde --test authoring_bundle --test-threads=1`
  passed 5 tests for the canonical bundle envelope, owner ordering/conflicts, lineage, and budgets.
- `cargo nextest run -p nara --features 'project-workflow,runtime-2d,serde' --test
  project_workflow_creation --test project_workflow_check --test-threads=1` passed 13 tests,
  including starter publication, compatible superset acceptance, and selected-owner
  missing/divergent/unbound rejection.
- `cargo nextest run -p nara --features
  'runtime-2d,serde,project-workflow,tooling,runtime-ui' --lib -E
  'test(project_host_rejects_content_without_runtime_schema_agreement) |
  test(trusted_compiled_editor_rejects_a_bundle_missing_selected_native_owners) |
  test(sequential_starts_share_immutable_authority_and_rebuild_runtime_sessions)'
  --test-threads=1` passed 3 tests.
- `cargo nextest run -p nara-editor --test application_contract -E
  'test(starter_opens_one_coherent_frame_and_empty_repaint_preserves_document_state) |
  test(clean_room_custom_owner_opens_through_the_ordinary_editor) |
  test(reference_game_opens_through_the_same_ordinary_editor_api)' --test-threads=1` passed 3
  representative ordinary-Editor product contracts.
- `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --test
  plugin_composition --test project_manifest_ingest --test-threads=1` passed 6 Reference Game
  bundle/composition and manifest-ingest tests.
- `cargo nextest run --manifest-path reference-game/Cargo.toml --bin authoring_bundle
  --test-threads=1` passed the capability-relative stale/write/check publication test on Windows.
- `cargo run --manifest-path reference-game/Cargo.toml --bin authoring_bundle -- --check` verified
  the checked-in bundle against the current normal Reference Game product plan.
- `cargo check -p nara --features 'runtime-2d,serde,project-workflow,tooling,runtime-ui' --locked`
  and `cargo check -p nara-editor --locked` passed.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- Security review found no remaining P0/P1 after the generator was moved to manifest-declared,
  capability-relative atomic publication. Correctness findings for trusted Editor agreement and
  Windows directory-sync semantics were fixed and covered by the focused tests above. The simplify
  pass removed callback-bearing public provider exposure, duplicate agreement witnesses, redundant
  identity facts, linear owner lookup, repeated encoding, and repeated fixture bundle generation.

# Follow-up

SGE-U5 is the only next active unit. It owns the final guarded clean-room Save/Close/Reopen and
explicit Runtime-unavailable trajectory, one real native Project Manager -> Recovery -> Open canary,
the final dependency/public-surface inventory, user launch guidance, ADR/ledger closure, and the
clean proving record. External Play transport, project-wide browsing, and extension hosting remain
inactive.

# Citations

- `2d42516` (`feat(editor): bind projects to authoring schema bundles`)
- `docs/plans/2026-08-18-2158-feat-standalone-generic-editor-project-open-and-recovery-plan.md`
- `docs/architecture/adr/0090-unavailable-schema-and-lossless-authoring.md`
- `docs/architecture/adr/0114-standalone-generic-editor-authoring-and-execution-boundary.md`
