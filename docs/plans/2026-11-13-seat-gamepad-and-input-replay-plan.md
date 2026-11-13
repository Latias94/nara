# Seat, Gamepad And Input Replay Replacement

Owner: R10, with R4c timing and ADR0041/0057 in the
[foundation rebaseline](2026-10-23-foundation-architecture-rebaseline-plan.md).
Status: implemented and qualified for normalized seat input and the finite ordinary replay
consumer described below. Full R10 and the foundation Goal remain active. Editor UI and physical
mixed-DPI work remain paused. Only local Conventional Commits are authorized for this unit.

## Complete outcome

An independently compiled ordinary runtime consumer controls two game-owned players through
keyboard and gamepad/analog seats using the same action names. It records, serializes and loads
the trajectory in a fresh Runtime. Physical-pipeline replay pins normalization/bindings; semantic
replay preserves game meaning after physical rebind and rejects incompatible vocabulary before
consumption. Original Input, command, fixed tick and retirement owners remain authoritative.

The [pinned source study](../research/2026-11-13-seat-input-and-replay.md) distinguishes current
Nara implementation, upstream mechanisms and new design requirements. A numeric helper, extra
event variant, serialized command vector or a Replay source label does not complete this outcome.

## Ordered work and owning seams

1. Establish the two-seat consumer and compare shared global definitions, seat-scoped mapping
   state and direct per-device callbacks. Select the smallest complete action interface; keep
   immutable definition sharing separate from seat/context policy and durable player identity.
2. Extend the existing finite Input owner with typed, non-reused gamepad connection authority,
   button/axis membership, ordered terminal credits and neutralization. Delayed old/foreign
   samples reject before mutation. Reconnect or reassignment cannot adopt old held contributions.
   Replace ordinary unowned gamepad-button admission; do not keep a parallel callback path.
3. Add validated finite normalized scalar values and explicit dead-zone transformation. Event
   outcomes and retained semantic queries use the same selected value. Reject invalid values
   at admission/decoding. No unchecked public constructor, hidden product dead-zone default,
   vector/radial policy claim or fabricated hardware sampling evidence.
4. Extend the action definition/state and semantic owners for seat identity, typed values,
   changed-value delivery and configuration cancellation. Preserve same-seat aggregation and
   held state across zero/multiple ticks. Observed rebind/context exits cancel once; a control
   already held or non-neutral must not be adopted by a new lifetime before release/neutral.
5. Capture actual seat/value changes through a game-owned adapter into the existing
   LocalGameplayCommands/GameplayCommandQueue. Retain next-open-tick admission and narrow
   Capture/Ack semantics. Do not create a second command queue or static purity checker.
6. Define bounded portable recording/import under the original owners. Distinguish physical
   binding revision from semantic vocabulary version/value kind. Preserve mixed-source command
   ordering without allowing arbitrary callers to forge reserved local source authority.
   Use a finite complete consumer before extracting any reusable recording interface.
7. Replace superseded callers/format assumptions, reconcile ADR0041/0057 and guides, qualify
   the integrated public/independent workflow, independently review and commit precise scopes.

The initial numeric prerequisite belongs in `nara_input` rather than a new crate. The comparison
below selects mapping ownership; persistent compatibility and replay import require their
complete consumer evidence before the production interface is frozen. Design selection alone
does not qualify its implementation.

## Initial comparison and selected production direction

The existing global projection fails the new two-seat public trajectory: two physical seats
press the same action while one seat also holds a second binding. It emits one Started outcome
instead of two. This is a newly required behavior, not a retroactive defect claim against the
previous global contract. The regression retains same-seat multi-binding aggregation.

| Alternative | Disposition |
| --- | --- |
| One mutable global map and global held actions | Current baseline; cannot isolate the two players or cancel one seat independently. |
| Shared immutable definitions, independent seat map/context state and semantic contributions | Selected direction. Reuses existing bounded Input and staging/command owners while giving each seat explicit policy. |
| Per-device gameplay callbacks | Rejected for the production interface: pushes connection, cancellation, mapping and fixed timing into each game adapter and bypasses the accepted semantic ingress. No performance result is claimed for this source-level rejection. |

Replace the World-global mutable `ActionMap` resource with one finite `ActionMaps` resource.
`ActionMap` remains a non-cloneable owned value for one seat, retaining its original definition,
context/mapping lifetime and state ticket. `ActionMaps` installs/removes those values by
`InputSeatId`; an install refusal returns the original supplied value. Definitions may share
their actual immutable bindings owner. Do not retain two overlapping global/seat resources.
Semantic held queries and outcomes include seat explicitly; physical convenient button queries
may still aggregate original endpoint memberships under their documented contract.

The existing single source/processed Input selection supplies the default PRIMARY seat. Another
seat's explicit code configuration does not impersonate a competing PRIMARY selection. A missing
seat has no gameplay mapping, rather than silently borrowing another seat's policy; its raw/UI
input may still be admitted. This refines the previous global selection contract and needs
corresponding source/processed controls before qualification.

Staging keeps separate observed lifetimes for each installed seat. Removing/restoring a map or
returning an older owned map cannot restore an observed lifetime. Captured events retain their
own state allowance; held contributions retain the original ticket until termination. Initial
state allowances remain conservative per owned map and must be reported as admission units,
not measured RSS or a performance win.

Digital aggregation remains OR within one seat/action/context. Scalar axis contributions sum
in stable admitted-member order using a finite wide accumulator and clamp once to [-1, 1].
Retain each contribution's current typed value; removing one contributor recomputes the value.
Zero is inactive. Inactive-to-active emits Started, active-to-zero emits Released, and a changed
active value, including a sign flip, emits Changed. Opposite held contributions may produce zero
without releasing physical members; their later removal can start an active value again. A
definition rejects mixed Button/Axis kinds for one action/context and duplicate source bindings.
An axis update may update an already adopted original member, but cannot newly adopt a member
whose first sample belonged to a missing/expired map lifetime. Neutralization followed by a new
member permits adoption; no second suppression/member ledger is required.

Raw recording includes ordered driver, control and configuration operations and their frame
boundaries: ordinary drives and supplied real deltas, Pause, Resume, exact Step, and the points
where staging observes map/context changes. Equal event values, bindings and application ticks
alone are insufficient. The consumer must compare the explicit `paused input -> Step -> Resume`
trajectory; replay uses the original Runtime control/drive interface rather than reconstructing
clocks or silently substituting normal frames for exact steps.

The first semantic replay consumer may submit the complete recorded canonical order through one
Replay stream with monotonically assigned recording ordinals. That is valid only for an explicit
isolated recorded trajectory: construct the original queue empty in an immutable replay ingress
mode accepting only that exact Replay stream. Other Replay IDs, local adapters, AI, Test and
External submissions must refuse before mutation. No mode switch can adopt already pending
intent. Disable recorded producers in the finite consumer inventory so they do not reproduce
their own future commands; arbitrary trusted native side effects are not sandboxed by this
admission policy. Preserve original provenance as data. Do not merely relabel source kinds and
assume mixed live/recorded ordering still
matches. Prove the selected import with competing original sources and exact per-tick state
before extracting a reusable replay interface. The original queue remains authoritative.

## Executed baseline

Run `961e7569-744e-4fea-8e1b-25d584b380e7` executes the new public
`independent_seat_actions_preserve_same_seat_contributions` trajectory through the existing
managed driver and retires the Runtime before assertions. It fails at the expected Started
count: actual 1, required 2. The other nine semantic tests are outside this focused filter.
Toolchain/profile: Rust 1.95.0, locked offline release, `nara_input --all-features`, one build job
and serial nextest. This RED does not establish the unimplemented gamepad/axis/replay path.

## Integration Checkpoints

The production Input and Gameplay libraries pass their initial all-features check. The first
internal test compilation exposes five stale migration assumptions (`digital` and exhaustive
binding matches); replacing those call sites preserves their original edge assertions. The
owning libraries then pass 147/147. That intermediate source precedes the final semantic output
allowance and static command-seat corrections, so it is not the final integrated result.

Run `6ecc0f3a-57b3-4326-bf71-ea5ac982da5a` passes Input/Gameplay libraries and public tests
243/243. Run `35e50f59-b9ba-4c5e-976e-2efba7975e1d` then passes their affected UI/winit adapters
424/424, with 13 native interactive tests outside that execution. It includes actual static
seat-to-command selection, unchanged endpoint/retirement controls and typed source/runtime
format round trips. Neither run establishes a physical gamepad backend or native UI qualification.

The independently locked ordinary consumer passes 15/15 in run
`9f322763-603a-45d9-9a76-4816a97a20c6`: six new seat/replay controls, three existing bounded Input
workflows and six Source selection/travel controls. It records 27 actual fixed ticks, writes and
loads raw and semantic JSON artifacts, compares each tick in a fresh Runtime and continues for
four more ticks. Actual LocalAction/AI/External batches retain complete canonical order when
imported through the isolated Replay stream after physical rebind. Pause/input/Step/Resume,
axis context/map loss, opposing contributors, UI-consumed members, focus/disconnect/reconnect and
foreign/stale connection refusal remain part of that trajectory. Separate controls execute Stop
with held input and pending commands, refuse late ingress and prove fresh-Runtime reuse.
Exact/over-limit recording bytes, operations, ticks and commands are exercised before effects.
The finite consumer's bounds are not a reusable recording format or measured memory budget.

Independent physical, semantic/format and queue/consumer reviews find no remaining P1/P2 within
their specified source scopes. They do not replace runtime execution. The consumer review identifies
missing Stop/budget acceptance, which the three additional boundary controls above supply.
Existing Runtime input tests remain responsible for actual acquisition/state reservation refunds
on preparation failure, escaped handles and paused map replacement; a synthetic recording ledger
is not introduced. Full failed-Startup connection custody is not separately qualified here.

An additional integration audit finds that public `ActionMaps::default()` accidentally exposes
the Input owner's pristine configuration when inserted before `InputPlugin`. Regression run
`ae7e85e0-a5a3-4b26-a812-b13df1ab5586` fails 0/1 as expected: selection returns `Ok(())` instead
of `CompetingConfiguration`. The correction restores an explicit public default and keeps
pristine construction private to Input installation. Final affected qualification follows this
correction; the earlier green runs must not be represented as covering it.

After that correction, final owning run `0d76594f-0d01-4aaa-95f7-4d0940431260` passes 245/245
Input/Gameplay library and public tests with no skips. This includes both the explicit-default
regression and the static two-seat command-mapping trajectory. The original 147-test library
checkpoint was `899561e5-fb27-46a8-9ca1-018ab12366b2`; its narrower source scope remains historical.
The final independently locked source/bounded/seat-replay run
`c521a111-8d60-450c-8453-6d665153c775` again passes 15/15 after that correction, with no skips.
Removing a redundant clone in the new definition test is followed by a 21/21 definition run,
`07ac5c56-4b02-4cb8-99f8-e0e1a223b252`. The final workspace all-target/all-feature release check
and affected Root/Input/Gameplay/UI/winit all-target/all-feature correctness Clippy pass on Rust
1.95. Existing unrelated warning families remain visible; this is not a warning-free claim.
Root/runtime package and independent module formatting checks pass; migrated samples and runners
are formatted successfully. Paused native geometry and GPUI consumer sources remain unchanged.
The independently locked renamed-root consumer then passes 3/3 in
`cbb6fcb2-0724-440c-9604-15016de6c875`, including actual prebuilt Player execution after its
complete source fixture is removed. This reuses the fixed processed-input case collection and
normal owner-local close control; it does not qualify new packaging or hardware/controller work.

The migrated Reference Game's desktop-flow, runtime-core and UI-product controls pass 18/18 in
`c1b8fc41-d86b-493c-8b19-09f17c755001`; those tests execute managed CPU/UI adapters, not physical
OS input or a native display. Two-Level Game passes 32/32 in
`83dfb9ee-aa59-447c-b381-d20a5cf81695`, including the real save/restore child process and marker
recovery trajectory. Collect Game passes 4/4 in `0d71046e-256c-4915-a09e-c48114e683e0`.
Sample desktop Input plugins are feature-gated; separate all-feature/all-target checks must
cover those migrated paths rather than inferring compilation from headless tests.

Final API closure removes redundant configuration fallibility from the new replay constructor.
`GameplayCommandQueueSettings` and `GameplayCommandSourceId` are already validated owned values;
replay construction should return the queue directly, like ordinary `GameplayCommandQueue::new`.
Invalid configuration remains the settings/ID constructors' responsibility. Queue/public replay
consumers require focused requalification after this signature refinement; do not re-run
unaffected platform or original source-selection behavior solely for this type-level cleanup.

The last public-interface review identifies two real closure gaps: the independent consumer
uses the doc-hidden `InputSet` path and directly completes a raw UI sequence through a hidden
writer. Earlier green results therefore qualify the integrated mechanism, not a supported-only
consumer boundary. Amended ADRs0041/0102 select documented existing ordering anchors plus
`InputTimeline::route_pending_ui_input`, a synchronous callback over that owner's pending
entries. The original owner retains completion, held-member and terminal policy; no naked
sequence/deferred token, second queue, UI receipt or renderer authority is exposed. Existing
captured UI transactions keep their internal completion seam. A real consumer now uses the
supported router, and an unwind control preserves its completed prefix, fallback and original
release ownership. The finite headless routing policy remains distinct from actual UI pixels.
The hidden scheduling alias is removed and all existing adapters use the supported path.
This refinement needs affected Input/UI and independently compiled consumer requalification.

The independent consumer's `-D warnings` gate also finds three `collapsible_match` diagnostics
in the new finite raw reader. Their validation branches now use ordinary match guards; no lint
allowance or validation weakening is added. Existing bounded import controls must pass again.

Final supported-interface Input/Gameplay/UI/winit execution passes 426/426 in
`b4bdc75c-2eb1-4461-9193-8f6bf2d1bfca`, with 13 native interactive tests outside the run.
This includes the synchronous-router unwind/fallback/release regression and the earlier default
authority correction. Existing exact UI transaction and winit owner controls remain intact.
The new public router is a normalized-input policy seam; this CPU result does not publish or
qualify native presentation, display/DPI behavior or a physical controller backend.
The supported-only independent consumer then passes 15/15 in
`5f4adf12-0970-4312-8b13-312a9c68746e`. Its source no longer calls doc-hidden Input scheduling,
raw finalization or driver-port seams. The original complete trajectory and boundary assertions
remain intact; prior green checkpoints are not substituted for this final interface execution.

The final supported-interface workspace all-target/all-feature check, affected Root/Input/
Gameplay/UI/winit correctness Clippy and independent consumer all-target `-D warnings` pass.
Two-Level Game, Collect Game and Reference Game all-feature/all-target checks also pass, covering
their feature-gated desktop Input plugins and compiled Host/Editor combinations. The independent
Cargo-build consumer all-target check passes; its ignored nested Cargo cooker/Player trajectory
is not re-executed for this mechanical probe migration. The actual source-free process evidence
remains the renamed-root execution above. Minimal no-default-feature Input/Gameplay production
libraries pass separately on Rust 1.95, retaining the no-serde module boundary.

A separate bounded probe verifies installed Rust 1.100.0 and passes the same locked offline
release Input/Gameplay all-feature production libraries. It does not run their tests or qualify
the whole workspace, independent locks, native GPU/UI or packaging on that compiler. The
[compiler-impact note](../research/2026-11-13-rust-1-100-impact.md#executed-core-compatibility-probe)
records the exact scope. Rust 1.95 remains this unit's complete runtime qualification baseline.

The selected normalized-input/replay unit is complete. Hardware sampling/controller backends,
broader background/focus policy, vector/radial dead zones, chords, reusable recording APIs,
network/rollback authority, cross-platform floating-point parity and numeric performance budgets
remain open in their appropriate future consumers. No Editor UI or physical mixed-DPI work is
resumed, and this unit does not complete the foundation rebaseline or all R10.

Runtime/test checkpoints use Rust 1.95.0, locked offline release, the original root target, one
build job and serial Cargo/nextest. The separately marked Rust 1.100 production-library compile
is narrower evidence, not a silent toolchain/dependency change or replacement for this runtime
qualification. Editor UI/mixed-DPI work stays paused.

Root integration initially finds two stale `SemanticActions::pressed` calls without a seat in
the Game View fixture; both now explicitly query PRIMARY. Run
`e0887a19-d636-48d0-9a58-d13cd7bff8d0` passes 82/82 affected Input Runtime-content, Processed Host
and public Runtime-instance controls, with 1,465 tests outside that filter. The additional
correctly scoped Game View input modules pass 6/6 in
`63a1e2df-5ec6-4282-a703-d7f85902710a`, with 1,476 outside that filter. The original root filter's
generic `game_view::tests` spelling did not select these named modules; their separate run supplies
the execution evidence rather than treating compilation as a pass.
Actual Processed Host controls retain global Input and its charges through failed Startup and
cancelled/aborted preparation, then refund through original retirement. The independently held
bindings/state controls also preserve exact refusal/retry and last-owner refunds. These existing
owners supply preparation-custody evidence without adding a recording reservation authority.

## Acceptance

- Two seats with identical action/context names remain independent; multiple bindings/devices
  within one seat retain their other contributions when one source releases or disconnects.
- Finite positive/negative axis samples, exact dead-zone boundaries, changing active values and
  neutralization produce matching semantic state and outcomes. Invalid/nonfinite values refuse
  without allocating a new member or changing existing state.
- Focus/capture transfer, held rebind, disconnect/reconnect, same-label ABA and foreign tokens
  cannot resurrect a cancelled action. Pending and terminal storage remain bounded even with
  paused consumers, a full ordinary log and late observation copies.
- Zero/one/multiple fixed ticks, pause, exact Step, admitted commands and frame cleanup retain
  their existing separation. Exact Step does not secretly run a second action-resolution pass.
- A fresh-runtime replay compares actual player state per tick and continued simulation.
  Physical rebind changes raw replay admission or pinned policy, but not semantic meaning.
  Incompatible vocabulary/value kinds reject before the first gameplay effect.
- Stop rejects late input while preserving its observable stopped World until final Runtime
  disposal. Final owner disposal retires connection/action/recording custody; failed preparation
  preserves or refunds the original actual reservations. Neither a copied ID nor a diagnostic
  is completion. Do not clear accepted pending intent merely to make a stopped World appear idle.
- Root and independently locked consumers compile and execute with serial nextest. Run affected
  Input/gameplay/UI adapter controls for API migrations; native claims require actual relevant
  native evidence. Normalized injected input does not qualify a physical controller backend.

Use Rust 1.95.0, `CARGO_BUILD_JOBS=1`, the original root target and one Cargo invocation at a
time. Keep paused UI/DPI and concurrent work intact. Record actual RED/qualified results and
unresolved choices here; do not add registry snapshots or mark all R10/R4c complete.
