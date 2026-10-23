---
type: "Verification Evidence"
title: "Workflow-driven foundation waves and R1a owned Application submission qualification"
description: "Target owner boundaries, dependency corrections, exact direct dispatch observations, consumer evidence and explicit remaining scope."
timestamp: 2026-10-23T12:35:23Z
record_id: "5e5f6c3eb3b84150bc3a5e06e9862abf"
producer_id: "codex-foundation-integration"
run_id: "foundation-r1a-20261023"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
verified_by: "codex-root; independent Codex review 01a168a6-8983-7e43-91a2-d267e7108b80"
---

# Verification

Continuation of the foundation rebaseline on 2026-10-23, branch
`test/compiled-asset-domain-export`. Design commit `a694db6` follows `e4c4160`; the R1a
implementation and caller migrations are the dirty source tested here and are committed with
this evidence. SHA-256 of `git diff --binary a694db6 -- editor/src editor/tests
tests/fixtures/module-consumer/editor-panel-consumer` after formatting:
`f9a841737927bf824da997b9f7a948bc5cd99cc7e1d801884d5650d04533081c`.

The default compiler is Rust 1.99.0 (`b940084d7`, Windows MSVC). The three explicit native
trajectories use Rust 1.95.0 (`59807616e`) and Windows/DX12 as required by the native procedure.
All Cargo commands ran serially with `CARGO_BUILD_JOBS=1`, sharing the root `target`.

# Result

R1a is qualified within the direct Application submission boundary. It is not completion of
R1, wave 1, installable tools, prepared editing, Behavior representation or the whole foundation.
ADR 0118 and the panel guide describe the implemented contract; 0120/0121 remain Proposed.

- `submit_and_drive` now returns owned per-action observations in original input order plus a
  separate subsequent drive update. Domain rejection, whole-batch rejection, Closing and
  WorkspaceUnavailable are different facts. Existing direct callers are migrated without an
  old-return compatibility shim, `Deref` or forwarding methods.
- Later drives and project retirement cannot overwrite retained observations. Save admission
  remains Accepted even when its later persistence completion fails; action ordering is not
  transaction atomicity. Frontend observation/callback custody is unchanged.
- Private slot/report corruption cannot claim an action was not dispatched. Exact consumption
  and no-unclaimed-outcome assertions protect that internal invariant, not a new user-input
  admission framework. No result bus, global latest-result slot or ticket layer is introduced.

The target architecture now distinguishes Authoring, Runtime, Content, physical Hosts and
Product composition as responsibility groups, not mandatory new crates. The execution plan
uses seven complete workflow waves instead of a linear sequence of module rewrites. Independent
review corrected these important dependency mistakes:

- Durable source/property addressing precedes prepared-edit and final Behavior/Prefab contracts;
  projection/history lifetime can proceed independently of a selected language.
- Physical native close precedes selected narrow executable replacement, not every disposable
  representation experiment. Compare representation first, language second, and only qualify
  replacement modes the selected owner actually needs.
- A real useful installable tool includes inline Inspector placement and local update/disable/
  remove, not only a source-built panel or a toy child-process proposal.
- A bounded immutable product catalog can qualify base delivery before a full CAS. Shared
  View/color/time facts belong with external render composition, not after all asset work.
- Every wave includes integration, relevant native verification, deletion and current contracts;
  none waits until a final all-engine cleanup wave to remove its superseded path.

# Evidence

The final relevant runs are:

| Command / scope | Executed result | Nextest run ID |
| --- | --- | --- |
| `cargo nextest run --locked -p nara-editor --lib --test application_contract -E 'test(direct_submission) \| test(submission_tests) \| test(action_feedback)' --test-threads 1` | 13 passed, 208 skipped | `df218cb9-2358-484a-b21d-405af26740b0` |
| `cargo nextest run --locked --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-editor-panel-module-consumer --test imgui_contribution --test source_import --test dialogue_panel --test-threads 1`, with `CARGO_TARGET_DIR` set to root `target` | 11 passed, 0 skipped across 3 binaries | `3ea4c642-ad14-4556-9ac1-768ea8602707` |
| `cargo nextest run --locked -p nara-editor --lib --test application_contract --test generic_open_recovery --test generic_component_composition --test registered_panel_interactions --test first_game_workflow --test authoring_loop --test-threads 1` | 214 passed, 25 skipped across 7 binaries | `ae123709-2826-43fc-b6bf-751fe4e33fed` |
| `cargo +1.95.0 nextest run --locked -p nara-editor --lib --run-ignored only -E 'test(native_resume_redraw_rebases_paused_idle_before_the_owner_drive) \| test(native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime) \| test(native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts)' --test-threads 1`, with `WGPU_BACKEND=dx12` | All 3 selected native trajectories passed; 198 unselected tests | `272f3c9a-d4fe-4f6b-a722-6d9b8070c171` |

The focused 13 are included in the related 214, not another independent coverage total.
The external consumer imports and asserts the new public observation type. The native group
executes actual cadence, Game input/exact-Step and independent-view retirement/recovery paths,
including direct-submission cleanup callers; skipped native cases are not counted as passing.

`cargo check --locked -p nara-editor --all-targets --keep-going` passed after the final Rust
changes. Root and external-fixture `cargo fmt --all` passed. Formatting after the external run
only wrapped the new private invariant-test table; it did not alter the exported implementation
or external consumers. Pre-existing dead-code warnings remain in FilePublicationHost,
ProjectStartupSceneIssue, Shell generation fields and supervised output helpers.

Five new public tests protect ordered partial rejection, whole-batch rejection, mixed and all
Closing filters, real missing-workspace dispatch and Save admission versus failed completion.
The retained-result test also checks after physical project retirement. Existing panel tests
protect callback routing from unrelated direct calls.

Independent reviewer `01a168a6-8983-7e43-91a2-d267e7108b80` first identified the false
WorkspaceUnavailable fallback. Before fixing production, the five-case private regression failed
as intended: every missing report, missing outcome, duplicate slot, out-of-range slot and unclaimed
outcome was silently accepted (RED run `a691bb4b-9211-44c5-8c36-6ced4ff23f88`). The corrected
conversion passed the focused and related suites. The same reviewer rechecked the exact correction
and legitimate empty/Closing/workspace cases, found no remaining issue in that scope, and was
closed. The reviewer ran no Cargo or writes; executed tests are main-thread evidence.

An initial breaking-return compile exposed old callers. They were migrated rather than restoring
boolean forwarding. That failed compilation and the deliberate red test are not final failures.
The design/plan local links and anchors pass a one-off check; final document/diff and memory
checks pass during integration: six relevant documents have 644 valid local links/anchors,
the source-diff digest is unchanged, and memory validates with existing legacy advisory warnings.
Root and external-fixture formatting checks also pass. This record does not rewrite the earlier
docs-only evidence.

# Follow-up

Next implement R5a's minimum durable address decisions together with R2's owner-owned prepared
editing and selection-independent queries. Keep the same accepted candidate, inverse and history
admission from prepare through commit, with transaction-local references for mixed construction.
Complete a public semantic tool/window-free consumer, failure of the last operation, stale basis,
one Undo/Redo, Save and Reopen before claiming wave 1.

Independent read-only next-unit review `01a168c3-51b2-7c61-9c14-7b0faf927646` narrowed three
design risks; the main thread re-read their owning source before retaining them:

1. `ComponentFieldPathSegment::{Field(String), Index(u32)}` and name-only
   `ComponentShapeMember` locate a current value, not a proven rename/reorder-stable address.
   Preserve the existing structured `PrefabProjectionPath`; separate source/entity/projection/
   property identity from revision, Schema admission and Runtime incarnation. This does not
   imply all current revision-local editing paths are defective.
2. Generic structure admission deliberately accepts one operation and an empty new entity,
   while field transactions use another path. A public action list is still not mixed construction
   atomicity. Existing private prepared documents lack the public delayed-commit owner/revision
   contract; making the type public alone is insufficient. Reuse candidate, inverse and history
   admission within the Scene/Prefab owner instead of adding another Undo authority.
3. `EditorWorkspace::scene(document)` and read-only document access already exist. The gap is
   normal contribution access without active-tab/Inspector selection choreography. An Inspector
   traversal followed by truncation would bound output but not scanning. Query and cursor scope
   need distinct scan/output limits plus document, revision and Schema binding.

Use a non-active document B while A and its selection remain unchanged. Construct two entities,
multiple components and their mutual reference with transaction-local references; prepare a
read-only owner diff, commit it once and return local-to-persistent identities. Failure at the
last operation, insufficient history, stale/cross-owner candidate and Reopen incarnation must
publish nothing; cancellation must not mutate history/dirty state. Undo/Redo and Save/Reopen
must pass through normal public tooling/Editor APIs in an independent module consumer.

Start that consumer with direct entities and the existing stable root fields. Do not guess
durable nested/member or keyed-collection identity or implicit source-versus-instance writeback.
Those require explicit subsequent R5a/R5b decisions. This bounded first consumer does not remove
existing legal nested editing or satisfy the full wave's previous-capability coverage by itself.

The comparison used pinned Bevy `85503941e488152711a4695bbe248e9ed9f453fc` and Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558`. Bevy's
`crates/bevy_world_serialization/src/dynamic_world.rs` creates its entity map before writing
components, which informs local references but does not promise Nara's all-or-nothing editing:
later missing type data can return an error after mutation. Godot's
`scene/resources/packed_scene.cpp` distinguishes normal node paths from ID-chain recovery;
`editor/editor_undo_redo_manager.cpp` checks history ownership. These are focused primary-source
comparisons, not a claim that either engine implements Nara's complete candidate contract.
This reviewer ran no Cargo/writes and was closed after returning the recommendation.

No full-workspace test matrix, benchmark, dynamic installation, ABI, sandbox, source-free delivery,
external-process scheduling, IME/accessibility or Linux/macOS native claim follows from these runs.
Existing performance evidence is unchanged. The pre-existing untracked `vendor/` directory and
read-only `repo-ref/` remain untouched. No new Goal, budget, migration guide, push or PR was created.

# Citations

- [Target architecture](../../../../architecture/foundation-architecture-reassessment.md#target-architecture-and-refactor-decision-rule)
- [Seven workflow waves](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#seven-delivery-waves)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Application implementation](../../../../../editor/src/app.rs)
- [Public contract tests](../../../../../editor/tests/application_contract.rs)
- [Property locators](../../../../../crates/nara_reflect/src/path.rs)
- [Shape members](../../../../../crates/nara_reflect/src/schema.rs)
- [Structural admission](../../../../../crates/nara_scene/src/authoring_decode/structure.rs)
- [Candidate preparation](../../../../../crates/nara_scene/src/authoring_decode.rs)
- [Authoring history](../../../../../crates/nara_scene/src/authoring.rs)
- [Workspace queries](../../../../../crates/nara_tooling/src/workspace.rs)
- [Panel guide](../../../../guides/editor-panels.md)
- [Native qualification procedure](../../../../verification/native-editor-qualification.md)
