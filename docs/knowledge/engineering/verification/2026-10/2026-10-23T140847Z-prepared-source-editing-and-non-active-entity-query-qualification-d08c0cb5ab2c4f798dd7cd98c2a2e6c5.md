---
type: "Verification Evidence"
title: "Prepared source editing and non-active entity query qualification"
description: "Owner-bound candidates, replayable inverses, public Scene/Prefab workflow, exact query custody and per-kind Host limitation."
timestamp: 2026-10-23T14:08:47Z
record_id: "d08c0cb5ab2c4f798dd7cd98c2a2e6c5"
producer_id: "codex-foundation-integration"
run_id: "foundation-r2-prepared-20261023"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "2c6021c1f612c706d7524862bf3760fbb9e40547"
verified_by: "codex-root; owner review 01a1691b-26ef-7e82-a874-73357b0a1823; integration review 01a1691b-5526-71c1-a238-5e6bcb6a144b; final review 01a1692f-f074-7262-ab9e-116c5673fd8a"
---

# Verification

Qualified implementation: `2c6021c1f612c706d7524862bf3760fbb9e40547`. The following closure updates are documentation-only.

This record covers the first concrete R2 source-owner slice on top of `4f3b485`.
It does not close delivery wave 1 or the complete foundation rebaseline. Source includes
the prepared Scene/Prefab owners, mixed structural/field preparation, workspace query and
commit adapters, public Application submissions and the independent panel consumer.
The pre-existing untracked `vendor/dear-imgui-wgpu` files are outside this change.

# Result

The implementation retains one unpublished candidate and its owner-generated inverse.
Direct apply delegates to the same preparation/commit path; neither tools nor frontends
provide a second inverse or validation authority. Commit requires the original session
incarnation, revision and exact Schema admission, including after Undo or Reopen.

The independent consumer targets a non-active Scene and a non-active Prefab in separate
iterations, creates two entities with two components each and reciprocal source-local
references, observes exact commit/rejection outcomes, and uses one Undo/Redo followed by
Save and Reopen. It checks preservation of unavailable component bytes and unchanged
active-document selection. This is explicit-ID construction, not an allocation API.

Entity queries enumerate direct source IDs without constructing an Inspector or changing
navigation. Continuations retain workspace, kind, source incarnation, revision and Schema
basis. Scan and output limits bound records per query, not identifier bytes, a whole batch
or the subsequent Application frame drive.

# Evidence

The serial integration run first found missing `ScenePatchDocument` and
`ScenePatchOperation` imports in the existing Editor UI-domain rejection regression;
the imports were corrected before rerunning. A compile failure is not passing evidence.

Owner cases exercise unpublished preview/cancel, foreign/stale/reopened owner rejection,
last-operation failure, reciprocal references, unavailable-record retention, bounded
construction, complete two-way history admission and refusal of a field inverse that
cannot be replayed under the existing value limits. Workspace/Application cases cover
query custody and closing/retired submission rejection. Test results are recorded below
only after the corresponding commands complete.

## Independent Review Corrections

- The owner review identified a structural inverse admitted by custom 128 MiB history limits
  but refused by the actual default 64 MiB replay limit. A 65 MiB data record reproduces it
  without a large wire fixture; the fixture retains matching widened source/shape limits.
  Scene/Prefab component and subtree removal now refuse before publication when their complete
  inverse cannot replay. This is not merely a retained-history byte check.
- Premature full history accounting changed cheap invalid-format/count rejection into a history
  rejection after scanning the payload. The two new cases fail on the starting source in run
  `33fe9403-7982-4e38-a6b0-1384c3d8e9c5`. Remove that premature pass; keep bounded value and complete
  forward/inverse input admission ahead of publication. Shared logical-cost traversal uses
  borrowed iterator continuations instead of pushing an entire wide container.
- The integration review found ordinary raw Session mutations still admitted after physical close
  while prepared commits and Save already refused. All six Scene/Prefab mutation, Undo and Redo
  cases fail in run `cd3644b0-5bb0-4b66-96b8-5bdb2a94712a`. The common mutation ingress now enforces
  the existing Session fence independently of Application filtering.
- The external test previously relied on best-effort Drop cleanup. Its successful path now
  explicitly asserts physical Application close; an elapsed cleanup deadline cannot pass silently.
- The final bounded review found a second publication route through a pre-close UI gesture.
  The common-workspace and trusted UI gesture cases both reproduce the defect in run
  `e2357220-0e9f-4e7c-a3db-503045d2e643`. The fence now also lives at the common workspace ingress,
  while gesture admission rejects before consuming its draft. Explicit Cancel remains usable;
  it is not rejected as new authoring work.
- Tracing all source publication paths also reproduces late Runtime Edit and Apply Changes
  admission in a genuinely Running Runtime (run `6dbbad8c-3e4a-43d9-8328-4c04fcedbc68`, two failures).
  A third case first applies a real live Sprite edit, queues Apply Changes, closes, then drives:
  the original source incorrectly changes after close in run
  `4b8a814c-2be1-4d6b-b088-31039a9756b7`. Both new requests now refuse as Busy, queued pure-data
  export returns a preserved Busy result before publication, and its eventual apply uses the
  same workspace ingress. Stop remains admitted and is explicitly completed by these tests.

The final cut has eleven real Session-close cases rather than relying on the Application filter
or testing only one source mutation route. Already-admitted filesystem persistence keeps its
existing physical completion/custody behavior; the new fence does not preempt its OS call.

The five-lens simplification review retains the proof tests and separate nominal owners. It removes
duplicate Schema custody and field-operation predicates, retains the admitted two-way cost in the
move-only candidate rather than rescanning at commit, uses exact removed records instead of an
all-survivors scan for each newly created component, and short-circuits pure field Prefab checks
before domain scanning. Limit, incarnation, Schema and history publication checks remain.
These are structural work removals, not a measured edit-latency or throughput claim.

The test-value review correctly limits the error-order regression: format, count and field-value
failures share a diagnostic code. That public test demonstrates the observed regression against
the old history preflight, not independent timing/visit attribution for every guard. Private
logical-cost tests separately count visited nodes to prove early wide-container and scalar-byte
refusal. Do not present the public diagnostic assertion as a preparation-cost benchmark.

## Executed Cohorts

All Cargo commands are serial on Windows/MSVC Rust 1.95.0 with `CARGO_BUILD_JOBS=1`, reusing
the existing target. The initial Scene/Tooling/Editor cohort passes 547 cases (26 excluded ignored
cases), including a real generated-starter Check/Play/Stop/exit workflow, in run
`ae946465-2054-4f19-a196-03df088e9663`. This precedes the review corrections and is baseline evidence,
not final-source qualification.

The corrected root/Reflect/Scene/Tooling library cohort passes 642 cases with one ignored case in
run `a2cbe813-dfa6-44c9-b5ef-dc37d9260835`. The standalone prepared-authoring integration cohort
passes all 10 cases in run `183b036d-b4ad-49d5-bb31-18b5a0a8c53c`, including field-before-removal,
references to a later-created entity and deleting a target while repairing its inbound reference.
Both were executed before the final behavior-preserving simplification; subsequent qualified
source and applicable reruns are recorded with their own IDs below.

After simplification, the same 642-case library cohort passes in run
`7f557f72-b996-421b-bedd-6862b7ecff0e`, and the 10 prepared-owner cases pass in run
`b9974ed7-9d3b-4129-97c2-dbcbc2788862`. All 30 independent panel-consumer cases pass in run
`ae48ef89-33d7-402d-b28c-9513da885ff3` (two ignored cases are not acceptance). This includes
extension source/alias/lifetime regressions as well as the new non-active Scene/Prefab workflow.

The complete ordinary Editor cohort passes 223 cases with 25 ignored cases in run
`c859cdd0-190b-499f-87bb-9b388655abaa`, including a fresh generated-starter Check/Play/Stop/exit
execution. The subsequent selected Application/generic-authoring/native cohort passes 33 cases
in run `a33fe2e4-69a7-43e6-90f5-aec11f366bf6`: 29 ordinary integration cases and all four selected
actual Windows/DX12 trajectories (ordinary Scene/Prefab widgets, resume cadence, Game input/
Reopen/Step, and independent view lifecycle). These precede the final shared-close correction;
that correction receives its own applicable reruns rather than inheriting these results.

### Final Qualified Source

After the complete close cut, all 826 library tests across root, Reflect, Scene, Tooling and
Editor pass in run `5709c1d5-38b2-43f9-bf7d-b910cf320f5d`; 23 ignored cases are not acceptance.
All eleven Session-close regressions are in that executed cohort. The unchanged backend-free
prepared-owner module retains the ten-case qualification above.

All 30 independent consumers pass again in run `b1a9f3ba-13b0-48eb-883f-52ceb76af348` (two ignored).
The final 33-case Application/generic-authoring/native run
`73a083b0-492c-4db2-96d6-48ecb52fd395` passes all selected cases, including the four actual
Windows/DX12 trajectories. Its remaining 197 filtered/ignored cases are not a whole-native-suite
claim. These final commands use the same source; the repeated compact-log runs retain exact run
IDs after the earlier combined terminal output was truncated.

`cargo +1.95.0 fmt --all -- --check` passes. The excluded independent prepared-authoring test is
formatted explicitly with Rust 1.95.0 rustfmt. Source qualification is Windows/MSVC/DX12 only;
no Linux/macOS native support, OS IME delivery, accessibility, release export, latency SLA or
whole-foundation completion follows from it. Existing unrelated dead-code warnings remain.

Focused library Clippy for `nara_reflect`, `nara_scene` and `nara_tooling` also passes with
`--locked -- -D clippy::correctness`. This is a correctness-lint gate, not warning-free Clippy:
large error/enum layouts and other non-correctness suggestions remain, including warnings in
dependencies. No automatic Clippy fixes or unrelated cleanup are applied.

### Review Scope And Limits

The final bounded peer recheck finds no remaining actionable issue in the closed-ingress cut:
common commands, pre-consumption gestures, Runtime Edit, new/queued Apply Changes, retained
request outcomes and Stop/Cancel cleanup. It independently checks the final source and eleven
test cases; execution evidence remains the root's serial results, not reviewer assertions.

Code review: skipped (ce-code-review unavailable) -- the dedicated CE full automation terminated
without a completed full receipt; its controller and subprocesses were stopped. This phrase
describes that dedicated gate, not the separate completed Codex owner, integration, five-lens
and final bounded source reviews recorded here. Do not claim a CE full/merge/validator gate or
a new whole-Play-state-machine audit. The root also reviews the actual final diff before the
precise local commits; no remote publication is part of this unit.

## Mechanism Comparison

The pinned Bevy `CommandQueue` (`repo-ref/bevy/crates/bevy_ecs/src/world/command_queue.rs`)
stores deferred executable World commands. Its queue is not an admitted persistent
document candidate, does not supply an inverse and does not establish all-or-nothing
document publication. Reusing it as the authoring transaction would move those
responsibilities into callers rather than remove them.

The pinned Godot Editor undo owner
(`repo-ref/godot/editor/editor_undo_redo_manager.h`, `create_action_for_history`,
`add_do_methodp`, `add_undo_methodp`, `commit_action`) groups caller-supplied operations
under a history owner. Nara retains that useful owner/history separation while deriving
its data inverse from the validated source candidate. This comparison does not claim
Godot callbacks have Nara's data-publication guarantees, or that either upstream should
adopt Nara's representation.

# Follow-up

The integrated Host still has one Scene persistence lane and one Prefab lane. A second
source of the same kind replaces its predecessor through existing dirty-close and
physical-retirement custody. The consumer does not claim arbitrary same-kind multi-open;
R5c must introduce per-document file custody before that claim is made. Keeping a tab
without its file Host would not solve the ownership problem.

The raw `advance_close` cut closes authoring admission and its existing filesystem/task custody;
it is not a new automatic retirement protocol for the separately retained trusted Play Runtime.
The new running-Runtime cases explicitly retain and complete Stop afterward. The integrated
Application keeps its existing ordered Play retirement and source close. Composite native close
and activity-scope design remain R4 work, not implied by these admission tests or by a method name.

R2 still owes transaction-local allocation/reference mapping, durable nested property
addressing, full field queries, prepared-edit panel ergonomics and measured preparation/
commit cost comparison. The panel in this consumer qualifies read-only query/result
routing; it is not an inline Inspector or prepared-edit panel qualification. No dynamic
tool SDK, cross-document transaction, multi-file atomic Save, continuous gesture or new
platform support is implied.

The [per-document custody design](../../../../architecture/editor-document-persistence-custody-design.md)
records the separate read-only next-unit review: concrete maps, exact source/Host observation,
operation-bound receipts, independent preview/startup and multi-source refresh obligations.
It does not claim an implemented lane replacement or qualify the remaining R5c lifetime work.

# Citations

- [Rebaseline plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Editor panel and direct submission guide](../../../../guides/editor-panels.md)
- [ADR 0026](../../../../architecture/adr/0026-editor-command-patch-and-undo-model.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
