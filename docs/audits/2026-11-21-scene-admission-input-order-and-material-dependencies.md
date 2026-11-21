# Edited Server Scenes, input ordering and cooked Material dependencies

All eight ninth-round counterexamples pass. Of the nine findings first recorded in this round,
seven now pass their original counterexamples. Retesting the revised Server exposed two further
cases in checkpoint startup and finite verification. Four findings remain open, one P1 and three P2.
The review began at `be13116` and covers the Input/Window split at `fcbae58`, the Editor repair at
`0c0b0da`, the input repair at `5f62501` and concurrent working-tree changes. Measurements are
from Windows x64 with Rust 1.95.0.
[Ninth review](2026-11-21-server-residency-input-and-cook-review.md)

## P1 · An idle 8× Server still faults on Windows while stdin stays open

Run the stock 20 ms game with `profiles.server.runtime.time_scale = 8.0` and `server --serve`.
Hold stdin open after Ready and read stdout without sending requests. Two fresh processes
reported `project.run.runtime-faulted`, `close="failed"` and exited 1 after 1.66 and 1.68 seconds.
In another process the Server accepted a marker for tick 600. One observation after a second
showed tick 326, 72 ticks of debt and the marker still pending. The process then faulted. In the
same pack, sending `shutdown` or EOF immediately after admission settled tick 600 in about 1.51
seconds. Each drain acknowledged exactly one command, placed marker 1 and exited with zero debt.

The initial clock requested 20 ms and both closing paths also failed at tick 201. The revised
clock requests 2.5 ms and those paths pass. The live loop still waits on the input channel.
With five ticks allowed per frame, sustaining 8× requires a mean drive interval of at most
12.5 ms. On this machine, 64 waits of 2.5 ms on an open, empty `sync_channel(16)` had a median
of 15.31 ms. The same thread and sample count using the drain's `thread::sleep` had a median of
2.94 ms. The delivery test repeatedly sends `observe` while it waits for tick 150, then enters
drain. Make the simulation deadline wake the live loop independently of incoming commands.
Exercise an open, silent stdin past tick 600 alongside the two successful closing paths.
[Wake period](../../samples/two-level-game/src/bin/server/clock.rs#L26) · [Live wait](../../samples/two-level-game/src/bin/server/service.rs#L302) · [Drain wait](../../samples/two-level-game/src/bin/server/service.rs#L360) · [Delivery test polling](../../samples/two-level-game/tests/support/server_delivery/service.rs#L630) · [Fixed debt limit](../../crates/nara_app/src/lib.rs#L917) · [Bevy fixed loop](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_time/src/fixed.rs#L239-L254)

## P2 · A zero-scale Server with a checkpoint slot cannot reach Ready

Set `time_scale = 0.0`, leave the authored `paused` setting false, and start the Server with
`--serve --paused --save ABSOLUTE_PATH`. With a new slot in a writable directory and complete
`observe` and `shutdown` lines in stdin, the process emitted only its terminal record after
60.06 seconds. It reported `ready=false`, `stage="service_startup"`, a checkpoint launch timeout,
and exit 1. The identical pack without `--save` returned Ready, served `observe` and exited 0
in 0.09 seconds. An authored-paused 1× pack with that slot reached Ready at tick 1.

The slot is configured by a tick-1 command. The CLI computes its effective pause policy from
both the flag and the profile, but chooses the exact startup step only for the profile's pause
bit. The remaining path drives elapsed frames. At zero scale no such frame can execute tick 1,
and the reader starts after this loop. Use the requested paused boundary for this first-tick
initialization, or reject an unsupported combination before waiting. Keep zero-scale gameplay
and its normal no-progress shutdown behavior independent of slot initialization.
[Startup branch](../../samples/two-level-game/src/bin/server/service.rs#L71) · [Elapsed startup](../../samples/two-level-game/src/bin/server/service.rs#L239) · [First-tick command](../../samples/two-level-game/src/gameplay/player_launch.rs#L53) · [Paused step](../../src/project_host/runtime/action/session.rs#L240)

## P2 · The finite travel and region verifiers fail under two admitted time policies

`--verify-travel` and `--verify-regions` both passed on the stock, initially running 1× pack.
On an otherwise unchanged, initially paused 1× pack, both entered the scenario at Ready and
exited 1. Travel spent its 200,000-frame allowance at tick 0. Regions reported that Side Room
never opened. On the initially running 8× pack both verifiers instead reported an App-frame
fault in under 0.1 seconds. The same 8× pack can execute tick 201 through explicit advances and
tick 600 during shutdown. These failures do not require an edited Scene or a slow host timer.

Both scenario drivers pass one authored fixed timestep to `session.frame()` as real elapsed time.
A paused profile earns no ticks. At 8× each synthetic frame earns eight and can consume five.
Drive these fixed-command scenarios with explicit tick boundaries and allow Scene maintenance
between them. Qualify wall-clock pacing separately on the live Server. Bevy's time API also
exposes manual advancement for deterministic drivers.
[Travel driver](../../samples/two-level-game/src/bin/server/runtime.rs#L303) · [Region driver](../../samples/two-level-game/src/bin/server/regions.rs#L191) · [Headless exact step](../../src/project_host/runtime/action/session.rs#L240) · [Bevy time strategies](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_time/src/lib.rs#L113-L132)

## P2 · Per-frame rounding at half speed changes when an authoritative command executes

Run the cooked Server paused with a 20 ms fixed timestep and a `0.5` time scale. Queue marker 1
for the next fixed tick and use `advance` to supply exactly 40,000,000 ns. All intervals are
below the frame clamp and within the catch-up budget.

| Elapsed frames in ns | Virtual time at observation | Fixed tick | Acknowledged | Markers |
| --- | ---: | ---: | ---: | --- |
| `40000000` | 20,000,000 ns | 1 | 1 | `[1]` |
| `10000000` four times | 20,000,000 ns | 1 | 1 | `[1]` |
| `16666667, 16666667, 6666666` | 19,999,999 ns | 0 | 0 | `[]` |
| `9999999, 9999999, 9999999, 10000003` | 20,000,002 ns | 1 | 1 | `[1]` |

Two more real nanoseconds make the delayed marker appear. At `1.0` all four sequences yield
tick 2 and the same marker at 40 ms. In a separate `RuntimeInstance` probe, 10,000 repetitions of
the third sequence produced 9,999 completed ticks and a 19,990,000 ns remainder. The first
sequence produced 10,000 ticks and zero remainder at the same real time of 400 seconds.

The App rounds each scaled frame independently to integer nanoseconds. Carry the residual into
the next conversion and include it in a continuation that must preserve the next tick. Cover
pause, rate changes and clamping at the same boundary. Godot retains fractional scheduling time
in its physics accumulator.
[Virtual delta conversion](../../crates/nara_app/src/lib.rs#L653) · [Fixed accumulation](../../crates/nara_app/src/lib.rs#L892) · [Continuation](../../crates/nara_app/src/time_continuation.rs#L15) · [Godot physics accumulator](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/main/main_timer_sync.cpp#L348-L357)

## Repaired during review · A valid added wall blocked the shipping Server

Duplicate `wall-inner` as a new root named `alcove-post`, move it to `(110, 90)`, and cook the
game with the original Server content preparation. The authored level passes its gameplay rules.
The cooked pack has the same five Scene Products as the unedited game. Use the bootstrap generated
for that exact pack and start from an empty working directory.

| Edited level | Route | Result before the CLI repair |
| --- | --- | --- |
| Meadow | Public `HeadlessSession`, original game recipe | Five members pass deep validation. At tick 1 Meadow is `Running`, the extra wall resolves with `CollisionBox`, and marker 1 is placed. The active Scene has 12 entities. |
| Meadow | Shipping `server` and `server --serve --paused` | Both exit 1 at startup, before Ready, with `Server did not retain the live processed Scene members`. |
| Cavern | Shipping `server --serve`, then travel | Meadow starts with 11 entities. Cavern runs with 12, places marker 1 and shuts down successfully. |
| Cavern | Shipping `server --verify-regions` | The verifier reaches its scenario, checks the edited destination's receipt and exits 1 with the same member-count diagnostic. |
| Neither | Shipping entry points and public Host | Startup and the ordinary 11-entity level pass. |

The CLI required exactly `AUTHORED_LEVEL_MEMBERS` (11). It now verifies the receipt against
the entity IDs and count in the admitted compiled Scene. Relinked against the original packs,
both Meadow entry points started with 12 entities, and `--serve --paused` placed marker 1 at
tick 1. The Cavern verifier exited 0 with 12 entities. Live travel also reached that Cavern,
placed marker 1 and closed cleanly. Godot's `SceneState` reads the node count from each packed scene.
[CLI admission](../../samples/two-level-game/src/bin/server/runtime.rs#L267) · [Game content admission](../../samples/two-level-game/src/gameplay/content.rs#L153) · [Godot Scene instantiation](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/scene/resources/packed_scene.cpp#L243-L275)

## Repaired during review · An authored-paused 1× Server could not start with a checkpoint slot

With `profiles.server.runtime.paused = true`, a unit time scale and a writable slot, startup
previously emitted only a timeout at 60 seconds. The revised CLI runs the launch through an
exact paused step. On that same pack, with and without the additional `--paused` flag, Ready
reported `paused=true`, `checkpoint_configured=true` and tick 1 in 0.05–0.08 seconds. Both
processes served `observe`, placed marker 1 at tick 2 and exited 0. Without the slot the same
profile became Ready at tick 0. The zero-scale, authored-running case is recorded above.
[Startup driver](../../samples/two-level-game/src/bin/server/service.rs#L71) · [Reader creation](../../samples/two-level-game/src/bin/server/service.rs#L103)

## Repaired during review · Native Material defaults admitted a deferred Image past first use

With a custom codec declaring a native Material's default texture as `Reference`, Project Cook,
source revalidation and deep validation previously accepted the three-member pack. Direct Image
acquisition returned `[24, 120, 220, 255]`. Starting the Headless Host then failed with
`project.scene.material-texture-unavailable`. A bound `nara.material2d` Scene reproduced it.

After rebuilding the publication owner, both Project Cook and the lower-level builder rejected
the declaration before publication with `DependencyMismatch` reporting declared `Reference` and
actual `Immediate`. A separate `Required` edge from the Scene to the Image could no longer mask
the Material error. With the Material edge set to `Required`, frame 1 published one native
Material and one Image. A CPU-only Material retained `Reference` and the same pixel was readable
on demand. Bevy exposes a retry outcome for a material bind group awaiting a GPU resource.
[Material demand check](../../src/runtime_content/materials.rs#L55) · [Native publication](../../src/project_host/runtime/content_residency/materials.rs#L395) · [TileSet demand check](../../src/runtime_content/native.rs#L46) · [Bevy bind-group readiness](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_render/src/render_resource/bind_group.rs#L112-L117)

## Repaired during review · Cook rejected build dependencies from another registered domain

A Scene selected A, whose build introduced `B/sub` in another registered domain. Previously,
submitting B's verified group failed with `artifact group has no selected source` for either
dependency demand and either group order. A source-level reference to B made the same A bytes
and edges pass. Repeating an identical group from an empty second Scene also failed.

`add_group` now checks Cook-wide staged groups and the recipe's domain registrations. With the
root crate rebuilt, both group orders produced byte-identical packs for each of `Required` and
`Reference`. After source retirement, all four members and five Products passed deep validation.
Direct acquisition of `sub`, frame 1 and demand-specific residency also passed. A duplicate
contribution from the empty Scene preserved the complete pack bytes. Unreachable groups, stale
build seeds and duplicate source UUIDs still failed. Bevy visits a finished asset's dependencies.
Godot's export traversal recursively deduplicates selected dependencies.
[Cook-wide lookup](../../src/project_tooling/project_cook/content.rs#L323) · [Registered domain lookup](../../src/project_content/domains.rs#L108) · [Bevy finished asset](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_asset/src/loader.rs#L577-L598) · [Godot export traversal](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/editor/export/editor_export_platform.cpp#L674-L692)

## Repaired during review · A gamepad disconnect overtook a keyboard press waiting for UI routing

The original probe bound Space and gamepad South to one Button action. It held South for one
tick, paused, pressed Space, disconnected the pad and requested an exact fixed step before UI
had routed the key. The physical sequence was `#3 Space pressed`, then `#4 South released`.
Tick 2 consumed `stop` from #4. When UI released #3 to gameplay, tick 3 consumed `dash`. Giving
UI a paused frame to resolve the identical input first produced no command. One binding remained
held throughout. Fresh Runtime replay also reproduced the spurious `stop` and `dash`.

The first implementation blocked subsequent work for a seat only when a lifecycle batch was
waiting. The revised owner (`c716b0258b23` blob) applies the barrier to the earlier ordinary
press as well. Rebuilding the input and gameplay crates and rerunning both UI outcomes confirmed
the repair. Passing Space preserves the held action. Consuming Space yields exactly one `stop`
after routing. Another seat still delivers its `dash` in the first exact step.
[Deferred ordering](../../crates/nara_input/src/timeline/owner.rs#L1950) · [Exact-step control](../../crates/nara_input/tests/input_lifecycle_batches.rs) · [Godot per-device action removal](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/core/input/input.cpp#L697-L705)

## Repaired during review · Leaving or retiring a pointer started a cancelled mouse axis

The original probe bound one pointer's left and right buttons to `−1` and `+1` and held both
at zero. `PointerDriverInput::Left` and pointer retirement each produced two physical releases
carrying `PointerLeft`. The semantic consumer also received `Started(+1)` followed by
`Released(0)`. One fixed tick consumed both `dash` and `stop`. Window focus loss on the same
held buttons published both raw releases and no semantic transition. Ordinary manual releases
yielded the expected intermediate value.

The pointer exit path originally counted the releases and published them as ordinary entries.
The revised publisher marks those releases as one semantic cleanup batch. With the new build,
both pointer exit routes retain the two raw `Released` records and emit zero semantic edges or
commands. Winit forwards native `CursorLeft` to this path.
[Pointer batch publication](../../crates/nara_input/src/timeline/owner.rs#L1401) · [Winit ingress](../../crates/nara_winit/src/lib.rs#L1750) · [Bevy pointer cancellation](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_picking/src/events.rs#L1305-L1321)

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
Both ran serially against the shared target. The expanded input lifecycle binary passed **7/7**
after the two additional repairs. The initial independent Cook suite passed **23/23** cases,
including assertions that reproduced the failures. After rebuilding the root crate, **10/10**
focused Cook and Material cases passed with the repaired behavior. The input probes followed
normalized ingress through `GameplayCommandSet::Consume`. The new CLI was linked to the matching
generated expectation for each pack. The edited-level public Host also validated the pack,
looked up the new wall in the current Scene and observed an actual fixed-step marker. Server
probes used local stdin, independent processes and existing release engine libraries. The final
Server pass relinked seven prepared packs from one CLI source snapshot (Git blobs `00ccd2447c04`
for `clock.rs` and `931f38321e19` for `service.rs`). The time test additionally ran through a
freshly built debug `nara_app`. Native Editor observations
used Windows/DX12 and real window lifetimes. Full-workspace and additional-platform runs were
outside this review.
