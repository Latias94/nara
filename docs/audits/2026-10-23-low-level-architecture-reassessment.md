# Low-Level Architecture Reassessment

**Status:** source-grounded research; proposed corrections, not implementation evidence.
**Date:** 2026-10-23, Asia/Shanghai.
**Nara baseline:** `74cbe3ba5586944bf24c60c11c06f441ae001e6b`.
**Scope:** low-level ownership, derived state, temporal contracts, typed extension interfaces,
asset reconciliation and their upper-level consumers.

## Assessment

Retain Bevy ECS, one managed execution owner, domain-owned effects, and the separation of
persistent documents, runtime placement, spatial math and lifecycle ownership. Several current
interfaces nevertheless require redesign. The recurring failure is loss or fragmentation of a
fact that a lower owner already knows: type identity, resource custody, current intent,
publication validity, or the interval in which a derived value can be consumed.

The [original low-level review](2026-10-22-low-level-module-and-interface-review.md) records ten
defects and six interface debts. Their repetition across owners is evidence to inspect the
mechanism; it does not establish that every guard is unnecessary or that ECS itself is unsuitable.
The [redesign](../architecture/low-level-contract-redesign.md) records recommended replacements,
alternatives, upper-level impact and acceptance. Existing Accepted ADRs remain authoritative
until their affected clauses are explicitly reconciled during adoption.

## Evidence Discipline

- Re-read current source and actual consumers. A historical report's recommendation is not proof
  that its original API still exists; the source-domain associated settings/facet correction is
  already implemented and is not reopened here.
- Follow a failure to its missing fact and owner. Distinguish an omitted check from a state model
  that has nowhere to retain the required fact.
- Check counterevidence: existing guards, explicit trust exclusions, bounded admission windows and
  real Editor/Export/close requirements. Avoid broad claims from a hostile safe-Rust example.
- Compare pinned primary mechanisms, rather than attributing identical guarantees to Bevy/Godot.
  The [mechanism study](../research/2026-10-23-low-level-contract-mechanisms.md) owns this comparison.
- Mark source-only hypotheses and execution gaps. This pass runs no product test or benchmark;
  prior cached-artifact probes retain the limits recorded in the original review.

## Complete Disposition Of Existing Findings

IDs retain their original meaning and severity. The final column names the redesign section,
not a completed implementation.

| Finding | Missing or misplaced fact | Disposition | Design |
| --- | --- | --- | --- |
| L01: close registration drops rejected participant | Custody through every rejection branch | Repair the App transfer contract; do not make plugins recover dropped owners | D1 |
| L02: first identity adoption omits target observers | Actual target lifecycle admission before first insertion | Repair the existing validator and qualify the ordinary writer | D6 |
| L03: retained completion tokens survive raw hierarchy mutation | Revocation at the owner, observed by every consumer | Redesign completion access and repair all structural invalidation paths together | D2 |
| L04: immutable globals can move between entities | Association between derived value, entity, World and source baseline | Redesign derived-state validation; `immutable` alone is insufficient | D2 |
| L05: Image loses latest metadata recipe across attempts | Desired recipe independent of last-good pixels and current attempt | Correct the Image state model | D4 |
| L06: watcher loses directory/subtree scope | Invalidation coverage through transport | Preserve scope or conservatively request an owned rescan | D4 |
| L07: several guards share one native lock | One release owner for one acquired native lock state | Replace borrowed reentrant-looking lock API | D3 |
| L08: map-key copy precedes budget check | Admission before the allocation controlled by Nara | Move check into key visitor; retain encoded-input gate | D7 |
| L09: facade derives need hidden dependency spelling | Complete ordinary derive path through declared dependency | Qualify and complete facade-supported derives | D6 |
| L10: finite angles overflow interpolation | Numeric closure of admitted endpoints under interpolation | Use a sufficiently wide intermediate and preserve endpoint behavior | D7 |
| E01: codec erases `T` at public registration | Relationship between declared component and prepared value | Keep typed candidates until private registry erasure | D5 |
| E02: domains repeat physical-return flags | Executor-owned disposal completion for one task | Add narrow task retirement observation; migrate real consumers | D1 |
| E03: asset callers reconstruct store/revision checks | Captured identity of both stores and current publication baseline | One shared mutation admission for managed publication | D4 |
| E04: spatial helper specifies only `after` | Complete mutation/propagation/consumption/finalization interval | Define semantic consumer windows, including Extract | D2 |
| E05: identity callers extract their own World resource | Borrowing and reinsertion choreography | Small World-oriented identity writer, with detached use advanced | D6 |
| E06: schedule ordering mistaken for success ordering | Explicit successful-operation state | Specify detection granularity and gate domain effects on success | D8 |

## Repeated Mechanisms

### M1: A Value Is Mistaken For Its Continuing Authority

The current [opaque Global component](../../crates/nara_transform/src/lib.rs#L50) does not encode
which entity owns it. The [Transform fast path](../../crates/nara_transform/src/propagation.rs#L203)
observes source changes but not every replacement of its derived values. Meanwhile
[extraction](../../crates/nara_sprite_render/src/extract.rs#L65) reconstructs validity from retained
tokens instead of the hierarchy owner's full predicate.

These are related architecture problems, not three independent opportunities for local fallback.
The owner must define what is complete, how that fact is revoked, and which phase it describes.
A SystemParam can declare ECS access; it does not automatically prove semantic freshness or
prevent a source writer from running earlier in the same consumption interval.

Counterevidence: the hierarchy owner already has a stronger
[completion query](../../crates/nara_hierarchy/src/lib.rs#L80), and
[ADR 0100](../architecture/adr/0100-runtime-structural-hierarchy-and-completed-2d-transform-projection.md#L64)
already requires completion and raw-mutation invalidation. Retain that responsibility; remove
weaker consumer-side reconstructions.

### M2: Ownership Is Transferred Before The Failure Contract Can Preserve It

[App registration](../../crates/nara_app/src/lib.rs#L1449) weakens the ownership-returning
[ledger interface](../../crates/nara_app/src/runtime/close.rs#L102). Separately, the
[borrowed FileLock](../../crates/nara_fs/src/lock.rs#L27) permits several Rust release owners for
one native lock state. Both demonstrate that RAII-shaped code is insufficient when the Rust owner
does not correspond to the actual protocol owner.

Counterevidence: ordinary TaskPlugin and checkpoint declarations are valid, and the Editor's
forgotten lock guard deliberately retains an OS lock until capability closure. Do not relabel
these real paths as observed leaks. Replace the mismatched interface so the workaround is unnecessary.

### M3: Terminal Observation Is Confused With Physical Return

[TaskHandle](../../crates/nara_tasks/src/runtime.rs#L896) documents the distinction correctly.
However, Scene, Browser and document Open reproduce capture-drop flags because the executor's
single-task return fact is absent from its interface. This is a shared lower-level responsibility.
Pool shutdown cannot substitute for normal request-slot reuse while the engine remains running.

Counterevidence: the existing domain guards protect their flows. A new observation must not claim
that delivered results, external Arc clones, spawned threads or native effects have retired.

### M4: Supersession Has No Durable Owner For Desired State

[Image snapshot capture](../../crates/nara_image/src/import/publication.rs#L143) gets settings from
published pixels; [reload policy](../../crates/nara_image/src/reload.rs#L687) may reuse them for the
next source modification. Correct generation rejection cannot recover desired settings discarded
with an earlier attempt. The state model needs a distinct desired recipe or retained metadata
invalidity; it must never silently infer new intent from last-good output.

Counterevidence: slot, state, source identity and generation checks already protect publication
against many stale results. Those facts remain necessary after introducing desired state.

### M5: Public Interfaces Discard Static Relationships

[Codec registration](../../crates/nara_reflect/src/registry.rs#L1232) names a Rust component type
but accepts an erased candidate. The encoder also repeats selection from World. Asset mutation
accepts independent resources that the caller must pair. Identity mutation makes callers arrange
resource extraction. These interfaces can hide more mechanical coordination without hiding
stable schema IDs, async revisions or domain-specific policy.

Counterevidence: runtime codec binding already rejects mismatches. Earlier source settings/facet
work demonstrates that the project can move relationships into associated types without adding a
package kernel. Preserve that completed correction.

### M6: Ordering Is Used Where A State Transition Is Required

[Managed scheduling](../../crates/nara_app/src/lib.rs#L1901) detects errors after a Schedule,
while [Gameplay finalization](../../crates/nara_gameplay/src/lib.rs#L135) is ordered inside it.
An ordered successor is not a success continuation. This matters only where a consumer requires
success before publishing an acknowledgement or capture; ordinary system ordering remains valid.

Counterevidence: Accepted ADRs do not promise whole-schedule rollback or immediate abort. Repair
the domain's success contract rather than claiming an existing scheduler guarantee was violated.

## Additional Architecture Findings

### A01: Spatial Domain Boundaries Contradict The Accepted Model

**Classification:** P2, source-confirmed contract/implementation conflict; no new executed probe.

[ADR 0097](../architecture/adr/0097-future-capable-2d-3d-spatial-transform-model.md#L140) says a
structural parent outside the same Transform domain ends spatial inheritance. Current
[propagation](../../crates/nara_transform/src/propagation.rs#L303) instead rejects the complete
projection with `MissingParentTransform`. The
[parent-removal test](../../crates/nara_transform/src/lib.rs#L859) explicitly preserves that failure.
ADR 0100 requires continuous same-domain composition and rejects consumer-side fallbacks, but
does not explicitly select whole-projection failure for a valid non-spatial grouping parent.

This is a domain rule that must be decided once, not a renderer exception. Recommend the Accepted
0097 interpretation: a real non-spatial parent terminates inheritance and its spatial child is a
domain root. Do not skip to a more distant ancestor. Missing structural entities/cycles remain
errors. D2 records the decision, test replacement and effects on grouping, removal/re-addition and
snapshot publication. Reconcile the affected ADR wording before implementation.

### A02: Existing Spatial Consumers Already Need A Named Historical Cut

**Classification:** source-confirmed design pressure, not a newly reproduced defect.

The [Reference Game consumer chain](../../reference-game/src/lib.rs#L206) reads completed space,
then [spawns projectiles](../../reference-game/src/systems.rs#L1028) and
[retires entities](../../reference-game/src/systems.rs#L1308). An interface promising that every
read is current with the entire live World would invalidate legitimate later reads after these
operations. Conversely, retaining old data without naming the cut hides its temporal meaning.

This consumer selects phase-stamped immutable snapshots in D2. Remove the Global Component's
authority rather than perpetuating it under another validating wrapper. Snapshot identity does
not prove that all live rendering configuration is from that cut; Render retains its own extraction
responsibility. The representation and API changes require ADR 0100/0117 reconciliation and new
consumer/scale evidence. The recommendation is not a measured performance improvement.

### A03: Single-Entity Retirement Extracts Identity Before Admitting Its Effects

**Classification:** P2, source-confirmed admission-order defect; regression not executed.

[prepare_exact_entity_retirement](../../crates/nara_identity/src/domain.rs#L2075) enters
`resource_scope` before validating identity support topology. Bevy 0.20 removes the resource and
reinserts it through a guard, so resource Remove/Insert observers may run before the operation
returns `LifecycleConflict`. The preparation itself only needs a shared domain borrow.
The public [Scene single-entity retirement](../../crates/nara_scene/src/spawn.rs#L1746) reaches
this preparation without the corresponding prior support admission.

Counterevidence: the [whole-Scene retirement path](../../crates/nara_identity/src/domain.rs#L894)
does validate before entering scope. Existing Scene-spawn observer tests do not qualify this
single-entity route. This is distinct from L02's first marker adoption and makes E05 more than
caller syntax: unnecessary extraction can create the effects a rejected operation promises to avoid.

D6 removes extraction from read-only preparation. Test through the public Scene operation with
resource Remove/Insert observers, zero effects on rejection, unchanged indexes/entity and a still
installed domain. Keep genuinely necessary extraction behind effect admission rather than
forbidding resource scopes throughout the engine.

### A04: Running Statistics End Before Worker-Owned Disposal

**Classification:** P2, source-confirmed physical-work observation gap; gated regression needed.

[execute_pending](../../crates/nara_tasks/src/runtime.rs#L2690) removes the running-map entry before
dropping pending work and its last local control reference. If the caller already dropped its
handle, the last state reference may then destroy an untaken completed result on that worker;
its destructor can block while `running` is already zero and running age is absent.
[TaskState::drop](../../crates/nara_tasks/src/runtime.rs#L854) performs that terminal disposal.

[ADR 0052](../architecture/adr/0052-task-backpressure-cancellation-and-long-running-diagnostics.md#L70)
requires physical running count/age through release by the worker. The
[existing cancellation test](../../crates/nara_tasks/src/tests/execution.rs#L524) covers a still
executing closure, not the subsequent worker-owned disposal. D1 should move final accounting and
retirement observation after this disposal, while excluding a result transferred to another owner.
The observation state must not itself retain `TaskState<T>` and thereby postpone the disposal it
purports to observe.

Counterevidence: pool close still joins workers and therefore is not shown to return `Stopped`
while this destructor runs. This is a statistics/retirement-boundary defect, not an asserted
current premature native cleanup. Test a dropped handle with a gated output destructor, correct
running age until release and successful shutdown only after worker exit.

## Existing Adjacent Opportunities Remain Distinct

The [post-refactor review](2026-10-22-post-refactor-ownership-and-ergonomics-review.md) has six
separate follow-ups. They are preserved here so redesign does not silently lose them or turn
unmeasured costs into new defects.

| Existing item | Treatment in this redesign |
| --- | --- |
| O1: ordinary `run` has outer preparation Result plus terminal product outcome | Consider one ordinary success/exit conversion while retaining the full report; not a new outcome framework |
| O2: submitting thread may dispose rejected/coalesced captures | Measure real destructor cost; D1 retirement observation alone does not change disposal placement or bound latency |
| O3: per-root catalogue copies and repeated immutable input proof | Attribute clone/fingerprint cost before choosing operation-local borrowing; no global cache |
| O4: open entity picker builds full labels | Measure real hierarchy/label workload; frontend projection optimization only when material |
| O5: thread-affine service may need pollable close | Require an actual non-Send native owner before changing participant traits |
| O6: extension first Open and Save capture are synchronous | Measure real codec/capture cost before broadening owned background preparation |

## Research Limits And Adoption

This report is a bounded architecture reassessment, not exhaustive engine certification. Source
findings do not supply occurrence rates, GPU trajectories, fresh build qualification, latency
improvements, portable lock guarantees or a claim that all future engine domains fit these seams.

The worktree initially contained only the preexisting untracked `vendor/`; it is excluded from
research and delivery. Research lanes use fixed source baselines and no concurrent Cargo. The
proposal must pass independent correctness/feasibility review before its contracts are treated as
ready for implementation. Adoption then reconciles the affected ADRs, removes obsolete interfaces,
migrates complete consumers and executes the stated tests. A document review is not product acceptance.
