# nara_gilrs

An optional native gamepad adapter for `nara_app`, `nara_ecs` and `nara_input`. It pins
Gilrs `=0.11.2` (MIT OR Apache-2.0), retaining its default Windows WGI backend. It has no
dependency on the root `nara` crate, a window runner, a renderer or a second Input owner.
The embedded controller mappings retain their [original third-party notice](THIRD-PARTY-NOTICES.md).

```rust,no_run
use nara_app::App;
use nara_gilrs::{GamepadButton, GamepadRoute, GilrsInputPlugin};
use nara_input::{InputPlugin, InputSeatId, InputWindowId};

let mut app = App::new();
app.add_plugin(InputPlugin)?;
app.add_plugin(GilrsInputPlugin::new(Some(GamepadRoute::new(
    InputWindowId::new(1),
    InputSeatId::PRIMARY,
))))?;
let south_binding_code: u16 = GamepadButton::South.code();
# Ok::<(), Box<dyn std::error::Error>>(())
```

`GilrsInputPlugin::new` only stores configuration. Plugin build registers the original
close participant before acquiring one Gilrs context. Its NonSend state is polled in
`CoreStage::First` before `InputSet::ProjectTimeline`. The adapter creates no polling
thread, scheduler, window, endpoint or seat. Force feedback is disabled at construction
to avoid the optional Gilrs FF server thread; rumble and battery APIs are not exposed.

The host must connect and focus the exact supplied Input endpoint. A default route applies
to each newly connected native device. Without a default route, devices remain available
in the inventory until explicitly assigned. At most 16 devices and 256 mapped events per
frame are admitted by default. Public configuration permits 1–64 devices and 1–4096 events
per frame. Extra native devices fault rather than silently evicting accepted devices.
Gilrs and OS internal discovery storage is outside this adapter's allocation bounds.

For two seats, read current tokens and assign them through the same `GilrsDevices` Resource:

```rust,no_run
use nara_ecs::ResMut;
use nara_gilrs::{GamepadRoute, GilrsDevices};
use nara_input::{InputSeatId, InputWindowId};

fn assign_two_seats(mut devices: ResMut<GilrsDevices>) {
    let tokens: Vec<_> = devices.connected().iter().take(2).map(|d| d.token()).collect();
    for (index, token) in tokens.iter().enumerate() {
        devices.assign(token, GamepadRoute::new(
            InputWindowId::new(1), InputSeatId::new(index as u32),
        )).unwrap();
    }
}
```

Each target seat still needs an original, focused Input endpoint. `unassign(&token)` removes
assignment for that native connection. This is finite current state, with no command queue.
Foreign and disconnected tokens are rejected. Reconnection renews the token even if Gilrs
reuses its native label. Names are bounded display metadata; no hardware identity is persisted.
Route changes, including unassign/reassign between polls, terminate the original connection.
Reassignments take effect after queued events drain; a continuously busy native stream can
delay a new route. Assignments written by ordinary systems take effect on the next poll.
When original disconnected Input connections still occupy every slot, activation waits for
their actual consumers to retire them. The bridge retries on a later drained poll without
replaying held controls. A zero-slot or entirely live full pool remains an explicit error;
the adapter neither increases the selected limit nor releases the old owner's storage.

The adapter drains queued native events into a private physical-state shadow before admitting
a new route. Startup, route changes, focus loss/regain and endpoint replacement suppress known
held buttons and nonneutral axes until release/neutral. It never replays cached holds into Input.
The endpoint observation's original invalidation sequence detects focus loss/regain between
polls; it grants no input authority. Original `GamepadConnection` values and timeline admission
remain authoritative. Button repeat and analog `ButtonChanged` samples do not duplicate digital
edges. Analog triggers use Gilrs's threshold-generated press/release events.

Gilrs state is an event cache, not a synchronous hardware snapshot. WGI may initially provide
no event for a held control. Unknown controls accept fresh native events after the drain boundary;
instantaneous hardware snapshots and detection of every unknown startup hold are not promised.
Previously observed nonneutral axes remain suppressed until an actual neutral observation.

Gilrs SDL mappings and normalization remain enabled. Bounded explicit filtering applies Gilrs's
`axis_dpad_to_button` followed by its radial `deadzone`, including companion-axis neutral events.
The adapter manually updates the original cache and counter; it does not enable the default
filter loop or Jitter filter. Redundant filtered axis samples still preserve neutral observations.
There is no platform-specific Y inversion. Authored `ActionTransform` dead zones remain explicit
and are applied after native normalization; the native radial filter does not substitute for them.
Portable `GamepadButton` and `GamepadAxis` codes use deliberate mappings independent of Gilrs
discriminants. Shoulder buttons map to Gilrs `LeftTrigger`/`RightTrigger`, while Nara trigger
buttons map to Gilrs `LeftTrigger2`/`RightTrigger2`. No analog trigger binding is implied by a
consumer using only button edges.

Close quiesces ingress, disconnects original Input connections, clears inventory and drops the
original Gilrs on its owning thread, including configured retirement and startup/runtime failure.
Admission and terminal errors enter the managed runtime fault/close evidence paths. No native
thread join receipt is invented. The pinned Windows WGI destructor signals and joins its native
backend thread; that join has no bounded-latency guarantee. The pinned Linux/macOS backends do
not expose synchronous retirement of all their internal threads. The concrete plugin therefore
refuses acquisition before creating Gilrs on those platforms. Their source can compile, but
dropping an asynchronous context is not reported as managed physical retirement. A future
foreign native contract must resolve that lifetime before enabling Runtime acquisition.

Deterministic unit tests execute the same private bridge reducer with actual Gilrs `EventType`
values and small private state snapshots. Lifecycle tests exercise the original close participant
with an owner-thread drop probe. An ignored Windows-only test initializes actual Gilrs and closes
without requiring a controller. Tests have not established physical controller execution or
Linux/macOS native runtime qualification. Run the ignored test explicitly for native Windows
initialization evidence; physical button, stick, reconnect and multi-seat checks require hardware.
