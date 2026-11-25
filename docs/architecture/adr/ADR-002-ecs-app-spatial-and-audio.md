# ADR-002 ECS, application, spatial state, and audio

**Accepted on 2026-11-24**

- Expose full `bevy_ecs 0.20` through `nara_ecs`, including `#[require]`, component hooks,
  observers, and `ChildOf`/`Children`. Use thin `bevy_tasks 0.20` pools for `Future`-based work.
- Provide `App`, plugins, schedules, `Time`, and graceful `AppExit`. Support both `App::run` and a
  deterministic headless fixed step. Keep a configurable fixed-step catch-up policy.
- Put `Transform2d` and `GlobalTransform2d` in `nara_scene`. Make both ECS components and require
  `GlobalTransform2d` when inserting `Transform2d`. Propagate changes along the hierarchy only
  through affected branches, including reparented subtrees. Keep room for a parallel 3D transform.
- Put W3C physical key codes, text, mouse, wheel, touch, and lightweight `ActionMap` in `nara_input`.
  Offer optional `gilrs` gamepad support. Remove `nara_gameplay` and its untyped command queues.
  Use RON for authored action maps. `nara check` compiles the selected file with the same input
  rules as the source runner. The existing cooked input wire format remains JSON.
- Put ECS playback, mixer, bus, spatial, and meter types under `nara_audio`. Use `kira` internally
  with a headless mock backend and keep `kira` types out of public APIs and scene data.
- Grow `nara_physics2d` from `nara_rapier2d`. Synchronize transforms and collision events in
  `FixedUpdate`. Keep the shared core free of a physics dimension.

Retire `nara_tasks`, the ECS restrictions, `fault_route`, thread-local quarantine, and private
runtime driver tunnels as this phase takes ownership. Qualify the API with the reference game and
a complete 2D example of at most 100 lines.
