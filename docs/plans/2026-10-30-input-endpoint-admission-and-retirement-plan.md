# Input Endpoint Admission And Retirement Plan

Date: 2026-10-30. Status: proposed design and execution outline, ready for review.
Implementation and qualification: not started by this document.
Parent: [foundation R10](2026-10-23-foundation-architecture-rebaseline-plan.md#r10-motion-time-repeatability-and-concrete-domain-seams).
Source baseline: working tree based on `75e1284ed28bc5a96686609f7dae8805d1924788`.
Evidence and alternatives: [endpoint admission review](../research/2026-10-30-input-endpoint-admission-and-retirement-review.md).

## Problem And Complete Consumer

The current event and presentation-route limits do not bound persistent endpoints or held
contributions across drained frames. Balanced press/release churn retains historical empty
keyboard/mouse rows. Distinct live pointers can retain arbitrarily many members relative to those
limits while producing one aggregate action. These are separate storage and live-state cases.

The owning correction must also fix the confirmed
[platform-observed failure path](../../crates/nara_input/src/timeline.rs#L1031): an oversized
focus-loss/retirement batch can clear ingress state without admitting terminal events, leaving
button and semantic projections held. Adding a count check or empty-row pruning alone is not
the complete correction.

Prove a public Input-to-Gameplay workflow with small explicit capacities: install the existing
global key/mouse map and action-to-command bindings, admit keyboard/pointer and synthetic input,
drive normal, zero-tick, paused and exact-step execution, and observe actual gameplay effects.
Exercise admission refusal, focus/capture transfer, endpoint retirement/reconnect and context or
mapping cancellation. Drain and reuse capacity through many more historical identities than the
event limit without changing the original global definition owner.

## Scope And Authority

This plan proposes an Input-owned lifecycle/member replacement. It is not Accepted ADR adoption,
an implementation report or a qualification record. The initial red counterexamples are source
traces and have not been executed. Before production changes, compare the candidates with the
small-capacity proofs below, then reconcile the selected capacity/lifetime contract with
[ADR 0041](../architecture/adr/0041-input-routing-actions-text-focus-and-accessibility.md).
Keep existing aggregation, press ownership, context/mapping cancellation and fresh-press rules.

The parent is completing the separate
[processed Input unit](2026-10-30-processed-input-startup-and-process-proof-plan.md). This document
does not alter its ownership, test results, release scope or completion criteria. Immutable
definition/source/processed acquisition custody remains intact; ingress storage has its own cost.
Future seat-scoped maps, gamepad/axis held semantics, analog work and raw/semantic replay remain
separate. Existing gamepad UI-event consumers require migration only where shared APIs change.

## Recommended Design For Review

The generation model below is a candidate for comparison, not an adopted endpoint public API.

1. **One ingress owner.** Center admission and mutable lifecycle state in `InputTimeline`.
   Represent finite window/seat records, pointer children and explicitly admitted synthetic
   endpoints with checked connection generations. Focus, position, routes and raw-ID indexes
   cannot allocate uncharged lifecycle rows. Keep UI presentation generation distinct from the
   physical connection generation.
2. **One finite member pool.** Reserve a member for each accepted press, retaining input,
   endpoint incarnation, press provenance and UI/gameplay ownership. Count queued, held and
   terminal-pending members until every relevant internal consumer finishes. A released ingress
   member still referenced by paused staging or projection is not reclaimed capacity.
3. **Bounded projections.** Make `ButtonInput` an observation projection and route synthetic
   mutations through the same ingress owner. Replace independently growing semantic contribution
   sets with references to admitted members. Remove obsolete mutable held stores, mutation ports
   and parallel synthetic resolution paths as the replacement makes them unnecessary. Migrate
   consumers directly to the selected owner API, without a compatibility forwarding layer or a
   second mutable owner. Raw-ID entry points can remain if selected as the actual bounded design.
4. **Retirement before reuse.** Active, prepared-only and retiring records all consume endpoint
   capacity. Prepared references pin their actual record without double counting it. Finish
   internal terminal observations before recycling; remove old indexes and advance generation
   without wrapping. Reject operations carrying a handle captured for the retired incarnation.
   The adapter must capture and pass that admitted handle; attaching the current handle to a late
   raw OS event cannot recover its origin. A route unbind preserves physical holds.
5. **Reserve terminal obligations.** Each admitted hold reserves its release, each applicable
   pointer record its leave, and each lifecycle its required terminal control event. Ordinary
   ingress reserves its own events and the complete combined terminal obligation for any supported
   scope or union. Compare a dedicated terminal pool against shared event credits before choosing
   the storage contract. Both need sequence headroom, idempotent publication and retention through
   all relevant cursors, paused staging and surviving projection queues. Define checked
   generation/route retirement before accepting a lifetime that promises eventual termination.

Use the review's simpler finite raw-ID admission/pruning candidate as a design control with the
same capacity, cancellation and synthetic requirements. The raw-ID control is viable if it proves
an adapter event-drain/nonreuse rule covering both internal and externally queued work. Test a
normalized operation queued with handle A, retirement of A, reuse as B and delivery of A: a handle
model must reject A without changing B. Test separately an event carrying only a reused raw label;
neither model may claim to identify its old origin without an actual adapter guarantee. If the
raw-ID control satisfies the workflow with less complexity, it may be selected. This comparison
is bounded to the current owners; it does not require a generic registry or budget framework.

## Retained-State And Failure Invariants

Let E bound all lifecycle slots, M all member slots and K the compiled fanout ceiling, currently
[32](../../crates/nara_input/src/definition.rs#L12). Required invariants are:

```text
active + prepared-only + retiring lifecycle slots <= E
queued + held + terminal-pending member slots <= M
retained semantic contribution links <= M * K
next sequence + reserved terminal-sequence headroom <= u64::MAX
```

For a supported cancellation scope S, prove the sum of all held releases, required pointer-left
records and distinct remaining terminal control records. Count actual outputs once, including
overlapping/repeated cancellation. The reserve must cover a union of scopes, not just each
component or endpoint in isolation. With shared credits, retained ordinary/terminal records plus
unspent terminal credits must fit the shared event limit. With a dedicated terminal pool, retained
terminal records plus outstanding terminal obligations must fit that pool; ordinary events have
their own finite limit and all consumers need a correct common sequence/ordered merge.

Publishing a terminal record spends its reservation once but retains storage until every relevant
consumer completes. Include projection, frame observation, UI, action staging/resolution, paused
lifetimes and exact-step paths. Physical ingress clearing or advancing one cursor does not release
member, endpoint, event or projection capacity still referenced elsewhere. A dedicated pool does
not supply extra sequence headroom.

Use disjoint slot states, checked sizing and validated cross-limit settings. Select E/M defaults
only after proving that their concrete terminal shapes fit; this plan supplies no measured
defaults. Include seats, positions, route reservations, free lists, indexes, staging metadata,
retirement pins, cancellation scratch space and internal transient outputs in the storage design.
Prefer fixed-capacity pools or another representation with an explicit maximum allocated capacity.
Logical `.len()` checks do not bound vectors that retain allocations from arbitrary prior growth.
Caller-owned observation copies are outside the live owner's accounting.

An ordinary rejection must leave membership, endpoint topology, ownership, sequence and observable
edges unchanged. Projection and semantic insertion must not discover a missing capacity after a
press was accepted. Do not evict valid holds to admit a new one. Coordinated retirement can reject
before commit; an already observed platform loss must revoke routing authority and terminate
accepted members using reserved capacity, preserving the original press ownership. Repeated loss,
retirement or late release cannot duplicate terminal outcomes. Rejected presses cannot produce
releases. Context/mapping cancellation remains semantic, with its actual cause and no invented
physical provenance.

## Assumptions And Deferred Choices

The design assumes complete physical/synthetic admission through one owner, explicit accounting
for every internal reference and a real adapter origin/drain guarantee. Existing raw-ID delivery
does not establish admitted-handle capture. Prove these assumptions in the selected workflow.
Defer raw-ID versus handle identity, adapter queue/reuse policy, handle ownership and generation
representation, dedicated terminal pool versus credits, exact scope/merge accounting, storage
layout, numeric limits and the final public API surface. The migration inventory is conditional;
no entire endpoint API or terminal storage mechanism is predeclared Accepted by this plan.

## Review And Bounded Proof Before Production

1. Add focused regressions for historical empty rows, live growth across drained frames and the
   platform-observed clear-without-terminal case. Record actual pre-change results; this plan
   supplies no red run. Compare raw-ID and generation candidates against the same adapter workflow.
2. Execute small-capacity owner proofs for dedicated terminal storage and shared credits before
   choosing an Accepted contract. A mixed window scope with one held key, two held mouse buttons
   on two positioned pointers needs six terminal records: three releases, two leaves and one
   lifecycle event. Include retained ordinary/staged prefixes, paused lagging consumers, overlapping
   cancellations, multi-endpoint unions, exact/one-short admission and sequence near exhaustion.
   Reject a new obligation before acceptance if its eventual combined cancellation cannot fit.
   Select the identity/storage model from these results and reconcile ADR 0041 for the chosen
   contract. These proofs do not freeze a whole public API or qualify native event-origin behavior.
3. Replace the ingress owner with finite configuration and the chosen identity/admission surface.
   Implement member storage, prepared/retiring accounting, chosen terminal storage, sequence
   reservation and any selected generations together. Preserve checked overflow and truthful
   refusal. Delete historical indexes and obsolete mutable state instead of forwarding into it.
4. Migrate button/semantic projections and synthetic input to the single owner. Preserve staged
   mapping/context identity and original acquisition/state custody. Derive the M * K link bound
   and maximum internal retained/transient capacities from actual data structures.
5. Migrate the review's [public consumers](../research/2026-10-30-input-endpoint-admission-and-retirement-review.md#public-migration-inventory):
   Winit, Editor Game View, UI prepared presentation/lifecycle, source and processed input fixtures,
   synthetic test writers and default driver users. Verify the default paths do not reconstruct
   unadmitted identities. Keep gamepad UI navigation working without expanding held/axis claims.
6. Execute the acceptance below, review the complete owner and public API diff, and record actual
   results and limits. Update affected contracts, guides and implementation evidence only for the
   adopted and executed successor. Do not infer completion from this plan or from predecessor
   processed/source/native evidence.

## Acceptance

Use six focused test families. Internal tests may inspect slot and allocation counters; the
complete consumer must use public admission/driver/control APIs rather than calling semantic
internals. Capture outcomes before frame cleanup, using finite expected histories or counters
so the test observer does not manufacture the growth being measured.

| Test family | Required behavior and evidence | Concrete starting location |
| --- | --- | --- |
| A1. Historical churn and allocation | With small E/M, connect, press/release, retire and drain repeatedly across more than 4,096 historical identities. Empty secondary indexes disappear; only intentionally connected idle records retain capacity. Endpoint/member/link pools and retained container capacities remain within admitted maxima, and retired capacity becomes reusable. Include keyboard and pointer paths. | [Timeline owner](../../crates/nara_input/src/timeline.rs#L567), [runtime projection tests](../../crates/nara_input/tests/timeline_runtime.rs#L139) |
| A2. Live saturation after log drain | Add distinct held pointers one frame at a time until endpoint or member capacity is full, with event logs empty between frames. Refuse the next endpoint/position/focus-seat/member without clearing existing holds. Include multiple distinct inputs on one endpoint, maximum fanout and a synthetic endpoint. Reclaim only after the relevant consumers finish. | [Pointer projection test](../../crates/nara_input/tests/timeline_runtime.rs#L322), [semantic aggregation](../../crates/nara_input/tests/semantic_actions.rs#L387) |
| A3. Atomic failure and no fabricated edges | Exercise exact/one-short endpoint, member, event, sequence and generation boundaries, including prepared/retiring records and synthetic input. Assert unchanged owner state, cursors and outcomes after ordinary rejection. A release for a rejected press emits no physical/semantic edge or gameplay effect. Successful ingress cannot fail later because a projection lacks capacity. | [Sequence regression](../../crates/nara_input/src/timeline.rs#L3006), [prepared route accounting](../../crates/nara_input/src/timeline.rs#L2004) |
| A4. Focus/capture and combined terminal capacity | Hold one action through multiple endpoints, including a UI-consumed press and positioned pointers. Unbinding presentation preserves physical membership. Focus loss and PointerLeft terminate only accepted members with original UI/gameplay ownership. Under both storage candidates, test mixed release/leave/control scope sums, multi-endpoint unions, overlap, retained prefixes and paused cursor lag. Fill all allowed ordinary space and prove complete terminal delivery plus exact/one-short obligation admission. No ingress/projection disagreement or premature recycling is allowed. Coordinated refusal remains atomic. | [Endpoint focus isolation](../../crates/nara_input/tests/semantic_actions.rs#L657), [coordinated refusal](../../crates/nara_input/src/timeline.rs#L2949), UI lifecycle tests |
| A5. Disconnect/reconnect identity | Retire a keyboard/pointer endpoint with paused/staged input and reconnect using the same raw labels. Under the generation candidate, adapter-captured A handles must be rejected after B admission without changing B; under raw-ID control, prove the chosen drain/nonreuse guarantee. Separately document that an originless late raw event cannot be distinguished by minting or looking up a generation. Pending retirement remains charged until all relevant consumers finish; repeated retirement is harmless. This is not gamepad or native event-origin qualification. | [Timeline paused/exact-step tests](../../crates/nara_input/tests/timeline_runtime.rs#L524), UI prepared-route and lifecycle tests |
| A6. Context/mapping and gameplay effects | At capacity with paused input, disable/enable and rebind/restore. The old semantic lifetime ends once with the correct cause and no physical provenance; other contexts survive. A real later release does not repeat cancellation. Only a fresh press restarts the action. Observe exact start/stop gameplay effects across zero and multiple fixed ticks. | [Context cancellation](../../crates/nara_input/tests/semantic_actions.rs#L449), [paused mapping](../../crates/nara_input/tests/semantic_actions.rs#L517), [actual command bridge](../../crates/nara_gameplay/src/lib.rs#L212) |

The complete independent public workflow should run through an Input consumer fixture, such as
the existing [module consumer](../../tests/fixtures/module-consumer/tests/source_input_workflow.rs#L474),
with small explicit capacities and real `ActionCommandMap` consumers. Configure and start the
Runtime, connect input, drive normalized and synthetic presses, observe semantic-to-command game
state changes, reject an over-capacity press without an effect, transfer focus/capture, retire and
reconnect, cancel/restore contexts, and reuse the drained capacity. Include pause, exact-step and
zero/multiple-tick frames. Reuse the original immutable definition owner throughout; source and
processed decoding are regression consumers, not a new qualification program for this unit.

Future verification should use focused serial `cargo nextest run` for Input, Gameplay and affected
UI/Winit/root consumers, plus the independent public workflow and relevant feature checks. Reuse
root `target`, use `CARGO_BUILD_JOBS=1` for substantial builds and never run Cargo concurrently in
this checkout. Any native/platform statement requires its own actual evidence under the existing
qualification procedure. Document checks alone establish neither implementation nor acceptance.
