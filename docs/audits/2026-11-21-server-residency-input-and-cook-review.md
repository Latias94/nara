# Server shutdown, Material residency and gameplay input review

The ninth workflow review started at `04a71e1` and followed the sustained Server, fresh-process
continuation and Editor changes through `4e7554b`. Eight findings reproduced. A
Server can retire with an accepted command still queued during orderly shutdown. A multi-profile
Host can also refuse to start a deeply validated Server pack when gameplay reads a CPU Material.
The previous review's two outstanding authoring cases now pass through the registered panel.
[Previous review](2026-11-20-recovery-format-and-runtime-input-review.md)

## P1 · A slow Server retires before an accepted command's fixed tick

Run the cooked Two-Level Game with a 20 ms fixed timestep and `profiles.server.runtime.time_scale`
set to `0.5`. At Ready, submit the following line to `server --serve`. The reply is `accepted`
for tick 600, within the configured queue horizon. Send `shutdown` immediately, or close
stdin. Both runs exit 1 with `Server could not settle admitted work before the shutdown deadline`.
The terminal receipt still shows one retained command and no marker. Keeping the process open
long enough for the same command to execute places exactly one marker.

```json
{"request":"command","command":{"operation":"marker"},"after_ticks":600}
```

| Time scale | End condition | Wall time | Final tick | Acknowledged / retained | Markers |
| --- | --- | ---: | ---: | --- | --- |
| 0.5 | Immediate shutdown | 22.06 s | 549 | 0 / 1 | `[]` |
| 0.5 | Immediate stdin EOF | 22.10 s | 549 | 0 / 1 | `[]` |
| 0.5 | Observe execution, then shut down | 24.18 s | 602 | 1 / 0 | `[1]` |
| 1.0 | Immediate shutdown | 12.10 s | 600 | 1 / 0 | `[1]` |

`Service::drain` adds 599 fixed intervals to a ten-second wall-clock deadline. This gives the
process 21.98 seconds. At half speed, reaching tick 600 takes about 24 seconds. The public
protocol promises to settle admitted work during live shutdown and EOF. Base the wait on the
Runtime's conversion between elapsed time and remaining fixed ticks, or bound the time spent
without observable progress. Exercise both orderly exits at the admitted horizon and keep a
bounded failure for a genuinely stalled simulation. Bevy's virtual clock applies the speed
factor before its fixed loop consumes accumulated time. [Shutdown contract](../../samples/two-level-game/README.md#operate-the-server) · [Drain deadline](../../samples/two-level-game/src/bin/server/service.rs#L299) · [Queue horizon](../../crates/nara_gameplay/src/queue.rs#L37) · [Bevy virtual time](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_time/src/virt.rs#L249-L254)

## P1 · A CPU-only Material prevents a multi-profile Host from starting a Server pack

Use a Host compiled with `runtime-2d` and the ordinary typed CPU codec for `Material2dDomain`.
Cook a Server Scene with one gameplay component. Its Product requires a Material with zero
texture slots. The source also contains a native `nara.material2d` component which Server Cook
strips. The resulting catalog is identical to the catalog produced with that component absent
from the source. Move the pack away from the source project and start a fresh process.

| Preset and dependency | Deep validation | First frame |
| --- | --- | --- |
| `Server`, `Required` Material | 3 members pass, direct CPU read succeeds | `HeadlessSession::start()` returns `project.scene.residency-unavailable` |
| `Server`, `Reference` Material | 3 members pass | Starts with Material decoding deferred |
| `Server`, `Required` TileSet → Image using CPU codecs | 4 members pass | Both typed collections are readable with native TileSet and Image stores absent |
| `LocalHeadless`, `Required` Material | 3 members pass | Material is available through the typed collection and native store |

Image and TileSet preparation select their native publications by codec role. The Material
residency path instead counts every decoded `Material2dDomain` value and requires
`Assets<Material2dAsset>`, `AssetServer` and the associated state and event resources. Server
correctly omitted the native plugin. Select Material publication from the consuming recipe or
Scene binding and keep CPU acquisitions readable on their own. Qualify the original three-member
pack through the first frame and check native binding, replacement and retirement with the local
recipe. This reproduction used the multi-profile Host. The dedicated `runtime-core` executable
and the stock five-Scene sample are separate feature and content selections. Bevy checks for a
render sub-app before installing its material render systems. Godot's Server exporter applies
its selected resource policy during customization. [CPU codec contract](../../src/product.rs#L458) · [Image and TileSet selection](../../src/runtime_content/prepared.rs#L223) · [Material count](../../src/project_host/runtime/content_residency/stock.rs#L59) · [Store requirement](../../src/project_host/runtime/content_residency/stock.rs#L187) · [Bevy material plugin](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_sprite_render/src/mesh2d/material.rs#L339-L359) · [Godot Server resource policy](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/editor/export/dedicated_server_export_plugin.cpp#L114-L129)

## P2 · Focus loss and gamepad disconnect can start a gameplay action

Bind A and D to `−1` and `+1` on one action. Bind its semantic `Started` to `dash` and
`Released` to `stop`. Press A, press D, and let the game observe the neutral aggregate. Losing
focus in the next frame emits `Started(+1)` and `Released(0)`. The same fixed tick consumes
both `dash` and `stop`. Reversing press order emits `Started(−1)` instead. A normalized
gamepad disconnect with its stick and D-pad cancelling each other produces the same extra start.

One focus loss or device disconnect produces an ordered batch of physical releases. The semantic
owner recomputes the sum after removing each contribution, exposing a value that existed only
midway through the cancellation. Preserve the individual binding releases and compare each
affected action's aggregate before and after the whole lifecycle operation. Keep contributions
from surviving endpoints. Pressing only D and losing focus must still emit one `stop`. Releasing
A and D deliberately in two separate frames must still emit `dash` and then `stop`. Both controls
were exercised. The input-capacity control also delivered precisely one final `stop` after a
third binding was refused. Godot removes the disconnected device's state before updating each
action cache. [Nara action contract](../architecture/adr/0041-input-routing-actions-text-focus-and-accessibility.md) · [Focus-loss batch](../../crates/nara_input/src/timeline/owner.rs#L1577) · [Device batch](../../crates/nara_input/src/timeline/gamepad.rs#L115) · [Aggregate transition](../../crates/nara_input/src/semantic.rs#L168) · [Godot disconnect](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/core/input/input.cpp#L697-L705)

## P2 · A detached Editor Game window can still resolve world coordinates after destruction

Present a camera image in a detached Dear ImGui window on Windows/DX12. Minimize that window.
For three consecutive frames the main window presents successfully and the detached window has
zero renderer outcomes. `WorldPointer::window_to_world` at the old image center still returns
`Some(Vec2(0.0, 0.0))`. Restore the window and present the image again. Closing the detached
window removes it from the platform registry and Win32 `IsWindow` returns false, yet the same
query still succeeds. In the control, presenting a frame in that window with the Game image
removed immediately makes the query return `None`.

The locked Dear ImGui backend skips minimized viewports. Nara derives secondary window outcomes
from the viewports the renderer observed. The Editor compares the previous composite against
those outcomes. The Editor reads the secondary window IDs from a snapshot that also includes
their live platform state. Reconcile the incumbent composite with the target's
current platform lifetime and minimized state. Preserve an image that has actually been
presented on another window. The probe used the original displacement predicate from `ec035f0`,
real native windows, WGPU composite receipts and `WorldPointer`. The downstream Editor Play path
was traced in source. The measured effect is a stale coordinate conversion. Bevy's picking
API associates a pointer location with a specific render target. [Displacement predicate](../../editor/src/platform/imgui.rs#L769) · [Platform snapshot](../../editor/src/platform/imgui.rs#L1215) · [Secondary outcomes](../../crates/nara_render_wgpu/src/host/imgui.rs#L506) · [Game image settlement](../../editor/src/platform/imgui.rs#L1943) · [Deferred Game presentation](../../src/project_host/runtime/editor/game_view.rs#L970) · [Bevy picking target](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_picking/src/pointer.rs#L224-L248)

## P2 · A failed checkpoint Restore increments the counter carried into the next Server

Save in the incumbent Server, place an unsaved marker, enqueue Restore, and capture the paused
Server with that command pending. Start the candidate with the same checkpoint path, retire
the incumbent and claim the slot. Advance one 20 ms frame. Once the candidate reports
`Preparing`, replace the checkpoint in the test directory with a byte-identical copy having a
different file identity. The candidate reports `Degraded` and `restored checkpoint baseline
changed`, but `restores` has already advanced from 0 to 1.

`retry` returns the candidate to `Running` and clears the error. It now satisfies the existing
handoff test's completion predicate for Restore. The next Save is correctly refused and the
replacement file retains its identity and bytes. One subsequent successful Restore allows Save
again. Capture and start a third Server and its Ready record reports `restores=2`. In the control
without a replacement, the first Restore allows Save and the third process reports `restores=1`.

Scene commit increments `travel_count` and `restores` together, before reconstruction and before
`Storage::accept_load` can establish the new checkpoint baseline. Count a completed checkpoint
Restore after both succeed. The Scene commit can still count as travel. Keep the file protection
from `2e8bd1f`, and check the counter through a failed Restore, Retry, a successful Restore and
fresh-process continuation. [Persistence contract](../architecture/adr/0027-save-game-and-runtime-persistence.md#L40) · [Restore count and binding](../../samples/two-level-game/src/gameplay/travel.rs#L510) · [Consumer predicate](../../samples/two-level-game/tests/support/server_delivery/checkpoint_handoff.rs#L273)

## P2 architecture · Project Cook cannot admit a verified group reached only by a built Product

Use the chain `Scene → A → B → Material → Image`, with A and B built from different source
files by the same external asset domain. First let A's source importer also declare B. Project
Cook succeeds. Remove that source declaration while keeping A's encoded bytes and its `Required`
edge to B identical. Project Check still passes and both verified groups are cache hits. Supply
the two current groups explicitly to project Cook. Both insertion orders fail with
`artifact group has no selected source`. Supplying only A reaches `MissingDependencyProduct` for B.

`add_group::<D>()` documents its requirement that the source already appear in the typed project
snapshot. The snapshot retains the identities of sources visited by the importers. A dependency
first declared by a built Product can select a stock Material or TileSet, but an additional
external Product cannot use this route, even with its verified group in hand. With the same
compiled Scene, the two groups and the control's stock payloads, `CookedContentBuilder` produces
the **same 7,354-byte pack** as the successful project Cook. Fresh processes reading the project
packs observed both external Products, the Material and the expected Image pixels before and
after changing the transitive PNG. Changing B or the PNG invalidates the old project selection.

Let the project Cook retain an explicitly supplied typed group until a Product edge reaches it.
At that point resolve the source identity and check the current build seed, output ownership,
file limits and publication fence. Accept the two insertion orders and keep refusal for ambiguous
owners and stale groups. This extends the documented snapshot contract for a concrete contributor
workflow. Bevy records dependencies found on a finished asset separately from loader dependencies.
Godot queries the imported resource for dependencies and traverses them during export.
[Snapshot identities](../../src/project_content/catalogue.rs#L576) · [Typed group admission](../../src/project_tooling/project_cook/content.rs#L277) · [Cook contract](../architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md#L177) · [Bevy finished asset](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_asset/src/loader.rs#L577-L598) · [Godot imported dependencies](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/core/io/resource_importer.cpp#L461-L475) · [Godot export traversal](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/editor/export/editor_export_platform.cpp#L674-L692)

## P2 · The region verifier mistakes a Host frame for one fixed tick

Cook the stock Two-Level Game with only `profiles.server.runtime.time_scale` changed. Run
`server --verify-regions` in an empty working directory. At `1.0` all three trials passed.
At `0.5` all three reported `Server did not retain the relic observation after releasing movement`.
At `2.0` all three reported `Server actor continued moving into the wall while the command was held`.
This matrix reproduced on both the initial baseline and the `4e7554b` Server entry point.

The verifier submits a movement release, drives one real-time frame and immediately reads the
held state. At half speed that frame can finish before the release tick. Its wall check requires
`blocked.tick == contact.tick + 1`, while a double-speed frame can finish two ticks later. Driving
the same two packs through the live control protocol found both relics, released movement and
travelled to Cavern with both regions closed. With the later CLI, the actor's X was exactly
`-279.9898986816406` at both tick 117 and tick 119 at double speed while west was held.

Wait for the submitted command's acknowledged tick before observing the release. To verify a
wall contact persists, compare positions on distinct completed ticks while the input stays held.
Use a fixed-step observer if consecutive ticks are part of the assertion. Keep all three authored
speeds in the finite verifier. Bevy and Godot both derive the number of fixed steps from an
accumulator. [Release check](../../samples/two-level-game/src/bin/server/regions.rs#L294) · [Wall check](../../samples/two-level-game/src/bin/server/regions.rs#L123) · [Bevy fixed loop](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_time/src/fixed.rs#L239-L254) · [Godot physics clock](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/main/main_timer_sync.cpp#L348-L356)

## P2 · The live Server's terminal receipt loses its level history

Start the installed `server --serve` in Meadow, travel to Cavern, place marker 1 and save. Wait
for each operation to complete. Both explicit shutdown and stdin EOF exit successfully with
`close="complete"`, `travel_count=1`, `saves=1` and `visited_levels=[]`. Both wrote a 288-byte
checkpoint which a fresh Server restored in Cavern with marker 1. The finite startup control
reports `["meadow"]`, and the region verifier reports `["meadow","cavern"]`.

`Report` starts with an empty array. The finite runners fill it, while `service::run` updates
the final snapshot, queue and timing evidence and leaves that array empty. This first reproduced
on the developing service and reproduced again with the release executable after `--serve` landed
in `23127bd`. Seed the session's visited set at Ready and update it on committed Scene changes.
Test shutdown and EOF with and without travel. Bevy exposes applied state transitions and Godot
emits `scene_changed` after installing the new Scene. Both give consumers an event suitable for
recording visits. [Initial report](../../samples/two-level-game/src/bin/server.rs#L143) · [Service observation](../../samples/two-level-game/src/bin/server/service.rs#L138) · [Finite region evidence](../../samples/two-level-game/tests/support/server_delivery/mod.rs#L248) · [Bevy state transition](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_state/src/state/transitions.rs#L160-L166) · [Godot scene change](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/scene/main/scene_tree.cpp#L1701-L1712)

## Verification and previous repairs

- Windows x64, Rust 1.95.0. The Server CLI and its normal and scaled packs ran outside the
  source project. The later region matrix used CLI sources from `4e7554b`, the existing release
  game and engine libraries, and the independently generated bootstrap for each pack. The
  stock release executable used for level history and checkpoint handoff had SHA-256
  `e75c45734defc04208e9b657dd58a563e892787f72297f1239f2914589416f75` and ran with its matching pack.
- The public Cook consumer ran 15 cases including its normal selection controls. The Material
  and cross-source group probes reproduced again when linked to the November 21 16:28 debug
  Host. The input probe also reproduced through the working tree's separately installed
  `GameplayActionPlugin`. Keyboard and gamepad observations used normalized Runtime ingress.
  [Stock Cook controls](../../tests/fixtures/module-consumer/asset-domain-consumer/tests/project_cook_runtime_selection.rs#L114) · [Input capacity control](../../crates/nara_input/src/projection_tests.rs#L140)
- The four registered-panel release tests passed serially. The 39,500-row RON sources and the
  86-row omitted defaults each completed editing, Undo, Redo, Save and Reopen in both Scene
  and Prefab. All seven independent upgrade recovery process cases also passed. The historical
  RON case resumed both all-original and mixed Pending projects, compared all eight successor
  files and checked the identities and modification times of the two previously published RON files.
  [Panel workflows](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/property_pages/round_eight.rs) · [Historical recovery](../../tests/fixtures/behavior-languages/tests/support/authored_upgrade_session/process_recovery/legacy.rs#L52)
- The stock single-window HUD passed its native DX12 control. Removing the last camera cleared
  the world mapping and adding a new camera restored picking for its new View ID. The empty UI
  capture replay returned black on all 115,200 pixels of a 480 × 240 target. The clipped-camera
  HUD example went through `1 → 0 → 0 → 1` world views and activated the menu once. Native
  presentation receipts and offscreen pixel readback supplied separate observations.
  [Stock HUD consumer](../../tests/fixtures/stock-composition-consumer/examples/hud_only_window.rs#L149) · [Pixel checks](../../tests/fixtures/stock-composition-consumer/examples/hud_only_window.rs#L620)
