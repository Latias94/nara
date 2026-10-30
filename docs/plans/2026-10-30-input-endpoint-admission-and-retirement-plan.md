# Input Endpoint Admission And Retirement Plan

Date: 2026-10-30. Status: bounded owner and coupled UI replacement qualified.
Qualification: [actual owner/public/neutral/native evidence](../knowledge/engineering/verification/2026-10/2026-10-30T140508Z-bounded-input-ownership-and-total-ui-presentation-qualification-b97643044e4a4f92b892a3539d2ad415.md).
Parent: [foundation R10](2026-10-23-foundation-architecture-rebaseline-plan.md#r10-motion-time-repeatability-and-concrete-domain-seams).
Source baseline: `b939da269c693349c5aeb099831731e70e013c95` plus the bounded test prototypes.
Evidence and alternatives: [endpoint admission review](../research/2026-10-30-input-endpoint-admission-and-retirement-review.md).
Actual red/model evidence: [current progress](../knowledge/engineering/progress/2026-10/2026-10-30T105046Z-engine-research-reconciliation-and-bounded-input-terminal-comparison-84d7a4b9cbdf44419a23d851f92b9a7d.md).

## Problem And Complete Consumer

At the reviewed baseline, event and presentation-route limits did not bound persistent endpoints
or held contributions across drained frames. Balanced press/release churn retained historical empty
keyboard/mouse rows. Distinct live pointers could retain arbitrarily many members relative to those
limits while producing one aggregate action. These are separate storage and live-state cases.

The owning correction addresses the reproduced
[platform-observed failure path](../research/2026-10-30-input-endpoint-admission-and-retirement-review.md): an oversized
focus-loss/retirement batch could clear ingress state without admitting terminal events, leaving
button and semantic projections held. Adding a count check or empty-row pruning alone is not
the complete correction.

Prove a public Input-to-Gameplay workflow with small explicit capacities: install the existing
global key/mouse map and action-to-command bindings, admit keyboard/pointer and synthetic input,
drive normal, zero-tick, paused and exact-step execution, and observe actual gameplay effects.
Exercise admission refusal, focus/capture transfer, endpoint retirement/reconnect and context or
mapping cancellation. Drain and reuse capacity through many more historical identities than the
event limit without changing the original global definition owner.

## Scope And Authority

This plan selects an Input-owned lifecycle/member replacement after executing the red regressions,
shared-credit/dedicated-pool controls and adapter source review. The selected owning contract is
reconciled in [ADR 0041](../architecture/adr/0041-input-routing-actions-text-focus-and-accessibility.md).
Production topology, projections and real consumers are replaced and qualified in the evidence
above. Existing aggregation, press ownership, context/mapping cancellation and fresh-press rules
remain. The broader motion/seat/analog/replay work in the foundation plan remains separate.

The parent completed the separate
[processed Input unit](2026-10-30-processed-input-startup-and-process-proof-plan.md). This document
does not alter its ownership, test results, release scope or completion criteria. Immutable
definition/source/processed acquisition custody remains intact; ingress storage has its own cost.
Future seat-scoped maps, gamepad/axis held semantics, analog work and raw/semantic replay remain
separate. Existing gamepad UI-event consumers require migration only where shared APIs change.

## Selected Design

1. **One ingress owner.** `InputTimeline` owns finite connected window/seat endpoints, pointer
   children, accepted members and the ordered event log for physical and synthetic input.
   Focus, positions and prepared routes use admitted records rather than creating independent
   historical rows. Configuration bounds all persistent pools and retained/transient outputs.
2. **Raw adapter labels, private lifetime identity.** Keep raw-ID delivery as the actual bounded
   API, with explicit bounded connection/retirement and a once-admitted default synthetic endpoint.
   Private non-reused member/record identity protects internal references. Do not add a public
   connection handle that current adapters cannot capture at the origin. The adapter must drain
   or discard old external operations before reusing labels; pending internal consumers prevent
   recycling. Current Winit and Editor source facts justify this bounded selection, not a promise
   for arbitrary future native recreation or retained external batch replay. A future asynchronous
   adapter must establish its actual origin contract before reuse.
   UI/Host association captures a private Input incarnation claim before Surface installation.
   Preserve it through logical unbind/suspension and use it for coordinated retirement; it cannot
   claim a same-label replacement or pin an already retired mutable slot. Product composition
   explicitly connects the logical endpoint before installation, even before native creation.
   Explicit restore distinguishes the original Input incarnation from a same-label successor:
   only the original incarnation may retain ordinary suspended focus. A successor clears the
   predecessor interaction and reducer state before presentation.
3. **One finite member pool and projections.** An accepted press retains its input, endpoint,
   provenance and original UI/gameplay owner until every relevant internal consumer finishes.
   `ButtonInput` becomes read-only observation. Synthetic presses enter the same owner; remove
   its direct mutation ports and parallel resolver. Semantic contributions reference admitted
   members and retain a derived finite fanout bound. There is no compatibility forwarding path.
4. **One shared ordered log with terminal credits.** Admit new work only when actual retained
   records plus the complete post-operation terminal obligation fit, including sequence headroom.
   Release/leave/control publication converts existing credits to retained records, which remain
   charged until actually reclaimed. The private dedicated-pool control satisfies the same math
   but needs a second storage limit and sequence merge for every consumer; it is not production
   storage. Credits do not infer topology: the real owner accounts for every distinct obligation.
5. **Loss revokes without allocating identity.** Platform-observed focus/geometry/retirement loss
   immediately revokes only its routing authority and completes applicable accepted holds using
   their reserved capacity. Revocation and unbind do not mint a binding generation. Generation
   exhaustion refuses a new publication without preventing termination of old work. Remove the
   global route-poison workaround. Presentation unbind preserves physical holds.
6. **Prepared loss has a total outcome.** A prepared route keeps its immutable exact declaration
   and a separate owner-side invalidation fact. Platform loss can revoke while that token pins the
   record. Commit consumes it as Published or Invalidated; an invalidated commit never rearms a
   route. UI commit likewise reports committed or invalidated and cannot restore a prepared clone
   of cancelled focus/capture/reducers. Backend pixel acceptance remains truthful; root skips the
   interactive receipt/observation/accessibility success tail for the invalidated candidate.
   UI also captures its own live interaction basis. Ordinary release, outside-content movement
   or accessibility dispatch can change it without Input loss. Check it before publishing a new
   route; stale UI aborts the token and preserves live effects with an explicit UI-owned cause.
   ADRs 0102/0108 adopt this successor; their changed public inventory remains open until migration
   and neutral/public/native qualification.

The selected contract does not claim that an originless late raw event can be recognized as old.
Neither looking up a current generation nor minting a public handle recovers missing event origin.
The bounded adapter workflow, truthful refusal and consumer-held lifetime are the proof boundary.

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
unspent terminal credits must fit the shared event limit. The complete post-operation formula is:

```text
retained_after = retained_before + drafts
retained_after + terminal_obligations_after <= event_capacity
last_sequence + drafts + terminal_obligations_after <= u64::MAX
```

FocusLost, CloseRequested and SurfaceRetired are three distinct control obligations. A positioned
pointer owes Left, and an unpositioned held pointer also needs its future Left allowance. Active
geometry invalidation needs its own credit while physical holds survive; it cannot spend a future
leave promise twice. The actual mixed one-key/two-pointer trace produces eight terminal records
across focus loss, close and retirement. The final ordinary press consequently needs nine shared
slots before retained prefixes or geometry obligations are added. New ordinary activity explicitly
rearms applicable control/leave obligations; duplicate already-terminal notifications are idempotent.

Publishing a terminal record spends its reservation once but retains storage until every relevant
consumer completes. Include projection, frame observation, UI, action staging/resolution, paused
lifetimes and exact-step paths. Physical ingress clearing or advancing one cursor does not release
member, endpoint, event or projection capacity still referenced elsewhere. A dedicated pool does
not supply extra sequence headroom.

Reestablish the frame pin when a previously late event actually produces the next frame's
projection/observation/action copy. Cleanup can end the admission frame before any copy exists;
that cannot make future copies uncharged. Also exercise Game View batches with an ordinary
quota refusal before real release/focus-loss/retirement events. Retain the first error but settle
the observed terminal tail; disposing the rest of a consumed batch is not terminal delivery.

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

The selected design requires complete physical/synthetic admission through one owner, explicit
accounting for every internal reference and the adapter drain/nonreuse rule above. Prove these
in the actual public workflow. Concrete storage layout, numeric defaults, final API names and
future asynchronous/native recreation adapters remain implementation or subsequent consumer
choices. The private count ledger is not endpoint topology or a new general budget framework.

## Review And Bounded Proof Before Production

1. Preserve the executed red baseline for historical rows, live growth, clear-without-terminal and
   generation-dependent revocation. Keep the mixed-control trace as an actual output-shape oracle.
   The source-reviewed raw-ID/handle comparison and executed shared/dedicated capacity controls
   select the design above; they do not qualify production topology or native event origin.
2. Apply the capacity formula to actual endpoints, members and every consumer-held lifetime.
   Include retained ordinary/staged prefixes, paused lagging consumers, overlapping cancellation,
   multi-endpoint unions, geometry, exact/one-short admission and sequence near exhaustion.
   Reject a new obligation before acceptance if its eventual combined termination cannot fit.
3. Replace the ingress owner with finite configuration and the chosen identity/admission surface.
   Implement member storage, prepared/retiring accounting, chosen terminal storage, sequence
   reservation and any selected generations together. Preserve checked overflow and truthful
   refusal. Delete historical indexes and obsolete mutable state instead of forwarding into it.
4. Migrate button/semantic projections and synthetic input to the single owner. Preserve staged
   mapping/context identity and original acquisition/state custody. Derive the M * K link bound
   and maximum internal retained/transient capacities from actual data structures.
5. Migrate the review's [public consumers](../research/2026-10-30-input-endpoint-admission-and-retirement-review.md#public-migration-inventory):
   Winit, Editor Game View, UI prepared presentation/lifecycle, source and processed input fixtures,
   synthetic test writers and default driver users. Migrate UI/root/manual presentation commits
   to the total invalidated outcome and prove prepare/loss/cancel/late-commit cannot restore focus,
   capture, reducers or accessibility. Verify default paths do not reconstruct unadmitted identities.
   Keep gamepad UI navigation working without expanding held/axis claims.
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
| A1. Historical churn and allocation | With small E/M, connect, press/release, retire and drain repeatedly across more than 4,096 historical identities. Empty secondary indexes disappear; only intentionally connected idle records retain capacity. Endpoint/member/link pools and retained container capacities remain within admitted maxima, and retired capacity becomes reusable. Include keyboard and pointer paths. | [Timeline owner](../../crates/nara_input/src/timeline/owner.rs), [projection tests](../../crates/nara_input/src/projection_tests.rs) |
| A2. Live saturation after log drain | Add distinct held pointers one frame at a time until endpoint or member capacity is full, with event logs empty between frames. Refuse the next endpoint/position/focus-seat/member without clearing existing holds. Include multiple distinct inputs on one endpoint, maximum fanout and a synthetic endpoint. Reclaim only after the relevant consumers finish. | [Retention tests](../../crates/nara_input/src/timeline/retention_tests.rs), [semantic aggregation](../../crates/nara_input/tests/semantic_actions.rs) |
| A3. Atomic failure and no fabricated edges | Exercise exact/one-short endpoint, member, event, sequence and generation boundaries, including prepared/retiring records and synthetic input. Assert unchanged owner state, cursors and outcomes after ordinary rejection. A release for a rejected press emits no physical/semantic edge or gameplay effect. Successful ingress cannot fail later because a projection lacks capacity. | [Owner/route tests](../../crates/nara_input/src/timeline/tests.rs), [capacity controls](../../crates/nara_input/src/timeline/capacity.rs) |
| A4. Focus/capture and combined terminal capacity | Hold one action through multiple endpoints, including a UI-consumed press and positioned pointers. Unbinding presentation preserves physical membership. Focus loss and PointerLeft terminate only accepted members with original UI/gameplay ownership. Test actual shared-log release/leave/control unions, geometry, overlap, retained prefixes and paused cursor lag; retain the private dedicated-pool comparison as its bounded control. Fill all allowed ordinary space and prove complete terminal delivery plus exact/one-short obligation admission. No ingress/projection disagreement or premature recycling is allowed. Coordinated refusal remains atomic. | [Endpoint focus isolation](../../crates/nara_input/tests/semantic_actions.rs), [coordinated owner tests](../../crates/nara_input/src/timeline/tests.rs), [UI loss tests](../../crates/nara_ui/src/adapter_prepared_loss_tests.rs) |
| A5. Disconnect/reconnect identity | Retire a keyboard/pointer endpoint with paused/staged input and reconnect using the same raw labels. Prove adapter drain/nonreuse and private-reference fencing; pending retirement refuses reuse and stays charged until every relevant consumer finishes. Prepared loss followed by late commit/abort cannot revive old interaction or affect a replacement. An originless late raw event cannot be distinguished by looking up a generation. Repeated retirement is harmless. This is not gamepad or arbitrary native event-origin qualification. | [Timeline paused/exact-step tests](../../crates/nara_input/tests/timeline_runtime.rs), [UI claim/restore tests](../../crates/nara_ui/src/adapter_prepared_loss_tests.rs) |
| A6. Context/mapping and gameplay effects | At capacity with paused input, disable/enable and rebind/restore. The old semantic lifetime ends once with the correct cause and no physical provenance; other contexts survive. A real later release does not repeat cancellation. Only a fresh press restarts the action. Observe exact start/stop gameplay effects across zero and multiple fixed ticks. | [Context/mapping tests](../../crates/nara_input/tests/semantic_actions.rs), [actual command bridge](../../crates/nara_gameplay/src/lib.rs), [independent workflow](../../tests/fixtures/module-consumer/tests/bounded_input_workflow.rs) |

The complete independent public workflow runs through the
[bounded module consumer](../../tests/fixtures/module-consumer/tests/bounded_input_workflow.rs),
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
