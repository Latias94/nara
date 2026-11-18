# Seat Input, Gamepad Lifetimes And Replay

Date: 2026-11-13. Scope: initial source study for R10 in the foundation rebaseline, starting
from `a0ce7d1101943687494759b44398d69b50fd020e`. The implemented successor and its owning
qualification are linked at the end; this study does not establish native controller support.

## Starting Nara pressure

`nara_input` already retains finite endpoint/member/event storage, original press routing,
terminal capacity credits, physical provenance and observed mapping/context lifetimes. Preserve
that owner and its cancellation/physical-return contracts. The starting immutable definition and
World-local state intentionally implement one global map; this study does not reinterpret their
previous qualification as a multi-seat guarantee.

The initial gaps were:

- `InputSeatId` survives raw provenance, but semantic `ActionKey` is only action/context. A
  configuration cancellation has no physical provenance, so recovering seat from provenance is
  insufficient for action identity.
- `InputEventKind::GamepadButton` is admitted as an ordinary timeline event. It has no owned held
  membership, does not participate in `action_transition`, and has neither connection lifetime
  authority nor axis values.
- `ActionValue` carries only a digital bool; `ActionPhase` has Started/Released. The immutable
  definition has no action value-kind or semantic vocabulary revision.
- `ActionCommandMap` copies preconfigured command drafts. Dynamic value/seat conversion already
  belongs in `add_local_gameplay_command_systems` and `LocalGameplayCommands`; those are suitable
  for the first game-specific analog adapter without inventing a universal expression system.

These observations come from `crates/nara_input/src/{lib,state,definition,semantic}.rs`,
`timeline/{types,owner,topology,capacity}.rs`, and `crates/nara_gameplay/src/{lib,queue,command}.rs`.

## Pinned upstream evidence

All reference source was read with `git show` at these exact revisions:

| Repository | Revision | Relevant source |
| --- | --- | --- |
| Bevy | `03ed5a27c92715c8fa2188f31f04e454ec39942f` | `crates/bevy_gilrs/src/gilrs_system.rs`; `crates/bevy_input/src/{gamepad,axis,button_input}.rs`; `crates/bevy_time/src/lib.rs`; `crates/bevy_ecs/src/message/messages.rs` |
| Godot | `7a37606a433c0e7735525037cdf774f043146a21` | `drivers/sdl/joypad_sdl.cpp`; `core/input/{input,input_event,input_map}.{cpp,h}`; `scene/main/scene_tree.cpp`; `main/main.cpp` |

Bevy separates GilRs collection from gamepad state processing. Disconnect removes the Gamepad
component and its button/axis state, but can retain the Entity, settings and backend-ID mapping.
The Entity generation therefore does not identify a unique physical connection. Godot reuses
integer joy slots; those slots likewise do not supply a connection generation. Neither mechanism
provides Nara's required seat/player contract.

Godot retains action contributions by device and binding/event index, then aggregates digital
pressed with OR and strengths with MAX. Disconnect removes that device's contribution while
preserving others. However, removing a binding can release the entire global action, and public
action queries remain global. Adopt the contribution principle, not those unrelated semantics.

Window focus is distinct from physical disconnect. Bevy's inspected gamepad path does not use
the keyboard focus-loss cleanup. Godot's configurable background gamepad policy can retain
joypad input; releasing a mixed action because of a keyboard contribution can also clear its
joypad contributions. Nara must revoke the affected seat/source contribution explicitly rather
than release every action with the same name.

Bevy's inspected axis defaults use a scalar dead zone of 0.05 and a change threshold of 0.01.
Processed events carry scaled values, while the retained getter uses the accepted raw value
clamped to [-1, 1]. Those observations can disagree inside the dead zone. Godot's SDL path
normalizes sticks to [-1, 1] and triggers to [0, 1]; action strength and vector queries perform
additional dead-zone transforms. Arbitrary injected values are not necessarily checked by the
same backend normalization. A Nara event and its semantic state query should expose the same
documented normalized value; backend-normalized input is not OS-raw sampling.

Bevy retains Messages across zero-fixed-tick frames using the TimePlugin update state, but its
digital just-pressed flags clear in PreUpdate. Godot tags action edges in process/physics frame
domains and flushes buffered events around loop phases. Neither is a durable, ordered tick
replay format. Nara's existing fixed command capture/admission remains the appropriate owner.

## Replay is more than a Replay source label

`GameplayCommandQueue` already accepts explicit tick/source/sequence submissions and imposes
canonical ordering. Local drafts target the next open tick. Capturing accepted commands and
submitting equivalent commands in a fresh Runtime bypasses current physical bindings. Synthetic
keyboard/mouse injection instead traverses the current Input mapping and is a physical-pipeline
replay input, not semantic replay.

The queue validates IDs, payload structure, finite numeric values, duplicate keys, tick horizon
and capacities. It does not know whether a game's command name changed meaning. Action-map file
version checks likewise do not establish semantic vocabulary compatibility.

Recorded envelopes can contain the reserved LocalAction source, while public external
submissions deliberately reject that source. Relabeling every record as Replay changes the
source sort key and can change mixed-source ordering. A proper import must preserve the selected
ordered semantic meaning through the original queue and explicitly define compatibility and
live-input isolation; it must not expose arbitrary forging of local source authority.

Capture precedes Ack under ADR0057. It does not automatically mean that bytes reached a file,
nor does it certify success of every later fixed system. Preserve that narrow contract and the
existing fault/quarantine behavior. A replay cannot promise rollback of partial World/native
effects or reverse already confirmed audio.

## Consumer and design consequences

Use one complete two-player runtime consumer. One local seat uses keyboard controls and another
uses a gamepad/axis stream; both use the same action names and map to distinct game-owned player
identities. Verify actual positions/held state through zero/one/multiple fixed ticks, pause and
exact steps. Exercise multiple contributions, focus/capture changes, held rebind, neutralization,
disconnect/reconnect with the same backend label, and stale/foreign connection samples.

Compare a shared global definition with seat-scoped mapping state and per-device callbacks.
Immutable definition sharing is independent of per-seat mutable policy. Connection identity,
local seat identity and durable game player identity are separate. Use the existing Input owner
for physical lifetimes, the definition/state owner for mapping lifetimes, and the existing
gameplay queue for tick command admission. Do not introduce another scheduler, member ledger,
command queue or universal replay kernel.

Record/encode/load the same trajectory into a fresh Runtime. Raw-pipeline replay pins its
declared normalization boundary, mapping and routing policy, and logical device lifetimes.
It also records ordinary/control/configuration operation order and frame boundaries, including
paused input followed by exact Step and Resume; identical tick/event lists alone do not retain
the PreUpdate action-resolution boundary.
Semantic replay carries a stable game vocabulary and preserves meaning after physical rebind.
Unknown types, value-kind mismatch and incompatible vocabulary must refuse before gameplay
consumption. OS handles are not recording identities.

The existing two-level and Collect Game movement adapters demonstrate game-owned held intent;
the independent `bounded_input_workflow` consumer demonstrates Input-to-command and exact-step
timing. Extend those complete paths rather than proving only a numeric axis function. Physical
controller/backend execution, platform delivery, haptics and broader 2D-stick policies require
their own evidence and must not be inferred from normalized injected input.

## Implemented Successor

The [owning unit](../plans/2026-11-13-seat-gamepad-and-input-replay-plan.md) now records the
implemented comparison outcome and executed qualification rather than treating this initial
baseline study as current implementation status. `ActionMaps` replaces the global mutable
resource with bounded independent seat state. Typed button/axis transforms, original gamepad
connection authority and terminal membership extend the existing Input owner. Static command
bindings include seat; a game-owned dynamic adapter forwards changing values through the
original local command collection and queue.

The finite independently compiled two-player consumer owns its recording format. It writes and
loads complete raw operation/drive/configuration boundaries and complete canonical semantic
command batches. The original queue's immutable Replay-only ingress refuses competing live
streams. Actual per-tick and continued-state comparison after physical rebind, Stop with pending
intent and held input, malformed tail rejection and finite import bounds are part of the
executed controls. This does not select a universal recording API, rollback model, vector dead
zone, physical backend or cross-platform floating-point profile. Pinned Bevy/Godot findings
above remain source evidence; Nara's seat/ownership/replay choices are its own contracts.

## November 18: Physical Adapter Scope And The Actual Game

The normalized seat/gamepad/replay unit is already implemented. The current Winit adapter has
no physical controller context, startup enumeration, event sampling or device wake path. A
read-only Windows XInput query observes no connected slots on this machine. That observation
neither proves other controller APIs are empty nor qualifies physical execution. The current
Runtime successor instead closes authored Input through the actual Two-Level Game, which still
compiled its physical keys in Rust and explicitly rejected a startup Input file in its cooker.
The [selected successor](../plans/2026-11-13-seat-gamepad-and-input-replay-plan.md#authored-game-input-through-source-and-player-delivery)
reuses existing source and processed selection rather than rebuilding the completed Input owner.

Pinned Bevy `03ed5a27c92715c8fa2188f31f04e454ec39942f`,
[`bevy_gilrs`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_gilrs/src/gilrs_system.rs),
separates physical device collection from its action consumers. Pinned Godot
`7a37606a433c0e7735525037cdf774f043146a21`,
[`joypad_sdl.cpp`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/drivers/sdl/joypad_sdl.cpp),
covers enumeration, mappings, hotplug, axes, buttons and device close. Those implementations
support a future concrete Winit adapter, not a new generic Input backend interface. Keep the
original Nara connection token at event origin, define physical button/axis meanings and account
for focus/retirement on every associated seat. Gilrs trigger pressure can arrive as button-value
changes; an axis-only adapter would lose it. Winit's indefinite Wait also needs an original-session
device wake or explicit poll deadline; a maintenance wake alone does not request an App frame.

Cached `gilrs 0.11.2`/`gilrs-core 0.6.8` use WGI by default on Windows. Their WGI sampling path
includes a background thread and unbounded channel. Nara's bounded admission cannot establish
an end-to-end queue bound for that implementation. Direct `windows-sys 0.61.2` XInput bindings
are also cached, but cover four Windows slots rather than general controller support. Dependency
cache observations are not resolution/build/hardware results. Defer that choice until a concrete
controller consumer can exercise startup-before-connect, hotplug, sticks/triggers, Wait wake,
focus loss/held recovery, Pause/Step and original-session close. Haptics, battery, sensor and
touchpad behavior remain separate requirements.

## November 18: Actual Game Replay And Scene Barriers

The next [game-owned successor](../plans/2026-11-13-seat-gamepad-and-input-replay-plan.md#actual-game-recording-and-replay-successor)
selects a finite source/headless exact-step profile. It records actual command consumption,
requires the original queue acknowledgement, and then waits for the actual game's Scene activation
and reconstruction. Both paths remain paused between steps, so variable loading waits do not
advance the authoritative fixed or game clocks. This is a selected Nara game policy, not an
upstream determinism guarantee. SaveSlot and audio effects, cooked replay and physical desktop
recording remain separate workflows.

At the pinned Bevy revision above,
[`bevy_asset` dependency tests](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_asset/src/lib.rs)
release controlled reads, update until a concrete condition and compare actual event vectors.
[`SceneInstanceReady`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_scene/src/lib.rs)
has a specific Scene/dependency lifecycle; it does not complete arbitrary work started by ready
callbacks. The
[`bevy_dev_tools` CI driver](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_dev_tools/src/ci_testing/systems.rs)
executes frame-indexed operations without supplying a game-specific Scene barrier.

Pinned Godot separates
[`ResourceLoader` threaded status/result collection](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/core/io/resource_loader.cpp)
from actual
[`SceneTree` replacement and `scene_changed`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/scene/main/scene_tree.cpp).
A pending replacement can change, and an uncorrelated ready notification is not a request token.
Nara therefore confirms the selected target, original new activation and expected game counters
before publishing a structural row. It keeps the existing Scene request and task-return owners.

These mechanisms justify a finite game-owned state machine and explicit observations. They do
not justify a new global validator, static purity analysis, second command queue, or repeated
whole-state checking in ordinary gameplay. State/file checks belong to the opt-in recording and
playback boundaries. Runtime capabilities take priority over further micro-optimization; future
performance work needs a demonstrated cost in a representative workload.

## November 19: Concrete Native Controller Adapter

The normalized Input owner is now independently useful, but it still has no physical controller
collector. The [native successor](../plans/2026-11-13-seat-gamepad-and-input-replay-plan.md#native-gilrs-input-and-playable-controller-successor)
selects a separate optional Gilrs package and an actual source-game consumer. This remains
distinct from UI work and from a claim that every hardware or foreign backend has executed.

The existing Bevy revision `03ed5a27c92715c8fa2188f31f04e454ec39942f` declares `gilrs = "0.11.0"`
as a version range. Its repository has no lockfile proving one resolved Gilrs version. The local
Gilrs 0.11.2/core 0.6.8 packages record VCS revision `07e286e24b046cf39e5c367daa2770b805a64692`;
that package metadata is not a fresh remote verification. The adapter selects the cached
0.11.2 input implementation. Gilrs/core permit MIT or Apache-2.0; their embedded controller
mapping database has its own retained zlib-style notice and does not link the SDL runtime.

Bevy's `crates/bevy_gilrs/src/{lib,gilrs_system,converter}.rs` enumerates existing devices before
ordinary polling, disables automatic filters/cache updates, converts axis-based D-pads and
updates Gilrs state explicitly. It forwards analog changes to Bevy's own settings layer rather
than providing Nara's seat, route or focus lifetime. Its backend IDs are not durable player or
connection identity. Nara retains its own exact connection token and terminal-credit owner.

Gilrs already normalizes axes and platform Y conventions. Its native device dead-zone filter
can produce radial stick neutralization and companion-axis zero events; Nara should not invent
another platform sign correction or an arbitrary epsilon. Axis-based D-pad conversion also
settles the opposite direction on a direct sign change. Filtered neutral observations remain
relevant to reactivation even if they do not produce another public value change. Author-selected
action dead zones remain a separate transform over admitted normalized input.

The public Gilrs cache is not an instantaneous hardware snapshot. WGI initializes its first
reading as a baseline, so an already held control may have no initial event, while an absent
cache entry reports zero/false. Do not replay cached holds on assignment or treat cache absence
as proof of physical release. A strict requirement to observe every unknown control neutral
would also swallow an ordinary user's first press; the selected adapter instead drains the
old queue, suppresses known holds until actual neutral/release, and admits later fresh events.
This preserves the original accepted-member cancellation barrier without claiming unavailable
hardware knowledge. Native hardware execution must qualify its actual focus and reconnect path.

Disable force feedback for this input-only context: the selected package otherwise creates an
additional server thread without an explicit disconnect-exit path. Windows WGI owns an upstream
roughly 8 ms polling loop and stops/joins it in Drop, with no bounded close-time guarantee.
The selected Linux/macOS implementations have different asynchronous thread/drop behavior and
are not synchronous physical-retirement evidence. Linux also requires libudev development files
and appropriate input-device read/write permissions. Retain these platform limits rather than
adding another Nara thread, generic process owner or universal controller backend.
