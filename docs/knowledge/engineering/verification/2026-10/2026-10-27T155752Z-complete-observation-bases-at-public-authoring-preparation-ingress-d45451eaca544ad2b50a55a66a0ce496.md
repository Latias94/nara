---
type: "Verification Evidence"
title: "Complete observation bases at public authoring preparation ingress"
description: "Qualifies full source/Schema observation checks in Workspace, Session, Application and Panel preparation without a second transaction owner."
timestamp: 2026-10-27T15:57:52Z
record_id: "d45451eaca544ad2b50a55a66a0ce496"
producer_id: "codex-root"
run_id: "observation-bound-preparation-2026-10-27"
related_plan: "docs/plans/2026-10-27-observation-bound-preparation-plan.md"
git_branch: "refactor/observation-bound-authoring-preparation"
supersedes: "1acb9412a2544e98b059654fe5ec2ca7"
verified_by: "01a17e0f-0b7f-7000-9d3b-af90afe15cbe"
---

# Verification

Successor to `32b650a`, under the complete active foundation Goal. The containing commit owns
the qualified implementation. This corrects an actual preparation ergonomics gap exposed by the
EnemyStats consumer; it does not select an extension SDK or complete the foundation program.

# Result

Workspace, Project Session, Application and Panel patch/property preparation and construction
begin/prepare now take `&EditorDocumentQueryBasis`. Their public ingress validates original
source incarnation, revision and exact Schema admission before interpreting intent. Session
retains lifecycle and specialized UI-domain gates. Workspace independently protects direct
consumers; Panel delegates without reproducing a manual validate-then-prepare protocol. The
source-local Scene/Prefab owner retains its revision-based draft/candidate contract, ordinary
inverse/history and commit-time checks. A fresh complete observation cannot authorize a stale
draft, whose own source/Schema basis is still checked before lowering.

Old argument signatures are replaced directly, with no overload, deprecated alias or reader.
Rejection remains typed `EditorEditPreparationError::QueryBasis` at Session/Application. Already
prepared candidates retain their normal consuming Workspace commit result. Shared query-basis
diagnostics move from Prefab declaration code into the authoring-query owner, and use accurate
operation-neutral text for edits, Revert and conversion. Existing diagnostic codes are retained
where their meaning is unchanged; the generic fallback names authoring basis refusal.

Real public Workspace regressions exercise Scene and Prefab across all four entries: matching
document/revision but missing or separately admitted equivalent Schema, foreign source
incarnation, missing document, stale revision and fresh-query/old-draft refusal. Preparation and
refusal preserve the complete authoring model. Independent real Session and Application
consumers exercise both source kinds and non-active documents, and compare original data,
history/navigation, unavailable-record preservation and file bytes. Existing late invalid
operation, retained candidate, Undo/Redo, Save/Reopen, domain and lifecycle workflows now use the
same complete-observation API.

The Snap planner, finite child responses and EnemyStats tool keep their original observations.
No result is repaired by capturing a fresh basis. EnemyStats begins its source draft immediately
after publication admission, letting that owner check the retained basis before request/output
processing, without a separate duplicate validation call. The concrete tool still checks its
code-publication/request identity and revalidates reviewed results before commit. Source
preparation does not assume responsibility for those separate execution identities.

ADRs 0047/0118 and current guides adopt this ingress contract. Prior immutable EnemyStats evidence
records the former gap; this successor implements it. No migration guide is needed pre-release.

# Evidence

Serial Rust `+1.95.0`, `CARGO_BUILD_JOBS=1`, shared root `target`. Independent workspaces explicitly
reuse that target. All writers stop before formatting; independent review has no write/build
authority. Actual results:

| Scope | Result |
| --- | --- |
| Tooling all-feature library, authoring queries and Prefab conversion | 83 passed / 1 pre-existing ignored; `76b03954-d5b5-48f9-85fa-143d0769e62c`; build 1m04s, tests 1.977 s |
| Root all-feature library | 575 passed / 3 pre-existing ignored; `7fa1241c-a000-405e-b763-44f9652f6a53`; build 1m17s, tests 164.392 s |
| Independent window-free property authoring and public data consumers | 11 passed / 0 skipped; `481516a4-5d28-489d-b49f-cb1c30483c77`; build 21.21 s, tests 1.367 s |
| Complete independent Panel consumers with child-process experiment | 51 passed / 3 pre-existing ignored; `05e21634-ac81-4515-be11-cefdb2ddac99`; final build 34.60 s, tests 11.585 s |
| Initial Editor library and affected integration cohort | 269 passed / 1 failed / 32 pre-existing ignored; `a458157b-5c47-4084-9684-079666eb1707`; build 1m54s, tests 45.507 s |
| Corrected Reference Game ordinary Open workflow | 1 passed / 21 excluded; `1ddca9d0-a3ca-4d49-8754-d2df3fc774e5`; build 8.58 s, tests 0.292 s |
| Final Editor library and affected integrations after that correction | 270 passed / 32 pre-existing ignored; `418e6750-beb5-499f-93c6-5f33af186908`; build 0.96 s, tests 45.115 s |
| Windows/DX12 critical native trajectories, selected explicitly | 3 passed / 269 excluded; `abb642b9-c9c3-4bf2-9aa0-ad67beb8c952`; build 0.96 s, tests 9.015 s |
| Root UI-domain refusal with no default features and `tooling,project-workflow` | 1 passed / 0 skipped; `498eeffe-975c-4286-8c83-5434022c70e3`; build 2m33s, tests 0.095 s |
| Actual C#/Lua runner after public API migration | 10 passed / 0 skipped; `d1aadd10-b7bb-4229-ab88-3a496622e93d`; Rust build 17.29 s, tests 4.117 s |
| Tooling/root/Editor all-feature library and test Clippy, correctness denied | Passed, 1m16s; existing warnings remain |
| Independent Scene/Panel all-targets Clippy, including child-process feature | Passed, 1m32s; correctness denied, existing warnings remain |
| Actual language fixture all-targets Clippy | Passed, 4.19 s; correctness denied, existing feature-dependent warnings remain |
| Root and both independent workspaces formatting checks | Passed |
| Changed-document local links, diff checks and engineering-memory validation | Passed; legacy memory advisories remain, derived views regenerated at integration |

The root all-feature command also names the UI-feature-off integration binary, but its cfg
excludes cases in that feature union. It is not feature-off evidence; that profile is tested
separately. Headless API and documentation checks alone are not native qualification.

The first independent Panel build found one missed test variable after replacing a revision-only
capture: final project-replacement polling still named `revision`. It now compares source
incarnation with `captured.revision`; the original disposal oracle is preserved. The same complete
consumer command is rerun rather than excluding that case or weakening its lifecycle condition.

The Editor cohort exposed stale assumptions in the ordinary Reference Game test. The actual
Scene capture uses owned asynchronous composition and source-validation tasks; one Application
drive does not prove a preview candidate is ready. The test now drives that public owner within
a finite deadline, checks the same document/revision and renderable Sprite, and physically closes
through the existing Application driver. It also expected a removed global omission diagnostic
with no production emitter. Current shared capture returns source-qualified typed omission facts
on the accepted candidate instead.

An initially over-specific replacement oracle incorrectly classified WaveSpawn as KnownUnbound;
actual facts distinguish it from EnemyRole. The final test checks all three actual Prefab root
anchors for EnemyRole/RecordUnavailable(KnownUnbound) and WaveSpawn/NativeBindingUnavailable.
Both partial readiness and separate native-preview availability retain their meanings. The
failure oracles are strengthened rather than converted into an omission-free success or a skip.
Intermediate diagnostic runs remained failures until this exact assertion passed; no production
compiler or preview behavior was relaxed to satisfy the test.

Hume independently reviewed the production boundaries and all caller migrations and found no
P1/P2. The review explicitly checked held result basis retention, new-basis/old-draft refusal,
independently public Workspace/Session checks and exact error changes. Locke and Erdos implemented
disjoint Editor and fixture migrations, then stopped and were closed. Later semantic changes
or verification failures require their affected follow-up rather than relying on this source
review alone. The frozen-source follow-up also approved the Reference Game correction, confirmed
the two reasons and three exact anchors, and found no production capture/classification/preview
relaxation. All implementation workers and the independent reviewer are closed.

Native evidence covers the existing hidden-window Windows/DX12 pause/redraw, input/Reopen/resize/
Step and view/device lifecycle trajectories selected by the qualification procedure. It does not
extend Linux/macOS support or qualify the future Nara-owned drawing runtime.

# Follow-up

Keep the full foundation Goal active. Runtime language/tool contribution admission, local
installation and update/enable/disable/remove, held asynchronous work, dependency cohorts,
Stop-first, Schema migration, compiled Editor mixed preview/Apply Changes and Nara-owned
custom-drawn UI remain separate production workflows. Complete query-based preparation does
not establish generalized nested discovery, complete R1/R2/R3 or release platform parity.

# Citations

- [Preparation plan](../../../../plans/2026-10-27-observation-bound-preparation-plan.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR 0047](../../../../architecture/adr/0047-editor-workspace-and-scene-document-state.md)
- [ADR 0118](../../../../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Session ingress](../../../../../src/project_host/runtime/editor/prepared_edit.rs)
- [Workspace preparation](../../../../../crates/nara_tooling/src/workspace/prepared_edit.rs)
- [Public Workspace regressions](../../../../../crates/nara_tooling/tests/authoring_queries.rs)
- [Real Session regressions](../../../../../tests/fixtures/module-consumer/tests/property_authoring.rs)
- [Real Application regressions](../../../../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/property_read.rs)
