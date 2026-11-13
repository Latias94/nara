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
