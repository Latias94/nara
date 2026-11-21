# Edited Server Scenes, input ordering and cooked Material dependencies

The tenth review began at `be13116` and includes the Input/Window feature split at `fcbae58`
and the concurrent working-tree repairs. All eight original ninth-round reproductions now pass
in the revised implementations. Eight new issues remain. A ninth issue in the Windows Editor
test target was fixed during the review. The tests exercised public Host, fixed-command, Cook and Editor paths on
Windows x64 with Rust 1.95.0. [Ninth review](2026-11-21-server-residency-input-and-cook-review.md)

## P1 in the Two-Level Game · An idle 8× Server accumulates fixed debt until it faults

Set `profiles.server.runtime.time_scale = 8.0` on the stock game and run `server --serve`.
Sending only `observe` requests reached 119 unprocessed fixed ticks before the process failed.
In two other runs, the Server accepted a marker for tick 201, then received `shutdown`. Both
ended in under a second with `project.run.runtime-faulted`, `close="failed"`, and no marker.
Closing stdin reproduced the failure. Both shutdown routes also failed after the subsequent
50 ms drain wake cap was applied to the working-tree CLI. The same 8× pack, started with `--paused`, accepted
50 advances of 10 ms and one of 2.5 ms. It reached tick 201, placed exactly one marker, and
exited with zero debt. Automatic 2× operation also completed the command and shut down.

At the stock 20 ms fixed timestep, `FrameClock` waits at least 20 ms between automatic drives.
Each such drive at 8× earns at least eight ticks while the Runtime can execute at most five.
Even an idle process must fall behind at that cadence. Select the wake interval from the time
scale, fixed timestep and permitted work per frame, leaving room for normal timer jitter.
Retain the existing debt cap for real overload. Qualify continuous idle operation and accepted
commands through shutdown and EOF. Bevy credits virtual elapsed time before spending it on
fixed steps. [Server wake period](../../samples/two-level-game/src/bin/server/clock.rs#L17) · [Live drive](../../samples/two-level-game/src/bin/server/service.rs#L303) · [Fixed debt check](../../crates/nara_app/src/lib.rs#L913) · [Bevy fixed loop](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_time/src/fixed.rs#L239-L254)

## P1 in the Two-Level Game · Adding one valid wall prevents the shipping Server from starting

Duplicate `wall-inner` as a new root named `alcove-post`, move it to `(110, 90)`, and cook the
game with the original Server content preparation. The authored level passes its gameplay rules.
The cooked pack has the same five Scene Products as the unedited game. Use the bootstrap generated
for that exact pack and start from an empty working directory.

| Edited level | Route | Observed result |
| --- | --- | --- |
| Meadow | Public `HeadlessSession`, original game recipe | Five members pass deep validation. At tick 1 Meadow is `Running`, the extra wall resolves with `CollisionBox`, and marker 1 is placed. The active Scene has 12 entities. |
| Meadow | Shipping `server` and `server --serve --paused` | Both exit 1 at startup, before Ready, with `Server did not retain the live processed Scene members`. |
| Cavern | Shipping `server --serve`, then travel | Meadow starts with 11 entities. Cavern runs with 12, places marker 1 and shuts down successfully. |
| Neither | Shipping entry points and public Host | Startup and the ordinary 11-entity level pass. |

The CLI verifies every receipt member and additionally requires exactly `AUTHORED_LEVEL_MEMBERS`
(11). Game content admission accepts up to 32 entities, checks the required roles, and builds
collision geometry from the walls actually authored. A fixed test-fixture count therefore decides
whether a valid edited starting level can ship. Keep the stock fixture's count in its delivery
test. The executable can validate membership against the admitted compiled Scene and verify its
gameplay roles. Godot's `SceneState` instantiates the node count recorded in each packed scene.
[CLI gate](../../samples/two-level-game/src/bin/server/runtime.rs#L273) · [Fixture count](../../samples/two-level-game/src/levels.rs#L8) · [Game content admission](../../samples/two-level-game/src/gameplay/content.rs#L151) · [Godot Scene instantiation](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/scene/resources/packed_scene.cpp#L243-L275)

## P2 · An initially paused Server with a checkpoint slot never reaches Ready

Set `profiles.server.runtime.paused = true`, leave time scale at `1.0`, and supply a writable
checkpoint path to `server --serve --paused --save ABSOLUTE_PATH`. Send complete `observe` and
`shutdown` lines. After 60.137 seconds the sole reply is the terminal record, with `ready=false`,
`stage="service_startup"`, and `Server did not apply its checkpoint launch before the startup deadline`.
The same initially paused pack without the slot produces Ready, a reply and a clean
shutdown in 0.145 seconds. An initially running pack with the slot and `--paused` becomes Ready
at tick 1 and serves both requests.

The game's slot configuration is a command for tick 1. The startup loop calls `session.frame()`
while the authored Runtime is already paused. These maintenance frames cannot run that tick.
The service creates its stdin reader only after slot configuration completes. Perform the
initialization in a phase that can complete under the selected pause policy. If tick 1 owns
the command, an exact paused step is available. Assert the first Ready still reports the
requested paused state and owns the requested slot. [First-tick command](../../samples/two-level-game/src/gameplay/player_launch.rs#L53) · [Startup driver](../../samples/two-level-game/src/bin/server/service.rs#L211) · [Reader creation](../../samples/two-level-game/src/bin/server/service.rs#L89) · [Paused step](../../src/project_host/runtime/action/session.rs#L240)

## P2 · Native Material defaults can pass deep validation with an Image deferred past first use

Use a custom registered Material codec that declares the default texture as `Reference`. Require
native publication of the Material and give the Scene no independent `Required` edge to that
Image. Project Cook, source revalidation and `validate_deep()` on the reopened three-member pack
all succeed. Direct acquisition returns the expected pixel `[24, 120, 220, 255]`.
`HeadlessSession::start()` then returns `project.scene.material-texture-unavailable`. A Scene
with an actual `nara.material2d` binding reproduces the same refusal.

Changing just the Material edge to `Required`, or letting the Scene separately require the Image,
publishes one native Material and one Image on the first frame. A CPU-only consumer handles the
original `Reference` edge correctly and can acquire the texture on demand. Deep validation checks
the texture Product and codec role. Native publication also requires the Image to have been
prepared. For a native default that must bind immediately, validate its edge's demand as the
TileSet validator already does. Keep the deferred CPU case as a control. Bevy exposes a retry
outcome when a material bind group's dependent GPU resource is still pending.
[Material default check](../../src/runtime_content/materials.rs#L27) · [Native publication](../../src/project_host/runtime/content_residency/materials.rs#L395) · [TileSet demand check](../../src/runtime_content/native.rs#L46) · [Bevy bind-group readiness](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_render/src/render_resource/bind_group.rs#L112-L117)

## P2 architecture · Project Cook still rejects a build dependency from another registered domain

A Scene loads A. Building A introduces `B/sub`, a named auxiliary Product in a different registered
source domain. Supply B's verified group, which contains its primary and `sub`, and register both
domains' Runtime codecs. `Required` and `Reference` edges each fail with `artifact group has no
selected source`, in both submission orders. Adding a source-level reference to B makes the same
Runtime bytes and edges from A pass. After source retirement the four-member, five-Product pack
passes deep validation, direct acquisition of `sub`, and the first frame. Its startup residency
follows the selected edge's demand.

`add_group` tests for a typed collection in the current Scene snapshot before checking the staged
groups. Collections are built only for domains that have records in that snapshot. The same
ordering also breaks repeat submission. In a two-Scene cook, offering a group once from startup
succeeds. Offering that identical digest again when processing the empty second Scene fails.
Check whether the recipe registered the domain, and deduplicate against the Cook-wide staged
group first. At actual acquisition, keep the selected Product, source identity, build seed,
reachability and publication checks. The corrected same-domain path already accepts both orders
and rejects an omitted group. Bevy visits the finished asset's dependencies. Godot's export
traversal recursively deduplicates selected dependencies.
[Per-Scene gate](../../src/project_tooling/project_cook/content.rs#L323) · [Empty domain collection](../../src/project_content/domains.rs#L550) · [Bevy finished asset](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_asset/src/loader.rs#L577-L598) · [Godot export traversal](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/editor/export/editor_export_platform.cpp#L674-L692)

## P2 · A gamepad disconnect can overtake a keyboard press waiting for UI routing

Bind Space and gamepad South to one Button action. Hold South for one tick, pause, press Space,
disconnect the pad and request an exact fixed step before UI has routed the key. The physical
sequence is `#3 Space pressed`, then `#4 South released`. Fixed tick 2 consumes `stop` from #4.
After UI releases #3 to gameplay, tick 3 consumes `dash`. If UI gets one paused frame to resolve
the identical input first, the fixed consumer sees no command. One binding stays held across
the entire device change. Fresh Runtime replay reproduced the first sequence as well.

The deferred resolver blocks subsequent work for a seat when a lifecycle batch is waiting.
It allows a ready disconnect batch past an earlier ordinary press whose routing is undecided.
Keep subsequent changes to an affected aggregate behind the pending decision. Scope any ordering
barrier so other seats can still advance. Test both outcomes for the key. If UI consumes Space,
the last gameplay source really did release. If UI passes Space, the aggregate stays pressed.
[Deferred ordering](../../crates/nara_input/src/timeline/owner.rs#L1950) · [Exact-step control](../../crates/nara_input/tests/input_lifecycle_batches.rs) · [Godot per-device action removal](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/core/input/input.cpp#L697-L705)

## P2 · Leaving or retiring a pointer still starts a cancelled mouse axis

Bind a pointer's left and right buttons to `−1` and `+1`. Press both and observe zero. Send one
`PointerDriverInput::Left` or retire that pointer. Both physical releases correctly carry
`PointerLeft`, yet the semantic consumer receives `Started(+1)`, then `Released(0)`. The same
fixed tick consumes `dash` and `stop`. Window focus loss on those same held buttons publishes
both raw releases and no semantic transition. Ordinary manual releases still yield the expected
intermediate value.

The pointer exit path counts the releases, then publishes them as ordinary entries. Mark the
whole pointer invalidation as one semantic cleanup batch, retaining the individual raw records
and contributions from other pointers. Winit forwards native `CursorLeft` to this path.
[Pointer batch publication](../../crates/nara_input/src/timeline/owner.rs#L1401) · [Winit ingress](../../crates/nara_winit/src/lib.rs#L1750) · [Bevy pointer cancellation](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_picking/src/events.rs#L1305-L1321)

## P2 · Per-frame rounding at half speed changes when an authoritative command executes

Run the cooked Server paused with a 20 ms fixed timestep and a `0.5` time scale. Queue marker 1
for the next fixed tick and use `advance` to supply exactly 40,000,000 ns. All intervals below
are below the frame clamp and neither control reaches a catch-up limit.

| Elapsed frames in ns | Virtual time at observation | Fixed tick | Acknowledged | Markers |
| --- | ---: | ---: | ---: | --- |
| `40000000` | 20,000,000 ns | 1 | 1 | `[1]` |
| `10000000` four times | 20,000,000 ns | 1 | 1 | `[1]` |
| `16666667, 16666667, 6666666` | 19,999,999 ns | 0 | 0 | `[]` |
| `9999999, 9999999, 9999999, 10000003` | 20,000,002 ns | 1 | 1 | `[1]` |

Two more real nanoseconds make the delayed marker appear. At scale `1.0` all four sequences yield
tick 2 and the same marker at 40 ms. In a separate `RuntimeInstance` probe, 10,000 repetitions of
the third sequence produced 9,999 completed ticks and a 19,990,000 ns remainder. The first
sequence produced 10,000 ticks and zero remainder at the same real time of 400 seconds.

The App converts each scaled frame duration independently to integer nanoseconds. The rounding
error from one frame is lost before the next frame arrives. Carry the
fraction across frames and include it in fixed-time continuation if continuation must preserve
the next tick. Check totals at unit speed, paused intervals and frame clamps alongside the
half-speed cases. Godot retains fractional scheduling time in its physics accumulator.
[Virtual delta conversion](../../crates/nara_app/src/lib.rs#L653) · [Fixed accumulation](../../crates/nara_app/src/lib.rs#L892) · [Continuation](../../crates/nara_app/src/time_continuation.rs#L15) · [Godot physics accumulator](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/main/main_timer_sync.cpp#L348-L357)

## Repaired during review · Three stale calls in the Windows Editor library test target

The revised `settle_game_image` takes an iterator of window outcomes and an optional platform
snapshot. Three calls in the overlay tests initially passed separate primary and secondary
outcomes. `rustc --test --emit=metadata` on the Windows Editor library reported `E0277` and
`E0308` at all three calls. The production library passed its type check with the same dependencies.

The three calls now pass `std::iter::once(&skipped)`, `std::iter::once(&clear)` and
`std::iter::empty()` respectively, with `None` for the platform snapshot. After that migration
arrived in the shared working tree, the actual Windows library test target type-checked and
linked. Both the native Game overlay test and the window lifecycle test passed against that
binary on DX12.
[Changed signature](../../editor/src/platform/imgui.rs#L1944) · [Skipped window](../../editor/src/platform/imgui/game_input_tests/overlay.rs#L346) · [Cleared window](../../editor/src/platform/imgui/game_input_tests/overlay.rs#L372) · [Pending image](../../editor/src/platform/imgui/game_input_tests/overlay.rs#L433)

## Ninth-round controls and verification

| Workflow | Result on the revised implementation |
| --- | --- |
| Server shutdown, region verification, level history and checkpoint restore | Half-speed shutdown and stdin EOF both settled tick 600 in about 24.1 s. Zero speed correctly reported a stall at about 10.08 s. `--verify-regions` passed at 0.5×, 1× and 2×. Shutdown during travel recorded Meadow and Cavern, and a fresh continuation from Cavern recorded only Cavern. Replacing the checkpoint file during Restore kept the counter at zero. A successful Restore raised it to one, retained by the next process. |
| CPU Material and same-domain Cook | The stripped Server started, read the required zero-texture Material on its first frame and had no native Material store. A CPU texture case also returned the expected pixels. With B in A's domain, both group orders produced identical 7,366-byte packs and the relocated Player observed the changed transitive Image. Omitting B still failed. |
| Keyboard focus loss and gamepad disconnect | Opposing keys and opposing controls on a single pad cancel as a unit. An extra seat and a surviving device keep their expected contributions. Separately delivered manual releases retain their intermediate values. |
| Detached Game window | The revised native Editor path cleared `WorldPointer` on minimization and destruction of the window showing Game. The same image retained picking when it actually moved between main and detached windows. Minimizing only the main window preserved the detached image. Restoring the Game window regained picking after a fresh camera image was presented. |

The root `cooked_material_publication` nextest run passed **3/3** (`a522e7f7-747f-4490-9056-c10e9878eaa9`).
The `input_lifecycle_batches` run passed **5/5** (`c29179a8-c90a-494c-90c4-c2225fa2fea8`).
Both ran serially against the shared target. The independent Cook suite ran **23/23** cases,
including explicit assertions for the expected failures above. The input probes followed
normalized ingress through `GameplayCommandSet::Consume`. The new CLI was linked to the matching
generated expectation for each pack. The edited-level public Host also validated the pack,
looked up the new wall in the current Scene and observed an actual fixed-step marker. Server
probes used local stdin, independent processes and existing release engine libraries. The time
test additionally ran through a freshly built debug `nara_app`. Native Editor observations
used Windows/DX12 and real window lifetimes. Full-workspace and additional-platform runs were
outside this review.
