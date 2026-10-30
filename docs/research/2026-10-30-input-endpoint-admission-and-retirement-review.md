# Input Endpoint Admission And Retirement Review

Date: 2026-10-30. Status: proposed owner design, ready for review; not adopted or implemented.
Source observation: working tree based on `75e1284ed28bc5a96686609f7dae8805d1924788`, including
concurrent processed Input changes. Source links identify the observed symbols and line numbers.

## Scope And Evidence

This review addresses foundation R10's confirmed cross-frame endpoint and semantic-contribution
growth. It recommends a concrete Input owner replacement and compares a simpler finite raw-ID
control. It does not reopen immutable definition/source ownership or review the concurrent
[processed Input unit](../plans/2026-10-30-processed-input-startup-and-process-proof-plan.md).
That unit's tests appear below only as migration sites.

The counterexamples are source-derived public-operation traces, not executed reproductions.
No Cargo command, product test, native qualification or process proof was run for this review.
Recommendations require reconciliation with [Accepted ADR 0041](../architecture/adr/0041-input-routing-actions-text-focus-and-accessibility.md)
before changing its production contract. This review and the
[proposed execution plan](../plans/2026-10-30-input-endpoint-admission-and-retirement-plan.md)
do not adopt that contract or establish qualification.

The immediate consumer remains the current global key/mouse action map and its action/context
aggregation. Future seat-scoped mapping, gamepad/axis semantics, analog admission and raw/semantic
replay remain separate R10 work. Endpoint connection generation is a local lifetime fact, not a
durable player identity or replay vocabulary.

## Current Owners And Limits

| Boundary | Observed source and implication |
| --- | --- |
| Event retention | [Timeline constants](../../crates/nara_input/src/timeline/types.rs#L14) set 4,096 retained events and 64 presentation routes. [Admission](../../crates/nara_input/src/timeline.rs#L1340) checks retained events and sequence space, not endpoint or held-member counts. |
| Route admission | [Prepared route admission](../../crates/nara_input/src/timeline.rs#L221) counts active routes and new route reservations. This does not admit every physical endpoint. |
| Persistent ingress state | [InputTimeline fields](../../crates/nara_input/src/timeline.rs#L144) include independently keyed focus, keyboard, mouse and pointer-position maps. |
| Keyboard mutation | [Press/release](../../crates/nara_input/src/timeline.rs#L567) inserts a window/seat row; ordinary release removes the inner key without pruning an empty outer row. |
| Mouse mutation | [Press/release](../../crates/nara_input/src/timeline.rs#L641) inserts a window/seat/pointer row; ordinary release also leaves an empty outer row. |
| Position and focus | [Pointer movement](../../crates/nara_input/src/timeline.rs#L724) inserts a position; [focus gain](../../crates/nara_input/src/timeline.rs#L795) inserts a seat target. Neither requires a presentation route or a finite physical endpoint admission. |
| Button projection | [ButtonInput](../../crates/nara_input/src/lib.rs#L155) stores endpoint memberships per button. [Projection](../../crates/nara_input/src/lib.rs#L380) inserts them independently of the bounded edge log; [removal](../../crates/nara_input/src/lib.rs#L412) does prune an empty button row. |
| Semantic projection | [SemanticActions::apply](../../crates/nara_input/src/semantic.rs#L110) inserts a physical contribution into an unbounded set for each action/context. [PhysicalContribution](../../crates/nara_input/src/semantic.rs#L200) uses input plus raw endpoint identity. |
| Synthetic mutation | [ButtonInput::press/release/release_all](../../crates/nara_input/src/lib.rs#L204) and its [driver port](../../crates/nara_input/src/lib.rs#L448) mutate a second live owner outside timeline admission. |
| Frame completion | [Cleanup](../../crates/nara_input/src/lib.rs#L1222) drains resolved button edges and [timeline reclamation](../../crates/nara_input/src/timeline.rs#L1546) removes completed events. Neither imposes a bound on persistent endpoints or contributions. |
| Definition/state costs | [ActionMap::required_state_bytes](../../crates/nara_input/src/state.rs#L52) explicitly excludes ingress endpoints, held semantic contributions and transient action output. |

Current [endpoint projection identity](../../crates/nara_input/src/lib.rs#L166) contains window,
seat and optional pointer, without connection generation. The
[route stamp](../../crates/nara_input/src/timeline/types.rs#L314) carries presentation generations;
those generations do not identify a physical connection lifetime.

## Public Counterexamples Across Drained Frames

### Live Held Members And Contributions

Use the public `ActionMapBuilder` to declare enabled `ActionContext::gameplay()` and bind
`MouseButton::Left` to `ActionId::new("hold")`. Install the compiled bindings through `ActionMap::new`
and `InputPlugin`, then start `RuntimeInstance`. This follows the existing
[public semantic fixture setup](../../crates/nara_input/tests/semantic_actions.rs#L112).
Run the following operations, with the usual `nara_app`, `nara_input` and `Duration` imports:

```rust
for raw in 1..=8_192u32 {
    let outcome = runtime
        .with_driver_scope(|scope| {
            apply_mouse_driver_input_at(
                scope,
                InputWindowId::new(1),
                InputSeatId::PRIMARY,
                InputPointerId::new(raw),
                ButtonDriverInput::Press(MouseButton::Left),
            )
        })
        .unwrap();
    assert!(outcome.post_operation_fault.is_none());
    outcome.output.unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.world().resource::<InputTimeline>().retained_len(), 0);
    assert!(
        runtime
            .world()
            .resource::<ButtonInput<MouseButton>>()
            .transitions()
            .is_empty()
    );
}
```

The [managed helper](../../crates/nara_input/src/lib.rs#L594) accepts raw IDs. No presented route
is required: [pointer routing](../../crates/nara_input/src/timeline.rs#L1196) returns an unrouted
stamp when none is installed, and gameplay action delivery remains possible. Each iteration
uses a distinct pointer; a duplicate press on the same held pointer/button would be ignored.

After N iterations, source insertion and reclamation establish these counts:

| Retained owner state | Count after each completed frame |
| --- | ---: |
| Timeline event entries and projected button transitions | 0 |
| Presentation routes | 0 |
| Timeline mouse endpoint rows and held buttons | N |
| Projected `Left` endpoint memberships | N |
| Held semantic action/context groups | 1 |
| Contributions to the `hold` action | N |

These are live admitted holds, not abandoned history. Aggregation reduces the action count, not
the number of physical facts needed to release it correctly. The contribution count grows while
the logs repeatedly return to zero. The scalar ID space and eventual sequence exhaustion are
finite, but neither is an operational retained-state budget.

### Historical Empty Rows

Change the trace to press and release `Left` for each fresh pointer before driving the frame.
After cleanup, the button projection has removed its memberships and the semantic action has
ended. Timeline mouse rows nevertheless accumulate as empty inner maps because
[ordinary release](../../crates/nara_input/src/timeline.rs#L675) does not remove the outer row.
Fresh fallback windows with the same seat/key produce the equivalent
[keyboard case](../../crates/nara_input/src/timeline.rs#L590), provided no focused window overrides
the fallback. Each pair is balanced before the next one begins.

These rows retain actual tree keys and values with zero logical held work. They also enlarge
[keyboard origin lookup](../../crates/nara_input/src/timeline.rs#L1105). Removing them is necessary,
but it does not address the live-held trace. Conversely, `Moved` on fresh pointers or `Focused(true)`
on fresh seats retains live position/focus records even without button contributions.

Retained container storage and current logical occupancy need separate accounting. An empty row
still stored in a map is not merely allocator high-water behavior. After removal, allocator page
retention or process RSS is a different observation and is not proven by logical row counts.

### Platform-Observed Cancellation Can Lose Terminal Events

Continue the live-held trace with public
`apply_window_input_driver_event(scope, window, seat, InputWindowEvent::Focused(false))`.
There are no positions or held keys in this trace, so
[physical invalidation](../../crates/nara_input/src/timeline.rs#L904) constructs N mouse releases
plus one focus event. At N = 8,192, the 8,193 drafts exceed the 4,096-entry limit even though the
timeline was fully drained.

The [PlatformObserved branch](../../crates/nara_input/src/timeline.rs#L1031) first revokes route/focus
authority. When draft admission fails, it calls
[clear_physical_route_state](../../crates/nara_input/src/timeline.rs#L1317) and returns a capacity
error marked as route-invalidated. No release entries were admitted. The managed helper wraps
that error as `ButtonInputDriverError::Timeline`.

The projected `ButtonInput` and `SemanticActions` can remain held: their
[consumer](../../crates/nara_input/src/lib.rs#L1012) received no terminal transitions, and
[semantic reconciliation](../../crates/nara_input/src/semantic.rs#L72) only expires mapping/context
lifetimes. A later raw release can be ignored because the ingress membership was already cleared.
The [PointerLeft failure branch](../../crates/nara_input/src/timeline.rs#L778) has a related
clear-without-admitted-release path when its batch cannot fit.

This is a confirmed source-level consequence, not an executed failure result. Prioritize fixing
the admission/terminal owner together; an empty-row patch alone leaves this behavior intact.
The existing [coordinated retirement test](../../crates/nara_input/src/timeline.rs#L2949) checks
atomic refusal, whereas [platform invalidation coverage](../../crates/nara_input/src/timeline.rs#L2914)
does not prove terminal delivery for this accumulated-held case.

## Candidate Comparison

| Candidate | Benefit | Remaining obligation and cost |
| --- | --- | --- |
| Prune empty raw-ID rows only | Removes historical keys and their lookup cost with little code. | Leaves live endpoint/member growth and oversized cancellation batches. Insufficient as the owning correction. |
| Finite raw-ID admission plus pruning | Can bound current maps if all physical/synthetic writers and pending states participate. May be sufficient when the adapter can prove its event drain and ID-reuse boundary. | Must reserve terminal capacity, bound container allocations and delay recycling until all relevant consumers finish. A reuse rule must cover external queued work as well as internal cursors. This is a viable owner candidate, not a deliberately weaker fallback. |
| Finite lifecycle/member pools with generations | Can reject old-incarnation handles retained by an adapter or queued normalized operation after slot/raw-label reuse. Makes member and retirement accounting explicit. | Requires the adapter to capture and pass the actual admitted handle, plus ingress/projection migrations. It cannot classify a late raw OS event that carries no incarnation. This remains a conditional recommendation for review. |

The simpler control is credible only with the same end-to-end accounting and terminal guarantees;
comparing it against a stronger candidate without those obligations would hide displaced work.
TTL/LRU removal of valid holds would change delivered action state to recover storage and is not
an acceptable replacement for admission or explicit cancellation.

The concrete generation workflow to prove is: the adapter captures admitted handle A for a
normalized operation, queues it, retires A, and admits B using the same raw labels or slot. When
the queued operation arrives carrying A, the owner rejects it without changing B. The current
[Winit adapter](../../crates/nara_winit/src/lib.rs#L696) constructs raw IDs at delivery, and the
[Game View bridge](../../src/project_host/runtime/editor/game_view.rs#L1313) reconstructs a raw
window for its batch. Neither cited call site currently supplies this admitted-handle fact.
This successor workflow is a proof obligation, not an executed stale-event defect report.

If an OS event arrives only after B has been installed and the adapter looks up the current handle
from its raw label, attaching B does not recover the event's original incarnation. Both candidates
need a source ordering, queue-drain, nonreuse or explicit event-origin guarantee for that case.
Document the actual adapter guarantee and any unresolved ambiguity; generations alone do not
distinguish late raw OS events. If the raw-ID control proves the required workflow with less owner
and consumer complexity, it may be selected instead. The generation candidate must demonstrate
its benefit against that same workflow before its additional public API is justified.

## Recommended Owning Model

The following is the generation candidate, contingent on the comparison and bounded proofs below;
it is not a declaration of the final endpoint API. Keep the correction inside `nara_input`,
centered on `InputTimeline`. Use finite typed
window/seat lifecycle records and pointer children, with connection generations and an explicit
retiring state. Store focus, position and routing facts in these records or bounded indexes.
Presentation routing remains distinct from physical connection: geometry unbind must preserve
balanced physical holds, as the [current unbind contract](../../crates/nara_input/src/timeline.rs#L375)
requires. A new presentation must not manufacture a physical connection or resurrect a retired one.

Each accepted press reserves one member containing the input, endpoint incarnation, original
press sequence/route and UI/gameplay ownership. Its release becomes terminal before the member
is reclaimed. Hold its slot through every internal projection or staged event that can still
reference it; a physically released but unresolved member is not free capacity. Retiring endpoint
slots similarly remain charged until their internal references are finished. Copied external
observations can carry value stamps; they need not pin an entire mutable endpoint record.

`ButtonInput` becomes an observation projection. Synthetic presses/releases use the same admission
owner and finite member pool; remove the independent mutation/driver-port bypass. Preserve the
public distinction between synthetic provenance and normalized physical provenance even if both
use one private ordered log. Default synthetic endpoints, if offered, consume fixed admitted slots.

`SemanticActions` aggregates references to admitted member identities instead of owning an
independently growing `(input, raw endpoint)` set. Keep current global action/context behavior and
the non-reverting mapping/context lifetimes. Do not add a seat/action model in this correction.
An admitted press must not later fail at button projection or semantic insertion for a capacity
that its successful ingress operation failed to reserve.

Let E bound all lifecycle records, including window/seat parents, pointer children and synthetic
endpoints; M bound all member slots, including queued, held and terminal-pending members; and
K = `MAX_BINDINGS_PER_INPUT` = 32. The following are required candidate invariants:

```text
active + prepared-only + retiring lifecycle records <= E
queued + held + terminal-pending member slots <= M
retained semantic contribution links <= M * K
```

The categories are disjoint; a prepared pin on an active record does not count a second record.
Existing binding/context compilation remains finite: the current
[ceilings](../../crates/nara_input/src/definition.rs#L9) are 1,024 bindings and 64 contexts. Count
prepared route reservations and retirement pins before admitting another record. Reuse a slot
only with a fresh checked generation; reject exhaustion without wrapping. Remove retired raw-ID
indexes instead of accumulating tombstones for every connection ever observed.

These are retained-container bounds, not just live-count bounds. Prefer fixed-capacity slot/link
storage, or equally explicit storage whose maximum allocated capacity is admitted up front.
Include free lists, indexes, staging metadata, temporary cancellation drafts and every retained
projection container. Checked sizing must cover M * K and reject invalid cross-limit settings.
Bound internal transient output from the finite event/member/fanout ceilings as well; retain the
existing separate definition/state charges rather than claiming they pay for ingress storage.
Caller-owned copies of observations are outside the live owner's bound and must be described so.

### Reserve The Ability To Finish Accepted State

Admit terminal capacity with the obligation. For every supported cancellation scope S, including
a union of endpoints, derive the combined number of actual terminal records:

```text
terminal_need(S) = held key/button releases in S
                 + required pointer-left records in S
                 + distinct remaining terminal control records for S
```

Count each actual output record once. A pointer-left record that also terminates that pointer is
not a second control record. The current window cancellation constructs precisely this combined
[release/position/control shape](../../crates/nara_input/src/timeline.rs#L949). Proving a bound for
each component separately does not prove their sum fits, or that multiple scopes can terminate
together. Overlapping/repeated invalidations must not double-publish or double-spend obligations.

Two storage candidates remain open:

| Terminal storage candidate | Required capacity proof | Cost to compare |
| --- | --- | --- |
| Shared event log with reserved terminal credits | Retained ordinary and terminal events plus unspent terminal credits fit the event capacity. Ordinary admission preserves space for the complete outstanding cancellation union; publishing a terminal record converts its credit into retained event storage exactly once. | Reduces ordinary-event space dynamically and couples admission to all terminal obligations. May reuse the existing ordered consumer path. |
| Fixed dedicated terminal pool | Retained terminal records plus unpublished terminal obligations fit the dedicated capacity, while ordinary records fit their separate capacity. Reserve the full combined scope; free terminal records only after every relevant consumer finishes. | Adds fixed storage and requires a proven ordered merge with ordinary events for every consumer. A separate pool alone does not establish correct ordering or reclamation. |

Both must preserve `next_sequence + reserved_terminal_sequence_headroom <= u64::MAX` with checked
arithmetic. A separate terminal pool does not create sequence space. Include projection, frame
observation, UI routing, action staging/resolution, paused retention and exact-step paths in the
proof. Conversion to a published event is not reclamation; members, endpoint pins, event storage
and any surviving projection queue remain charged until all consumers that can reference them
have finished. Checked generation allocation and final route invalidation also need a defined
exhaustion path before accepting a lifetime that promises successful retirement.

Before choosing an Accepted contract, execute a small-capacity comparison for both candidates.
A useful mixed scope holds one key and two mouse buttons on two positioned pointers in one
window/seat: three releases plus two pointer-left records plus one lifecycle event require six
terminal records together. Include ordinary/staged retained prefixes, paused lagging cursors,
overlapping cancellations and a multi-endpoint union; demonstrate exact capacity and one-short
admission refusal without accepting an obligation that cannot finish. These are proposed tests,
not executed evidence. Neither a terminal storage choice nor E/M/capacity defaults is selected here.

Coordinated retirement may refuse atomically before publication. An already observed platform
loss must revoke routing authority and deliver terminal outcomes for accepted members; the
eventual reservation policy must prevent capacity-induced clear-without-terminal behavior.
Duplicate loss/retirement and release after retirement must be idempotent. A rejected press has
no member and cannot justify a fabricated release. Context cancellation remains a semantic cause
without physical provenance; it does not invent low-level physical release edges.

### Assumptions And Deferred Decisions

The boundedness recommendation assumes one receiving Input owner sees every physical and synthetic
mutation, every internal reference is included in reclamation, and the adapter supplies whatever
incarnation or drain guarantee the selected identity model needs. These are requirements to prove,
not facts established by the current raw-ID call sites. The initial red counterexample has not run.

Defer the identity model, adapter capture/reuse rules, handle ownership and generation representation,
terminal pool versus credits, exact scope/ordering accounting, storage layout, numeric limits and
public API shape until the bounded comparison. Any chosen raw-ID entry points would be the final
owner's API, not a compatibility bridge into the old mutable owner. The invariant to adopt is
finite admitted state with truthful terminal delivery; a complete new handle API is not yet adopted.

## Public Migration Inventory

All signatures below are current APIs. The table records migration pressure under the generation
candidate, not an adopted replacement surface. The finite raw-ID control may keep raw-ID signatures
if their chosen owner enforces the required admission and reuse guarantees. New handle names and
constructor spelling remain open; no compatibility facade or dual mutation owner is proposed.

| Current signature or representation | Migration required by the recommendation |
| --- | --- |
| [`admit_keyboard(&mut self, window: InputWindowId, seat: InputSeatId, input: ButtonDriverInput<KeyCode>) -> Result<InputAdmission, InputIngressError>`](../../crates/nara_input/src/timeline.rs#L434) | Receive a currently admitted endpoint incarnation and reserve its member/terminal obligations. |
| [`admit_mouse_button(&mut self, window, seat, pointer, input)`](../../crates/nara_input/src/timeline.rs#L446), [`admit_pointer(..., PointerDriverInput)`](../../crates/nara_input/src/timeline.rs#L459), [`admit_wheel(..., Vec2)`](../../crates/nara_input/src/timeline.rs#L482), all returning `Result<InputAdmission, InputIngressError>` | Carry an admitted pointer incarnation; positions and wheel routing cannot create hidden endpoints. |
| [`admit_window_event(&mut self, window, seat, event)`](../../crates/nara_input/src/timeline.rs#L472), returning `Result<InputAdmission, InputIngressError>` | Bind focus/loss/retirement to the admitted lifecycle and its terminal reservations. |
| [`apply_keyboard_driver_input_at(scope, window, seat, input) -> Result<Vec<KeyCode>, ButtonInputDriverError>`](../../crates/nara_input/src/lib.rs#L565) and [`apply_mouse_driver_input_at(scope, window, seat, pointer, input) -> Result<Vec<MouseButton>, ButtonInputDriverError>`](../../crates/nara_input/src/lib.rs#L594) | Runtime-scoped delivery uses the same admitted identity. Migrate implicit-default helpers to the same explicit ownership. |
| [`apply_pointer_driver_input_at(...) -> Result<(InputAdmission, Vec<MouseButton>), ButtonInputDriverError>`](../../crates/nara_input/src/lib.rs#L625) and [`apply_window_input_driver_event(...) -> Result<(InputAdmission, Vec<KeyCode>, Vec<MouseButton>), ButtonInputDriverError>`](../../crates/nara_input/src/lib.rs#L679) | Preserve actual terminal outputs and platform suppression coupling while replacing raw-ID mutation authority. |
| [`ButtonInput<T>::press/release/release_all`](../../crates/nara_input/src/lib.rs#L204) and [`__RuntimeDriverPort` implementation](../../crates/nara_input/src/lib.rs#L448) | Move live synthetic mutation to Input admission; keep observation methods on the projection. |
| [`publish_presented_route`](../../crates/nara_input/src/timeline.rs#L213), [`prepare_presented_route`](../../crates/nara_input/src/timeline.rs#L1560), [`unbind_presented_endpoint`](../../crates/nara_input/src/timeline.rs#L387), [`retire_presented_endpoint`](../../crates/nara_input/src/timeline.rs#L406) | Resolve and retain the actual endpoint incarnation through prepare/commit/abort/retirement. Keep presentation unbind distinct from physical retirement. |

The existing gamepad event API is currently event-only for this semantic pipeline:
[`action_transition`](../../crates/nara_input/src/timeline/types.rs#L481) maps key/mouse events only.
Any shared event-signature migration must keep the current UI navigation consumer working, but
does not qualify gamepad held state, disconnect semantics or axes.

| Actual consumer | Current source anchor and change |
| --- | --- |
| Winit owner | [Mouse/pointer ingress](../../crates/nara_winit/src/lib.rs#L614), [keyboard conversion](../../crates/nara_winit/src/lib.rs#L682), [destroyed window](../../crates/nara_winit/src/lib.rs#L770), [target retirement](../../crates/nara_winit/src/lib.rs#L1133): retain admitted lifetimes and retire them with actual native ownership. |
| Editor Game View | [`apply_input_batch`](../../src/project_host/runtime/editor/game_view.rs#L1285): replace reconstructed raw window and PRIMARY identities with the matching admitted lifetime. |
| UI lifecycle/presentation | [`update_ui_surface_slot`](../../crates/nara_ui/src/runtime.rs#L321), [`suspend_ui_surface`](../../crates/nara_ui/src/runtime.rs#L366), [`restore_ui_surface`](../../crates/nara_ui/src/runtime.rs#L393), [`retire_ui_surface`](../../crates/nara_ui/src/runtime.rs#L432), [`prepare_input_route`](../../crates/nara_ui/src/runtime.rs#L641): retain exact incarnation through atomic presentation/retirement. [UiSurfaceEndpoint::new](../../crates/nara_ui/src/runtime/surface.rs#L20) may remain a target descriptor; raw labels alone do not grant mutation authority. |
| Synthetic callers | [Input semantic test writer](../../crates/nara_input/tests/semantic_actions.rs#L66), [timeline synthetic setup](../../crates/nara_input/tests/timeline_runtime.rs#L438), Input local tests, Gameplay tests at [552](../../crates/nara_gameplay/src/lib.rs#L552), [744](../../crates/nara_gameplay/src/lib.rs#L744) and [1533](../../crates/nara_gameplay/src/lib.rs#L1533), and concurrent [`change_input`](../../src/runtime_content/input/tests.rs#L370): migrate to one admitted synthetic path. |
| Public source/processed consumers | [Source workflow injection](../../tests/fixtures/module-consumer/tests/source_input_workflow.rs#L474) and [processed process probe](../../tests/fixtures/runtime-runner/renamed-root/tests/support/processed_input_probe.rs#L429): obtain endpoint admission rather than calling raw-ID mutation. Preserve existing definition/acquisition custody. |
| Driver and UI fixtures | [UI test driver](../../crates/nara_ui/src/adapter_test_driver.rs#L215), [runtime driver test](../../tests/runtime_instance.rs#L3413), reference-game [desktop flow](../../reference-game/tests/desktop_flow.rs#L552), [parity](../../reference-game/tests/desktop_parity.rs#L37), [render](../../reference-game/tests/desktop_render.rs#L323), [UI interaction fixture](../../reference-game/tests/support/ui_product_fixture/interaction.rs#L219), and [gallery navigation](../../examples/ui_presentation_gallery.rs#L167): migrate real entry points, not just owner tests. |

`ActionMap` compilation, startup selection and context/revision costs remain separate owners.
`SemanticActions::pressed/as_slice` and the actual
[Gameplay command bridge](../../crates/nara_gameplay/src/lib.rs#L212) can retain their public
action/context/phase meaning. Migrate internals and callers directly to the selected owner API;
preserve a raw-ID signature only if it belongs to the selected design, not solely for compatibility.

## Pinned Bevy Facts

The read-only `repo-ref/bevy` HEAD was checked as
`85503941e488152711a4695bbe248e9ed9f453fc`. Its
[ButtonInput storage](../../repo-ref/bevy/crates/bevy_input/src/button_input.rs#L125) contains held
and transient sets. [`clear`](../../repo-ref/bevy/crates/bevy_input/src/button_input.rs#L249) clears
only transient sets; [`release_all`](../../repo-ref/bevy/crates/bevy_input/src/button_input.rs#L179)
moves held inputs into released observations. The
[keyboard system](../../repo-ref/bevy/crates/bevy_input/src/keyboard.rs#L170) clears transient state
and releases cached keycodes on focus loss. Its keyboard resource is window-ID agnostic.

The [gamepad connection system](../../repo-ref/bevy/crates/bevy_input/src/gamepad.rs#L1502) removes
`Gamepad` on disconnect while leaving the entity and possible settings alive for reconnection.
These are useful lifetime mechanisms, not evidence of a finite Nara endpoint/member allowance,
equivalent aggregate release semantics, or a complete reference retirement policy to copy.

## Verification Successor

The [proposed plan](../plans/2026-10-30-input-endpoint-admission-and-retirement-plan.md#acceptance)
defines six focused test families and one complete public Input-to-Gameplay workflow. Internal
storage assertions must prove endpoint/member/link capacities and reclamation; public runtime
execution must prove truthful edges and effects. Neither this source review nor empty event logs
alone establish bounded retained state, terminal delivery or native/platform qualification.
