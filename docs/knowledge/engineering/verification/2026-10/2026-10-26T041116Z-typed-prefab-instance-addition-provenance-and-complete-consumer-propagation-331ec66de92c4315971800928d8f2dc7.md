---
type: "Verification Evidence"
title: "Typed Prefab instance-addition provenance and complete consumer propagation"
description: "Replace flattened base-source provenance with tagged instance-entry origins through expansion, retained custody, Runtime and opaque UI keys while retaining the current durable patch contract."
timestamp: 2026-10-26T04:11:16Z
record_id: "331ec66de92c4315971800928d8f2dc7"
producer_id: "codex-root"
run_id: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/prefab-addition-provenance"
base_commit: "59f911d"
---

# Verification

Production provenance replacement after `59f911d`, following the
[private structural/reference comparison](2026-10-26T034052Z-private-prefab-structure-and-reference-comparison-with-owner-boundary-counterexamples-c823420a09444940993ed9d948a28273.md).
This unit amends Accepted ADRs 0038/0058/0102, not the current durable patch storage contract.
The initial broad Reference Game run exposed two adjacent Root/caller failures below; their
correction retains its separate evidence. Final integration and native scope are distinguished
from the initial RED and focused owner checks.

# Result

- `PrefabProjectionStep` distinguishes `SourceEntity` from `InstanceAddition` at every step.
  Additions enter the preceding anchor's overlay-local namespace rather than claiming the
  overridden base source as owner. Added anchors can themselves instantiate Prefabs or add
  further local entities without losing their actual owning source or outer placement.
- Expansion classifies admitted top-level `AddEntity`/`RestoreEntity` creation declarations,
  including restored descendants, before consuming the patch. It does not infer origin by
  subtracting final IDs from base IDs: remove/recreate of the same spelling remains an addition.
  Nested override payloads are classified in their own instance expansion. Only successful
  complete patch application proceeds to candidate publication.
- A bounded temporary creation index is charged before copying IDs. Path accounting chooses
  the exact source/addition branch before cloning its step, and retained custody models the
  full typed chain without charging an addition for a source key it does not own.
- `SceneEntitySource` now retains one encapsulated `SceneEntityProvenance`. The old separately
  mutable source-ID/path fields and misleading source-only getters are removed; public consumers
  inspect the terminal local ID and complete typed steps. No compatibility wrapper is added.
- UI encodes a variant tag at every step. A source step includes its canonical source and ID;
  an addition includes its addition-local ID. Containing source, logical occurrence, root anchor
  and all ancestor steps remain. Canvas uses the same encoding through an explicit local-record
  entry point; the ECS-independent opaque UI kernel is unchanged.
- A complete logical addition key may remain stable across a newly admitted base replacement.
  That neither preserves an old runtime incarnation nor proves the old override/reference
  intent valid. The actual source/Host owner still captures and revalidates the resolved base
  closure; provenance is not a dependency inventory, especially for addition-only instances.

# Evidence

All Cargo verification is serial, Rust 1.95.0, `CARGO_BUILD_JOBS=1`, reusing root `target`.

- Focused `nara_scene --test prefab_sources --all-features` run
  `f3161b4d-ec00-49c8-a06e-1690576201cb`: 10 passed. New cases prove repeated nested ancestry,
  remove/recreate, restored descendants, exact Runtime publication and modeled creation-index/
  typed-path byte boundaries. Existing source/alias admission and pre-copy budgets remain.
- Strengthened same-name nested-payload isolation in the existing production fixture, then
  reran its 10 cases successfully as `243f6c8e-07c2-4736-bbfa-7543603af28e`. A nested creation
  cannot reclassify an inherited record in its containing effective source.
- Root and independent-fixture `cargo fmt --all` completed before combined verification.
- Combined Scene/Tooling/UI all-feature nextest run
  `cf40b104-b277-4836-bb80-abf9acc6bcb9`: 545 passed, 4 skipped. This includes the three real
  UI projection groups for source/addition disjointness, repeated/nested placements, added
  instance ancestry and addition-key continuity across a newly admitted base replacement.
- Focused root all-feature nextest run `d60a5183-f4e1-4745-9517-65be007f2e2f`: 56 passed,
  453 filtered/skipped. It covers all Scene retention cases plus `project_content_boot`,
  `project_content_delivery`, `project_scene_content` and `project_runtime_boot`, not the whole
  root library. The two new retention cases compare independently expressed source/addition
  path and map costs for Path/StableId requests and nested origin combinations.
- Separately compiled `nara-scene-module-consumer` run
  `2e38eee4-1621-4ddf-bbe9-13ceb099a4fa`: 11 passed across `durable_identity`, `nested_schema`
  and `prefab_sources`. The new public expand/spawn case checks nested addition ancestry and
  exact unchanged provenance through Runtime publication; no private construction hook is used.
- Reference Game all-feature run `72258b04-72f8-433a-b78d-45be3fabd185`: 168 passed, 2 failed
  of 170. The failures are the supervised Stop/Editor Exit driver and real host-parity fault
  envelope. This is not a passing full Reference Game result.
- Isolated host-parity RED `66779cfb-8181-415a-8007-b64e64b035f4` reproduces in 2.3 seconds.
  A bounded temporary diagnostic probe showed the intended tick-3 system fault correctly as
  `project.desktop.runner-failed` (`managed-runtime`, `system`, `nara.ecs.fallible-execution`),
  plus a second fieldless `project.run.runtime-faulted`. `CleanupOwner::completion_failure_report`
  repeats the already observed fault during retained retirement. These Root paths are unchanged
  by the origin refactor. The temporary `[DEBUG-origin-parity]` instrumentation is removed.
- Scene/Tooling/UI all-feature/all-target Clippy with `-D clippy::correctness` passed. Existing
  unrelated warnings remain; no warnings were emitted in the changed provenance/compiler/key
  files or their new public tests. This is not a zero-warning workspace claim.
- After the separate Root/caller corrections, final full Reference Game all-feature run
  `17c65e73-f002-4349-a7b7-50087e91db76`: 170 passed, 0 skipped in 88.124 seconds, following
  a 6m51s incremental build. This is one complete run, not a sum of targeted reruns. Its strict
  parity and physical Exit assertions remain unchanged in meaning.
- Final workspace all-feature/all-target Clippy with `-D clippy::correctness` and formatting
  passed. Existing warnings remain. A tracked-Rust search found no old source-only provenance
  getter or independently mutable legacy `SceneEntitySource` field consumer.
- Selected actual Windows/DX12 Editor run `f5a69fd9-5ee4-4c95-a923-5721817ddc37`: 3 passed,
  242 excluded/skipped by the ignored-only/name selection, in 5.438 seconds. Executed:
  `native_fixed_world_canvas_keeps_ui_input_resize_and_step_coherent`,
  `native_ui_canvas_priority_survives_closing_retained_scene`, and
  `native_unrelated_prefab_close_preserves_presented_play_source_until_source_close`.
  These hidden-window production trajectories cover fixed-World Canvas/input/resize/step,
  retained Canvas priority and exact Play-source association/retirement. They do not establish
  the complete native matrix, new overlay UI, SDK failure injection or other platforms.
- Documentation checks found no missing target among 1,050 local Markdown links in the 12
  affected documents/derived views. Engineering memory validation and deterministic rollup
  checks passed with existing legacy advisory warnings; historical records were not rewritten.
  `git diff --check` passed. The separate Root correction is committed as `6b3d458`.

## Independent Challenge

Reviewer `01a1765e-ed9b-7cf0-a71f-3be3b0a6cb55` challenged the proposed contract before code
qualification. It found no counterexample requiring a different origin enum or an early durable
format switch. Its concrete ADR correction narrows "every projection step retains the key" to
`SourceEntity` steps and explicitly keeps base-closure capture separate. Runtime occurrence,
activation/incarnation and epoch fencing remain authoritative. The same reviewer subsequently
reviews the actual implementation; execution results remain the parent's responsibility.
The completed code review found no confirmed origin misclassification, pre-copy budget bypass,
partial publication or complete-key alias. It inspected actual Scene, Runtime, UI, retention and
consumer code, and found the excluded example/Reference Game migration omission resolved.
Dedicated 32/33-step and full-key 256-byte boundary cases remain additional coverage, not
confirmed defects. The nested-payload same-spelling isolation fixture was strengthened afterward
and passed the parent's focused rerun above. The reviewer is closed and ran no Cargo.

Worker `01a1765e-5f19-70a3-a9ff-52589e8fb4b7` owned only `crates/nara_ui/**`.
Worker `01a1765e-9aa5-7402-87ba-82eae3dda543` owned only root `src/**` and `tests/**`.
Both returned concrete consumer/test changes, ran no Cargo and are closed. Parent integration
owns Scene production code, ADRs, formatting, tests and commits.
Worker `01a1766b-f730-7633-9a32-1bab16fc7306` subsequently owned the live example and excluded
Reference Game consumers, ran no Cargo and is closed. Source-prefab write-back observations
explicitly require a terminal source-entity step; an addition with a reused source spelling
cannot borrow the base's document. An inherited source entity below an added anchor still
retains its own true source; the filter does not reject all addition ancestry indiscriminately.
Worker `01a17685-e36b-7203-ac08-acdac88c34a4` corrected only the stale Reference Game Exit driver:
frame absence drives retained Open Host maintenance, never means ready, and success still requires
the application's physical `EditorApplicationUpdate::exit_ready()`. It is closed.
Worker `01a1768b-7243-7b02-bdbf-54f1aaacff5a` implemented the separate Root runtime diagnostic
delivery correction and real owner regressions, ran no Cargo and is closed. Its initial passing
cohort, original strict Reference Game reruns, independently reproduced metadata-loss regression
and corrected owner/review results belong to the
[Root correction evidence](2026-10-26T052536Z-retained-runtime-fault-delivery-and-reference-game-exit-driver-correction-19b2541631464d8798e3fa47ace8bfca.md).
They do not erase the initial broad RED or substitute for a final full Reference Game run.

# Follow-up

- Current `PrefabInstance.overrides` remains `ScenePatchDocument`. This corrects origin within
  that existing expansion, not the complete declarative overlay design or a new wire format.
- The flat effective copy still cannot hold same-spelled inherited and added records together.
  The new tags distinguish origin, not grant that missing source representation capability.
- Finish full-value effective candidate admission, complete durable property/reference grammar,
  bounded source/conflict encoding, actual nested closure publication, source overlay
  edit/history/repair and convert-to-local. Reconcile those owners before deleting persistent
  operation patches. Do not turn the temporary creation metadata index into a second replay engine.
- Read-only architecture sidecar `01a176be-aa6b-7af1-81ed-f2e1dbe671ff` identified the exact
  complete-record classification core, existing value/reference validators and pre-copy
  accounting boundary for that next unit. The Proposed overlay design records the narrowed
  extraction: no temporary authoring Session/history, no template-derived complete values, and
  no data-classification token standing in for native admission. The sidecar ran no Cargo, made
  no changes and is closed. These recommendations are not new production qualification.
- Native regression scope, independent consumption and logical key/origin correctness retain
  the separate qualifications above. No full-foundation completion or performance improvement
  is claimed. The overall Goal remains active.
- Separate ergonomics follow-up: an ordinary caller currently needs a frame/no-frame branch to
  progress Application close, calling the hidden `advance_project_open` after frame retirement.
  `editor/src/app.rs:600` does not drive ordinary authoring frames; `drive_editor_frame` is private
  and `submit_and_drive` requires an action batch/revision. Investigate an explicit ordinary
  phase-independent no-action progress entry before making another caller copy this choreography.
  The corrected Reference Game loop respects today's contract, not proof this API is ergonomic.

# Citations

- [Overlay design](../../../../architecture/prefab-overlay-design.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR 0038](../../../../architecture/adr/0038-scene-prefab-authoring-identity-and-provenance.md)
- [ADR 0058](../../../../architecture/adr/0058-stable-runtime-identity-and-entity-references.md)
- [ADR 0102](../../../../architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md)
- `crates/nara_scene/src/provenance.rs`, `prefab.rs`, `candidate.rs`, `spawn/publication.rs`
- `crates/nara_ui/src/persistent_key.rs`, `crates/nara_ui/tests/source_identity.rs`
- `src/scene_retention.rs`, `tests/fixtures/module-consumer/tests/prefab_sources.rs`
