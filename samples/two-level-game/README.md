# Two-Level Exploration

A small independent Rust product using Nara's public project, command, Scene and filesystem APIs.
It owns ordinary Rust modules for gameplay and checkpoint data; the Reference Game and its wave
CLI remain separate consumers.

The atlas-animated explorer moves with WASD and automatically collects nearby gold gems. E travels
between Meadow and Cavern. Q switches between two independently persistent instances of the current source.
M places a pink dynamic marker. K saves the current checkpoint and L restores the slot. R restarts
the current logical level at its checkpoint, or retries a failed reconstruction. Closing the window
uses normal Runtime shutdown, including pending Scene preparation and temporary-file cleanup.
The green square marks the checkpoint; the purple square marks the destination portal.

An opt-in source desktop flow adds a title, new round, pause, completion and return to title:

```text
cargo run --manifest-path samples/two-level-game/Cargo.toml --release --features play-flow --bin desktop -- --play-flow
```

Enter starts a round or continues a paused round. Escape pauses/continues. T returns to the title
from pause or completion, and X exits through ordinary Runtime shutdown. Collect one gold gem
in each main Meadow and Cavern occurrence to complete the round; E travels between them.
The window shows the current phase, goals and available controls. A new round replaces the Scene
through the original preparation/commit path, then resets game progress and round counters.
The opt-in contribution retains the authored gameplay bindings and adds its own menu context;
Enter, Escape, T and X must be unbound in the source map or Startup reports the conflict.
Preparation failure retains the old game; a failure after Scene commit is explicitly degraded.
Enter from that error screen attempts a new round.

Menus stop game time while Input, tasks, rendering and native cleanup remain active. Continuing
drains old ordinary gameplay intent through the original fixed transaction before reopening the
gameplay context. Held keys require release and a fresh press. The additional flow and mapping
lifetime are not part of exact Resume capture. Default exploration, existing cooked Players,
checkpoint saves and Resume wire formats are unchanged. The `play-flow` manifest profile owns
the UI composition; compiling that feature alone does not enable the flow on the normal desktop.

These are the default physical keys in [the authored action map](input/explorer.actions.json),
selected by `[input] action_map` in `nara.toml`. Edit its key bindings without changing gameplay
Rust. Stable action names identify movement directions and commands independently of their
physical keys; the existing names ending in `move-a/d/w/s` retain their original direction meaning
after rebinding. Source and cooked Players use the same finite definition and semantic command
adapter. The global Input definition stays installed across Scene travel. Changes to that source
require a fresh Runtime; the incumbent keeps its admitted mapping and refuses new travel against
different global input rather than silently adopting the edit.

Exact keyboard reconstruction reads the original Runtime's installed PRIMARY movement bindings.
It requires one enabled keyboard Button source for each direction, with distinct keys and no key
fanout. Gamepad Button alternatives can coexist with that keyboard inventory; actual extra held
members or native services still need the original capture eligibility. Other valid multi-binding maps remain playable but can be ineligible for exact keyboard
continuation. Native Windows reconciliation supports the same named keys, lowercase ASCII letters
and digits as ordinary physical ingress. It does not guess uppercase or Unicode key positions.
The original pre-Startup selection remains the reconstruction baseline; later runtime context or
definition changes are not serialized by the game. The resume and checkpoint formats are unchanged.

The game-owned [animation selection](src/animation.rs) uses the authored
[explorer clips](assets/animations/explorer.sprite-animations.json) and shared
[64x16 sRGB atlas](assets/textures/explorer.png), with nearest clamped sampling. Change frame
order, normalized UV regions, durations and `once`/`loop` mode in the descriptor without editing
Rust. Default data contains one Idle frame and three 100 ms Walk frames. Walking follows net held
intent, including movement against a wall; opposing keys cancel per axis.

The directly consumed [Sprite animation module](../../crates/nara_sprite_animation/README.md)
loads immutable named clips through the existing source/cooked Asset owners. The original integer
game clock supplies elapsed time. Pause leaves it frozen; an exact Step advances one fixed tick.
Travel and exact Runtime reconstruction derive the Sprite region before presentation, including
paused maintenance. Clip changes participate in the existing content compatibility stamp. There
is no hardcoded production fallback, additional playback cursor or resume wire field. Independent
playback state, events and blending remain outside this game workflow.

## Swept Character Collision

The explorer moves through Rapier's native shape sweep and slide against five authored static
walls per room. Four perimeter walls replace coordinate clamping. Meadow and Cavern have different
interior wall positions; moving between them and Retry replace the original collision scene.
The native controller targets a 0.01 world-unit separation margin for the explorer, which is
18×18 at default scale. Hold two movement directions to slide along a wall and round its end.
Walking animation follows held intent while blocked.

The game consumes [nara_rapier2d](../../crates/nara_rapier2d/README.md) directly. That package owns
immutable native query scenes and a stateless top-down controller. It is optional for Nara
applications and does not install a second World, clock, transform writer or physics backend
interface. The game owns `CollisionBox` data, desired movement and the final player Transform.
Static wall geometry and its query index are prepared once per admitted room. Actor shape data
is reused until its definition, rotation, scale, placement or room changes. Each move passes an
already integrated displacement and the original fixed duration to the native query.

The game's existing persistable center range remains `x=+-280`, `y=+-150`. Desired movement is
limited to that range before the sweep. Before writing a moved pose, the game tests its exact
position with the same native shape intersection used for restored poses. If it overlaps a wall,
the game uses the native contact normal to remove inward movement and sweeps once more from the
last accepted pose. A blocked retry or a result outside the center range keeps the last accepted
pose. An idle tick reuses that pose.

Checkpoint and exact-resume placements must be outside walls and have representable geometry.
This controller has no retained velocity, gravity, moving-platform or contact history. Existing
source geometry and player TRS reconstruct it; the resume wire format stays unchanged. Runtime
wall mutations are outside that format and make exact capture ineligible. The wire decoder can
accept finite zero scale while collision admission refuses it. Placement checks happen at
admission. Movement queries reuse the same prepared wall index and actor shape.

Each level has eleven authored members. The Immediate marker Scene places a two-entity
[Prefab](prefabs/marker.prefab.json) with a pink root, a child with an amber
[sRGB badge texture](assets/textures/marker.png), and an occurrence-local reference. Every
placement creates its own entities, and all occurrences share the level's resident badge Image.
Edit the badge PNG to change its appearance. A fresh load reads the authored image. Retry uses
the committed image and template, including when the files have changed since loading.

The cooked catalog has fifteen members, including five Scenes, five Images, one Material and one TileSet. Meadow and Cavern
are the two travel roots. The marker Scene declares its badge Image. Meadow declares the side room
and Lookout as Deferred dependencies. Regenerate older authoring bundles with native binding
revision 6 and recook existing installations. Checkpoint and resume retain their wire formats.
Exact resume requires a compatible authored source and executable schema.

### Play The Side Room

Press **B** in Meadow to open the north side room and **G** to open the south Lookout. Keep
moving during loading. Walk toward either turquoise relic and touch it to collect it for that visit.
Press the room's key again to close it, or cancel while it is still loading. The two rooms can stay
open together and share a relic Image and the authored
[region Material](assets/materials/region.material2d.json). Its [Shader](assets/materials/region.wgsl)
grades the relic in linear color and samples the existing Meadow palette Image. Both relics use
the same Material publication with authored parameter overrides. Open either room again for a
fresh visit.

Both floors use authored Tilemaps with eight cells each and one shared
[region TileSet](assets/tilesets/region.tileset.ron). The TileSet uses the relic's 2x2 Image atlas;
the north and south floors have different checkerboard cell colors. Opening the second room
retains the first publication's TileSet handle and version. Closing either room leaves the other
floor live; closing the final user or committing primary travel releases the declaration.

Travel and checkpoint restore close the active rooms when the destination commits. If the
destination fails to load, the current rooms remain playable. Normal checkpoints record the main
game's progress. Close the rooms before Retry or exact game capture.

Editing the Material or its Shader while one room is open cannot silently replace its live shared
definition. A conflicting second activation refuses and leaves the original room playable. Close
the live users before loading a changed definition, or restore the authored inputs and retry.
Cooked Players carry the Material and its Image dependency and need no source WGSL files.
The same rule applies to a changed TileSet atlas, sampler or tint. Cooked Players retain its
processed descriptor and actual Image dependency without requiring the source RON file.

## Build And Run

### Optional Native Controller

The source desktop executable can collect native controllers through the independently consumed
[`nara_gilrs`](../../crates/nara_gilrs/README.md) package. Select both the build feature and runtime
option explicitly:

```text
cargo run --features desktop,gamepad --bin desktop -- --gamepad
```

WASD remains available. The authored action map binds the D-pad to movement, South to travel,
West to instance switching, North to marker placement, East to Lookout, Select to the north
side room, Start to Retry, and the left/right shoulder buttons to Save/Restore. Save still
requires an explicit `--save ABSOLUTE_PATH`. These are mapped positional buttons, independent
of the glyphs printed on a particular controller. Analog stick samples are supported by the
backend but are not movement bindings in this game's Button-based control scheme.

For the complete round flow, launch with both selections.

```text
cargo run --features play-flow,gamepad --bin desktop -- --play-flow --gamepad
```

Start begins and restarts rounds and toggles pause and continue. While a menu is open, East
returns to Title from Pause or Complete, and Select exits. East and Select resume their authored
gameplay roles while playing. Keyboard R still retries. In the controller Round, Start replaces
the original gamepad Retry binding in the installed map. The authored definition stays intact.
If a custom map binds Start to another command, choose a different button for that command
before running or cooking the controller Round.

Each current controller uses the explicitly selected PRIMARY window/seat. The same ActionMap
combines keyboard and controller contributions, so releasing one source does not release another
held source. Focus loss and physical disconnection terminate accepted controls through Input's
original lifetime. Known controls held across reassignment/refocus wait for release or neutral;
cached holds are not replayed. Gilrs's event cache is not an instantaneous hardware snapshot.

Compiling `gamepad` without passing `--gamepad` does not initialize the source backend. The optional
source selection does not automatically enable a cooked Player backend or add native controller state to the
resume format. Exact development reconstruction can refuse the unrepresented native service.
The existing keyboard-only continuation path remains independently supported. Managed native
controller acquisition currently requires Windows WGI. Other platforms refuse before native
acquisition until their backend's physical retirement contract is implemented. Native hardware
and platform execution scope is recorded in the [owning unit](../../docs/plans/2026-11-13-seat-gamepad-and-input-replay-plan.md#native-gilrs-input-and-playable-controller-successor).

For a source-free Player, build the cooker with `cook,gamepad` and add `--controllers gilrs`
to the [desktop cooking command](#source-free-desktop-player). The choice is independent of
`--runtime`: both `desktop` and `audio-desktop` support it; the latter also needs the cooker's
`audio-cpal` feature. Omit the option or use `--controllers none` for the original keyboard-only
native placement. Headless native-controller selection refuses before content preparation.

The published Player activates its compiled selection without a `--gamepad` runtime argument.
`two_level_game::delivery::PlayerSelection` shares the selected recipe, binary/features and
content profile between cooker and Player. The controller profiles are
`two-level-desktop-gamepad-shipping` and `two-level-audio-desktop-gamepad-shipping`; the three
original profiles are unchanged. An all-features build alone does not activate native input.
Both choices keep the same fifteen content members and authored bindings. Successful observation
records report the actual presented Runtime's controller backend and inventory count. Zero devices
does not qualify physical button/stick/hotplug interaction. The [delivery unit](../../docs/plans/2026-11-13-seat-gamepad-and-input-replay-plan.md#explicit-controllers-through-cooked-player-delivery)
owns execution evidence and remaining hardware/platform limits.

### Ordinary Source Commands

Run commands from this package directory. The optional desktop target needs the platform's normal
native window and GPU support. Reuse the engine target directory and one build job when developing
inside the repository.

```text
nara schema . --write
nara schema . --check
nara check .
cargo run --bin headless
cargo run --bin headless -- --verify-travel
cargo run --features desktop --bin desktop -- --save /absolute/existing-parent/checkpoint.json
cargo run --features desktop --bin desktop -- --verify-first-present
```

On Windows, pass a fully qualified path such as `D:\GameSaves\checkpoint.json`. The application
does not choose the process working directory or silently create an arbitrary save parent. Omit
`--save` for a session without persistence; K/L then report the absent application slot.

The authoring bundle is generated from this product's real native schema provider with the existing
schema command. It is not a hand-maintained second definition of component layouts.

### Development And Shipping Features

| Selection | Compiled role |
| --- | --- |
| Default, or `--no-default-features --features development` | Source executables, game-owned rebuild Host, Cargo/process tools and the private continuity watcher |
| `--no-default-features --features cook` | Cook/package executable and Cargo tools, without the development Host or watcher |
| `--no-default-features --features cooked-player` | Headless shipping Player and shared gameplay, without Cargo tools, watcher, UI or GPU/window backends |
| `--no-default-features --features cooked-player,desktop` | Shipping Player with the explicit native window/GPU placement, without development tools, watcher or UI |
| `--no-default-features --features cooked-player,desktop,audio-cpal` | Explicit Windows audible Player, without development tools, watcher or UI |
| `--no-default-features --features cooked-player,desktop,gamepad` (plus `audio-cpal` for audible placement) | Native controller implementation available to an explicitly controller-selected cooked profile; availability alone does not activate it |
| `desktop,cost-measurement,scene-work-observation` | Development-only native workload with existing Scene owner span attribution; disabled by default |

Cargo features are additive. Enabling `cooked-player` with defaults still enables development;
the production cooker always disables defaults for its separate captured Player build. Its own
features and the supervising tests' tooling do not become Player features. Check the exact
normal/build graph with Cargo rather than the test process or an all-feature workspace graph:

```text
cargo tree --locked --no-default-features --features cooked-player --edges normal,build
cargo tree --locked --no-default-features --features cooked-player,desktop --edges normal,build
```

Every selection includes the Sprite clip importer/codec, CPU Sound Bank importer/codec and Hound
for WAV admission; headless and silent selections exclude CPAL and rtrb and never open an audio device.
The shared root Runtime still includes project/schema, CPU Image and Material support, including
Naga. This split does not claim to remove every source-oriented implementation or provide a
complete dedicated-server product. The delivery tests check these selected dependency/features
before generating and executing the actual relocated Players.

## Optional Windows Game Audio

The explicit `audio-cpal` composition adds two original game-owned one-shots through CPAL
0.18.2/WASAPI: a committed pickup and a successfully reconstructed travel entry. It currently
requires a real Windows output device supporting stereo 48 kHz `f32`. Missing or unsupported
devices refuse startup; another device or silent fallback is not selected automatically.

```text
cargo +1.95.0 run --locked --offline --release --features desktop,audio-cpal --bin audio_desktop
```

Use the same WASD, E/Q, marker, checkpoint and Retry controls as the ordinary desktop game.
Game Pause freezes simulation while already started short effects continue on the device clock.
An exact Step can produce one new pickup; Resume does not replay it. Successful Scene replacement
cancels that old instance's voices. Failed travel retains it; Retry and checkpoint Restore do not
invent a new travel-entry cue. Playback refusal reports its own error without undoing committed
gameplay. The existing exact-reconstruction inventory continues refusing this unrepresented native
audio service.

The optional recipe owns a finite cue queue, sixteen voices, a 4096-effect duplicate ledger,
bounded observation and managed device shutdown. The camera's `SoundPalette` declares the one
[Sound Bank descriptor](assets/audio/game.soundbank.json), whose metadata supplies its stable
primary Product. Source import reads both [original WAVs](assets/audio/README.md) through the
admitted raw-dependency reader; cooked content contains the validated canonical PCM and complete
Product identity. The device opens only after successful Scene/game initialization. Immutable
sample storage retains its original content collection and budget throughout playback. Its normal
data callback neither allocates nor locks game state. Actual stream
destruction stays on the original Runtime driver and waits for CPAL's original WASAPI worker;
pause admission or a consumed sample is not physical playback completion. Device shutdown and
native calls remain cooperative. Ordinary headless and silent desktop recipes do not acquire this
service merely because `audio-cpal` is compiled. One audio Runtime fixes the complete bank; travel
with changed Product or PCM meaning refuses before Scene commit and requires a fresh Runtime.

For the finite ordinary-game qualification, choose hardware DX12 or Vulkan and set
`NARA_WGPU_FORCE_FALLBACK=0`. The executable uses a private copy of the project for deliberate
source failure/repair, then retires both original native Runtimes and their recovered event loop:

```text
cargo +1.95.0 run --locked --offline --release --features desktop,audio-cpal --bin audio_desktop -- --verify
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop,audio-cpal --lib --test native_audio --run-ignored only -E 'test(audio::native_tests::) | test(ordinary_audible_game)' --test-threads=1 --no-fail-fast --no-tests fail
```

### Source-Free Audible Player

Select `--runtime audio-desktop` on the existing cooker built with `cook,desktop,audio-cpal`.
Use the same explicit project/output/target/tool paths documented for the desktop Player below.
This selects binary `player_audio`, content profile `two-level-audio-desktop-shipping` and exactly
`cooked-player,desktop,audio-cpal` with defaults disabled. All three placements contain five
Scenes, five Images, one shared Sound Bank, one Sprite clip resource and global Input. Both travel
Scenes require the atlas and clip Product. The marker Scene requires its badge Image. Input is a
separate global startup root. The finite content fence includes the action map, the clip and bank
descriptors, their metadata and both raw WAVs. Cook and Check prepare CPU content without opening
a device.

The published `player_audio.exe` opens only its generated executable-relative content pack.
There is no source-directory or embedded-WAV fallback. It accepts normal interactive play and
`--save ABSOLUTE_PATH`; `--verify` instead executes the complete Travel script. It starts with
isolated native input and completes original focus/physical-key reconciliation before Resume.
It requires the selected hardware DX12/Vulkan backend and a real WASAPI device; keep the desktop
idle and game keys Up. It emits
the existing `TWO_LEVEL_DESKTOP_RESULT` record with an `audio` member after original physical
retirement. Verification compares every captured output sample against the original acquired bank
and independent cause/scope/mix arithmetic. The separate delivery test compares this bank and
whole output hash against independently decoded source WAVs after relocation and source/cache
removal. Neither observation establishes human audibility, DAC timing or scanout.

This placement drains bounded callback observations. If the observer falls behind, playback
continues and reports the number of discarded observation frames. `--verify` requires a continuous
capture with zero discarded frames across its finite verification interval.

```text
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --all-features --test cooked_audio --run-ignored only -E 'test(production_cooked_audio_player)' --test-threads=1 --no-fail-fast --no-tests fail
```

The report compares complete captured device-callback samples with the original WAVs and exact
game causes, alongside Pause/Step, failed travel, cancellation and Save/Restore. Separate native
controls hold a real callback through managed Close, fail Startup after acquisition and unwind a
test assertion before proving original cleanup. Capture observes this game's output buffer only;
it proves neither human audibility nor system-mixer output, DAC delivery, latency or foreign
platform support. Device hot-switch/recovery, streaming music and native audio-state restoration
remain separate work. The [owning plan](../../docs/plans/2026-11-03-joint-motion-execution-plan.md#concrete-game-audio-and-native-retirement-successor)
records exact qualification and remaining scope.

## Checkpoint Acceptance

Create an application-owned directory, then run the same headless binary twice:

```text
cargo run --bin headless -- --verify-save /absolute/existing-parent/checkpoint.json
cargo run --bin headless -- --verify-restore /absolute/existing-parent/checkpoint.json
```

The save trajectory collects in Meadow 0, Cavern 0 and Cavern 1, places markers with IDs 1/2/3,
returns to Cavern 0, retries that current level and publishes the slot there. It requires score 30,
three collected facts and next marker ID 4 in the actual file readback. The second invocation starts
with a fresh process/World, restores Cavern 0, creates marker 4, visits Cavern 1 and returns to Meadow
0. It checks actual Sprite visibility, live dynamic marker IDs and changed `CurrentScene` instance
receipts, not just copied DTO values. Both invocations print `verified=true` only after completing
their finite command trajectory.

```text
cargo nextest run --manifest-path samples/two-level-game/Cargo.toml --all-targets
```

The command above is relative to the engine root. Tests include separate save/restore processes,
an actual destination source change after startup, corrupt save rejection, exact local conflict
checks, interruption before publication, and pure bounded format/migration cases. Desktop
`--verify-first-present` proves presentation only; it does not prove that a human exercised every
key or that a particular platform has passed the full interactive trajectory.

When sharing `CARGO_TARGET_DIR` with another independent workspace that also builds a `headless`
binary, explicitly build this package's `headless` before running these process tests. Use the
same profile/features as the test run, for example `cargo build --manifest-path
samples/two-level-game/Cargo.toml --bin headless` for the command above, adding `--release` for
release tests. Cargo's reused test artifacts alone may leave the shared top-level executable
alias belonging to the other workspace. Keep the shared target; select the intended binary
through Cargo before launch.

## Source-Free Cooked Player

The optional `cook` binary builds and publishes the same game as a native headless or desktop
Player. It uses Nara's existing Image importer, Sprite animation domain, the game's Sound Bank
domain, Scene compiler, cooked-content assembler, captured Cargo executable and whole-directory
publisher. The pack contains both levels, the marker Scene, two Deferred rooms, five Images, one
shared Material, one shared TileSet, Sound Bank, Sprite clips and startup Input.
Authoring retains `ProjectLocation` identities; shipping Scene Products are explicit entries in
`LevelId`, separate from logical level instances.

Run with Rust 1.95, one Cargo job, the shared absolute engine target and cached dependencies.
Create the output parent outside this source package first. Supply the actual native target
triple and absolute Cargo/rustc executables from the selected toolchain:

```text
cargo +1.95.0 run --locked --offline --release --no-default-features --features cook --bin cook -- --project-root ABSOLUTE_GAME_ROOT --output-parent ABSOLUTE_EXISTING_OUTPUT_PARENT --name TwoLevel --target-dir ABSOLUTE_ENGINE_TARGET --target NATIVE_TRIPLE --cargo ABSOLUTE_CARGO --rustc ABSOLUTE_RUSTC --profile release
```

Before generating code, the cooker uses the original source-delivery loader to check the
manifest-selected authoring Schema against its compiled definitions. A valid alternate bundle
path works without `schema/authoring.bundle.json`; missing, corrupt or divergent selected bundles
reject before bootstrap changes. The completed processed Schema must match the admitted source
Schema. Deep content validation also requires each Material texture to target an Image member
with a selected Image codec.

The game and the generated starter consume `project_tooling::project_cook`. The original source
delivery traversal supplies compiled Scenes, expanded Prefab provenance, Input definitions and
Image/Material/TileSet content directly to the existing processed builder. Meadow and Cavern
now declare their existing Scene Product IDs in `.scene.json.meta` sidecars; their containing
source keys keep the project's `project_location` policy. The cooker no longer reconstructs a
separate identity database, rereads Prefabs or lists every Scene/Image Product itself.

SoundBank and Sprite animation remain explicit typed build contributions. Their original build
groups and optional/auxiliary input observations stay retained through Player compilation and
publication. The shared source/code selection rejects changed or newly present captured inputs;
it does not turn per-file revalidation into a hermetic or atomic workspace snapshot. Sample
catalog counts are qualification facts rather than limits imposed by the common entry.

The default `--runtime headless` selection uses a fixed headless recipe even when the cooker
has the `desktop` feature enabled. The cooker owns `player-bootstrap.rs` as an immutable logical
input to the original Cargo request, then builds `player` with only `cooked-player`. Its trusted
build script tracks `NARA_CARGO_GENERATED_RUST_SOURCE` and writes only
`OUT_DIR/nara-player-bootstrap.rs`. It never writes a generated file into this source project.
These compiled constants independently select the exact catalog and pack; the Player accepts no expectation,
pack path or source-project override. An ordinary `player` build without this generated input
refuses execution. Do not copy bootstrap constants from another package.

An ordinary request removes inherited generated-source selection and cannot activate the cooked
branch merely because an older `OUT_DIR` or source file exists. Cargo configuration must not
define or override the reserved variable. Cooperating builds retain the existing target lease;
serial A/B/A requests can reuse the same target without sharing mutable source input. The
version-one package manifest records `generated_input_name` and its separate build-provenance
fact; the code-file inventory remains independent.

The published directory contains exactly `player` (with the platform's executable suffix),
`content.pack` and `package.json`. Move the whole directory to its installation path. The Player
opens the pack beside its actual executable and works from an unrelated current directory with
no source, metadata, import cache, Cargo or rustc available. The cooker keeps its reusable
`two-level-cook-cache` separately under the output parent; that cache is not an installation
member. Publication refuses an existing destination rather than replacing it.

The source fence keeps actual admitted delivery members, including the selected Schema, together
with verified Image/Sound Bank/Sprite clip/Material/TileSet observations through code generation,
Cargo build and publication. Renaming a raw WAV and changing its bank descriptor is
valid; no hardcoded old filename is required. A later change to the actual observed dependency
refuses publication instead of silently relabeling the previously prepared PCM.

```text
player --verify-travel
player --verify-save ABSOLUTE_CHECKPOINT_PATH
player --verify-restore ABSOLUTE_CHECKPOINT_PATH
```

The last two commands run in separate processes and retain the ordinary checkpoint semantics
described above. The fixed cooker/Player recipe does not include a user's save path or test
scenario. Instead, the original gameplay queue admits the finite launch intent at the first
fixed tick. The original SaveSlot and Runtime close participant own later file operations and
cleanup. Malformed, duplicate, late or incompatible launch intent is rejected without replacing
an existing slot or swallowing ordinary same-batch input.

`TWO_LEVEL_PACKAGE_PUBLISHED` describes actual directory publication. An optional
`postpublication_error` means the package was published before a later check failed; it cannot
be interpreted as an absent destination. A `TWO_LEVEL_PACKAGE_ORPHAN` diagnostic identifies the
original parent, staging name/identity and cleanup error for explicit recovery. It does not
authorize recursive deletion. A Cargo child whose cleanup remains pending keeps the original
process/target lease and retries; a notice is not a completed cleanup receipt.

`TWO_LEVEL_PLAYER_RESULT` reports actual gameplay, Scene receipts, processed representation,
historically observed Image proofs and the current resident Image set before close. A completed
cleanup result is separate from successful gameplay. These are CPU content/gameplay proofs;
the headless Player does not qualify desktop drawing, physical input, scanout, server stripping
or the source-only exact-state reconstruction inventory.

The actual-delivery acceptance uses the production cooker, removes only its owned copied
source/cache tree, and checks normal process/containment/output retirement. It also corrupts
the deferred Cavern member after a valid publication, requiring failed travel to preserve the
original Meadow receipt and progress, then separately checks catalog rejection at startup.
Run it serially from the engine root after setting the absolute shared `CARGO_TARGET_DIR`,
`CARGO_BUILD_JOBS=1`, `RUSTUP_TOOLCHAIN=1.95.0` and `CARGO_NET_OFFLINE=true`:

```text
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features cook --test cooked_delivery --run-ignored only -E 'test(production_cooker_delivers_the_source_free_two_level_game)' --test-threads=1 --no-fail-fast --no-tests fail --success-output immediate
```

The separate ignored `cooked_delivery_sealed_player_worker` is an implementation detail of
this acceptance and requires its original worker request; do not select every ignored case
without the production test filter. Ordinary CPU/fallback checks run with
`--features cook,cooked-player` and no `--run-ignored` selection.

## Source-Free Desktop Player

Build the cooker with `cook,desktop` and select `--runtime desktop` explicitly. It resolves the
project's `desktop` profile and builds `player_desktop` with `cooked-player,desktop`. The default
profile and `player` binary remain the headless placement. Both selections reject an incompatible
window setting. The selected global Input definition is cooked as its original immutable Product;
source and shipped gameplay use the same authored bindings.

```text
cargo +1.95.0 run --locked --offline --release --no-default-features --features cook,desktop --bin cook -- --runtime desktop --project-root ABSOLUTE_GAME_ROOT --output-parent ABSOLUTE_EXISTING_OUTPUT_PARENT --name TwoLevelDesktop --target-dir ABSOLUTE_ENGINE_TARGET --target NATIVE_TRIPLE --cargo ABSOLUTE_CARGO --rustc ABSOLUTE_RUSTC --profile release
```

The published directory contains exactly `player_desktop` (with the platform's executable
suffix), `content.pack` and `package.json`. Relocate the complete directory, then run the Player
from any current directory. No source, importer cache or Rust toolchain is needed at execution.
Supply an absolute save path to use K/L; ordinary mode stays running until the native window closes.

```text
player_desktop --save ABSOLUTE_CHECKPOINT_PATH
player_desktop --observe --save ABSOLUTE_CHECKPOINT_PATH
player_desktop --verify-first-present
```

The existing `--verify-travel`, `--verify-save ABSOLUTE_PATH` and `--verify-restore ABSOLUTE_PATH`
modes are finite scripted controls. The ordinary keyboard path remains WASD movement, E travel,
Q instance, M marker, K save, L restore and R retry. Startup completes an original paused native
presentation, submits the first-tick launch intent, and resumes through the original Runtime
control ticket. It does not construct a second input or window owner.
Queue admission and application are separate facts. The original gameplay consumer records its
first launch result, observable through `player_launch_result(&World)`; the desktop entry requires
that result before exploration Ready. An invalid initial save configuration is refused after original cleanup.
Later ordinary K/L operation errors remain recoverable in the running game.

`TWO_LEVEL_DESKTOP_READY` reports accepted launch and resumed gameplay after a matching native
presentation in exploration mode. Optional `TWO_LEVEL_DESKTOP_OBSERVATION` records are bounded semantic-change
diagnostics; `TWO_LEVEL_DESKTOP_RESULT` follows original Runtime/pump retirement. Their presentation
facts describe actual submitted/presented work and the associated gameplay state. They do not
establish screenshot pixels, scanout or GPU duration. The CPU Image proofs retain their own scope.
When the observation allowance fills, optional records stop and terminal output reports
`observations_truncated`; play continues. Interactive Scene history preserves startup plus recent
receipts and reports `scene_receipts_sampled` after eviction. A recoverable K/L failure is reported
as `gameplay_error` and remains in the game snapshot until an ordinary successful command clears it.

The [desktop successor plan](../../docs/plans/2026-10-20-persistent-two-level-game-plan.md#source-free-desktop-player-successor)
owns qualification and its limits. Physical acceptance runs the relocated exploration package
through actual keyboard play, checkpoint Save, normal native Close, a fresh process and Restore.
It requires an idle Windows desktop, all game keys released, an explicit adapter, and serial Cargo
because the test builds the real Player:

```text
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features cook,desktop --test cooked_desktop --run-ignored only -E 'test(production_cooked_desktop_accepts_physical_play_save_and_fresh_restore)' --test-threads=1 --no-fail-fast --no-tests fail --success-output immediate
```

### Cooked Round Flow

Build the cooker with `cook,play-flow` and select `--runtime desktop --flow round`. The omitted
`--flow` option selects `exploration`. For audible rounds use `--runtime audio-desktop` and also
compile the cooker with `audio-cpal`. Controllers remain an independent `--controllers` selection.
Round profiles bind the existing UI-capable `play-flow` manifest profile and the same game flow/HUD
recipe in preparation and in the generated Player. Headless round selection is refused.
For a controller Round, add the `gamepad` feature and `--controllers gilrs`. The cooked HUD
shows the same keyboard and controller controls as the source launch.

```text
cargo +1.95.0 run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --no-default-features --features cook,play-flow --bin cook -- --runtime desktop --flow round --project-root ABSOLUTE_GAME_ROOT --output-parent ABSOLUTE_EXISTING_OUTPUT_PARENT --name TwoLevelRound --target-dir ABSOLUTE_ENGINE_TARGET --target NATIVE_TRIPLE --cargo ABSOLUTE_CARGO --rustc ABSOLUTE_RUSTC --profile release
```

The same three published files remain executable-relative and relocatable. No source fonts,
importer cache or toolchain are needed to draw the HUD. Run `player_desktop --observe --save
ABSOLUTE_CHECKPOINT_PATH`, then use Enter, Escape, T and X as in the source round flow.
`--verify-first-present` can finish on the actual Title at tick zero. The old scripted
Travel/Save/Restore verification modes are refused for a round product; K/L remain ordinary
game actions after starting a round.

Round READY follows the original Host Resume and a matching current HUD. Its
`initial_title_presentation` retains the actual Title at tick zero. If Enter starts the game before
Resume confirmation is delivered, READY can report current Playing while retaining that original
Title. A supplied Save launch is admitted but remains pending on the Title; `launch_accepted` becomes
true only after the original first resumed fixed transaction consumes it. The `flow` observation
distinguishes the game snapshot from its matching presented HUD and carries original UI/native
receipt facts. `flow.input_focused` reports the original Input endpoint's acknowledged focus;
physical automation waits for it after activating the window. The cooked Input proof retains the
admitted source definition alongside the exact runtime menu derivation. Observation sequence,
stream limits and terminal cleanup policy remain shared with exploration. The `play-flow-check`
feature enables the optional native acceptance tool. Shipping requires only `play-flow`.

The [cooked round plan](../../docs/plans/2026-10-20-persistent-two-level-game-plan.md#cooked-round-flow-successor)
records the package and native qualification. Select both physical round tests on an idle
Windows desktop with `WGPU_BACKEND=dx12`, `NARA_WGPU_FORCE_FALLBACK=0`, and the same serial Cargo
environment. The audio case also requires a usable WASAPI endpoint.

```text
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features cook,play-flow,audio-cpal --test cooked_round_flow --run-ignored only -E 'test(production_cooked_round_)' --test-threads=1 --no-fail-fast --no-tests fail
```

## Exact Game Reconstruction

The separate `resume` format preserves the complete supported game semantics: progress across
level instances, the live player transform, held movement intent, logical game time, fixed
scheduling phase and actual time policy, counters and admitted configuration/Schema/content
identities. Resume format version two requires an explicit time value; version one is refused
rather than assumed to have zero phase. The finite continuity protocol also moves to version
two, so an older child is refused at the version gate. The progress-save slot above retains its original
anchor-based behavior and its separate version-one migration.

Use `capture_resume(&session)` after the original `HeadlessSession` has completed a paused frame.
Capture refuses pending commands, travel or authoritative tasks, scripted verification, unknown
plugin services, an open save slot and a non-unit Runtime time scale. The shipped source/headless
recipe is the first supported inventory. Extend that explicit inventory and compatibility policy
when adding authoritative gameplay state or services.
The default Runtime2d Material schema and empty asset storage are part of this inventory.
Populated Material assets or Material2d components refuse reconstruction because this game's
resume format does not represent their state. Installing the schema alone does not authorize
silently omitting later material edits.

App compatibility now includes the explicitly selected execution profile as well as the plugin
plan. Its digest differs from older plugin-plan-only records even for the default Serial profile;
recapture those records. The game still owns its complete source/service/state inventory.

Start and pause a fresh candidate with the same ordinary product recipe, then call
`request_resume(&mut candidate, &state)`. It first validates and initializes the phase through the
original time owner, completes paused maintenance, then admits one command through the existing
gameplay queue. Nonzero remainder and legal whole-tick debt survive the isolated exact tick and
later maintenance. Drive complete paused maintenance frames, with an explicit deadline, until
`GameSnapshot::restoring` clears or an error occurs. Compare `capture_resume(&candidate)` with
the original state before allowing gameplay to continue. Rejected reconstruction remains
ineligible even if an ordinary command later clears the displayed game error. Retain the original
paused session and captured executable until the candidate passes state and continued-play checks.

The fresh Runtime keeps its own clock, command authority and World identity. Logical game time
is restored separately. The portable time value carries only policy and pending duration; the
receiving Runtime validates its actual policy and derives interpolation while preserving its
local history. Durations outside the game's `u64` nanosecond representation refuse capture.
Scene numbers are local to a World; compare scoped identities when
proving fresh construction. Current Scene and dynamic markers rebuild through their existing
owners, with no old ECS entity, input connection or native handle in the format.

The Host-driven `continuity` binary exchanges bounded versioned files in an explicitly supplied
private temporary directory outside project source. Its arguments are `--project-root ABS`,
`--control-dir ABS`, `--session STRING` and `--generation STRING`. The process starts paused and
accepts finite Observe/Step/Frame/Capture/Restore/Close operations. Session, generation and request
IDs are validated before effects; stdout/stderr carry diagnostics. The test Host owns child
creation, deadlines, immutable executable installations and physical retirement.
Its native child execution uses the same `project_tooling::process` owner as supervised Editor
Play. Each game launch consumes restrictive executable/directory capabilities; only a matching
game Close reply and the original native cleanup receipt prove normal retirement. Diagnostic
output remains a bounded in-memory tail, independent of the file-based game protocol. Candidate
failure cleanup retains its original owner until containment and output readers have completed.

From the engine root, the actual Rust-edit qualification uses the existing shared target and
installed Rust 1.95 baseline:

```text
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --test continuity --run-ignored only --test-threads=1
```

Set `RUSTUP_TOOLCHAIN=1.95.0`, `CARGO_BUILD_JOBS=1`, `CARGO_NET_OFFLINE=true` and an absolute
`CARGO_TARGET_DIR` pointing to the engine's `target` before that command. The test copies source
into its own temporary tree and uses public Cargo executable capture. It performs real movement-body
and private Rust-structure edits, then checks fresh-process state and continued gameplay. It also
exercises build/startup/compatibility failure, stale requests/replies and incumbent retention. Run it serially; normal
package tests keep this nested-build case ignored. Execution results and limits belong to the
[reconstruction plan](../../docs/plans/2026-11-13-rust-state-reconstruction-plan.md).

## Source/Headless Recording And Replay

The `replay` CLI records a finite semantic program against the actual source game and plays its
recording in a fresh process. Build the `replay` binary with the default development feature,
then run it with explicit absolute paths (use `replay.exe` on Windows):

```text
replay record --project-root ABS_PROJECT_ROOT --program ABS_PROGRAM_JSON --output ABS_NEW_RECORDING_JSON
replay play --project-root ABS_PROJECT_ROOT --recording ABS_RECORDING_JSON
```

Use [the supplied program](replays/two-level.program.json) for a complete Meadow/Cavern trajectory:
movement and releases, three pickups and markers, a distinct Cavern instance, Retry and return to
Meadow. Programs use `{"format":1,"steps":[[{"operation":"move","direction":1,"pressed":true}],[],[{"operation":"move","direction":1,"pressed":false}]]}`.
Live and recorded movement share one game-owned decoder: no target, exactly `direction: i64`
in `0..4` and `pressed: bool`, with no additional fields. Malformed live movement reports a game
error without changing held input; the original queue still captures and acknowledges the consumed
batch. Valid releases clear holds during preparation or reconstruction, while new presses wait
for Running. Save/Restore and room commands keep their separate current vocabularies.

Each row advances exactly one fixed tick; `[]` continues held intent. Directions `0..3` mean
left, right, up and down. Other operations are `marker`, `travel`, `instance` and `retry`.

Both paths use the fixed interactive source/headless recipe, remaining paused between steps.
Ordinary paused maintenance waits for Scene activation, reconstruction and task return without
adding simulation ticks. Playback compares each newly produced supported state; source drift or
a mismatched recorded result fails. This profile excludes Save/Restore, open save slots, native
audio/desktop output, cooked Players, arbitrary runtime services and cross-platform determinism.

Input files are limited to 16 MiB. The output parent must already exist and the output must be
absent; recording never overwrites it, including if another process creates it during recording.
The output path must have a Unicode representation for the JSON result; unrepresentable native
paths refuse before gameplay or publication.
Serialization finishes before an owned sibling temporary is written and synchronized. Only the
complete file is published to the final name through the original no-replace filesystem operation.
Failed temporary cleanup retains its actual owner through CLI exit; a postpublication failure
never deletes the published target. Stdout may contain game logs; consume the final
`TWO_LEVEL_REPLAY_RESULT ` line as compact JSON. `ok` agrees with the process exit status.
Success includes `report.steps`, `report.commands` and `report.final_state` after original Session
cleanup. Record results also include `publication.status` (`not_published`, `published` or
`published_uncertain`) and individual data, metadata, name and parent-directory synchronization
facts. Unsupported parent sync does not turn a complete verified publication into a partial file;
it limits crash-durability evidence. If publication already happened but verification or parent
sync fails, inspect the retained destination rather than treating an error as proof of absence.
Failure includes `error.stage`, `error.message` and `cleanup_pending`. Runtime cleanup keeps its
original bounded retry; failed temporary cleanup receives an explicit retry and any unresolved
owner remains retained until process exit. These facts do not promise an atomic project snapshot
or hostile-writer isolation.

## Source/Headless Development Host

The ordinary [development Host](src/development/mod.rs) owns candidate preparation and the single
active command route. The lower-level A/B/C test above remains an independent regression. This
workflow supports the shipped source/headless game inventory; it does not extend desktop Play,
restore native GPU/audio/physics services or patch Rust code inside a running process.

Construct `DevelopmentHost` with the explicit absolute project root and an existing trusted work
parent outside that project. `start` consumes a `CapturedCargoExecutable` selecting this game's
native `continuity` binary with default features disabled and exactly `development` in both
requested and resolved features.
`capture` and `step` always target the active game. Supply an explicit public `CargoBuildRequest`,
the expected initial complete `ResumeState` and finite `ProbeStep` expectations to
`prepare_rebuild`. The Host builds the edited source and compares complete canonical state after
each probe step. It closes that probe through the game and native owners, then restores the
original basis in another fresh process. Probe progress is discarded.

The returned `PreparedRebuild` is a nominal ticket retained by its original Host. `adopt` checks
the prospective source envelope and both complete states before exchanging the command route.
The incumbent may keep stepping while a candidate is retained; if its basis changes, adoption
refuses and the ticket remains available for explicit `cancel`. Foreign, cancelled or consumed
tickets cannot select a candidate. After adoption, all ordinary commands target the new active game.

`AdoptionOutcome` reports committed adoption separately from predecessor retirement. Retirement
initially remains `Pending` until `retry_retirement` drives the original native owner. The new
active game can step/capture during that wait; another rebuild refuses until cleanup completes.
`close` fences new commands and retains all original owners and storage until physical cleanup.
Call it again if Pending. Each retained child gets its own ten-second cleanup drive; a batch may
take several drives. Drop is fallback cleanup, not evidence of successful Close.

The ordinary [`develop`](src/bin/develop.rs) binary exposes this workflow through newline-delimited
JSON. From the engine root, with absolute paths substituted:

```text
cargo +1.95.0 run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --bin develop -- --project-root ABS_GAME_ROOT --work-parent ABS_EXISTING_PARENT --target-dir ABS_ENGINE_TARGET --target x86_64-pc-windows-msvc --profile dev
```

Set the same toolchain, jobs and target environment as the actual-build test above. The explicit
target must match the build compiler's native host. This project authorizes execution of trusted
Rust/Cargo build inputs. The CLI emits one `ready` record, then one result for each command:

| JSON operation | Result |
| --- | --- |
| `{"operation":"observe"}` | Complete current `capture`. |
| `{"operation":"execution_status"}` | Desktop-only current managed Running/Paused state, native activation and read-only game observation. Does not imply canonical capture or presentation. |
| `{"operation":"pause"}` | Apply original desktop Pause and reach a complete paused boundary. Pending input can still refuse canonical capture. |
| `{"operation":"resume"}` | Apply original desktop Resume after explicit native activation; the child continues pumping while the CLI waits for later input. |
| `{"operation":"step","commands":[]}` | One ordinary fixed tick and its complete capture. Semantic movement commands use `{"operation":"move","direction":1,"pressed":true}`; directions are left/right/up/down = 0/1/2/3. |
| `{"operation":"prepare","basis":STATE,"probes":[{"commands":[],"expected":NEXT_STATE}]}` | Actual build, complete-state probe qualification and one retained candidate. Replace `STATE`/`NEXT_STATE` with full objects; expected state is supplied by the caller. |
| `{"operation":"adopt"}` | `committed:true`, new active generation and independent `retirement:"pending"`. Uses the CLI's retained ticket; generation observations grant no selection authority. |
| `{"operation":"cancel"}` | Cancel the CLI's exact retained candidate. |
| `{"operation":"retry_cleanup"}` | Retry retained physical cleanup without changing the active command route. |
| `{"operation":"isolate_input"}` | Freeze the desktop active game's original native keyboard ingress without discarding held intent. |
| `{"operation":"confirm_presentation"}` | Confirm a fresh desktop presentation joined to the complete current canonical state. |
| `{"operation":"activate_input"}` | Reveal/focus the desktop active window and return its original activation state and any activation error. |
| `{"operation":"close"}` | Fence commands and close the session. EOF also closes it. |

Operation refusals produce `ok:false` while retaining the active game. Malformed input ends the
session through Close and exits unsuccessfully. Unknown fields on no-argument commands are
refused, including invented generation/candidate selectors. Responses preserve bounded structured
Cargo diagnostics and distinguish adoption from cleanup. `retirements` reports each original
generation's game Close, fallback termination and native exit/containment/readers/output facts.
Physical cleanup completion alone does not claim graceful game Close. The CLI accepts at most
128 commands, 256 KiB per input line and 64 probe steps. Host rebuilds are limited to eight attempts
and 1 GiB of cumulative captured executable bytes. These allowances do not establish RSS or latency limits.
Session/idle checks occur between operations and after stdin reads; they do not interrupt blocked
stdin or the synchronous Cargo adapter's compiler/build-script calls.

The prospective source envelope covers the exact project directory and complete finite source,
assets, scenes, prefabs and schema trees, plus declared build inputs. It is checked before/after
build and before adoption. This trusted-local per-file envelope is not an atomic filesystem
snapshot, dependency discovery or rollback of arbitrary native/build-script side effects.

Run the production Host's actual-edit/adoption qualification separately and serially:

```text
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --test development_host --run-ignored only --test-threads=1 --no-fail-fast
```

Actual results and platform limits belong to the
[current plan](../../docs/plans/2026-11-13-rust-state-reconstruction-plan.md#game-owned-development-host-and-adoption).

## Caller-Driven Desktop Control

The separate [`desktop_control`](src/bin/desktop_control.rs) consumer uses the ordinary desktop
recipe with the original `DesktopRun` retained between polls. It starts hidden and paused, drives
movement, pickups, markers and travel through public semantic commands, then exercises exact
fixed steps, reveal, Resume/Pause and physical retirement. The original presentation receipt is
joined to its exact retained stock render input. After the original native owners retire, a
separate image replay checks finite player, marker, pickup-interior and background pixels.
That readback is not a capture of the swapchain or display.

From the engine root on Windows, set `WGPU_BACKEND=dx12` and
`NARA_WGPU_FORCE_FALLBACK=0`. Reuse the toolchain, serial build and shared target settings above,
then run:

```text
cargo +1.95.0 run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop --bin desktop_control
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop --test desktop_control --run-ignored only --test-threads=1 --no-fail-fast
```

The ignored test launches that ordinary executable through the shared contained-process owner
and checks its actual exit, containment, readers and output. Executed results, the finite pixel
oracle and platform limits belong to the
[desktop development plan](../../docs/plans/2026-11-13-rust-state-reconstruction-plan.md#desktop-development-and-original-native-session).
This consumer does not reconstruct a desktop game or adopt a rebuilt executable. Its visual
expectation is not an authoritative `ResumeState` capture; the separate reconstruction and
development-Host consumers below own those complete-state and route-exchange checks.
The keyboard-continuation trajectory additionally starts isolated, restores the actual movement
bindings without Started, and reconciles a key released before focus acquisition. One original
exact Step must stop movement before native activation. This exercises the input prerequisite;
it does not qualify a rebuilt desktop candidate.

## Desktop Reconstruction And Explicit Development Placement

With the `desktop` feature, `capture_desktop_resume(&run)` captures through a healthy completed
paused `DesktopRun`. It also captures the original declared keyboard inventory and refuses if
its holds differ from game movement intent or any original input work remains pending. The
headless adapter refuses a native presentation placement. Typed gameplay Move alone is not a
native keyboard member and cannot satisfy desktop capture.

`restore_desktop_resume(&mut run, &state, deadline)` targets one fresh paused isolated game.
It validates the complete bounded state, configuration, Schema and empty native membership,
initializes original time, executes one original restore command/Step and waits for Scene
reconstruction before restoring keyboard membership. Complete canonical bytes must match.
It does not reveal, activate or replace a route. After any applied failure, retain the exact
candidate and drive physical Close; no rollback is implied. The deadline is cooperative and
cannot interrupt a graphics driver, native API or user callback.

The ordinary `desktop_reconstruction` executable uses these APIs with the same recipe in two
sequential native runs. It retains meaningful three-instance progress, a non-anchor player,
held D and 13 ms scheduling phase, verifies incompatible preflight refusal, then reconstructs
the complete state in fresh Runtime/Scene/render/native/GPU identities. Its presentation
receipt must match a restored stock-input seal at or after the capture frame; separate GPU
replay runs after original physical cleanup. Sequential reuse of one retired native pump does
not demonstrate cross-process adoption.

```text
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop --test desktop_reconstruction --run-ignored only --test-threads=1 --no-fail-fast
```

The development Host's explicit `SourceDesktop` placement uses the same existing source,
executable, process and candidate owners. Its candidates start hidden and input-isolated. A
presentation confirmation joins the exact sealed canonical state to the original native receipt;
startup READY alone cannot qualify restoration. Prepare/adopt isolates incumbent native input
before final capture. `activate_active()` is separate from route adoption, so denied focus or
reveal failure retains an already adopted paused game with an actual activation result. Logical
probe movement uses the original Input driver with Synthetic provenance; it does not pretend to
observe OS input. The [owning plan](../../docs/plans/2026-11-13-rust-state-reconstruction-plan.md#desktop-candidate-reconstruction-successor)
records actual execution results and remaining integration limits.

For the CLI, compile `develop` with `--features desktop` and pass `--placement desktop` after
the Cargo argument separator. The CLI selects exactly `desktop,development` for its `continuity`
child; compiling the Host with desktop support alone does not select native execution. `ready` reports
the actual placement. Contained product launches retain the reviewed desktop-session environment
projection; parent `WGPU_BACKEND` and `NARA_WGPU_FORCE_FALLBACK` hints are not inherited. The child
uses the stock default selection and reports its actual adapter/backend in presentation results.
Explicit graphics selection is not yet a development-Host option. Desktop-only JSON operations refuse source/headless
placement. Prepare and adopt can leave the incumbent isolated even after refusal; explicitly
activate the retained game when native input should resume. A committed adoption remains
committed if subsequent activation fails. A `closing` child keeps its original native owner
until an explicit retry reaches physical completion.

Run the production desktop Host's actual body/private-structure edit qualification separately:

```text
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop --test development_desktop --run-ignored only --test-threads=1 --no-fail-fast
```

This game-owned development protocol is version six and requires explicit placement in every
reply. Original bounded envelope decoding and version checks refuse old children. The resume format remains version
two; progress saves and the supervised Editor Play protocol retain their separate formats.

For continuous desktop play, call `activate_input`, then `resume`. Query `execution_status` while
the original native pump runs; its Runtime state is independent of the game's travel phase.
`observe`, exact `step`, capture-dependent `prepare`/`adopt`, presentation confirmation and native
input changes retain their paused-only contract. A running refusal preserves the same live game.
Call `pause`, settle any original pending Input through explicit controls if necessary, then
`isolate_input` and capture before preparing a rebuild. Pause never discards an input edge or
silently inserts a simulation tick. After adoption, call `activate_input` and `resume` explicitly.
Predecessor cleanup may stay Pending while the adopted game runs.

The source-desktop child explicitly uses original Wait cadence for its known paused workload
and Poll for continuous play. Pause itself does not change the engine-wide default. Frame,
Step, reconstruction, native events and presentation confirmation still request original work;
reconstruction requests full paused frames while asynchronous Scene preparation is pending.
This recipe does not install an asset/source watcher or require continuously animated paused shaders.
Consumers adding such work must retain Poll or explicitly drive the required App stages.

Successful execution controls return `result.execution_status`. Complete capture remains a
separate `capture` or `capture_error` field: expected refusal to capture a Running Runtime cannot
turn an applied Resume into an unsuccessful control response. Repeated controls return the actual
current state; no new ticket is claimed when already at the requested state. A control failure
after admission retains the original child for cleanup. Close and EOF can retire a Running child
through the same original native owner. Blocking CLI stdin does not block that child; the existing
finite process/session limits and synchronous build/cancellation limits still apply.

The independent continuous native qualification uses actual matched Windows D Down/Up, original
paused captures, real Rust body/private-structure rebuilds and post-adoption native gameplay:

```text
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop --test continuous_desktop --run-ignored only --test-threads=1 --no-fail-fast
```

Keep the desktop idle and physical A/D/W/S initially Up. Other keyboard input during the trajectory
invalidates its neutral movement oracle. The guard rejects existing holds instead of releasing
keys it does not own. Its bounded native input guard releases
an injected hold during failure cleanup. Actual results and limits belong to the
[continuous development unit](../../docs/plans/2026-11-13-rust-state-reconstruction-plan.md#continuous-desktop-development-successor).

## Audible Development And Effect Reset

The explicit `SourceDesktopAudio` placement joins the existing source-desktop development Host
to the actual Windows CPAL game. Its child selects exactly `audio-cpal,desktop,development` with
defaults disabled. Compiling those features alone does not enable sound in a silent placement.
Run the ordinary CLI with explicit absolute paths:

```text
cargo +1.95.0 run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop,audio-cpal --bin develop -- --project-root ABS_GAME_ROOT --work-parent ABS_EXISTING_PARENT --target-dir ABS_ENGINE_TARGET --target x86_64-pc-windows-msvc --profile dev --placement audio-desktop
```

Every audible development process starts with an empty mixer and gameplay publication disabled.
Step still changes normal game state; pickup/travel sounds while disabled are discarded, never
queued for enablement. Probe and prepared candidate processes remain disabled. A fresh adopter
restores the complete canonical game state and original presentation, without restoring an audio
cursor or replaying old events. The recipe fingerprint records that reset policy. Ordinary
`audio::recipe` continues refusing exact reconstruction.

`{"operation":"audio_status"}` observes only the current adopted child's reset/publication and
device facts. `{"operation":"enable_audio"}` enables future gameplay effects explicitly.
`activate_input` also enables sound before requesting original native activation. In this
placement, enablement, activation and Resume refuse while original predecessor audio/process
physical retirement remains unfinished or unconfirmed. Adoption still reports committed plus Pending;
the adopted paused game can Step/capture during that wait. Drive `retry_cleanup`, then enable or
activate and Resume. A postcommit cleanup, device or focus error does not restore an old route.
Completed cleanup errors remain in the retirement report. An unrelated failed game shutdown
cannot block a new healthy audio output after the original stream, callbacks and contained
process have physically retired; error severity and physical completion remain separate facts.

Ordinary Pause preserves the device-clock policy: an already started short effect can finish.
Samples already delivered to the device cannot be rolled back. Preparation does not suppress
the incumbent's already enabled effects; the prospective processes cannot publish new ones.
Final retirement reports retain the exact child's audio snapshot alongside the original native
process receipt. Neither counters nor audio flags replace stream/callback/Runtime/process cleanup.
Production development leaves callback PCM capture disabled. The existing native audio controls
use finite capture for sample comparison; this does not establish human audibility or hardware
output latency.

```text
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop,audio-cpal --test audio_development --run-ignored only --test-threads=1 --no-fail-fast --no-tests fail
```

The [owning unit](../../docs/plans/2026-11-03-joint-motion-execution-plan.md#audible-development-with-explicit-effect-reset)
records actual execution results and limits. This game-owned reset policy does not admit arbitrary
native service restoration or change the supervised Editor Play protocol.

## Physical Task Return Before Capture

Canonical capture also requires original admitted tasks to have physically returned.
Cancelled terminals and zero queued/running statistics are insufficient while a capture's
Drop is still owned by an executor. The game uses `TaskPools::return_state()` and refuses
pending return or cleanup failure without changing game state. Child startup/capture/restore
waits preserve their original deadlines. A failed task's eventual physical Close remains a
failure; it does not become a successful capture or clean shutdown. Resume format v2 and
the game-owned protocol v6 retain their existing field definitions outside the explicit audio extension.

## Native Request Notifications

The development child watches only its existing private control directory. Create/rename/modify
hints wake its original native maintenance boundary, or a condition variable in headless placement.
Notifications grant no file or command authority: the original capability-relative inbox and exact
version, placement, session, generation and request validation still admit every packet. A 250 ms
reconciliation ceiling handles lost notifications within the existing operation and idle limits.
The child checks immediately at startup, then waits before another inbox probe unless a hint is
already pending. Notification and reconciliation both return through the original lifetime and
request-budget checks before the next receive; responding does not require an extra empty probe.
Parent response polling remains 10 ms. This is not a zero-wakeup or hard real-time contract;
the Windows vendor watcher retains its 100 ms internal wait and backend reporting limitations.

Protocol v6 requires `control_service` diagnostics in every response. The public Host exposes the
last validated response from the active child through `control_service()`, and the CLI includes
the same field. Candidate/predecessor observations do not replace active observations. Counters
are outside canonical state and never prove authority, successful commands or native cleanup.
`frame()` requests original paused App maintenance without a fixed simulation tick.
Observations include the original process-local `app_frame` sequence independently of
`engine_tick` and logical ticks. It records original execution, not cross-process authority
or canonical state.

Close freezes notification delivery, closes the original game and joins the original watcher
before replying Closed. Pending/failure retains those owners; the native process cleanup receipt
remains independently required. Retirement reports preserve the last validated diagnostics after
the active route is gone. These changes do not extend the Editor's supervised Play protocol.

The finite Windows hardware comparison builds two variants differing only in the child's 2 ms
versus 250 ms reconciliation constant. It uses three alternating pairs per placement, twelve
fresh children, complete game-state checks, request latency, actual desktop presentation and
physical cleanup. Supply a new absolute output path before running serially:

```text
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop,cost-measurement --test development_notifications --run-ignored only --test-threads=1 --no-fail-fast --no-tests fail
```

Set `CARGO_BUILD_JOBS=1`, the absolute shared `CARGO_TARGET_DIR`, and
`NARA_DEVELOPMENT_NOTIFICATIONS_OUTPUT` to the new JSON path. Actual results and scope belong
to the [runtime cost report](../../docs/benchmarks/2026-11-14-development-runtime-costs.md).

## Active Gameplay Runtime Baseline

`runtime_workload` measures three complete ordinary `Scenario::Travel` scripts through the
native Runtime. They move the player, collect pickups, create markers, travel between levels
and instances, restart and return to Meadow. It uses normal semantic commands, native pacing
and the original renderer. Native keyboard input stays isolated; no manual delta or exact
Step is used by the measurement. Each run must finish the script and physically retire its
original native owners before the next recovers the same physical event loop.

Select hardware DX12 and an absolute new output file in PowerShell:

```powershell
$env:WGPU_BACKEND = 'dx12'
$env:NARA_WGPU_FORCE_FALLBACK = '0'
$env:NARA_RUNTIME_WORKLOAD_OUTPUT = Join-Path $env:TEMP 'nara-active-workload.json'
cargo +1.95.0 run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop,cost-measurement --bin runtime_workload
```

The file is created exclusively; choose a new name for another run. The report retains actual
original presentation IDs, gameplay facts, per-run finite wall-time distributions, original
queue-completion samples, whole-process CPU/memory and telemetry/native cleanup. These
descriptive quantiles are not production tail bounds, GPU execution or scanout. Loaded code,
driver and filesystem caches are shared between runs. Scripted verification remains ineligible
for the interactive resume format; the measurement does not weaken that policy.

The same consumer explicitly enables original Desktop startup observations and five fixed native
Runtime operation wall spans. Every original frame retains namespace preparation, Surface
acquisition, candidate preparation, program execution and Surface presentation in the report's
`runtime_phase_order`, along with issued native work counters. Their checked sum and the remaining
App-frame wall span are separate from CPU counters and queue-completion observations. The remainder
stays `null` when an initial presentation has no matching App start; actual operation spans are
still retained. Startup spans overlap and must not be added together. The completed startup
snapshot must remain unchanged through gameplay and original retirement.

Add `scene-work-observation` to the desktop/cost-measurement features to record the existing
`nara::scene::work` spans for compiled-component preflight/native decoding, Prefab expansion,
flat admission and Scene preparation/publication. `scene_decode` is nested in preparation and
other receiving validation; it excludes source reads and JSON/RON parsing. The separate source
envelopes below attribute those operations at their own existing boundaries. These
are nested wall spans across worker and owner threads. The explicit measurement binary installs
its subscriber once, binds each span to the original run and seals after original physical
retirement, before independent GPU replay. `owner_thread_calls` and `worker_thread_calls` expose
coverage; retained, unfinished or reentrant spans reject the observation. The
`nested_span_wall_sum_ns` field overlaps phases and is not CPU time, an additive phase budget or
an extraction/queue measurement. The feature is opt-in and leaves the default runtime without
tracing instrumentation. A failed run retains its Scene and physical cleanup facts in the report;
stderr carries the short failure rather than duplicating that report.

The source split also records `project_content_read`, `scene_source_decode` and
`prefab_source_decode`. The read envelope includes `LoadContext::read_file_inner` capability
opening, bounded reads, digesting and budget bookkeeping, including catalogue metadata bytes
read through that method. It excludes separate SourceReadiness reads and catalogue discovery.
Candidate decoding includes wire checks, header/payload parsing, domain limits and canonicalization;
later composition-source admission and retained-size bookkeeping remain outside those spans.
`scene_program_composition`, `scene_component_materialization` and `scene_reference_closure`
separate existing staged expansion operations. Nested Prefab source loading is part of
`scene_structure`; that label does not mean pure layout time. `status: complete` describes the
registered spans' lifetime, not measurement of every source-loading operation.

The source-format subphases are `scene_wire_preflight`, `scene_header_decode`,
`scene_payload_decode` and `scene_source_finish`. They preserve the existing byte/shape/header/
compatibility gates; finish covers domain checks and canonical ordering. Each successful source
Scene/Prefab candidate decode contributes one call to each phase. The counters distinguish owner
and worker execution, and nested wall sums remain separate from CPU or single-transition latency.

Materialization also separates `scene_value_admission`, `scene_value_clone`,
`scene_value_migration`, `scene_value_defaults`, `scene_value_validation`,
`scene_value_reference_plan` and `scene_value_reference_validation`. Migration includes its
post-migration value admission. These are existing compiler operations, not another profiler
owner or permission to skip checks. The ordinary workload compares their actual owner/worker
call counts and seals only after the original Runtime physically retires; their nested wall
sums do not replace whole-game or allocation measurements.

`entry_to_first_presentation_observation_wall_ns` starts before source-root/recipe construction and
ends at the original first actual presented observation. It includes caller/native waits and
excludes process loading; it is not cold-start, GPU duration or scanout. The first presentation and
first initialized presentation are reported separately. These explicitly observed recipes retain
their measurement cost; ordinary shipping options leave the new clocks disabled.

The nested `runtime_wall.namespace` report follows `namespace_phase_order`: Instance preparation,
Surface creation, Adapter request, Device request and Surface configuration. It checks their sum
inside that same frame's parent namespace span. Reused devices have absent Adapter/Device requests,
distinct from a measured zero. The complete native control requires exactly one original request
pair across each Travel lifetime; continued Scene changes reuse that original device. This is
returned-call wall attribution, including original wgpu candidate probing and driver waits, without
another adapter enumeration or a device cache shared between Runtimes.

```text
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop,cost-measurement --test runtime_workload --run-ignored only --test-threads=1 --no-fail-fast --no-tests fail
```

Add `-- --tiles 64`, `-- --tiles 4096` or `-- --tiles 16384` to the `cargo run` command
to execute the same three complete Travel lifetimes with source-authored Tilemaps. Each population
covers the same 256-by-256 world footprint. The consumer constructs and later removes a private
content copy; it leaves this checkout's authored sources unchanged. The 16,384-cell variant uses
a root-bound Prefab for half of its terrain to respect the original per-file wire limits, so its
source layout differs from the smaller populations. These finite workloads do not establish a
supported-world capacity or a pure cell-count scaling curve.

Tile reports include original Scene-instance retirement checks, current CPU asset counts,
extracted versus visible queued cells, actual drawing statistics and logical GPU resource points.
After the original native Runtime, World and sources retire, three retained original inputs
are replayed into equal-sized images through an independent public Host. Known-source ground
and background pixels are checked, earlier outputs are reread after later publication and producer
retirement, and original submission prefixes and Host resources must retire. This proves selected
captured-input pixels and ownership; it does not read the swapchain or measure total VRAM, a
memory plateau, GPU execution or scanout. Source-copy preparation is timed separately from the
entry-to-first-presentation endpoint. The default baseline retains its original source path.

The ignored native population test runs all three populations serially. Its separate observer-panic
control must retire the original native owners and remove the copied content. Set
`NARA_RUNTIME_WORKLOAD_EVIDENCE` to a new absolute JSON path to preserve positive reports; population
reports use that filename stem followed by `-tiles-64`, `-tiles-4096` or `-tiles-16384`.

## Development Cost Observations

`DevelopmentHost::enable_timings()` enables fixed cumulative elapsed observations. The default
is disabled and performs no instrumentation clock reads. `timing_snapshot()` returns counts,
returned failures, total elapsed and lifetime maximum for the fixed `DevelopmentTimingPhase::ALL`
boundaries. Subtract counts/totals across an attempt while unsaturated; a difference of maxima is
not an attempt maximum. Restore/probe/adoption spans include child phases, so never add phase
totals as disjoint work. These observations contain no authority and change no result or cleanup.

The `CargoBuild` total contains six elapsed segments taken at the existing adapter checkpoints:
`CargoAdmissionAndToolObservation`, `CargoMetadataAndRechecks`,
`CargoCompilationAndArtifactChecks`, `CargoCaptureAdmission`,
`CargoCapturedBytesAndProvenance` and `CargoFinalValidation`. Each includes the checks adjacent
to its endpoints; the compilation segment also includes Cargo output decoding and artifact
hashing. An error completes only its current segment as failed. These segments partition the
observed adapter interval, but adding them to `CargoBuild` would count the same work twice.

`ChildStartup` contains `ChildNamespacePreparation`, `ChildLaunchAuthority`, `ChildNativeSpawn`
and `ChildReadyWait`. They describe the original directory/command preparation, exact installed
executable admission, native process creation and initialized READY wait. Their sum is contained
by the parent with small construction gaps; adding parent and children would count work twice.
Complete `start` also includes executable installation and the immediate post-READY observation.
READY is separate from restored state or exact original presentation.

The ignored `development_startup` consumer alternates six independent headless/desktop starts,
excluding one warmup and retaining five processes per placement. Their features/recipes differ
deliberately. Each verifies initialized complete state and original physical cleanup; desktop
additionally checks a fresh original presentation. An excluded invalid-manifest case checks
post-spawn failure custody. Set an explicit absolute **new** `NARA_DEVELOPMENT_STARTUP_OUTPUT`
JSON path and the same toolchain/offline/jobs/target environment used below:

```text
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop --test development_startup --run-ignored only --test-threads=1 --no-fail-fast
```

Output is written only after every process and the copied source fixture retire. Startup spans
are elapsed parent observations, not pure native creation, GPU initialization, CPU execution or
scanout. A failing READY wait retains the original process for physical cleanup.

The ignored `development_turnaround` consumer measures one excluded warmup and ten retained
requests for each of capture, execution status and fresh canonical presentation. Request order
rotates within one hidden, input-isolated paused game. Every result must retain the complete
original state or original presentation identity; no Step or Resume is used during sampling.
Set an explicit absolute **new** `NARA_DEVELOPMENT_TURNAROUND_OUTPUT` JSON path and the same
toolchain/offline/jobs/target environment used below, then run:

```text
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop --test development_turnaround --run-ignored only --test-threads=1 --no-fail-fast
```

Its API wall spans include original file transport and native work, not CPU alone. Output is
written only after normal original process cleanup and source-fixture removal.

The ignored `development_cost` test measures one excluded warmup plus five retained no-edit,
actual body-edit and private-structure-edit attempts per class, through the complete production
desktop workflow. Set an explicit absolute **new** `NARA_DEVELOPMENT_COST_OUTPUT` JSON path,
Rust 1.95.0, offline mode, jobs one and the shared root target, then run serially:

```text
cargo +1.95.0 nextest run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop --test development_cost --run-ignored only --test-threads=1 --no-fail-fast
```

The separate `runtime_cost` executable compares three alternating pairs of ordinary desktop
execution and the same game with `DevelopmentPresentationPlugin`. Both groups use identical
native pacing, neutral input and original telemetry. Each excludes 30 successful frames and
retains 120, then compares explicit paused cadence and physical retirement before reusing the
same event loop. Keep the desktop available and do not manipulate its input or windows during
the run.

```text
cargo +1.95.0 run --manifest-path samples/two-level-game/Cargo.toml --locked --offline --release --features desktop,cost-measurement --bin runtime_cost
```

`--pairs`, `--frames` and `--warmup` accept bounded positive counts; defaults are 3, 120 and 30.
One frame contains zero between-frame CPU intervals and is unsuitable for sustained-cost
comparison. Set `WGPU_BACKEND=dx12` and `NARA_WGPU_FORCE_FALLBACK=0` for the recorded Windows
hardware scope. Optional `NARA_RUNTIME_COST_OUTPUT` writes the structured report to an explicit
absolute new file; ordinary game startup text still goes to stdout. Without it, stdout ends
with one `two-level-runtime-cost/v4` JSON record after those startup messages.

Each run compares three 300 ms paused conditions in alternating order, with 100 ms settling
excluded: Poll, Wait with an explicitly requested 2 ms observer deadline, and Wait without an
observer deadline. Native waits may use the remaining interval instead of an independent
2 ms caller cap. Complete canonical bytes and simulation/logical ticks must remain unchanged;
App frames, actual presentation receipts, observer callbacks, native poll calls and quantized
whole-process CPU are observed separately. Acceptance requires at least an 80% App-frame
reduction under Wait and at least 90% fewer callbacks/poll calls without the periodic observer.

A separate control arms one absolute 100 ms observer deadline through original maintenance,
then allows a 1 s caller wait. Its original callback must observe the deadline by 250 ms in
this finite workload without advancing App/game state. The cooperative bound does not interrupt
native calls or callbacks. Existing Wait maintenance, exact Step/full maintenance, resumed
gameplay and telemetry/native retirement checks remain. A rejected reduction gate preserves
the complete report and exits unsuccessfully. Version four distinguishes caller wait caps,
explicit observer requests and original pending-GPU maintenance; counts do not establish
CPU-free idle, stable CPU savings, power or a general background-task wake mechanism.

Both comparison recipes also select the original `DesktopRunOptions::with_startup_timings()`.
Their additive `startup` diagnostics retain content preparation, Runtime startup, native session
construction, first native poll and accumulated native polling through first actual presentation.
The aggregate includes the remainder of the presenting poll and excludes caller gaps; it overlaps
the first-poll duration. A fast first poll can defer most initialization to later polls.
Separate public-call wall spans cover initial paused settlement and neutral keyboard
restoration/maintenance. The completed snapshot is copied after neutral maintenance and canonical
capture, before reveal and running-frame warmup; later work and physical cleanup must leave it
unchanged. Missing phase values are `null`, not zero. Native-poll wall time includes its original
window/device/frame work; it is not pure GPU initialization. The six runs share a process, loaded
driver/code caches and recovered physical event loop, so these observations are not independent
cold-process startups.

The separate `first_presentation` record retains the first actual receipt's exact Runtime,
App/render frame, window, surface, backend and device identities, adapter and App-start-to-post-Host
wall span. It does not consume the running warmup or 120 measured frames, and includes original
driver waits rather than measuring GPU execution or scanout.

The opt-in `cost-measurement` feature enables read-only Windows current-process counters through
the already locked SDK dependency. It does not enter normal desktop or reconstructed-child
features. Whole-process CPU/working-set/private observations are separate from frame wall
and original GPU-completion spans; they are not per-frame CPU execution, GPU execution, scanout,
UI-exclusive memory or a leak proof. See the [measured report](../../docs/benchmarks/2026-11-14-development-runtime-costs.md)
for raw observations, complete endpoints, finite sample limits and the next attribution.

## Ownership And Limits

- `CurrentScene` owns the loaded level source and its root activation. Gameplay retains the logical
  `LevelKey` and the separate activation receipt for each authored marker. Normal R uses the retained
  committed level and marker sources even if their disk files have changed.
- `ProjectScenes` prepares only startup or declared content through the original Host loader and
  budget. The game inspects a read-only target, validates all checkpoint-referenced level sources
  one at a time, then explicitly commits the final target with the existing protected overlay.
- Collected authored pickups remain Scene members. Their game-owned collected facts make them
  inactive and set their actual Sprite alpha to zero. They are logically removed, not despawned.
- Dynamic marker IDs and positions are game-owned. Each authored tree has its own engine
  activation and internal references. Travel retires the current marker activations in the level
  replacement, then reconstructs the target markers. A reconstruction failure reports `Degraded`.
  R can reconstruct missing marker geometry or a legally retired child while preserving the
  current level receipt and checkpoint facts. See the
  [authored marker workflow](../../docs/plans/2026-10-20-persistent-two-level-game-plan.md#authored-marker-instantiation-successor)
  for the supported reconstruction cases.
- The versioned slot contains only explicit checkpoint/progress facts. Restore rebuilds movement,
  player position and dynamic entities at the authored checkpoint; it is not arbitrary-tick replay
  or serialization of World entities, tasks, handles, native resources or every component.
  In this game score has one source: each distinct collected pickup awards ten points. Content
  admission rejects otherwise well-formed checkpoints whose score disagrees with those facts.
- The single local writer uses an explicitly opened `TrustedLocal` parent capability, bounded
  decoding, advisory/platform locks, expected identity plus content digest, a synced temporary file,
  atomic name replacement and identity-bound readback. This is a cooperative local-file contract,
  not protection against hostile concurrent writers. A changed target is rejected. Failed temporary
  cleanup remains owned by the slot and its Runtime close participant. File-save success reports
  the parent-directory sync fact separately; unsupported directory sync is not crash-durability proof.
  The current Linux filesystem adapter cannot discard a failed candidate by exact opened-object
  identity. That failure remains `CleanupPending` and prevents clean Runtime shutdown; the sample
  retains the cleanup owner and does not substitute a pathname deletion or report false completion.
  Save is synchronous and bounded to 64 KiB of checkpoint data; this does not guarantee frame time
  or bound operating-system I/O latency. The headless binary's ten-second deadline bounds cleanup
  retries after gameplay returns, not the whole gameplay run. Child-process qualification adds its
  own external 90-second deadline and bounded captured output.

The headless entry uses complete managed frames through `HeadlessRunIntent::frames` so PreUpdate
collects semantic commands and TaskUpdate publishes prepared Scene results. Its supplied 20 ms
real delta is simulation input, not a sleep. App time settings remain authoritative. The separate
exact-tick API intentionally omits ordinary gameplay frame stages. This finite script has a
200,000-frame ceiling and reports 30 seconds without actual progress using a monotonic clock;
neither is a gameplay performance guarantee or an interrupt of a blocked synchronous operation.
Travel, fresh-process save/restore and retained-source Retry pass the sample's 24-test Windows
cohort. The November 13 release-mode CPU/headless cohort passes 32/32, including both marker
recovery trajectories and the original save/travel controls. The earlier release export
separately passes both startup product observations and the same three
travel/save/restore operations from an unrelated directory. This does not qualify other platforms
or a complete interactive desktop control trajectory.

## Export

```text
nara export . --parent /absolute/existing-parent --destination two-level-candidate
```

The destination must be absent. Run `bin/headless --verify-travel` and the two checkpoint commands
from an unrelated directory; use `.exe` on Windows. `bin/desktop --verify-first-present` verifies
actual first presentation through the ordinary shutdown path. Both declared level sources and their
Images/metadata are delivered in the sibling `project/` tree. This is a source-carrying development
candidate, with checkpoint location still chosen by the application, rather than a full project backup.

Export uses the existing protocol-2 headless/desktop targets. It checks every declared Scene for
the base Headless settings and again for the `desktop` profile. The sibling `project/` tree carries
the union of both complete deliveries, including profile-specific files for Cavern. Each executable
still has its own startup agreement. If the packaged tree exists, it is the executable's source
authority. The five-file build-input observation covers the selected entry sources and manifests.
Cargo compiles gameplay modules through its ordinary dependency graph.
