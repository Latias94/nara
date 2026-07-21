---
type: "Decision"
title: "RGF-U23 runtime and Host independent decision matrix"
description: "Records the independent ADR 0084 Runtime verdict, ADR 0082 Host verdict, combined compatibility result, trust scope, and required follow-up evidence."
timestamp: 2026-07-21T11:27:29Z
record_id: "a5b3266847924dfc93667c72c8929550"
tags: ["rgf-u23", "runtime", "host", "compatibility", "adr"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "f7e5ee2"
verified_by: "Independent Runtime, Host, and compatibility reviewers"
---

# Decision

RGF-U23 reviewed ADR 0084 and ADR 0082 independently at repository revision `f7e5ee2`, then
reviewed the resulting pair without changing either independent result.

| Scope | Verdict | Product consequence |
|---|---|---|
| ADR 0084 executable Runtime | **Remain Proposed** | The thin owner around one `App` remains the leading design, but failed metrics prevent acceptance. |
| ADR 0082 outer Host | **Remain Proposed** | The concrete Host topology passes its own success metrics, but its Accepted-runtime dependency is not satisfied. |
| Combined topology | **Compatible bounded Trial** | Current already-compiled, Host-trusted code-first and RGF paths may continue using the pair for evidence. The pair is not accepted product authority. |
| U20 admission | **Blocked** | U20 requires both product authorities to be Accepted or replaced by compatible Accepted successors. |

Neither ADR is Rejected. The evidence supports their core ownership split, and the failed cases are
repairable implementation or proof gaps rather than an architectural contradiction.

# Trust Scope

This verdict covers only already-compiled code that the Host has independently trusted, including
the current code-first and reference-game paths. Opening, parsing, validating, or editing project
data does not authorize Cargo resolution, build scripts, proc macros, native packages, native
importers, or in-process Play. Broader activation remains governed by OQ-031 or an Accepted
successor whose Host-owned decision binds the executable identity outside project data.

# Context

ADR 0084 owns the candidate, publication, runtime, fault, drive, and finite-close state machine.
ADR 0082 owns the outer project/process authority, recipe, parent lifetime, and concrete product
entry. They are conceptually compatible because neither duplicates `App` as the sole World,
schedule, plugin, and time authority. Compatibility alone is insufficient: U23 requires every
success metric and the combined evidence to pass before either proposal becomes authority.

## Evidence Revisions

| Evidence | Exact revision | Role in this decision |
|---|---|---|
| RGF-U3 | `4709689d50e1e5b4af41d062f7c308ef5bd6f377` | Bounded settings and pre-mutation rejection |
| RGF-U4 | `b6537579b3e48b11f36dca94fa28eb61b8262a3e` | Pure composition and fresh plugin preparation |
| RGF-U5 corrected runtime | `ff2e02a9ea087e32a00d90cde3b9e883dbc20c68` | Candidate/runtime, fault, exact-step, and close core |
| RGF-U12 | `f341255559e201f32dbcb09888b16cd50fecdd85` | Immutable startup content and schema lineage |
| RGF-U26 | `a2d695d6c58b8f8f21bb33aaf18fa27e18661a79` | Manual raw-App counterfactual |
| RGF-U24 | `5ddbf186712b6c829dc0134a9f41a0ac8250fa3e` | Concrete Host, atomic publication, and reversal evidence |
| RGF-U6 | `db511a780ad04e73940b72e6a4c3f0a48dbec70d` | Authoritative headless game path |
| RGF-U13 final | `5bc321d41aba59072a1f97ccc0473f91e0b2c161` | Desktop product path and manual confirmation |
| RGF-U17 final | `0a87503b43e1d6abc8b23404789dafc1a7cfe22b` | Editor persistence, Play, safe-point edit, and retirement |
| RGF-U19 | `347015e8d9fd5d529b9dd5482ceaa02086f4615a` | Dynamic ADR governance validation |
| U23 review baseline | `f7e5ee283e06ff156224b0f11fcc1df0c31284a3` | Source and evidence state reviewed by all three reviewers |

## ADR 0084 Runtime Metrics

| Metric | Result | Evidence or blocking gap |
|---|---|---|
| Startup publication | Pass | `RuntimePublicationSlot` and Host phase fault tests cover stale, duplicate, late, occupied, and faulted publication cuts. |
| Ownership handoff | Pass | The sealed App and obligation ledger move once through candidate admission and publication; U24 proves reverse retirement. |
| App admission | Pass | Unstarted/sealed/raw-runner/obligation checks are covered by `tests/runtime_instance.rs` and `tests/runtime_driver_boundary.rs`. |
| Play execution | Pass | U17 removed the bare-World Play owner; `EditorProjectSession` owns a scheduled runtime. |
| Driver parity | Insufficient | Headless/desktop snapshot parity exists, but Editor has not run the same reference-game command stream through the full product Host. |
| Driver authority | Insufficient | The first-party Winit boundary is covered; the required renamed-dependency external Platform/Runner candidate is absent. |
| Exact step | Pass | One accepted paused step runs the complete fixed/gameplay transaction and returns to Paused. |
| Fault closure | Pass | System, command, gameplay lifecycle, required task, and required service failures reach the sticky runtime fault. |
| Runtime isolation | Insufficient | World, queue, time, task, and generation evidence exists; service, backend, identity, and overlapping-drive isolation is incomplete. |
| Finite close | Pass | Incomplete owners remain retryable and never report `Stopped`; bounded task custody is covered. |
| Stop-first workspace | Pass | Stop, restart, close, and failed start retain the live or failed owner until truthful retirement. |
| API authority | Pass | The runtime wrapper exposes no second plugin, schedule, system, or time configuration surface. |
| Early ownership value | Pass | U26/U24 show less ordinary glue while preserving the named ownership and fault cuts. |

Two implementation conflicts also prevent acceptance:

1. `RUNTIME_SCHEDULE_AUTHORITY` and `ACTIVE_RUNTIME_REPORTER` are process-global. A contended drive
   currently sticky-faults an otherwise healthy runtime with `ScheduleAuthority`. This is a
   temporary Bevy fallible-error-routing constraint, not a supported product invariant.
2. `RuntimePlan::schema_validation` retains one `ComponentRegistry`, while plugins construct a
   World-local registry. The Host compares only `CatalogFingerprint` but executes scene codecs
   through the plan registry. The fingerprint covers schema/tombstone data, not exact native
   bindings, codecs, or migrations, so the two values can disagree behaviorally.

## ADR 0082 Host Metrics

| Metric | Result | Evidence or scope |
|---|---|---|
| Pre-mutation project rejection | Pass | Manifest, plan, and content rejection precede Host/App/service creation. |
| Recipe coherence | Pass | Project lineage and schema fingerprint mismatches reject before candidate construction. |
| Fresh plugin preparation | Pass | Repeat starts preserve definitions while creating fresh plugin instance state. |
| Early topology value | Pass | U26/U24 prove the private Host closes named ownership gaps without a universal public Host. |
| Runtime delegation | Pass | One private start attempt owns candidate construction and atomically publishes only a runtime owner. |
| Parent lifetime | Pass | Plan, snapshot, task, surface/provider, and persistence parents outlive their child ownership. |
| Cross-host parity | Pass for Host merits | Headless, desktop, and Editor delegate to the same RuntimeInstance frame/fixed contract; ADR 0084 still owns the stronger same-command-stream metric. |
| Least privilege | Pass | Server composition excludes window, render, audio-device, editor, and raw-input sessions. |
| Embedded path | Pass | Code-first runtime operation requires neither a project file nor a universal Host object. |
| Admitted external authority parity | Pass in current scope | No replaceable exclusive external authority role is admitted: ADR 0013 leaves the reusable Runner shape to OQ-038, while ADR 0078/0094 keep replacement Render Host authority evidence-gated. This metric must rerun when a role is admitted. |

ADR 0082 still cannot be Accepted because its explicit admission rule requires a compatible
Accepted executable-runtime decision or successor. ADR 0084 remains Proposed. The trust limitation
above closes only U23's scope statement; it does not decide OQ-031.

## Combined Runtime/Host Scenarios

| Required scenario | Current result | Required product behavior |
|---|---|---|
| Sequential RuntimeInstances | Partial pass | A stopped owner may reconstruct a non-reused generation. The new generation must have fresh World, queue, task, time, service, backend, and runtime identity state; immutable content may be shared explicitly. |
| Overlapping RuntimeInstances | Fail for drive semantics | Independent owners may coexist. A second start for one occupied Host/publication slot rejects before ownership transfer. Unsupported concurrent driving must reject before a transaction and must not sticky-fault either healthy runtime merely because another runtime is executing. |
| Process-global authority contention | Fail | Process-global reporter contention is temporary plumbing, not product authority. Fault routing must become execution-scoped, or a concrete Host must serialize/reject a real singleton capability before publication without corrupting a runtime. |
| Fresh-runtime reconstruction | Partial pass | Existing World/time/queue/task/generation evidence must be extended to required services, backend sessions/caches, and runtime identity. `CloseIncomplete` continues to block replacement. |
| Plan/World registry divergence | Fail | Runtime behavior must have one authoritative frozen registry snapshot or an exact binding receipt. Same catalog plus different binding/codec/migration behavior must reject before the first scene mutation. |

The Proposed/Proposed pair is therefore internally coherent as a bounded Trial but incomplete as
product authority. It cannot unblock U20.

# Required Follow-up

1. Eliminate the plan/World duplicate registry behavior authority or add an exact immutable binding
   receipt, including an adversarial same-catalog/different-codec test.
2. Replace process-global reporter routing with execution-scoped routing, or express a real shared
   executor as a Host capability rejected before publication. A contended drive must not fault a
   healthy runtime.
3. Prove independent owner coexistence, alternating and contended drive behavior, and complete
   service/backend/identity reconstruction across generations.
4. Run one reference-game command stream through the real `HeadlessRun`, desktop Host, and Editor
   Host and compare the authoritative semantic snapshot.
5. Add the already-required renamed-dependency external Platform/Runner candidate without creating
   a universal Runner SPI.
6. Re-run the independent ADR 0084 review. Only an Accepted Runtime or successor can trigger the
   ADR 0082 dependency and combined-compatibility review needed before U20.

# Alternatives

## Accept both proposals from implementation volume

Rejected. Landed types and many passing tests do not override failed success metrics.

## Reject either proposal

Rejected. The Host/Runtime ownership split remains coherent and useful; current failures do not
show that either design cannot meet its goal.

## Shrink the existing metrics

Rejected. Removing external Runner, three-Host parity, isolation, or registry-authority evidence
would make acceptance describe less than the product contract already reviewed.

## Introduce a universal Host, service registry, factory, or Runner SPI

Rejected. No second concrete replacement consumer proves those public abstractions, and neither ADR
authorizes them as a shortcut to acceptance.

# Consequences

- Existing concrete Rust/code-first, headless, desktop, and Editor paths remain usable Trial
  implementations within the trust scope above.
- ADR 0082 and ADR 0084 remain Proposed in the catalogue and implementation ledger.
- Foundation documentation may describe the implemented Trial only when it explicitly says it is
  not current authority.
- U20 remains blocked until the missing code and evidence are completed and independent review
  accepts both required authorities or compatible successors.
- Future work must not preserve the process-global runtime drive lock or duplicate behavior
  registries merely because current tests encode them.

# Citations

- `docs/architecture/adr/0082-process-host-authority-and-runtime-construction-topology.md`
- `docs/architecture/adr/0084-executable-runtime-ownership-and-isolation.md`
- `docs/architecture/open-questions.md#oq-031-product-package-contribution-and-trust-topology`
- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u23-decide-runtime-and-host-proposals-independently-then-reconcile`
- `crates/nara_app/src/runtime.rs#RuntimeInstance`
- `src/project_host/composition.rs#SchemaValidationInput`
- `src/project_host/runtime.rs#ProjectHost`
- `tests/runtime_instance.rs`
- `tests/runtime_driver_boundary.rs`
- `tests/workspace_play_runtime.rs`
- `reference-game/tests/desktop_parity.rs`
