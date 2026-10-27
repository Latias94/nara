---
type: "Verification Evidence"
title: "Production finite source tools and exact physical close observations"
description: "Qualifies Session/Application-owned trusted child proposals, deletes the displaced tool bridge, and fixes concurrent result and physical-close observations."
timestamp: 2026-10-27T18:09:41Z
record_id: "fc2363973364434096d350ae4a8d762c"
producer_id: "codex-root"
run_id: "finite-source-tool-owner-2026-10-28"
related_plan: "docs/plans/2026-10-28-finite-source-tool-owner-plan.md"
git_branch: "refactor/concrete-tool-product-admission"
verified_by: "01a17e5a-6e39-78e3-b5be-dedc65d8f1c0"
supersedes: "d45451eaca544ad2b50a55a66a0ce496"
---

# Verification

Successor to `30f172e` under the complete active foundation Goal. The containing commit owns
this finite production operation and its qualification. This does not complete R1b, R3 or the
foundation program. Work remains local; no push or PR is authorized by this record.

# Result

`EditorProjectSession` now owns explicitly enabled finite trusted child tools: actual catalog
probe, activation, copied-input request, retained review, acceptance, replacement/disable and
physical retirement. `EditorApplication` delegates those operations without a drawing toolkit.
The child returns property proposals; document, target, operation, original query basis,
activation identity, newest request and prepared candidate remain Host-owned. Exact review and
acceptance use one existing source-owned construction, inverse and generated-entity mapping.
Ordinary source history and persistence retain their existing owners.

Actual descriptions are decoded using the selected owner's admitted predecessor when present,
then compared against its complete current catalog. This reuses real Schema lineage rather than
requiring generation-one-only definitions. Requests separately verify the selected component
owner/declaration and original document/Schema observation. Failed activation keeps the active
predecessor and its review; successful replacement and disable revoke old request authority.

The existing Tasks activity owns process acquisition, bounded stdin/stdout/stderr IO, direct-child
reaping and all acquired pipe-worker joins. Owner polling does not wait or join. Logical
cancellation revokes routing immediately while physical return remains pending. The finite
two-job allowance includes retirement. A trusted command path is mutable Host configuration,
not an immutable executable package, project-code permission, descendant containment or sandbox.
Wire limits and logical-value budgets are finite allowances, not measured RSS or latency claims.

Application enable/request/disable publish a post-operation frame immediately. Frame maintenance
includes pending tool work without independently requesting presentation; complete Tool View
comparison detects probe/request changes while another obsolete task still retires. Acceptance
returns its exact commit result and separate subsequent application update.

The C# EnemyStats tool is a separately built executable and metadata artifact. The old authored
`IEnemyStatsTool`, managed catalog/proposal bridge, DTOs, Rust bridge methods and fixture-owned
proposal/review/commit executor are deleted directly. Gameplay C#/Lua execution and code/VM
lifetime comparisons remain independent consumers. There is no compatibility alias, old reader,
new migration guide, universal backend or second transaction/history owner.

ADRs 0047/0118 adopt this narrow operation; 0120 remains Proposed for executable publications,
dependency cohorts and migration. Guides and the foundation plan distinguish the adopted Scene
native/data family from those remaining contracts. The future Editor destination remains Nara's
own custom-drawn UI.

# Independent Findings And Corrections

Reviewer `01a17e5a-6e39-78e3-b5be-dedc65d8f1c0` found two P2 observation defects:

- One global error slot lost one of concurrent probe/request failures. It is removed in favor of
  independent fixed `attempt_error` and `request_error` slots paired with their retained IDs.
  New operations clear only their own slot. A deterministic dual-return test retains both errors.
- A successful probe blocked by a dirty-document Close decision silently ended without activation
  or failure. Publication admission now participates in the attempt's result; `Closing` remains
  observable while the predecessor and its exact review survive. The real Close/Cancel workflow
  proves that the retained predecessor review remains acceptable afterward.

Repeated actual C# execution then found a real same-poll Close observation race: the initial
tool poll saw Pending, Tasks completed inside `shutdown_tasks`, and Close returned Complete with
a stale retiring handle still visible. The Complete branch now reconciles tool state before its
receipt. The existing test-only Close hook moved from Prefab's private state to Session, its
actual owner; both consumers share that one hook and the old field is removed.

`physical_return_during_tasks_close_is_reconciled_before_the_complete_receipt` fixes the exact
interleaving: the hook returns the actual Tasks owner after the first tool poll, confirms its
return observer is Returned while tool state is still retiring, then the outer call must return
Complete and already expose no pending work. With only the final reconciliation removed, run
`89335219-7c41-4fe4-98aa-a2f6082b4056` fails at `!retiring`. The line was restored immediately;
the final complete root cohort passes. No extra poll or weakened assertion masks the race.

Fixture corrections preserve their assertions: description tests now send a real persistent
catalog envelope; Name uses its actual scalar storage; entity queries compare canonical ID order
while saved roots independently prove authored placement; direct source commits publish a frame
before history observations. Earlier 9/11 and 10/11 actual language runs are not final passing
evidence. The latter exposed the production Close race above.

# Evidence

Serial Rust `+1.95.0`, `CARGO_BUILD_JOBS=1`, shared root `target`, Windows 11. Independent
workspaces explicitly reuse that target. All writers froze before formatting. Actual .NET
artifacts use hostfxr `10.0.11`; no new SDK or runtime dependency enters the core/default path.

| Scope | Actual result |
| --- | --- |
| Tooling all-feature library | 69 passed, 0 skipped; `77da77bf-ba90-4a49-9baa-3b0cdb9ba97c` |
| Final root all-feature library, including 16 source-tool tests and existing Prefab Close tests | 591 passed, 3 pre-existing ignored; `59400ea5-6493-4c7c-966f-079c6b6599ea`; tests 196.026 s |
| Final Editor all-feature library | 241 passed, 33 ignored; `b7281812-79f6-4ef6-8cd9-9bab268a0f17`; tests 39.944 s |
| Actual rebuilt C#/Lua independent consumers | 11 passed, 0 skipped; `5ec031fc-8fdf-457d-a8c4-d7f64ed75c19`; tests 16.584 s |
| Explicitly selected actual C# Application consumer | 1 passed, 273 excluded; `fd8f1b89-5924-4b4f-a272-755d9453e63c`; tests 0.573 s |
| Three Windows/DX12 critical native trajectories | 3 passed, 271 excluded; `8993ce4d-b957-4d12-8e8e-6b98d12960d7`; tests 8.956 s |
| Root no-default library with only `tooling` | check passed |
| Root/Editor all-feature, all-target correctness Clippy | passed; existing warnings remain; Editor repeated after removing one redundant annotation |
| Independent language workspace all-target correctness Clippy | passed; existing dependency warnings remain |

The actual Session consumer covers JSON/RON Attach/Edit/Spawn, preview without mutation, exact
inverse/created ID, one changed transaction per Undo/Redo, Save, physical Close/fresh Open and
public Rust materialization/read of saved owner-tagged data. It rejects invalid final writes,
wrong owner/declaration or Schema admission, stale basis/request/publication, disabled execution,
malformed/oversized output, nonzero exit, timeout and incompatible candidate while preserving
the predecessor. The Application consumer uses the same actual C# command for Spawn review,
receipt/ID/inverse, property reads, Undo/Redo and disabled-review refusal. Ordinary Editor tests
do not count this explicitly ignored artifact-dependent case as executed.

Deterministic before-spawn/before-IO-return gates separately prove held physical custody,
nonblocking owner polls, bounded jobs, stale-result fencing and CloseIncomplete until return.
The C# child's short hold alone is not deterministic physical-custody evidence. Native runs use
the three hidden-window trajectories named in the qualification procedure, with actual DX12
submission/presentation and retirement. They do not establish monitor scanout, another platform,
new tool widgets or a custom-drawn frontend. Formatting, runner syntax and document-link/memory
checks are integration checks; they do not expand those product claims.

# Follow-up

Read-only researcher `01a17e85-8252-7511-a426-cc937541642d` traced the next R1b/R3b gap to the
existing Inspector's private fixed field renderer and Dear-only panel factory. The next selected
consumer is one EnemyStats `health` inline customization plus a small panel sharing the current
Session request/review. Qualify exact owner/component/field matching, frontend-owned drafts,
old-route refusal and fallback to the default editor while metadata remains admitted. Use only
the finite presentation/operation semantics that consumer needs. Godot's pinned Inspector plugin,
default editor and separate dock paths inform the concept; no Object/Control model, recursive
widget tree, global plugin manager or new transport is selected.

This precedes local install/update because packaging cannot prove inline author ergonomics.
Executable gameplay publications remain a separate R3 owner/lifetime contract. The current
mutable command path does not prove immutable package identity, dependency resolution or
installation receipts. Reconcile ADRs 0047/0118 when the concrete facet is designed; do not
promote Proposed 0120 or claim Nara UI implementation from this finite tool slice. Foundation
Goal remains active, with the full existing objective unchanged.

# Citations

- [Finite source-tool owner plan](../../../../plans/2026-10-28-finite-source-tool-owner-plan.md)
- [Finite source-tool guide](../../../../guides/finite-source-tools.md)
- [Foundation rebaseline plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Native Editor qualification](../../../../verification/native-editor-qualification.md)
- [Predecessor preparation qualification](2026-10-27T155752Z-complete-observation-bases-at-public-authoring-preparation-ingress-d45451eaca544ad2b50a55a66a0ce496.md)
