---
type: "Verification Evidence"
title: "Captured Editor Scene preview and ordered Image publication qualification"
description: "Actual working-copy capture, source receipts, native content and selection fences, and ordered Image observations have complete scoped owner, public, independent, and selected Windows evidence."
timestamp: 2026-10-26T22:14:36Z
record_id: "bd36086e52524bb1b227dbd21da81056"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/declarative-prefab-overlays"
supersedes: "0e0a11438b61465db4dd9818d1f0ee2a"
---

# Verification

Qualify the production declarative Prefab replacement through actual-demand Authoring Scene
capture, ordered Image publication and native preview/input. The complete foundation Goal stays
active. This is a coherent local checkpoint on `refactor/declarative-prefab-overlays`, based on
`47a2000`; it does not complete all Prefab workflows or the foundation plan.

Verification used Rust 1.95.0, Windows 11, serial Cargo with `CARGO_BUILD_JOBS=1`, and the root
`target`, also for independent workspaces. The native group used DX12 and actual hidden native
windows. All source writers froze before formatting and final checks. The commit containing this
record binds the verified replacement; no remote publication or main merge is part of this unit.

# Result

The replacement stores declarative instance intent rather than durable edit patches. One Scene
compiler selects actual sources and surviving component programs for Authoring and Runtime;
exact source transactions, inverse history and unavailable-data preservation keep their owners.
The old private comparison compiler and obsolete Host patch-retention helper are removed.

The actual Editor now captures the selected Scene and demanded open Prefab working copies on
background tasks, separately retaining session lineage, accepted file receipts and exact Schema.
Save/Reopen update source evidence; irrelevant open Prefabs do not invalidate the selected closure.
Captured assets use that capture's database. Cancelled work remains owned until physical return.

Native authorization uses the complete current content stamp across candidate consumption,
presentation, redisplay and picking. Prefab-derived geometry selects its containing Scene anchor
through typed provenance, never a materialized locator. A rejected or pending capture can retain
last-good pixels without input authority. Successful Image publication/retirement also invalidates
captured asset evidence before a successor Scene projection can be authorized.

Image reconciliation binds worker results to the actual catalog observation, retains a coalesced
successor after newer observations, refreshes capture receipts independently of pixel reuse, and
checks competing stable-ID metadata on the worker. Those reads share a 16 MiB aggregate ceiling
and the existing per-file metadata decoder limits. Failed runtime revocation latches the existing
terminal publication barrier, retires ineligible choices and settles without a retry loop.

# Evidence

| Scope | Command or selection | Result |
| --- | --- | --- |
| Scene, Tooling and UI | `cargo +1.95.0 nextest run -p nara_scene -p nara_tooling -p nara_ui --all-features --locked --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail` | **677 passed**, four existing ignored; run `b4e47a73-fe1e-4e73-88c3-0cb396a9e6a4`, build 6m14s, execution 34.663s |
| Final root and Editor libraries | `cargo +1.95.0 nextest run -p nara -p nara-editor --all-features --locked --lib --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail` | **754 passed**, 34 ignored; run `ec0c67dd-3cd3-4495-a7cb-4db1437b2281`, build 0.74s, execution 204.757s |
| Final targeted ordinary and native group | Same two packages, all features, `--lib --run-ignored all`, source-capture/preview/Image/Editor persistence filters plus the six native names below | **84 passed**: 78 ordinary and six native; 704 filtered; run `e7bd7cac-3ed9-421e-acbd-91dc2d4bb27e`, build 51.78s, execution 78.426s |
| Workspace compilation | `cargo +1.95.0 check --workspace --all-features --all-targets --locked` | Passed, 3m26s; existing warnings remain |
| Minimal Scene | `cargo +1.95.0 check -p nara_scene --no-default-features --lib --locked` | Passed, 0.87s |
| Independent consumer | `cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml --target-dir target -p nara-scene-module-consumer --locked --test prefab_sources --test property_authoring --test durable_identity --test nested_schema --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail` | **18 passed**, no skipped; run `3aa82098-7797-492e-8cd7-6ce12610e613`, build 56.71s, execution 1.417s |
| Reference Game | `cargo +1.95.0 nextest run --manifest-path reference-game/Cargo.toml --target-dir target --locked --test prefab_startup --test project_content_boot --test runtime_core --test runtime_drive --test public_surface --test ui_authoring_roundtrip --test vec2_schema_migration --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail` | **13 passed**, no skipped; run `45d17f07-4ca4-4745-9f82-13e417741bba`, build 9.22s, execution 1.849s |
| Public example | `cargo +1.95.0 run -p nara --all-features --locked --example prefab_overlay` | Passed; leaf pin, uncovered sibling inheritance, source Revert and Undo assertions execute |

The independent-workspace commands actually supplied the absolute root target directory; the
table uses `target` to express that same location without a machine-specific path.
These cohorts overlap; their counts are not an additive count of unique tests.

The six executed native tests are:

- `native_resume_redraw_rebases_paused_idle_before_the_owner_drive`
- `native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime`
- `native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts`
- `native_retained_scene_receipt_survives_prefab_navigation_and_bound_close`
- `native_ui_canvas_priority_survives_closing_retained_scene`
- `native_scene_reimport_replaces_presented_pixels_without_document_or_extent_changes`

The retained-Scene trajectory now places a real Sprite in a Prefab, dispatches the actual
presentation pick and verifies accepted anchor selection. It also changes a demanded open
Prefab without changing the root revision, refuses stale display/pick/composite authorization,
and checks stable target/token reuse after the successor settles. The Image trajectory checks
prepared blue/green pixels and distinct actual native submissions after reimport, with stable
subsequent reuse. This is not a new GPU pixel-readback or monitor-scanout claim.

Independent reviewers found and rechecked the invalid projected selection target, global
identity-conflict exemption, old-check/new-observation ordering and cached-receipt loss.
Separate final narrow reviews closed ordered Image semantics and the native handoff corrections
without remaining actionable P1/P2 findings in their specified scopes. All five used threads,
including the resumed predecessor Image worker, are closed; none owns an outstanding write set.

Earlier failures are retained in the linked progress record. The first full ordinary run had
five failures; a later run exposed one background-import assumption in the Reopen test.
Initial native execution passed four of six: the unsaved indicator changed viewport height and
Image publication retained old captured asset receipts. The final code reserves one status row
and reacquires capture evidence after publication. Native tests wait for validated/current
presentation while retaining their stable-reuse assertions. Reference Game initially passed
11/13; its canonical startup fixture and obsolete eager-Prefab queue expectation were corrected.
The final comparison remains byte-exact; six file reads and eight dependency edges remain exact.

Later changes after the final engine/native runs only canonicalized the Reference Game fixture
and updated its exact queued-job expectation. The current 13-case game run covers those changes.
No engine production code changed after the final 754-case run.

# Follow-up

- Keep the full foundation Goal active and the current branch separate from main until the
  remaining selected Prefab workflow is ready. A local checkpoint is not full phase completion.
- Add structured source-conflict observation and deliberate repair controls; complete declaration
  replacement is already the exact source-owned boundary, not an arbitrary expanded-record write.
- Implement source-owned convert-to-local with durable ID allocation, complete hierarchy/reference
  remapping and one inverse. Use the complete compiled source before native preview omissions;
  do not serialize viewport locators or infer a conversion from declaration detachment.
- Move UI Canvas onto the shared capture owner. Its existing direct Prefab helper remains distinct.
  First-class inherited subentity selection/Inspector remains separate from current anchor picking.
- Retain the repeated Schema-descriptor work finding and the unqualified startup/Candidate/Validate
  interleavings from the independent review. The private 500 ms full-closure validation policy has
  no measured large-project tail-latency or watcher-equivalence claim.
- This unit does not qualify every workspace runtime test, native Linux/macOS, arbitrary
  cross-document Apply, a managed Behavior representation, dynamic-code reload or the remaining
  foundation delivery waves. Check, external Play and Export retain file-backed inputs.

# Citations

- [Predecessor regression and active corrections](../../progress/2026-10/2026-10-26T211738Z-editor-capture-regression-and-ordered-image-observation-corrections-in-progress-0e0a11438b61465db4dd9818d1f0ee2a.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Overlay design](../../../../architecture/prefab-overlay-design.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0115](../../../../architecture/adr/0115-editor-image-import-metadata-and-authoring-publication.md)
- [Unity/Godot ownership study](../../../../research/2026-10-26-unity-godot-prefab-editor-ownership.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
