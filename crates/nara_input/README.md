# Input

Install `InputPlugin` to project keyboard, mouse, touch, and controller input. The Winit runner
writes `InputEdge` messages automatically. A headless host can write the same messages with
`World::write_message` before updating or stepping the app.

Install `ActionMaps::new(ActionMap::from_bindings(bindings)?)` as a resource for a single-seat game.
Read `ActionState::button`, `axis`, or `axis_2d` in `FixedUpdate`. The two-dimensional accessor
reads `<action>.x` and `<action>.y`. On a frame with several fixed ticks, held values are the same
in every tick. Read ordered actions once per frame after `InputSet::ResolveActions` in `PreUpdate`,
or in `FrameIntentSet::Collect` to include exact steps. Use `ActionState::for_seat` for other seats.

UI adapters inspect `InputFrame::edges()` in `InputSet::RouteUi` and call `consume(index)` for an
edge the UI handles. Register the routing system in `First` and `FrameIntent` to cover frame updates
and exact fixed steps. Skip routing when `InputFrame::is_resolved()` is true. The raw `ButtonInput`
resources still show physical input after consumption. A release always clears an existing gameplay
hold. `TextInput`, `ImeEvent`, `MouseWheel`, `TouchInput`, and `GamepadInput` are Bevy messages.

With `serde`, deserializing an action definition uses the same builder as code-defined bindings.
This RON definition binds Space to `jump`.

```ron
(
    contexts: [(context: "gameplay", enabled: true)],
    bindings: [(action: "jump", input: Key(Space), context: "gameplay")],
)
```

## Native gamepads

Enable `nara_input/gamepad` and install `GilrsInputPlugin::default()` after `InputPlugin` on Linux,
macOS, or Windows. Linux builds require the libudev development package and `pkg-config`. By default,
each controller maps to the first standalone window and primary seat. Use `GilrsDevices` to assign
or unassign a connected controller. Bind controls with `GamepadButton::code()` and `GamepadAxis::code()`.
Controller polling runs before input projection in `First`. Games that wait for explicit frame
requests need to continue driving frames while controller input is enabled. See the
[third-party notice](THIRD-PARTY-NOTICES.md) for controller mapping credits.
