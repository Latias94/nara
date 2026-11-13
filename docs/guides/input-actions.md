# Input Actions

`nara_input` maps normalized keyboard, mouse and gamepad input to named actions. Gameplay reads
`SemanticActions` for seat-local held values and Started/Changed/Released outcomes.
`ActionOutcomes` exposes each physical binding edge, including its route and event provenance. Gameplay command mapping remains
game-owned through `nara_gameplay::ActionCommandMap`.

## Code-First Definitions

Build a complete finite definition, declare its contexts and install independent World state:

```rust
use nara_core::ByteLimit;
use nara_input::{
    ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBuilder, ActionMapLimits, ActionMaps,
    InputPlugin, KeyCode,
};

let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
builder.declare_context(ActionContext::gameplay(), true)?;
builder.bind(ActionBinding::key(ActionId::new("jump")?, KeyCode::Space))?;
let bindings = builder.compile()?.into_bindings();
let map = ActionMap::new(
    bindings,
    ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
)?;

let mut app = nara_app::App::new();
app.add_plugin(InputPlugin)?;
app.insert_resource(ActionMaps::new(map))?;
```

The builder starts with no contexts. `ActionBinding::key` and `ActionBinding::mouse` select
`gameplay` by default; declare it explicitly or use `with_context`. Unknown contexts, duplicate
context declarations, duplicate `(action, context, input)` bindings and exceeded limits fail
during construction.
Different actions may share a physical input within the fanout limit. Binding order is preserved.

Definitions are immutable. A clone of `ActionMapBindings` shares its actual definition owner;
`ActionMap` itself is a move-only value for one independently mutable seat. The bounded `ActionMaps`
resource owns up to `MAX_SEAT_ACTION_MAPS` such values; `ActionMaps::new` installs PRIMARY.
Use `get_mut(seat)` to change its contexts or bindings, `insert(seat, map)` to install or replace
that seat, and `remove(seat)` to recover its original map. A refused insert returns that supplied
map through `ActionMapInstallError::into_map`. A seat without a map has no gameplay mapping;
it does not borrow PRIMARY policy. Raw input and UI routing remain independently available.

Create separately admitted map state for another seat or World. `required_state_bytes()` is the
conservative logical allowance for contexts and retained lifetime observations; it is not a
measurement of the entire input pipeline's memory.
`ButtonInput` and `SemanticActions` also own live state. Capture their public transitions, pressed
observations or semantic outcomes for recording instead of copying the runtime resources.

## Ingress And Bounded Lifetimes

`InputTimeline` owns connected endpoints, pointer children, gamepad connections, accepted
button/axis members and one ordered physical/synthetic event log. Configure smaller finite
capacities before installing `InputPlugin`:

```rust
use nara_input::{InputLimits, InputTimeline};

let limits = InputLimits::new(64, 4, 8, 16)?; // events, endpoints, pointers, members
let mut app = nara_app::App::new();
app.insert_resource(InputTimeline::with_limits(limits)?)?;
app.add_plugin(nara_input::InputPlugin)?;
```

The configured pools include the once-connected default synthetic endpoint and pointer. Event
capacity is between three and `MAX_INPUT_TIMELINE_ENTRIES`; three slots reserve the default
lifecycle controls before any ordinary event. Counts may impose different admission ceilings:
an available event slot does not imply another live member or endpoint is available. Queued,
held and terminal-pending members remain charged while their internal consumers need them.
Connected idle records intentionally retain their slots until explicitly retired.
`with_gamepads(count)` configures the separate finite connection-slot limit, including zero when
controllers are not selected. A slot does not allocate independent event or terminal storage.

A native adapter connects its actual labels once before delivering events. Managed Hosts use
driver scopes so Runtime state gating remains part of admission:

```rust
use nara_input::{
    ButtonDriverInput, InputPointerId, InputSeatId, InputWindowId, KeyCode,
    apply_keyboard_driver_input_at, connect_input_endpoint, connect_input_pointer,
};

let mut runtime = nara_app::RuntimeInstance::start(app)?;
let window = InputWindowId::new(7);
let seat = InputSeatId::PRIMARY;
runtime.with_driver_scope(|scope| {
    connect_input_endpoint(scope, window, seat)?;
    connect_input_pointer(scope, window, seat, InputPointerId::PRIMARY)
})?.output?;
runtime.with_driver_scope(|scope| {
    apply_keyboard_driver_input_at(scope, window, seat, ButtonDriverInput::Press(KeyCode::Space))
})?.output?;
```

The default helpers without `_at` send synthetic events through the already connected default
endpoint. They share the same admission, global sequence, mapping/context staging and termination
rules. `InputEventOrigin` distinguishes origin without creating a second mutation queue.
`ButtonInput` and `PointerState` expose observations; they have no independent public writers.
Each button transition and action provenance names the original accepted press sequence, so a
release cannot end another endpoint's or a later press's contribution.

Ordinary refusal leaves accepted state and event order unchanged. Admission reserves the complete
future release/leave/lifecycle obligations and their sequence space. Focus loss or physical
retirement can therefore finish accepted holds when ordinary storage is full. Presentation unbind
preserves physical holds; `InputWindowEvent::SurfaceRetired` ends the endpoint, while
`retire_input_pointer` (or direct-App `InputTimeline::retire_pointer`) ends one pointer child.
Pending consumers or prepared presentation tokens keep retiring records charged and prevent label
reuse. Drain or discard external queued
operations before reusing raw labels; looking up a current generation cannot identify an old
originless OS event. Frame cleanup releases observation copies, while paused action work retains
its own original members until resolution or the owning Runtime ends.

A UI Host connects its logical endpoint before `install_ui_surface`, including when native window
creation happens later. The Surface keeps that exact Input lifetime through suspension and geometry
changes. An explicit restore may attach a newly connected incarnation with the same labels; that
handoff clears predecessor interaction state. Late preparation or cleanup through the old Surface
cannot adopt or terminate the replacement connection.

## Gamepad Connections And Axis Values

Connect the endpoint first, then call `connect_gamepad_input(scope, window, seat, backend_label)`.
Keep the returned `GamepadConnection` for every `apply_gamepad_button_input`,
`apply_gamepad_axis_input` and `disconnect_gamepad_input` operation. The backend label is metadata;
it cannot authorize delayed events. Reconnect creates a new non-reused generation, even for the
same label. Foreign and retired connections refuse before accepting a sample. Synthetic
controllers use `connect_synthetic_gamepad_input` through the same bounded owner.

`AxisValue::new(value)` accepts finite normalized scalars in [-1, 1] and canonicalizes negative
zero. `AxisDeadZone::new(threshold)` accepts [0, 1); its explicit scalar transform maps values
inside the threshold to zero and remaps the remainder continuously. Choose the threshold in the
game's binding policy. Hardware normalization, calibration, radial/vector dead zones and a
physical controller backend are separate work.

Use `ActionBinding::gamepad_button` for digital actions, `gamepad_axis` for a transformed scalar,
and `key_axis` for a signed digital contribution to an axis action. One action/context must have
one value kind. `ActionValue` is either Button or Axis, including in serialized binding validation.
`SemanticActions::value(seat, action, context)` returns the retained aggregate;
`pressed(seat, action, context)` tests whether it is active.

Button contributions combine with OR. Axis contributions sum in admitted-member order using a
wide accumulator, then clamp once to [-1, 1]. Zero is inactive. A zero-to-active aggregate emits
Started, a changed active value emits Changed, and active-to-zero emits Released. Opposing held
controls can sum to zero while both physical contributions remain held; releasing one can start
the aggregate again. Disconnect neutralizes only the original connection's contributions and
cannot cancel another controller or key that still contributes to the same seat/action.

## Context Changes And Rebinding

`InputSet` exposes ordering anchors. In `CoreStage::First`, the order is ProjectTimeline,
PublishObservations, RouteUi, then StageActions. Configure seat maps before StageActions to
affect that frame's observed lifetime; configure route-changing policy before ProjectTimeline.
ResolveActions runs separately in PreUpdate. `before`/`after` constrain one schedule, and systems
inside a set still need their own relative ordering when it matters.

A custom UI adapter joins RouteUi and may call `InputTimeline::route_pending_ui_input`. Its
callback receives the original pending entry and returns `Some(InputUiDisposition::Consumed)`
or `RejectedClosed` after its routing decision, or `None` for another adapter/gameplay fallback.
The timeline records original held-member ownership synchronously. No bare sequence, delayed
completion or presentation receipt is accepted by this interface. An adapter must validate the
entry's captured route against its actual UI state; the finite replay consumer's headless policy
is not pixel or native UI evidence. Earlier decisions remain completed if the callback panics.

Enable and disable only declared contexts on the selected seat's map. Both operations report
whether the state changed and borrow the context identity, rejecting an unknown name. Disabling
a held context emits one semantic release with
`SemanticActionCause::ContextChanged` and no physical provenance. Re-enabling that context does
not synthesize a press: release the physical input and press it again.

Compile a complete replacement before calling `map.replace_bindings(bindings)`. A compilation
error leaves the current map unchanged. Installing compiled bindings begins a new mapping lifetime
even when restoring a previously used handle. Queued presses and held contributions from the old
lifetime cannot revive the action. Rebinding uses the state's existing fixed allowance, including
the period when older queued lifetime observations remain alive.

Removing a map and observing that absence also ends its staging lifetime. Restoring the original
owned map cannot restore old holds. An axis already non-neutral under a missing or expired map
must become neutral before a new member can activate the replacement. Later samples may update
an already adopted original member, but cannot newly adopt that stale member.

## Source Project Selection

Select the PRIMARY startup file in `nara.toml`:

```toml
[input]
action_map = "input/player.actions.json"
```

This path is relative to the project root and is read with exact spelling. It does not inherit
`paths.assets`. Profiles can explicitly clear the selection using their existing Input overlay.
The selected Rust recipe must include `InputPlugin`.

The file is a strict canonical version-one JSON envelope with kind `action_map`. It contains a
complete intrinsic primary Product, explicit contexts and their initial enable states, and ordered
key/mouse/gamepad bindings with explicit transforms. The [nonempty golden](../../crates/nara_input/tests/fixtures/formats/v1/action-map.json)
shows the complete shape. Construct and validate these bytes through `ActionMapFileCandidate`;
author its non-nil source UUID once and preserve that identity across content edits. Loading never
mints identity. A sidecar is optional; when present its identity and canonical location must agree
with the file and other admitted project sources.

Source capture retains the actual compiled definition and file receipt. The Host installs the
selection before Scene materialization and the first Startup stage. A plugin that explicitly
installs a PRIMARY map, including an empty one, competes with the file selection and is rejected.
An explicitly configured different seat may coexist with the selected PRIMARY map.
Keep game-specific action-to-command bindings in the recipe when using file-defined physical
bindings. With no file selected, code-first physical configuration remains valid.

Scene travel preserves the original global owner, context state and delivered held lifetimes.
Preparation verifies the original Input source and optional sidecar before reusing that owner.
Travel refuses changed or missing source. Restart the Runtime to use changed global bindings;
restoring the original admitted source permits a retry. Once a target is Ready, commit
uses its captured facts. Check and source Export include the selected file in the same content
receipt as the actual Scene closure.

## Processed Runtime Selection

Source-independent Runtime configuration selects a complete primary Product through
`RuntimeConfiguration::input.action_map`. This is a global startup root, independent of Scene
roots and travel permissions. The executable recipe must select both `InputPlugin` and an actual
`ActionMapDomain` decoder through `add_runtime_asset_codec`. The first-party codec is
`ActionMapRuntimeCodec`; a recipe may explicitly select another implementation of the same
typed family and format. The base preserves the actual selected implementation and its frozen
limits. Product data cannot choose a different decoder.

Cook an admitted source definition by consuming it into the processed CPU asset:

```rust
use nara_asset::RuntimeAssetCodec;
use nara_input::{ActionMapAsset, ActionMapFileCandidate, ActionMapRuntimeCodec};

let source = ActionMapFileCandidate::decode_json_bytes(source_bytes)?;
let (product, definition) = source.into_parts();
let asset = ActionMapAsset::new(product, definition)?;
let processed_bytes = ActionMapRuntimeCodec::encode_runtime(&asset)?;
```

The processed envelope uses kind `runtime_action_map`, version one. Source and processed readers
reject each other's kinds; both use the same finite declaration compiler. The processed asset
has no Required dependencies, filesystem capability or mutable World state. See the
[processed golden](../../crates/nara_input/tests/fixtures/formats/v1/runtime-action-map.json).
Construct the verified catalog and independent expectation as described in the
[Runtime base guide](runtime-content-base.md#global-startup-input).

`HeadlessRun::from_runtime_content` and `DesktopRun::from_runtime_content` install the PRIMARY
selection before Scene materialization and Startup, with the same explicit-code conflict rule
as the source path. Scene travel preserves its actual acquisition, context state and held mapping
lifetimes. A Scene may independently read the same Product as Required typed CPU data, with its
own acquisition charge; that read cannot install or reconfigure global Input. With no selected
Product, code-first Input remains valid.

## Ownership And Scope

`ProjectContentSnapshot::input_action_map()` borrows the source owner. Its `bindings()` handle
retains actual compiled data and the original reservation after the snapshot is dropped. The
last escaped handle releases that custody. World-local context/revision state has a separate
reservation; queued lifetime observations can retain it after a resource is replaced.

`RuntimeContentBase::acquire_startup_input()` returns the optional `RuntimeInputContent` owner.
Its `bindings()` handle retains the actual typed acquisition and original base reservation after
the content handle or base variable is dropped. A second acquisition of equal Product data has
its own charge and owner identity. First-party startup separately reserves World-local state
from that same base; replacing Scene collections does not release or recreate that state.

Independent Hosts install a prepared global selection through `finalize_input_selection` and the
existing `RuntimeCandidateScope`. This receiving operation finalizes once, distinguishes pristine
Input defaults from explicit configuration, and is unavailable through a post-Startup runtime
scope. Ordinary source Hosts perform this step as part of startup.

## Commands And Recording

`ActionCommandBinding::new` selects PRIMARY by default; use `with_seat` for another seat.
Static mappings match the complete `(seat, action, context, phase)` identity. For changing analog
values or game-owned player targets, an ordinary game adapter may read semantic outcomes and
construct payloads through `LocalGameplayCommands`. The original `GameplayCommandQueue` still
owns next-open-tick admission, canonical ordering, Capture and acknowledgement.

Physical replay must record configuration/control operations and supplied drive boundaries as
well as input: Pause, exact Step and Resume do not have interchangeable staging behavior. A
semantic recording can preserve meaning after a physical rebind, but must validate its own
bounded vocabulary, version and value kinds before submission. Runtime resources are not
recording artifacts.

For an isolated complete semantic recording, construct the original empty queue with
`GameplayCommandQueue::for_replay(settings, source_id)`. Only the exact Replay source is admitted;
local mapping, AI, Test, External and another Replay ID refuse before command storage or source
sequence mutation. This is an immutable ingress selection, not a switch over live pending work.
Disable recorded live producers in that recipe. To retain mixed-source execution order through
one replay stream, import the complete original canonical order with monotonically assigned
recording ordinals and retain original provenance as data. Relabeling source kinds alone is
insufficient. Arbitrary trusted Rust side effects remain outside this admission policy.

The independently compiled [seat/replay consumer](../../tests/fixtures/module-consumer/tests/seat_input_replay.rs)
owns the first bounded recording format and game-state comparison. This does not introduce a
universal recording schema, rollback, network authority or a physical controller backend.
Chords and a visual action-map editor remain separate workflows. Mutable action-map state and
bounded ingress storage have separate owners and allowances. See [ADR 0041](../architecture/adr/0041-input-routing-actions-text-focus-and-accessibility.md)
for the owning contracts, the [source plan](../plans/2026-10-30-input-definition-and-source-admission-plan.md)
and [processed plan](../plans/2026-10-30-processed-input-startup-and-process-proof-plan.md) for their
actual qualification scopes. Target-build selection and complete packaged delivery remain separate.
