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

## Native Gilrs Input And Playable Controller Successor

Status: implemented at `b607a64` and `6795753`, November 19. Windows WGI acquisition/retirement
and the normalized source-game consumer are qualified below; actual physical controller
button/stick/hotplug execution remains unqualified. Deliver a concrete optional controller
adapter and an ordinary game consumer. Normalized gamepad support alone does not connect
hardware. Keep UI/Dialogue product work paused and prioritize functional Runtime delivery
over another performance experiment.

Use an independently consumed `nara_gilrs` package with the mature Gilrs mapping/discovery
backend, not a new abstract backend, SDL runtime, scheduler or window owner. One native context
is polled in the original App's First stage before Input projection. Routes name an existing
window and seat explicitly; the adapter does not create or focus endpoints. The original
InputTimeline retains connection generations, accepted button/axis members, capacity refusal
and ordered terminal credits. Per-device assignment is product policy over the actual connected
inventory, not a persisted hardware or player identity.

An Input-owned read-only endpoint observation exposes current incarnation/focus and the last
physical-invalidation sequence. This permits detection of loss and refocus between backend
polls after old log entries have been reclaimed. It grants no mutation or ingress authority.
Actual native samples still require their original connection token.

Enumerate already connected devices, handle later connection/disconnection, and drain native
events while a route is inactive. Do not replay cached held values into a new route. Known
held/nonneutral controls remain suppressed until their observed release/neutral; fresh events
for previously unknown controls can begin new input after the queued-event barrier. Gilrs has
no portable instantaneous hardware snapshot, so do not claim complete unknown-startup-held
detection or swallow every first button press while waiting for an unavailable snapshot.
Reuse its mapped D-pad and device dead-zone logic; keep author-selected action transforms
separate. Filtered neutral observations must still settle suppression.

Disable force feedback in this input-only context. Register concrete native custody with the
original managed close owner, including pre-Startup failure. Windows WGI has an upstream
stop/join path without a bounded latency promise. Foreign backend drop behavior and actual
controller hardware require their own execution evidence; context creation with zero devices
does not prove physical button delivery. Do not introduce an additional polling thread.
The selected foreign backends lack synchronous thread retirement, so the managed plugin refuses
non-Windows acquisition before creating a native context. Compile support is not Runtime support.

The source Two-Level Game opts in through an explicit desktop `--gamepad` option and build
feature. Authored D-pad and ordinary game buttons feed the existing action names and command
queue alongside keyboard input. Releasing one physical contribution must preserve another
accepted contribution. The existing exact-capture eligibility remains authoritative; this unit
does not serialize controller connections, add an analog-to-movement expression engine or
automatically select a shipping Player backend. The backend's normalized axes and per-device
two-seat routing need their own complete ordinary controls.

Acceptance covers the actual bridge reduction, original Input lifecycle, supported button/axis
codes, startup/route/focus/reconnect behavior, saturated admission, two-seat routing and managed
cleanup. The actual game consumer must move, stop and perform structural actions through its
original Input-to-Gameplay flow. Run native backend initialization and ordinary window startup
separately where available. Preserve the keyboard-only continuation and source/cooked definition
consumers. Independent review and focused serial verification own the evidence; no hardware,
foreign-platform or numerical latency claim follows from synthetic event execution.

### Native Controller Implementation And Qualification

The original Input owner now exposes a read-only endpoint observation, including its actual
incarnation and the last physical-loss admission sequence. The private endpoint stores that
watermark when its original physical-loss operation publishes; no independent epoch allocator,
event retention or per-frame Scene validation is introduced. The initial full Input library
run passes 103/103 in `ebf84efc-f79b-4d74-be9d-0295c7118b66`, before the later keyboard-continuation
eligibility correction below.

`nara_gilrs` owns one concrete context, bounded native polling and current device inventory.
Its nominal tokens distinguish a reconnect from a reused backend label. Product assignment
uses explicit existing window/seat routes, while original Input connections continue to admit
each button/axis and settle terminal credits. Startup and changed routes wait for the old native
queue to drain. Known holds are suppressed rather than replayed. Portable codes deliberately
map Gilrs controls; no backend discriminant is serialized. Duplicate startup Connected messages
preserve an existing token/assignment instead of manufacturing a reconnect. FF is disabled.

The root and independent game locks add only the selected Gilrs closure; no previously locked
package version or source is replaced. The Linux CI build prerequisites gain libudev development
files and pkg-config because the concrete dependency must compile, even though managed native
acquisition remains Windows WGI-only. CI configuration is local evidence, not a remote run.

The first bridge test compilation found two old ECS NonSend method names in its fixtures; they
now use the actual current World methods. All twelve bridge controls pass in
`8f72f358-81e4-4548-a875-224c3288c54c`, including finite refusal, terminal custody, two-seat routes,
unassign/reassign, focus loss/regain, endpoint replacement, known holds, duplicate input and
configured/Startup/Runtime/ordinary retirement. The genuine WGI initialization/close test passes
1/1 in `ae8027e9-6e52-4153-b01b-e604ba26c802`. It retains the original native state through
`RuntimeInstance::finish` and observes its context removed. It reports **zero connected devices**;
this proves native acquisition/retirement, not physical controller gameplay or latency.

Independent review finds a keyboard-continuation integration regression before release: the
game's declaration helper accepts D-pad alternatives, but the original Input continuation
validator still requires exactly one binding of any kind per action. That would reject the
default map even without a native backend. The owning validator now permits Gamepad Button
alternatives alongside one unique declared keyboard Button, preserving original key fanout,
context, member and semantic checks. Actual gamepad holds still refuse keyboard capture.
New public and actual-default-map controls cover empty/held keyboard capture, fresh-Runtime
restore without Press/Started, actual controller refusal and release/disconnect recovery.
Bounded rereview finds no remaining P1/P2. The complete Input library plus public claimed-keyboard,
keyboard-continuation and semantic-action controls pass **135/135** in
`82b27a1b-4d20-4d4d-b764-acd410c4a1a5`, including that actual restore/refusal/recovery test.

The game adds twelve authored controller Button alternatives without replacing its twelve
keyboard bindings. The source desktop entry requires both `gamepad` and `--gamepad`, stores
its route/limits in the original ProductConfiguration fingerprint and otherwise starts no
native context. Its normalized Platform consumer executes the original game, command queue,
Scene travel and physical Session cleanup. These injected normalized inputs are not a hardware
test. The source-copy helper also preserves the new optional direct dependency's original
checkout path rather than resolving a nonexistent relative crate in the copied project.

The first complete game selection passes 206/207 in
`030938b7-63e5-4ce4-b187-5d43f1ce719a`: the old Cook rebind fixture changes every row with the
selected action name into the same key, including its newly added controller row. It now changes
only the original Key source. The authored-key and native-arrow rebind fixtures use that same
source distinction. This corrects test input rather than allowing duplicate bindings.
The final game selection passes **207/207** in `dd39df7c-4eea-4636-ba73-e0abe5be775d`: the
library and original authored Input, normalized controller gameplay, resume, Cook/source consumers.
The five controller tests cover four-direction movement, keyboard/controller contribution
release, disconnect/focus cancellation, Marker/Travel and actual keyboard capture eligibility.
Cooked definitions retain both source kinds without selecting a native Player backend.

The initial hardware DX12 selection `08229323-1626-4d8a-99d3-1430b2339d2b` passes both existing
default-key and authored-arrow sequential reconstruction trajectories, including their original
complete-state, retained-pixel, collision, animation, physical input and native retirement
assertions. The new optional source-entry first-present control fails before Runtime startup:
the ordinary process owner's closed anonymous stdin pipe is mistaken for a potential bootstrap
pipe, and inspection reports BrokenPipe. Its original physical child cleanup still completes.

The owning process correction uses standard read-only Windows `NUL` for non-bootstrap EOF.
It leaves actual duplex bootstrap construction and strict shape/access admission unchanged,
alongside exact executable custody, inherited handle allowlists, environment, Job containment
and reader/output retirement. Independent bounded review finds no P1/P2 in that correction.
All original process-owner controls plus the new character-device/EOF unit pass **36/36** in
`4eb74db1-be7b-4a84-bb6d-09b3397cdd0d`. The independently locked public process consumer passes
**3/3** in `03909189-eb76-4008-a7cc-96946d5152be`, retaining sealed-environment exact launch,
immediate stdin EOF, bounded raw protocol output, lossy diagnostic accounting and actual
containment/readers/output cleanup. Its independently resolved dependency versions are unchanged.
The corrected source-entry first-present control passes **1/1** in
`f358ce0e-28d7-4c87-bcd5-f0a4f3052b3d`, on actual Windows/DX12 hardware through the same
contained process owner. The process correction is committed separately at `841ea03`.

The final lifecycle pass identifies a valid one-slot handoff gap: disconnecting an original
connection retains its slot until actual consumers finish, but an immediate native reconnect
treated that temporary occupancy as hard capacity exhaustion. Three ordinary App/InputPlugin
tests reproduce managed Runtime failure for focus loss/regain, two-seat reassignment and
same-label native disconnect/reconnect in `b1dd9d71-2a3a-42ba-aca3-c9ccf19e9a45`.
Initial fixture iterations corrected public accessor/static diagnostic use and accidentally
focusing two windows on the same seat; distinct focused seats establish the actual original
connection before the reproduced failure.

The original Input topology now distinguishes `GamepadRetirementPending` from a zero-slot or
entirely live `GamepadCapacityExceeded` pool. The bridge defers only the former and retries on
a later drained poll. Actual original consumption still reclaims the slot; the bridge adds
no terminal queue, cursor-based reclamation guess or storage owner. Known holds remain
suppressed across the wait. The original deferred-action/frame-copy control keeps its full
retirement assertions and now expects that typed refusal. Final verification follows below.

The complete final Input library, public claimed-keyboard/keyboard-continuation/semantic suites
and Gilrs bridge/App controls pass **150/150**, with one explicit native skip, in
`bb11ec6d-20c2-4dd4-b0b2-153366d00a0d`. All three one-slot handoffs now wait for actual original
consumption, renew the connection generation, suppress held controls and admit fresh input after
release/neutral. The original zero-slot/all-live hard refusal, deferred action/frame-copy
retention, two-seat contribution and fresh-Runtime keyboard restore controls remain intact.
The genuine Windows WGI acquisition/close rerun separately passes **1/1** in
`858e71e5-7f8a-4546-a6bc-5385bb93ea7a`, again reporting zero connected hardware controllers.
The final affected actual-game controller controls pass **5/5** in
`e0acd072-edde-498d-8f8b-ce40402b01e1`, after the Input retirement correction. They preserve
four-direction movement, overlapping keyboard/controller contributions, disconnect/focus
cancellation, single-press Marker/Travel and keyboard continuation eligibility. The final
actual Windows/DX12 source-entry rerun passes **1/1** in
`9b58c658-92a7-4b67-b126-03b587833d75`, including actual first presentation and original
process/Runtime retirement. The earlier complete 207-case scope remains recorded above; the final
Input classification change does not require another unrelated Cook/graphics matrix.
Independent bounded rereview finds no remaining P1/P2 in the original Input backpressure,
bridge retry/suppression or corrected App regressions. It confirms that only actual original
consumers release retiring slots and that true hard-capacity failures still propagate.
Correctness Clippy passes for all Input/Gilrs targets/features, the selected game library,
desktop and changed test targets, and the root process-owning library profile. These are
`-D clippy::correctness` gates; nonblocking style and existing warning output is not reported
as a warning-free workspace.

Root and independent sample formatting checks pass. Both normal/build graphs for the
`cooked-player` and `cooked-player,desktop` selections with defaults disabled exclude `nara_gilrs`,
Gilrs and gilrs-core. The concrete package's direct production dependencies remain only Gilrs,
App, ECS and Input. The locks retain every previously resolved package version/source while
adding the eight local/native dependency entries. TOML/lock, authored JSON, CI YAML, added local
document links/headings and whitespace checks pass. CI configuration was not run remotely.
The paused UI/Dialogue files remain byte-identical. Implementation commits are local only.

This slice adds no native controller state to SaveSlot, exact continuation or recording wire
formats. Physical controller hardware, foreign native retirement, analog game movement,
shipping Player backend selection, rumble and numeric input-latency budgets remain open.
Full R10/R12 and the Foundation Goal remain active.

## Explicit Controllers Through Cooked Player Delivery

Status: implemented at `4b862a9`, with native qualification at `a332a2f`, November 19.
Windows source-free silent/audio controller composition and the preserved original placements
are qualified below. Physical controller interaction remains unqualified. Extend the source-game
backend into actual source-free delivery before selecting another runtime subsystem. Preserve
paused Editor/UI/Dialogue work and the full R7/R10/R12/Foundation scope.

The game owns an explicit `--controllers none|gilrs` cooker choice, independent of its
headless, silent desktop or audible desktop placement. The default remains `none`; headless
native controllers refuse. Gilrs selection requires the optional gamepad feature and supported
Windows target. Both audible and silent desktop Players must compose it with their existing
runtime recipes. Do not create a generic backend registry or enumerate a new engine runtime
for every native-service combination.

One finite game-owned selection binds its existing binary, requested Cargo features, content
profile and actual recipe. The cooker and Player share those rules; the core build and catalog
owners retain actual compiler features, fingerprints, target and immutable generated input.
All-features availability does not itself activate native controllers. Original keyboard-only
profile names remain unchanged; controller-selected profiles are distinct and must fail closed
when opened by an unavailable or wrong-placement Player. Source and processed selections use
the same fixed controller configuration and original native close participant.

Qualification must execute the ordinary cooker, relocate its complete installation, remove
copied source/import cache and tool access, then run the actual selected Player through its
original contained process owner. Preserve the complete physical keyboard, play, room, pixel,
Save/fresh Restore, continued-travel and physical cleanup controls. The combined audio/controller
selection retains the original WASAPI sample and native-retirement proof. Original keyboard-only
controls remain independently selected. A presented inventory observation can report the actual
installed backend and connected-device count; it is not physical button evidence or a new
cleanup receipt. Zero connected devices remain an explicit hardware limit.

Keep source/headless and silent default dependency selections free of Gilrs. Test finite profile
composition and actual cooked-content fingerprints without acquiring devices during preparation.
Reject unsupported combinations/targets at the selected boundary. No extra per-frame Scene audit,
wire field, input queue, clock or runtime manager is selected. Focused consumer tests, independent
review, actual native execution and precise local commits own the resulting evidence.

### Controller Player Implementation And Qualification

The game's shared `delivery::PlayerSelection` now owns the finite runtime/controller choice.
The cooker parses `--controllers none|gilrs`; it selects explicit Cargo features, recipe and
content profile while the original Cargo/catalog owners still capture actual compiler output
and fingerprints. The three old profile names are unchanged. Source desktop and both cooked
desktop placements share the same controller configuration helper, preserving original canonical
route and limit bytes. Headless and desktop entries check their own placement before opening
content. No runtime-native acquisition occurs during recipe/plan construction.

The first library/cooker/source-entry production check passes with `gamepad,cook,audio-cpal`.
The initial CPU test compilation rejects a new `unwrap_err()` because the owning
`PreparedProjectCook` intentionally has no Debug requirement. The test now explicitly matches
its expected error; no engine trait or resource owner is changed for that assertion. The complete
affected library, cooked-content/source, controller gameplay, Player entry, protocol-corruption
and PCM-oracle selection passes **270/270**, with fifteen explicit native skips, in
`be2f6c32-a42a-414e-9fcf-9d362fb4c350`. It includes real resolved plans and cooked catalogs for
both controller placements, unchanged authored Input/schema/codecs, wrong-recipe/profile refusal
and unsupported-target rejection before source/store work.

Independent production review finds no P1/P2 in the shared selection, feature/profile/recipe
binding or original Player cleanup. Consumer review identifies an acceptance gap: both old
keyboard native paths explicitly passed the new `none` option, leaving omission untested on
desktop. The silent desktop control now omits it, while audible desktop still passes `none`.
The original complete workflows and exact publication/features/catalog/controller observations
remain unchanged. Bounded rereview closes that finding with no remaining P1/P2. Native
generated-input compilation, complete Player execution and final checks follow below.

The first genuine controller-enabled silent desktop delivery passes **1/1** in
`b41670b0-ffd9-4139-8263-b8de4951d6eb`. The original cooker builds an explicit native-target,
default-disabled Player, publishes its bound pack, then the original fixture relocates the
installation and removes copied source/cache/tool access. Actual hardware DX12 execution
retains physical authored J input, rooms, collision, atlas pixels, complete Save/fresh Restore,
twenty further travels, scripted controls and software first-presentation control. Successful
saved, continued and finite results observe `gilrs-wgi` with **zero connected devices**, original
Runtime retirement and physical event-loop return. The original process cleanup reports empty
containment, stopped readers and drained output. The 1028-second trajectory includes the first
release build for the explicit target; it is not a runtime frame-time measurement. Combined
audio/controller and the original keyboard-only delivery controls follow separately.

The controller-enabled audible delivery separately passes **1/1** in
`6913dd3b-a123-46a8-80f3-fdfbc010eb92` on Windows, hardware DX12/RTX4090 and WASAPI. Its relocated
pack retains the original two positive processes and two distinct corruption refusals. The
independent oracle compares **348672 callback samples** against original acquired PCM with
**zero maximum absolute error**, including pickup/travel causes and stale-scope cancellation.
The finite verified and physical WM_CLOSE trajectories both observe `gilrs-wgi` with zero
devices, original Runtime retirement, event-loop return and successful physical process cleanup.
Source/cache paths remain unavailable. Callback equality does not establish human audibility,
hardware-controller interaction or an audio/input latency budget. The 381-second trajectory
includes its explicit-target build. Original keyboard/headless delivery controls follow.

The original delivery selection `dcb8fa27-0a98-4768-9138-dc8e98898ec0` passes the complete
keyboard-only audible and headless workflows. Audible success records observe `none` with
zero devices; its independent PCM oracle compares **351552 samples** with zero maximum
absolute error. Headless retains its real captured build, source/cache removal, fresh
Save/Restore, continued behavior, corruption refusals and original native process cleanup.
The default silent desktop case is interrupted during a physical L-key injection: the OS
foreground observation becomes None, and the existing exact-window assertion fails. The
original child/worker cleanup still completes. This result is a failure, not a skipped pass.

Without source or assertion changes, the isolated default-desktop rerun passes **1/1** in
`6ad0df3e-b3fc-4116-8d5e-7161242fea01`, retaining the original complete trajectory and twenty
additional travels. The cooker omits `--controllers`, and actual saved/continued/finite records
all observe `none`, zero devices, original Runtime retirement and event-loop return. The
first foreground interruption remains recorded separately from this final qualification.
No focus-assertion relaxation or auto-retry policy is added to the product or test framework.

Final correctness Clippy passes for the selected library, cooker/source and all three Player
entries plus affected source/content/native-consumer test targets. This is a correctness gate,
not a warning-free-workspace claim. The default-disabled `cooked-player` selection separately
passes **12/12** in `7403e208-791b-409a-97da-af208cb86185`: headless configuration/Player entry,
unavailable controller rejection and profile/target policy compile and execute without
development, desktop, audio or gamepad features.

Root and independent sample formatting checks pass. The headless `cooked-player` and ordinary
`cooked-player,desktop` normal/build graphs exclude Gilrs; explicit `cooked-player,gamepad`
includes the native adapter. This unit changes no dependency version, source or lockfile.
TOML/lock, CI YAML, added local document links/headings and whitespace checks pass. The local
CI addition runs selection/content controls without native acquisition; no remote CI or push
was performed. All five scoped workers/reviewers are closed, and the fifteen paused
UI/Dialogue files remain byte-identical. No Cargo or native-test process remains owned by
this unit after closure.

The selected Windows delivery unit is complete, including both new controller choices and
the original three Player placements. It does not establish physical button/stick/hotplug
gameplay, foreign native retirement, input/audio latency budgets, rumble, arbitrary service
reconstruction or a universal backend-selection interface. Existing SaveSlot, continuation
and recording formats are unchanged. Full R7/R10/R12/Foundation remains active.

## Actual Game Recording And Replay Successor

Status: implemented and qualified at `da96f1e` for the source/headless profile below.
Runtime capability takes priority over micro-optimization. This unit delivers
a usable source-game debugging workflow rather than another normalized-input demonstration.

The game owns a finite exact-step recording profile and an ordinary command-line consumer. A
caller supplies semantic movement, marker, travel, instance and Retry operations. Recording runs
the real Two-Level Game, captures commands from the original successful Capture set, checks the
original queue acknowledgement after the step, and records the resulting supported game state.
Playback starts a fresh game with the original queue's immutable Replay-only ingress and compares
each newly produced result. The existing development protocol shares the same semantic vocabulary.

Both paths remain paused between exact steps. After each step they drive ordinary paused
maintenance until the original Scene request, game reconstruction and task-return owners settle.
No additional fixed tick runs while waiting for a Scene. A command acknowledgement is not Scene
activation: the game must also reach its own healthy Running state with the matching target and
complete reconstruction before the row can be published. Failed or unacknowledged work cannot
be appended as a successful recording. Original Runtime faults require Close, not in-place retry.

The selected observation is the source game's existing finite reconstruction inventory: complete
checkpoint/progress, player pose, held intent, game clock, scheduling continuation, counters,
Schema and admitted level content. App configuration fingerprints are recorded separately for the
fixed recording/playback recipes: their queue ingress modes intentionally differ. This is not an
arbitrary ECS snapshot, native-effect rollback, an executable hash or cross-platform determinism.
Only the fixed interactive source/headless recipe is accepted. Save/Restore operations, open
SaveSlots, audio publication, desktop recording, cooked Player replay, unknown gameplay services
and arbitrary frame grouping remain outside this profile.

Admission is finite at program/file and row boundaries; normal gameplay does not acquire a new
whole-state validation loop. Files contain no Runtime authority. Import checks version, shape,
complete finite command vocabulary, contiguous step order and bounded retained bytes before a
fresh Runtime starts. Recordings preserve original command provenance as data; playback assigns
one monotonically ordered Replay source without authorizing local or competing producers.

Acceptance covers an actual two-level recording, held movement and release, pickups, persistent
markers, distinct Cavern instances, Retry, fresh-Runtime playback, portable encode/decode, source
drift and tampered-result refusal, original Ack and Scene barriers, competing ingress refusal,
and physical original Session cleanup on success and failure. The ordinary CLI records and
replays a file in fresh processes. Focused affected tests and an independent review qualify the
changed boundaries; no new microbenchmark or unrelated Editor/native matrix is required.

Ordinary startup errors keep the original Session cleanup owner. Arbitrary Rust startup panics
follow the engine's existing Runtime quarantine contract; this adapter does not claim that an
empty outer Session retired that owner or add a second quarantine manager. Post-start recording
operation panics close the original Session before resuming the original payload. A pending
ordinary Close remains retained in the returned error for explicit cleanup retry.

### Actual Game Qualification

The public game trajectory records 71 engine steps and twelve semantic commands, producing 67
game ticks. It collects three pickups, preserves three markers across Meadow/Cavern and distinct
Cavern instances, restarts the current level and returns to Meadow. Each structural loading row
retains its engine tick while paused maintenance adds no simulation time. A fresh copied source
project reexecutes every row and matches complete canonical supported state. Separate ordinary
CLI processes record the supplied 67-step/twenty-command program and reproduce all 63 game ticks.
The file contains provenance data and expected results, never an imported queue or World owner.

Initial run `8dfe63a3-09e1-4a47-83cb-88ebc78ee86b` passes 19/21. One regression reveals that Serde's
internally tagged unit variants accept unknown fields despite the outer attribute. The shared
semantic type now decodes through closed empty-struct variants while preserving its existing wire
format. The other failure is the CLI test's incorrect equality between engine and game ticks:
four loading rows intentionally do not simulate the game. The assertion now checks the actual
selected trajectory. Corrected run `84de6ad7-0a97-4b60-92d7-adf6b1626956` passes 21/21.

Final affected run `cbbe3664-cc39-4e6c-b941-0a67566d72f9` passes 186/186 with two explicit ignored
actual-build tests. It includes all default library cases and the ordinary authored-Input,
development-Host, resume, gameplay, recording and CLI consumers. Controls cover intermediate
result tampering, admitted content drift, bad destination repair, key/tick/version/byte refusal,
original ordered multi-command Capture, competing Replay/AI/Test/External ingress refusal and
post-start panic cleanup before the original payload resumes. Existing Save/Restore and source
continuation regressions remain in this affected selection.

Two independent source reviews close their concrete P2 findings: arbitrary startup panic keeps
the engine's original quarantine contract rather than a false empty-Session retirement claim;
later cleanup retry preserves an actual terminal failure in the returned diagnostic. Published
Runtime operations still close through their original Session. The review covers Capture/Ack,
generation custody, Scene confirmation, finite decoding and CLI output publication; it does not
create a new global validator or a static proof of arbitrary native behavior.

Default all-target correctness Clippy, no-default `cooked-player` all-target compilation, root
and sample formatting, added local documentation links/fragments, manifest/program parsing and
whitespace checks pass on Windows/Rust1.95, release/offline/locked with one Cargo job and the
existing target directory. Non-correctness warnings remain. The minimal compilation is not
cooked replay execution. Native output, source-free packages, actual Rust-edit adoption and
foreign platforms are not rerun or newly qualified by this source/headless tool unit. No numeric
performance result, arbitrary frame grouping, whole-runtime rollback or full R10/R12/Foundation
completion follows. Subsequent work prioritizes useful Runtime consumers over small performance
experiments; paused UI/Dialogue work remains preserved.

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

## Authored Game Input Through Source And Player Delivery

Status: implemented and qualified on Windows after authored Sprite clips at `93701c3`/`2c880a0`.
Original selection and native-key corrections are committed at `98969c4` and `7cbdd9f`.
The normalized seat/replay mechanism above was already qualified. The actual Two-Level Game
previously constructed physical bindings in Rust and its cooker rejected `input.action_map`.
This successor removes that product restriction through the existing source Input and processed startup owners,
not another Input backend, mutable mapping ledger or serializer. Runtime capabilities remain the
priority; UI/Dialogue and small performance experiments stay deferred.

Ship a native action-map file with its intrinsic complete primary Product, selected by the game
manifest. The game registers semantic command bindings independently of physical keys in every
placement. Its recipe explicitly selects the existing `ActionMapDomain`/`ActionMapRuntimeCodec`;
the original unpublished Runtime installs source/processed mapping state. Remove the displaced
code-first physical map and its competing installation. Baking retains the actual source bytes
and selected identity, adds one global startup Input member (eight catalog members total), and
preserves original source freshness, publication, Player and collection ownership.

Keyboard reconstruction must read the installed original PRIMARY map, rather than regenerate
WASD as authority. The game can resolve one enabled keyboard Button binding for each of its four
movement directions at capture/isolation/restore. Missing, ambiguous or unsupported bindings
make exact keyboard continuation ineligible without disabling ordinary gameplay or weakening
the Input owner's existing checks. Ordinary multi-binding semantics remain unchanged; this unit
does not invent a mapping for reconstructing several physical holds from one logical direction.
Semantic state and source/content compatibility remain game-owned; no OS key or mapping cursor
is added to the resume format. Driver checks stay at operation boundaries, not every simulation tick.

Complete actual game workflows with an edited action file: physical keys drive the original
command queue, fixed movement, collision and animation; default keys stop triggering the rebound
action. Pause/Step, context cancellation, travel/Retry, immutable source retention and fresh
restoration keep their existing owners. A changed global source refuses future travel while the
incumbent stays usable; a fresh Runtime loads the new mapping and incompatible old state refuses.
Cook and decode the changed map through the actual source-free Player, retain all existing
native gameplay/pixel/cleanup assertions, and rerun actual Rust-edit adoption because the copied
source inventory and native binding capture change. Trust unchanged Input/codec/platform owners;
verify changed game/adapters and relevant existing controls with serial release nextest, jobs1,
locked offline Rust1.95 and the original target. Physical controller execution, live rebinding,
Editor preferences, arbitrary held multi-binding migration and full R10/R12 remain open.

### Implementation And First Qualification

The game now loads `input/explorer.actions.json`, intrinsic primary source
`b23d8f3a-9770-5c1a-bf4d-81fb93171257`, through the original manifest selection. Its
unconditional game Input plugin only declares semantic commands. Source and cooked recipes
select the original action-map codec, and the cooker joins the actual decoded Product with
`source_runtime_projection()` before constructing processed configuration. It never clears
the source obligation to reuse source-independent projection. The action file joins both the
cook content fence and the development source inventory. Catalog membership is eight with one
separate global Input root; each Scene keeps its original four Required asset dependencies.

The first library check passes. Independent review catches a new continuity gap in the initial
implementation: sampling the mutable map in game Startup could accept a definition replaced by
an earlier Startup system as source truth. The original `InputInstallation` now retains the
selected immutable PRIMARY handle on successful unpublished finalization. Read-only
`startup_action_map_bindings` exposes that original owner; game capture/restore compares the
actual map and initial context flags only at the operation boundary. No input state or key enters
the resume wire, and no second installation or mutation owner is added.

Native reconciliation no longer limits its inverse conversion to four default movement keys.
It maps all43 codes already admitted by ordinary `convert_key_code`, including named keys,
arrows, lowercase ASCII letters and digits. Windows still resolves each declared key's original
physical scan code through the current keyboard layout with the original HWND/focus checks and
`GetAsyncKeyState`. Normalized keys without a native inverse remain explicitly unsupported by
that platform operation, while pure Input/game semantics may still use them.

Run `954914ae-5caa-478d-b76a-8835c778e1d1` passes44/44,183 outside the selected scope, in
1.637 seconds after a 4m22s affected release build. It includes original Input selection and
keyboard continuation, actual early-Startup map replacement with the original selection still
retained, code-first finalization and unchanged repeated-refusal behavior. Winit's existing
isolation/held/fault/activation controls remain intact; the added inverse table and extended-arrow
scan-code cases are CPU evidence, not physical keyboard execution. Game/cook/native/delivery
qualification follows. Build durations are observations, not performance targets or comparisons.

Initial game/cook run `8693ad7d-2548-43e7-8b4d-356a2d61b096` passes27/29 and exposes two
new fixture assumptions. A custom First producer must cross the original First staging boundary
before an exact Step; Step does not rerun arbitrary First systems. The corrected driver performs
paused maintenance before stepping. A rejected travel retains its error until the game's existing
successful command path clears it; the test now uses that actual Marker operation before proving
continued movement. Production Input scheduling and old game recovery are unchanged.

The separate adapter review catches a production mismatch: source Input validates an optional
identity sidecar, while the first cooker integration ignored it. Cooking now checks the same
intrinsic source UUID, kind and canonical path when metadata is present. The original content
fence retains its presence/absence, file identity and digest and rejects later addition, mutation
or removal. No mandatory sidecar or new identity format is introduced. Corrected run
`d9979c00-7d33-4a1f-b19c-2fde938f55ff` passes31/31,126 outside the filter, in4.580 seconds
after a38.58-second release build. This includes actual edited keys, old-key inactivity, fixed
movement, collision/animation, context and focus cancellation, travel/Retry, fresh reconstruction,
changed-global-source rejection, source-removed processed mapping and sidecar conflict controls.

The complete desktop/cost/cook/Player/audio-enabled ordinary selection passes347/347,31 explicit
native/build cases skipped, in `bbcdafee-d8b2-4e14-a8cd-ae418a0f43a6` (35.517 seconds after a
9m53s release build). Existing collision, animated restoration, sound data, source/cooked ownership,
finite copied inventories and CLI controls remain. A subsequent startup-console wording change
reports configured controls instead of promising default WASD for rebound files; later native
and delivery builds consume that same production source. The Input API change and native key
expansion remain limited to their recorded owner/native scope.

Both actual Windows/DX12 reconstruction trajectories pass2/2 without skips in
`e710512f-dae8-46e4-8527-d570bc6859eb` (55.363 seconds after a2m20s affected release build).
The original default-key trajectory stays intact. The new case edits only a private copied
action file to ArrowLeft/Right/Up/Down, then launches the same ordinary main-thread executable
against that project. It asserts the actual installed keys before repeating original held-state
capture, fresh reconstruction, exact paused continuation and native focus/release-gap activation.
Both retain original actual presentation associations, ten retained-input GPU replays, animation
and collision assertions, native target retirement and physical-loop recovery. This qualifies
the declared direction keys under the executed Windows layout/focus conditions; it does not
claim arbitrary Unicode physical keys, other native platforms or controller hardware.

The final independent consumer review requires edited definitions to survive an actual external
Player as well as the in-process source-removed Runtime test. The original headless package
trajectory now changes up from W to I before production cooking and compares its full processed
definition after source/cache removal. The ordinary desktop package trajectory changes right
from D to J and uses actual SendInput J for original wall/pickup/travel/Save/Restore operations.
Default audio controls retain D. Both package oracles compare complete independently reconstructed
edited definitions, rather than weakening identity/definition checks or replacing gameplay
assertions. Their execution and actual rebuild adoption are the remaining delivery gates below.

The actual relocated headless package trajectory passes1/1 in
`4538e43b-398d-47df-9247-45050fb17262` (511.782 seconds, five controls/private workers outside
this selection, after a4m50s outer release build). The production cooker separately compiles its
default-free Player; that first feature-specific dependency build accounts for most elapsed time.
Before cooking, the owned copy changes `move-w` from W to I. The complete eight-member pack and
every source-free Player receipt must preserve that exact edited definition. Original source/cache
removal, game progress, instance travel, save/fresh restore, damaged/missing content and environment
refusals, executable/catalog identity and actual process cleanup assertions remain intact.
This execution qualifies the external Player, not merely a typed closure or in-process decode;
the observed build duration supplies no iteration or runtime performance comparison.

The actual relocated desktop package passes1/1 in
`5accfb23-cf10-463f-b556-eccf24c51019` (147.268 seconds, one private worker outside the
selection). The ordinary and restored Players run DX12 on NVIDIA GeForce RTX4090 with right
movement authored as J; every actual Input proof retains that binding. Original physical key
operations exercise pickups, wall stopping/sliding, markers, Save/fresh Restore and twenty more
interactive travels. The saved run still observes all four default animation columns. The finite
first-presentation software control uses the Microsoft Basic Render Driver with the same J map.
All three normal exits and the intentionally refused startup retain original HWND destruction,
Runtime/pump return, empty containment, stopped readers and drained output without force-kill.
Source and cache are unavailable to the relocated Player. This result does not requalify the
unchanged physical audio service, foreign keyboard layouts or a numerical frame budget.

Actual source/headless rebuild adoption passes1/1 in
`1ac24f07-1487-41e3-9c7f-b3f6d3458df0` (136.833 seconds, two ordinary controls outside the
ignored-only selection). The original complete Host trajectory retains its meaningful game
basis and nonfirst animated frame across actual function-body and private-state-structure edits.
Compiler/probe/source-drift refusals, original incumbent retention, nominal tickets, fresh adopter
restoration, Pending-time new-game commands and physical predecessor/active cleanup remain.
Its finite copied product and source fence now include the actual action file; no separate
mapping restorer or serialized native key state is introduced.

The original continuous desktop Rust-edit/adoption trajectory passes1/1 without skips in
`8e6be2cd-e0ab-460d-8dd5-1acc65aa9529` (316.192 seconds after a10.36-second outer build).
Its separate `desktop` child feature closure first recompiles native/render dependencies. Actual
native keyboard play, pause/isolation, complete canonical capture and restored-presentation
association still precede route exchange. Body/private-structure edits adopt through the same
production Host, original new-active/old-retirement separation and Input activation owner;
the new game continues under actual native key operations before eventual physical cleanup.
The selected test requires hardware rendering and a consistent observed DX12-or-Vulkan backend;
this run does not independently record which allowed backend its sealed child selected. It is
not continuous Editor Play, cross-platform held-key support or a measured performance result.

The affected Input/Winit ordinary CPU cohort passes297/297,23 native cases explicitly skipped,
in `407f0c48-d822-4dc9-9a22-6755265149db` (11.034 seconds after a3m50s feature-specific
release build). It enables the existing Input serde facet and retains original declaration/
codec, finite membership, context lifetime, normalized gamepad, Window/session and keyboard
continuation controls. Its skipped native cases are not recounted as new native qualification;
the executed default/rebound reconstruction and source-free physical workflows above own this
unit's native claims. No unrelated Editor/UI or physical controller matrix is selected.

With defaults disabled and only `cooked-player`, the selected Input, animation and actual edited
game workflows pass22/22,92 outside the filter, in
`113efa0e-ab05-4958-9d4b-1cc4075b3736` (1.421 seconds after a2m21s feature-specific release
build). Source-game tests use the existing CPU content loader without a development Host, Cargo
tools, WGPU/window backend or UI. The separate relocated Players above establish actual source-free
execution. Source/processed input reuse introduces no new dependency or lock resolution.

Final all-target release correctness Clippy passes for Input/Winit with the existing serde facet
and for the complete selected desktop/cost/cook/Player/audio game closure. Existing warnings
outside `clippy::correctness` remain. Root and independently locked game formatting, affected
TOML/action JSON, added document links/fragments and ordinary whitespace checks pass. The fifteen
paused UI/Dialogue files retain their original bytes. Eight scoped research, implementation and
independent review threads are closed; the actual Startup baseline, sidecar and edited external
Player findings are resolved and exercised. No owned build/native child or review thread remains
after the recorded final cleanup. Commands use Windows, Rust1.95.0, locked offline release
dependencies, one build job, the original shared target and serial nextest; no remote publication
or unrelated native/UI matrix is part of this unit.

The actual authored-key workflow is complete through source play, cooked delivery and original
game-owned rebuild/adoption. Existing installed mappings stay immutable across travel; changing
the global source requires a fresh Runtime. Live rebind UI/preferences, arbitrary physical
multi-binding continuation, controller hardware, foreign native execution, numerical runtime
budgets and full R10/R12/Foundation remain open. Further small performance experiments and paused
Editor/UI/Dialogue product work stay deferred. Continue from missing Runtime/game capability,
not another Input validation or timing loop.
