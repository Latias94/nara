# Cooked Content And Captured Player Delivery

Date: 2026-10-31. Status: implemented and qualified for the recorded Windows and Linux
headless deliveries and local sustained Server operation. Broader target delivery remains open.
Parent: [foundation R7](2026-10-23-foundation-architecture-rebaseline-plan.md#r7-target-build-and-base-source-free-delivery).
Prerequisite: [explicit Cargo selection](2026-10-31-explicit-cargo-build-selection-plan.md).
Broader direction: Proposed [ADR 0088](../architecture/adr/0088-target-build-cook-package-and-runtime-content-catalog.md).

The [qualification](../knowledge/engineering/verification/2026-10/2026-10-31T050529Z-cooked-content-assembly-captured-player-binding-and-relocatable-source-free-delivery-qualification-816fbb7867fb4c3288a2b2b34f27e1ff.md)
records final106 owner tests, actual independent cooked delivery, original custom/foreign and
seven-case processed regressions, independent reviews and explicit native/budget/freshness limits.
The [guide](../guides/cooked-player-delivery.md) documents the production interfaces. This closes
the selected finite outcome without completing all R7 or the full foundation Goal.

## Dedicated Server From Authored Scenes

Use the same Two-Level Game source documents for the local Players and the dedicated server.
The compiled cooker loads the admitted authoring profile through the existing source delivery
owner. The game selects the Server profile from that same manifest, a server recipe, and an
explicit set of presentation component types to omit. Both profiles must use the same physical
project and source roots. Each target Schema owner must match the source composition's current
and predecessor catalogs. The server configuration clears the inherited action map.

Apply component omissions while the existing Scene cooker makes its independent flat output.
Preserve entity identity, hierarchy, Prefab provenance, surviving references and the source
selection's complete file observations. The original cooked builder traverses dependencies from
the projected payloads and the target recipe's selected build contributions. A retained component
still requires its target definition and every mandatory dependency. The same package owner binds
the target catalog to the generated expectation and captured `server` executable.

The game keeps one simulation, Scene owner, command queue and checkpoint format. Exercise
movement, collision, scoring, marker topology, both auxiliary regions, primary travel, Retry,
and checkpoint Save followed by Restore in a fresh relocated process. Inspect the actual server
World and delivered catalog, and audit the native compiler's locked target dependency selection.
The package must contain only the five Scene Products needed by this game, with two declared
travel roots. The server binary excludes native graphics and window adapters, UI, device audio,
controller backends, and project tooling. Audit the backend-neutral content and input vocabulary
separately. The selected scope is a local authoritative runtime consumer without networking.

Pinned Godot `b070e63b161aa816afac71dcfc9d60859d38f220` selects retained, stripped and removed
resources at export. Its placeholder semantics are owned by the resource type. Pinned Bevy
`779770720e6ad916c1de38cf66a67d0e079ed1e3` uses Cargo features and plugin composition as separate
decisions. The two references support separate evidence for delivered content, executable
dependencies and installed runtime services. [Godot export plugin](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/editor/export/dedicated_server_export_plugin.cpp) · [Bevy headless example](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/examples/app/headless.rs)

### Dedicated Server Qualification

The first Windows x64 Rust 1.95.0 release engine selection passes 106 cases, including projected
Prefab identity and surviving references, budget boundaries, original Cook, and processed
content (`ab9bcb6f-049d-4c39-99d5-877ef0fcf822`). Review extended the final publication
fence to record the source action map's sidecar and Product ownership even when the server
omits that member. The two selections compare the actual content roots and allow a different
cache path. The final compiled Cook and package selection passes 28/28 in
`66191f17-7f27-49e6-b738-092665e2ed8b`. A new sidecar for an originally unaccompanied Input and a
second source claiming that same UUID both refuse publication.

The game builds the generated entry with only the `cooked-server` feature and passes the
server-only all-target check. Local Player features still select the existing behavior and
now imply `local-player`. Development Host compares the exact requested placement and the
compiler's resolved feature list separately. The Windows release sample selection passes
309 cases across 15 test binaries (`4e871d21-3e36-4f51-b87c-79bd447d7396`). It includes
authored and cooked content, exact source resume, runtime collision, a local pickup whose
Sprite is temporarily absent, and the stricter Development Host feature boundary.
Both companion process tests pass in `ab59349e-eaf0-4908-af8c-b88808e9971a`. The relocated
desktop Player presents through Windows DX12, drives the native keyboard path with injected
input, saves and restores in a fresh process. The Development Host builds actual Rust body and
private-structure edits and adopts fresh candidates after their probe processes retire.

The production Windows release delivery passes both placements in
`6e26cc7c-214f-459d-968e-bc0158bed513`. The ordinary headless Player remains source-free after
relocation. The `server` package has exactly two declared travel roots, five Scene members,
two Immediate and two Deferred Scene edges, and zero non-Scene members. The test decodes all
five Scene payloads under the target Schema, checks each authored `ProjectLocation`, and checks
the published pack, executable, generated source input and Cargo lock digests. The fixture's
source and cache copies are removed before execution from an unrelated empty working directory
with a sealed environment and unavailable tool path.

The running server verifies that `FixedTime` preserves debt and that the World lacks raw Input,
Windows, renderer state, and Image, Material and TileSet services. Its region trajectory uses
eleven externally submitted command identities, all acknowledged by the original queue. It
finds both relics, closes one room while retaining the original other room, reopens the first
room, and confirms that Cavern travel retires both and the original Meadow. One held movement
tick at the west wall leaves the explorer at its collision contact. Separate finite processes
verify pickups, marker reconstruction, Retry, a score-30 Save, and fresh Restore of that file.
A valid pack with a different catalog digest refuses before startup. Original processes retire
normally without termination intervention.

The first published `server` selection had 170 unique package names in the locked Windows
`normal,build` graph. The build excluded native graphics, Winit, UI and Editor packages,
audio-device and controller backends, the game's Sprite animation and WAV packages, and
development tooling. That build compiled `nara_input`, `nara_window` and stock CPU 2D crates
through the processed Host while leaving their runtime services out of the World. The evidence
covers one game and target. Foreign execution, a long-running service loop, networking, and
processed exact-state reconstruction remain open.
[Delivered process workflow](../../samples/two-level-game/tests/support/server_delivery/mod.rs) · [Cargo graph audit](../../samples/two-level-game/tests/support/server_delivery/graph.rs)

### Dedicated Server Dependency Boundary

The `a36ce0f` Two-Level Server builds with the root `runtime-core` and `serde` features. These select
the same immutable Runtime content base, executable configuration and codec agreement, Headless
Host, Scene request, and residency transaction used by the local Player. The server keeps the
compiled Scene definitions, typed CPU collections, Transform2d, fixed time, command queue and
Rapier. Source loading and the first-party Image, Material and TileSet publication paths require
the client `runtime-2d` feature. Two-Level Game selects that feature through `local-player`.

Verify three independent claims. The locked `normal,build` graph for the actual server build
excludes Image, Material, Render, Sprite and TileMap packages as well as the previous native
client and tooling exclusions. The relocated package retains the five Scene members and two
travel roots, and the running World still lacks the raw Input and Window resources. Run the same
command, collision, region, travel, Save and fresh Restore trajectory through the generated server
executable. Run the local Player control through the existing client feature selection and the
original typed asset publication. Keep Input and Window's backend-neutral packages at their
existing core boundary for this unit.

Bevy's headless example selects its Cargo features and then configures the same application and
schedule runner used in other placements. Godot's server export selects its resource projection
at export. These references support separate checks for the executable graph, cooked content and
installed services. [Bevy headless example](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/examples/app/headless.rs) · [Godot export plugin](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/editor/export/dedicated_server_export_plugin.cpp)

The isolated Windows x64 `cooked-server` all-target check passes on Rust 1.95.0. The locked
`normal,build` selection has 142 package names, down from 170 in the prior delivery. Both checks
use the independent sample manifest. The all-target check compiles the selection with a bootstrap
placeholder. The actual Cook trajectory supplies the executable's generated expectation.

The core-only engine and public selection passes 105/105 in
`366c7154-a59d-4a0f-9f96-66e082479140`. It covers processed dependency and source-base admission,
coordinated travel and retry after a native participant refuses retirement, direct Scene custody,
and a `HeadlessSession` that publishes custom CPU collections from captured content after the
original files have been removed. Closing returns the original decoded-product reservation.
Compilation also passes with `runtime-core` alone, the stock 2D and source-tooling selection,
and `runtime-ui,serde`. [Typed CPU session workflow](../../src/runtime_content/prepared/tests.rs) · [Managed Scene cases](../../src/project_host/runtime/scene_requests/tests/instances.rs)

The `runtime-ui,serde` behavioral controls pass 2/2 in `65c48079-a0ea-423d-b679-bd09cac436b3`.
A solid `UiVisual` starts as a real Scene component. An image brush in the same selection
receives the typed missing-publication diagnostic in both deep validation and Headless startup.
The referenced CPU product remains independently decodable. This is a content and startup check.
The stock Host, content, Cook and public startup selection passes 193/193 in
`8c00e1dc-bf97-4789-a796-c9c55d71f7f2`, including source-backed startup and both Sprite and UI
Image publication controls. The Two-Level release selection passes 219/219 with one ignored
real-build continuity case in `b69d1312-2430-455e-aa68-2992fcf0785d`. It exercises source and
processed gameplay, scene groups, checkpoints, replay and the authored cook inputs.

The actual native release Server and headless Player deliveries pass 2/2 in
`503dd023-b4e0-4fd9-bfb4-a892b2a84056`. Cook generates the Server entry from the selected pack.
Its locked Windows graph contains the same 142 package names, and the pack has exactly five
Scene members, two travel roots, two Immediate and two Deferred Scene edges, and zero non-Scene
members. The relocated executable completes travel, concurrent regions, Save and fresh Restore.
The wrong-content control refuses while opening the pack. The original native processes retire
normally. The Player control runs through the same authored game with its local assets and
separate feature selection. [Server delivery assertions](../../samples/two-level-game/tests/support/server_delivery/mod.rs)

Independent reviews checked source and processed Host custody, schema and codec agreement
between cooker and executable, and stock publication order. Narrow test compilation exposed
two assertions using types gated by unrelated features. The corrected tests check the original
direct Scene's custody and select the renderer assertion when its crate is compiled. The core
and client library correctness Clippy checks pass. These runs cover Windows x64 and headless
execution. Foreign runtime, desktop GPU presentation and long-running server behavior have
separate qualification.

### Linux Headless Player And Server Qualification

Both Two-Level release deliveries pass on `430f07f` with Rust 1.95.0, `x86_64-unknown-linux-gnu`,
Ubuntu 26.04 under WSL2 kernel `6.18.33.2-microsoft-standard-WSL2`, and a local ext4 checkout and
test directory. Nextest run `12920e39-5cfc-479c-984a-ea70217c00e1` passes 2/2. The original
production Cooker builds both executables with their generated pack expectations, using the
committed lockfile. The fixture relocates each installation, removes its source and cache, and
runs the ELF binaries in an empty working directory with a sealed environment and an unavailable
tool path. The original Linux process owners report normal exit, empty containment, and drained
output.

The dedicated Server package contains five Scene members, two travel roots, two Immediate and
two Deferred Scene edges, and zero non-Scene members. Its locked `normal,build` graph has 143
package names and keeps the previous stock client crate exclusions. The Server completes
collision, travel, concurrent regions, Save and fresh Restore. A valid pack with a different
catalog digest is rejected before startup. The headless Player control also checks prepared
client resources, input, Save and fresh Restore, and continued play from Meadow after Deferred
Cavern corruption. [Server workflow](../../samples/two-level-game/tests/support/server_delivery/mod.rs) · [Player workflow](../../tests/support/cooked_delivery.rs)

A second publication used the ordinary Cooker CLI for both targets. Each binary ran in its own
Bubblewrap mount, user and process namespaces on the same WSL2 instance. The namespace exposed
the published three-file directory read-only, the ELF loader and the three system libraries
reported by `readelf`, and a private writable checkpoint directory. Each run started in an empty
working directory with `PATH` set to an unavailable location. The project, cache, compiler and
Cargo were outside the mount namespace.

All four processes exited successfully and reported completed cleanup. Each Save produced a
629-byte Cavern checkpoint with a score of 30, three collected pickups and three stored markers.
Each fresh Restore reached Meadow with the same score and pickup count, and the checkpoint bytes
matched the original Save. The Server also reported the restored Cavern state before travel.

The initial default `/tmp` run (`559f0275-206c-4984-ba8d-5c1646f75eb5`) built both selected
executables and refused directory publication before staging in both cases. `/tmp` was mounted
as tmpfs. Selecting an ext4 `TMPDIR` satisfied the existing cooperative-writer publication tier.
This qualifies native Linux headless execution on the observed WSL2 environment. Desktop
presentation, older distributions and a separate runtime machine retain their own evidence gates.
[Sample acceptance setup](../../samples/two-level-game/README.md#source-free-dedicated-server) · [Filesystem transaction authority](../architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md#filesystem-transaction-authority)

### Sustained Dedicated Server Operation

Implementation: `23127bd`.

Run the published Two-Level Server with `--serve` and an optional explicit checkpoint path.
Keep one original `HeadlessSession`, the authored fixed timestep, and the ordinary processed Scene
and storage owners. Pace full frames by monotonic elapsed time and let Runtime settle fixed steps
and debt. The wake interval can be shorter than the fixed step when required by the frame clamp.
The executable reports a terminal error if elapsed time crosses that clamp. Startup and shutdown
waits remain bounded. Idle input leaves the Runtime advancing.

Use a local standard-input management stream for state observation, game commands, checkpoint
Save/Restore, and an ordered shutdown. A finite reader hands messages to the Runtime thread.
The game allocates authoritative tick and source sequence through its existing external command
path. Report queue admission separately from applied ticks and observed gameplay state. Shutdown
and EOF close ingress, settle already admitted work and let the original Host retire the live
session. A separate bounded writer keeps pipe backpressure away from fixed execution and Host
retirement. The server reports gameplay errors and results through its structured observations.
The existing explicit verification invocations keep their finite drivers.

Qualify the relocated executable through the ordinary Cooker and original process owner on
Windows and Linux. Observe tick advances without input, refuse a malformed command, admit live
commands with their original identities, run travel and checkpoint publication, start a fresh
process for Restore, and close through both an explicit request and EOF. Confirm that the runtime
data and executable selection continue to exclude client and tooling dependencies. Transport and
replication retain the scope in [ADR 0028](../architecture/adr/0028-networking-and-replication-scope.md).

Bevy's pinned `ScheduleRunnerPlugin` keeps wait policy outside schedule execution and deducts the
work already spent in the interval. Godot's pinned main loop computes elapsed frame time and
lets its physics loop consume fixed steps. The Two-Level executable owns wake and input policy
while Nara keeps its existing fixed-time and close authorities.
[Bevy schedule runner](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_app/src/schedule_runner.rs) · [Godot main loop](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/main/main.cpp)

The Windows Rust 1.95.0 release run passes the production Cooker and relocated Server test
in `598292b4-8649-4dff-9936-b4a59a749384`. The locked Windows graph still has 142 packages and
the cooked pack still contains five Scenes and zero non-Scene members. With input held open
and idle, the served World advances from tick 1 to 10 and the explorer stays at the authored
anchor. The test opens both rooms, collects the three expected pickup identities over Meadow
and two Cavern instances, and saves exact marker positions. A fresh process restores the
score of 30, the selected Cavern instance and its marker. Saving again preserves the original
630 checkpoint bytes. The two ordinary exits acknowledge 17 and 4 commands respectively,
each with zero retained commands and a joined input reader. The original native process owner
reports ordinary exit, empty containment and drained output.

The same test refuses malformed complete requests during play and invalid checkpoint startup
before Ready. Oversized input and incomplete input at EOF settle the previously admitted
marker before closing with an error. The finite travel, collision, region, checkpoint and
wrong-package controls still use their original admission and cleanup assertions. The separate
Windows binary run passes 13/13 argument, input and clock cases
(`139bf400-ea12-47c8-80ff-407bf519139e`).
[Sustained process trajectory](../../samples/two-level-game/tests/support/server_delivery/service.rs)

The first Linux release run passes the relocated Server and headless Player 2/2
(`a9570b46-6c32-43ab-988b-ceca5d45bc71`). Two additional probes of that published Server expose
failures. Dropping the stdout reader and sending a single observation leaves the process running
with stdin open. Submitting a missing-file Restore followed by Marker at tick 2 clears the active
error before observation. The revised loop detects a failed output writer during its regular wake.
The game observes the original failed external key separately from the current active error. The
settlement loop also checks its deadline after work returns. These waits are cooperative around
original Runtime calls and file operations.

The revised Windows release run passes both relocated placements 2/2
(`0a4b0a21-053e-4efd-9563-6aea052e796e`). The exact same-tick regression observes the successful
Marker and the failed Restore's original tick, source and sequence in the same result, and
preserves that evidence through original process cleanup. The ordinary Restore path still
resaves its own 629-byte checkpoint byte-for-byte. The different checkpoint sizes across independent
runs reflect actual player positions after elapsed input intervals.

The final selected Windows release run passes 196/196 across Server arguments, clock and input,
the sample library, ordinary gameplay, collision, pickups and auxiliary regions
(`b4a842c0-9146-467c-80b7-880fb45308f8`). The revised Linux release run also passes both relocated
placements 2/2 (`1eab5d19-76cd-41e8-b1e3-109f6a3a6b38`). The Linux Server retains 143 locked
packages and the same five Scenes. It restores the same selected Cavern instance, preserves all
three pickup and marker identities, and resaves its own 631-byte checkpoint byte-for-byte. The
original input reader joins and the native process owner reports normal cleanup on both orderly
stop paths.

The ordinary Linux Cooker CLI publishes a second final Server installation for process probes.
With the stdout reader closed and stdin still open, a single `observe` now exits with status 1.
A 4 KiB stdout pipe filled by observation replies triggers output backpressure. Restoring the
reader after 250 ms delivers the terminal error record with complete original Runtime close.
Keeping the pipe full instead returns status 1 after about two seconds. That process cannot
deliver a terminal record to its blocked reader. Suspending the owned process for 400 ms also
reports a 250 ms frame clamp violation and complete original Runtime close.

In an isolated WSL2 mount and user namespace with only the installation and its ELF libraries,
the Server advances 3,249 fixed ticks across 64.988 seconds of idle simulation. It accepts zero
gameplay commands during that interval. The final explicit shutdown reports original Runtime
close, a joined input reader and normal exit. This is a sustained local execution check. Service
load, longer soaks, standalone Linux hosts and networking have separate scope.

Windows correctness Clippy passes for the selected generated Server binary and the affected
library and consumer test targets. Root and sample formatting and changed document links pass.

### Packaged Server Continuation

Implementation: `c488d72`. Process acceptance: `196f003`.

The installed Server supports a local handoff at a completed paused frame. The operator can
capture the game's logical state, fixed-time phase and supported pending commands, then start
a fresh candidate from the compatible package. The committed Scene identity, content catalog,
Schema and App configuration govern admission. Ready follows reconstruction and canonical
recapture. The caller holds the original process and chooses the command route after Ready.
The original Host drives both processes through their ordinary retirement paths.
The selected release Player and Server relocation cases pass on Windows and on Ubuntu 26.04
WSL2. Each Server comparison uses two processes from one compatible package on its own platform.
[Continuation contract and process evidence](2026-11-13-rust-state-reconstruction-plan.md#processed-server-continuation-successor)

The [live regions successor](2026-11-13-rust-state-reconstruction-plan.md#processed-continuation-with-live-regions)
at `b73a45d` passes packaged execution on Windows x64 and Linux WSL2. The candidate reconstructs
both rooms and their relic state from the packaged Scenes. The test hands off a paused session
with pending movement, exercises independent room closure and ordinary travel, and observes a
candidate's refusal when its selected level cannot supply those rooms.

The checkpoint handoff successor extends this same package workflow with a version-three local
SaveSlot commitment. Its Windows production case and Linux ext4 production case pass 1/1 each.
The candidate stays paused with storage pending until the original writer retires and an explicit
claim revalidates the marker and checkpoint object. The existing checkpoint format and ordinary
SaveSlot owner remain separate from this transient continuation record.

## Shared Compiled Project Cook Entry

Status: implemented and qualified for the scoped Windows Rust 1.95.0 workflows below, after
`ebc50da` / `f0b21b6`. One public compiled entry serves Two-Level and a newly generated independent
game. Source-carrying Export remains separate; full R7/R12/Foundation stays open.

Consume the original source loader's sequential delivery snapshots before they retire. Preserve
ordinary startup attestation, strict Deferred coverage, actual declared travel roots, canonical
source identity, Prefab provenance and the existing schema/codec composition. Encode the actual
compiled Scenes and admitted Input/Image data; do not create a game-side database, Prefab reader
or second Scene compiler. Keep incremental source generations bounded instead of retaining every
decoded root. The original builder remains the sole processed assembly/deep-validation owner.

Keep game-specific typed source-build contributions explicit. Source-only import does not prove
a separate build facet's output, optional/build-only inputs or compound Product membership.
Preserve original verified groups and their observations when a game selects that build facet.
Carry actual source facts, optional-input presence, code inventory and native cleanup through
the existing Cargo and whole-directory publication checkpoints. Do not infer Cargo dependencies
or validate the full project again whenever one member is added.

Remove Two-Level's duplicate source preparation and fixed member-count checks from the shared
entry. Put sample-specific facts in qualification. Generate usable Cook/Player source for the
starter, retaining game-owned recipes and explicit package/bin/profile/target selection. The
complete external consumer starts from a real generated project, adds authored Scene/Image data
and Rust behavior, cooks through the same public entry, relocates the installation, removes
source/cache/tool access and actually travels to its third Scene while checking behavior and
Image pixels. Preserve failed preparation, selected-schema, changed Input, source drift,
compound-domain, deferred corruption and original physical cleanup controls.

### Shared Cook Implementation And Qualification

Implementation: `c3698d3` (shared entry, generated starter and public consumer), `c2d09ca`
(Two-Level migration) and `abc0a95` (existing borrowed-value integration correction).
Qualification date: November 19, 2026. Native results below use Windows and Rust 1.95.0.

The original `ProjectContentLoader` now privately lends each strict delivery snapshot before
retiring it. Startup-only initialization does not emit incomplete Deferred output. The common
compiled entry consumes the bound Scene candidates and original resolver, plus admitted stock
Image pixels, Input definitions and Material/TileSet values. It derives declared travel roots
from the selected source manifest. The existing builder remains the only encoder/catalog and
complete processed validation owner. Source-domain build contributions retain their separate
`SourceAssetBuild` execution and original artifact-group observations, including absent inputs.

Two-Level removes its duplicate database, Scene/Prefab compilation, fixed source fence and
per-game Image/Material/TileSet preparation. Its existing Meadow/Cavern processed Product IDs
are now authored in source sidecars; ProjectLocation source provenance is unchanged. The starter
publishes twelve source members, including its project-owned build script, Cook and Player.
The stock Image codec is explicitly selected in both the actual template recipe and its internal
creation contract. Source Export retains its two original source products, while its code
inventory now includes the actual finite Rust tree and optional build script.

The first owner run passes 50/52; the two new code-set controls initially expect the wrong
refusal variants. Diagnostic assertions expose `MissingDeclaredInput` versus the earlier file-cap
check. The inventory now classifies an undeclared file through the original Cargo contract before
the size guard, and the controls preserve their original fail-closed behavior. The next public
cohort passes 54/54 in `9f9a525c-1244-4440-b411-9803ebd10e92`, covering source delivery, generation
release, project creation, original cooked assembly and source/code selection. Further typed-build,
real generated-project and Two-Level execution results follow after their actual runs.

Independent review finds and fixes consumer metadata that was allocated outside the builder's
work/retained accounting. Incremental Raw facts now merge before copying; conflicting digest or
presence rejects. Map/table reservations and temporary Input-sidecar paths are admitted before
allocation and included in the original deep-validation overlap. This is a build-boundary logical
policy, without allocator/RSS or runtime-speed claims. Input encoding borrows its original compiled
`ActionMapAsset` instead of cloning a deliberately move-only definition. The first later build
also catches a missing fixture `Default` and a stale sample-test fence reference; both are corrected
before execution. At that checkpoint the complete consumer and package trajectories still needed
their actual execution; their final results follow below.

The expanded owning/public cohort passes 86/86, with 924 unrelated or ignored cases excluded, in
`1a182a9c-1393-4c3d-abcc-b3667fd2ed09`. The shared compound fixture actually runs its distinct build
facet once across two roots and decodes both primary/auxiliary output mappings. Build-only optional
input changes and presence changes reject the retained source selection. Mid-traversal source
mutation, contribution panic, generation release and exact/one-short consumer work/retention
controls pass through the production entry. The original delivery/Scene/creation and processed
assembly regressions pass in the same cohort.

The first actual generated-project run reaches creation and exposes an invalid test identity
comparison across capability sessions. Its directory oracle now uses `same_object` on the actual
creation receipt and reopened root. In that initial two-case run, the separate live-child timeout
control passes: the original owner remains present after `finish` unwinds, then supplies the actual
termination, empty containment, stopped readers and drained-output receipt. It deliberately does
not describe forced test termination as graceful game shutdown. Testing review also adds an actual
Cook refusal for a valid but divergent selected Schema before refreshing the bundle. Missing and
corrupt selected files retain separate controls; a valid default-path decoy does not authorize
fallback. The final complete external-game result follows its actual execution.

The generated-project trajectory subsequently passes 1/1 in
`51c08aa9-bfaf-4def-b5f5-48fd1dd2afcc` (361.531 seconds, including actual nested compilation).
The real creation owner publishes the starter without changing its source universe or startup
metadata identity. The game adds a native persistent score component, three declared Scenes and
three distinct Images, refreshes its selected Schema through its own compiled tool, and cooks
through the same public entry as Two-Level. Valid-but-divergent, missing and corrupt selected
Schema cases all refuse without publishing an installation.

The complete three-file installation is relocated, and generated source, import cache and
captured tools are removed before execution. The sealed Player runs from an unrelated empty CWD,
uses no source database, performs two original Scene request/commit transitions, and verifies all
three authored component values and actual acquired Image Products/extents/pixels. Its independent
calculation finishes at `score=1108`. The original processes finish with empty containment, joined
readers and drained output without termination intervention. The exceptional timeout control
retains its separate, explicitly terminated receipt. Windows CI now selects these two public tests;
that configuration is local and no remote CI execution is claimed.

The migrated Two-Level Cook/source/package controls pass 38/38, with six native or nested-build
cases excluded, in `0a17d25a-1446-427b-9d77-2b6f04c2eff9`. The preceding 36/38 run exposed two
test-oracle errors: shared source merging correctly rejects a changed build-only input earlier,
and file identities from independently opened capability sessions cannot be compared directly.
The corrected controls assert the exact changed source and the original selection's unequal
expected/current identity. Source drift still rejects, while a fresh preparation succeeds.
No production change was needed for those two assertions.

All three real Two-Level packages then pass in
`ba06888f-72c2-447e-934c-900bbf5da341` (458.037 seconds), with seven unrelated cases excluded.
The same production cooker prepares headless, desktop and audible Players through the shared
entry. Their original source/cache removal, installation relocation, ordinary no-input Player
refusal despite old `OUT_DIR`, saved-state Restore and damaged-content controls remain intact.
The physical desktop trajectory retains hardware DX12 gameplay, fresh Save/Restore and repeated
travel plus its explicit software presentation control. The audible trajectory retains actual
WASAPI callback/PCM checks, physical input and original native teardown. Passing process receipts
are still required; neither these package results nor the nested compilation times establish
runtime performance or foreign-platform execution.

The ordinary generated-project all-target/all-feature compilation passes 1/1 in
`d1e2c4b3-df6a-4f68-8ee0-d80d645c95a5` (115.142 seconds), including the library, build script and
all four binaries. The original custom-schema/external-domain Check/Run/Repair/Export trajectory
passes 1/1 in `426afcb3-f2b6-40c4-8a2f-5d6fcf7d1a7f` (1034.354 seconds, including multiple real
nested builds). It preserves both original executable roles, compiled domain auxiliary content,
post-export gameplay and missing-input refusal. The source Export adapter now captures the
project's actual finite Rust tree and optional build script instead of a fixed five-file list.
Neither duration is a runtime performance measurement.

The final root all-target correctness Clippy gate first exposes eight stale uses in the existing
`product_data_components` integration: owned-reference comparisons, direct List matching and two
native decode closures. The fixture now reads the existing borrowed scalar/List views directly
and materializes owned data only for its two owning native values. Its ten original workflows
pass in `7ebd42d3-86ff-4958-877d-30d118bcfbbb`; production reflection code is unchanged. Root
all-target and selected release sample correctness Clippy then pass. Advisory warnings remain,
including build-boundary error-size and test-helper style suggestions; this is not a warning-free
workspace claim.

The changed Editor application-contract target compiles with its normal feature closure; this
only migrates the exhaustive starter-member consumer and does not resume Editor UI work. Root,
sample and explicit starter-template format checks pass. Added document links/fragments,
sidecar JSON, manifests/locks, workflow YAML and whitespace checks pass. Root/sample dependency
locks and versions are unchanged. All eleven scoped implementation/review agents are closed;
their concrete correctness and testing findings are fixed. The fifteen paused UI/Dialogue files
still match their original SHA-256 manifest and remain outside these commits. CI changes are
configured locally; no remote execution or publication is claimed.

Complete source observations remain a trusted-local per-file envelope, not an atomic/hermetic
snapshot or hostile-writer defense. The generated Player is headless; Two-Level separately
qualifies desktop and audible placements. General cross-compilation, foreign execution, shared
native-library deployment, complete shipping schema stripping and numeric runtime budgets
remain open. This closes the shared Cook unit, not full R7/R12/Foundation.

## Project Cook Runtime Dependency Selection

The [Cook review](../audits/2026-11-20-runtime-startup-cook-and-presentation-review.md#cook-forces-a-source-image-into-a-pack-after-the-importer-has-baked-it-into-another-product)
found a source importer whose build embeds an observed PNG in another Product. The shared Cook
entry now chooses stock payloads using the declared Scene and global Input roots and the Required
and Reference edges of actual cooked Scenes and supplied groups. The builder still validates
complete members and rejects surplus explicit groups. A stock input absent from the Runtime
closure stays in the original source and build observations used by the cache and publication.

The normal traversal encodes only selected stock payloads while each strict snapshot is borrowed.
Its initial implementation could revisit the source traversal once when a demand appeared after
an earlier snapshot retired. The first receipt released its source budget before that traversal.
The new receipt had to match both the original delivery digest and startup stamp. The Image
successor below uses direct acquisition. Other stock payloads continue to use the bounded replay.

The [repair evidence](../audits/2026-11-20-runtime-startup-cook-and-presentation-review.md#runtime-members-and-build-observations)
records the independently locked consumer with an embedded PNG, a linked Image, stale selection
and actual rebuild, and a second Scene adding a dependency on an earlier snapshot's Image.
The three new cases pass within the 11-test Windows consumer run. The engine's boot, delivery
and Cook cohort passes 64/64, and the Two-Level Game's Cook targets pass 37/37 with their selected
source and source-free payloads.

## Build-Declared Image Dependencies And Sprite Admission

Real artifact builds can introduce a required Image Product that no authored Scene selected.
After the original delivery traversal and explicit build contributions, satisfy that demand through
the Project Content owner and stock Image importer. Resolve the selected Product in the catalogue
and reuse the delivery receipt's file facts for an Image already observed. Build the bounded
catalogue only when needed. Read actual PNG and typed sidecar data through the admitted assets
capability, checking the original digest where available. New stock paths extend the deduplicated
delivery file count before reading. The original Cook fence retains their observations. Release each
Image's decoded pixels after encoding. A missing identity, an unsupported auxiliary ID or a Soft
edge provides no stock payload. Earlier non-Image demands continue to use the existing bounded
replay and source revalidation.

Deep delivery admission also joins the Sprite codec's selected Image binding to the executable's
native Image publication role. The same bytes decoded through a CPU-only codec remain eligible
for ordinary typed consumption. A package containing a Sprite that needs those bytes must select
the publication role that its Runtime applies. Reuse the Sprite field contract and the original
deep validation pass at the delivery boundary.

Run the independently built badge consumer for embedded, source-linked, single-scene build-linked
and cross-root build-linked cases. Verify exact runtime bytes, relocated execution with the original
source path unavailable, changed PNG and changed sidecar, one contribution per Scene and rejection
of unsupported/missing identities. Exercise the ordinary Sprite against both codec registrations
and confirm untextured Sprite and direct typed Image behavior. Godot walks dependencies from its
actual export roots. Bevy distinguishes recursive dependency state from the single decoded value.
Nara keeps that distinction through its own artifact, source and Runtime publication owners.
[Godot](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/editor/export/editor_export_platform.cpp#L674-L691)
[Bevy](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_asset/src/server/mod.rs#L1082-L1125)
[Fifth review](../audits/2026-11-20-cooked-dependencies-prefab-revert-and-input-review.md#p1--cook-cannot-resolve-an-image-first-requested-by-a-built-product)

### Build-declared Image and Sprite qualification

Commits `6be6521` and `c696d1f` implement Build-declared Image acquisition and Sprite admission.
The single-Scene case first reproduced `MissingDependencyProduct` in Windows x64 Rust 1.95.0
release nextest `750b6bba-0f88-4d5b-aae4-8c8a08bbd663`. A second red run,
`548bec69-8302-46a0-9814-86b8a366fffb`, reproduced two findings from implementation review:
path-only reacquisition visited an unrelated invalid sidecar, and extra stock files escaped the
combined file ceiling. The mixed-order control then reproduced a duplicate catalogue identity in
`18040f20-aa7b-4d10-87a9-b85bd8e1ec5e`. Exact supplemental reads now leave discovery state alone.
The final review checked both sort orders, bound source facts and the file-count cutoff.

The final independent asset-domain consumer passes 14/14 in
`74721f7d-d097-4601-9777-b20e2375607b`. It exercises a relocated single-Scene pack after the original
project path is removed, exact decoded pixels, embedded and source-linked controls, a stale build
observation, a distinct PNG selected from a cached build, and the exact file limit plus refusal one
file below. The cross-Scene controls verify one source import, one contribution per root and
identical success with both Image orders. The same run covers compound Scene travel and processed
Headless custody.

The affected engine release cohort passes 147/147 in `44a9343b-c69a-443c-ba6e-86accc5606d1`.
It covers source and Runtime owners, public boot and delivery, and the same Sprite fixture with
generic versus publication Image codec roles. The positive case reopens the pack and starts a
Headless Player, then checks the actual published Image. Null and omitted Sprite references and
direct typed acquisition from the generic codec also pass.

The Two-Level Game's ordinary Cook, source selection and pack corruption targets pass 47/47 in
`0a441787-6fbe-4d58-a7dd-8416400d7cc4` with `cook,desktop,audio-cpal` selected.
The production source-free Headless delivery passes 1/1 in
`f2a442db-c25f-45f6-b857-6cd7db49e034`. It runs actual Windows Cargo Player compilation, package
publication, source and cache removal, relocation, travel, Save/Restore, corruption refusal and
contained process cleanup. GPU presentation and additional platforms are outside this run.
Scoped engine correctness Clippy, strict Clippy on the independent Cook test and a
`project-workflow` library build without `project-tooling` pass.

### Selected Image Identity And UI Image Admission

The [sixth workflow review](../audits/2026-11-20-cook-identity-spatial-inheritance-and-round-input-review.md)
found two gaps between a prepared package and its Player. An Image already seen by path could
satisfy a later Product demand despite a second sidecar claiming the same UUID. A Scene could also
declare a `UiVisual` image brush against an Image codec that only supports CPU acquisition.

Use the original bounded catalogue to admit stock Image Products even when their pixels are
already available. With no complete catalogue from source delivery, discover only the needed
UUIDs in one pass through the configured roots. A prepared selection retains the Product's
canonical path and rechecks its unique ownership at the original publication fence. Preserve
path-only embedding, the original retained file and build observations, and the exact delivery
file allowance. Exercise both leading path choices and a new duplicate after preparation, plus
successful verification after repair and with an unrelated UUID.

The `UiVisual` codec owns its persisted background Image field. Deep validation uses that field
to require the executable's native Image publication role, alongside the existing Sprite check.
Use the same compiled recipe, PNG and codec for the generic-versus-published controls. The
publication-capable package must reopen and start a Headless Player and expose the expected
pixels. Solid UI and direct typed CPU decoding retain their original behavior.

The independent Cook target passes 10/10 within release run
`09f13890-544b-4e54-8917-90fe5b2b863d`. Both earlier path choices refuse the same duplicate UUID.
Late insertion, late edits and a claim from another configured root also refuse. Repair and
unrelated or malformed unselected metadata preserve delivery, as does the raw-only Player.
The exact file ceiling covers late Image acquisition separately from catalogue enumeration.
The owning engine and public-content release cohort passes 245/245 in
`e1bb447a-40eb-4f1a-acac-6a3a7ce7de7b`. It verifies the new UI and existing Sprite publication
checks, file and scan digest agreement, finite scan work, metadata preparation and Runtime boot.

## Build-Declared TileSet And Material Dependencies

A verified Product can declare a required TileSet or Material before any Scene loads that
resource. Acquire the selected primary Product through the Project Content owner using its unique
scoped sidecar and the existing source importer. Encode the TileSet or Material through
`CookedContentBuilder` and use its encoded dependency declarations to admit the actual Image
products. Shader files read by the Material importer contribute exact source observations. The
finished package retains the existing `Required` and `Reference` roles, independent of whether
the dependency appears in an authored Scene. The Scene root and travel catalogue stay selected
by the original delivery traversal.

Reuse the captured complete catalogue when available. Without one, a filtered pass resolves the
initial selected stock identities and a second filtered pass can resolve newly exposed Image
identities. Each additional exact read counts against the distinct source-file limit before
opening unless its path is already admitted. Repeated reads compare the original digest and reuse
the retained closure entry. Already-supplied texture Products still have their source association
verified. Retain source, sidecar and Shader commitments through the original publication fence.
The source owner borrows the decoded definition for encoding and releases it
before another acquisition. This replaces the stock-resource replay of earlier Scene snapshots.
Earlier dependency-only Scene payloads keep their existing bounded replay. Explicit game-owned
groups continue to supply other kinds and auxiliary products.

Godot's export walks the dependency index from selected resources and autoloads. Its imported
resource path asks the destination format reader for dependencies of the imported data. Bevy's
`ProcessContext` retains inputs used for processing, and `LoadContext::finish` walks dependencies
of the loaded value. Nara applies the same ownership distinction at its existing source, build
and Runtime catalog interfaces. [Godot export](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/editor/export/editor_export_platform.cpp#L1305-L1350) · [Godot imported resources](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/core/io/resource_importer.cpp#L461-L469) · [Bevy process observations](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_asset/src/processor/process.rs#L429-L453) · [Bevy loaded dependencies](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_asset/src/loader.rs#L578-L601)

Qualify both resource types through an independently locked, single-Scene consumer. Before
Cook, strict source delivery should omit the asset. Assert the verified artifact's exact edge,
successful source selection, the complete roles and member set, and rejection after a selected
source change. Relocate the pack away from the authoring project and verify the actual TileSet,
Material shader and pixels through the processed Runtime owner. Keep missing and conflicting
source selection failures distinct from an admitted pack. The workflow requires no new build
scheduler, asset loader or package format.

### Stock Dependency Qualification

The independent Windows release run `00dd6965-09ff-4341-a5f6-09c70eadb888` passes 23/23. The
new single-Scene tests cover TileSet and Material through both `Required` and `Reference`
dependencies, filtered and complete catalogue acquisition, source drift, and pack relocation
with the original project path absent. The startup observer checks actual publication for the
required resource. Explicit acquisition checks both roles against the original atlas, decoded
RGBA pixels and full compiled Shader source. Missing referenced TileSet, a texture path claiming
its Badge group's ID, and a file ceiling one below the combined distinct paths are rejected.
Twelve Material definitions sharing one WGSL and Image produce the same catalog under the exact
combined file ceiling.

The owning Project Content, Cook and processed-content cohort passes 160/160 in
`e939230c-a4ff-4587-9a97-22707b12388f`. Repeating a shared file observation retains one entry
charge and a changed digest still refuses. Original content-budget, Image acquisition and
file-backed TileSet controls also pass. Independent review identified the pre-existing-output
identity bypass, duplicate file and closure-index charges, and redundant catalogue scans within
the first implementation. The final source ownership check, exact read admission and filtered
discovery close those findings. The comparison and tests cover the finite stock dependencies
supported here. General build scheduling, server stripping and foreign execution remain open.

Two-Level Cook/source regression passes 43/43 in `2992ba4a-83c1-445b-b703-ff23f3a58ea3`.
Its production relocated desktop acceptance passes 1/1 in
`d277010d-eda0-4292-a33c-633f964b2bdd`. The Windows x64 Rust 1.95.0 run builds the actual
Player, removes access to source and cache, presents through DX12, exercises native keyboard
play, Save and fresh Restore, and retains original process/window retirement. This is adjacent
native coverage of the shared Cook entry. The independent build-declared resource cases use
typed CPU acquisition and Headless Runtime publication.

The `project-workflow` library build without tooling, scoped correctness Clippy, and root and
independent consumer formatting checks pass.

## Request-Owned Generated Rust Input Successor

Remove the shared-source bootstrap write used by Two-Level and the independent delivery fixtures.
The original Cargo request owns one bounded immutable generated Rust source and its canonical
logical name. The original command supplies its reserved environment value; requests without
that input explicitly remove ambient values. The trusted build script declares
`rerun-if-env-changed`, writes only Cargo's `OUT_DIR`, and enables its cooked branch only for a
selected input. Stale generated files in source or `OUT_DIR` do not activate an ordinary Player.

Keep ordinary source-file observations separate from the request-generated name/digest in
provenance. Package binding requires the actual injected generated fact, removing acceptance of
a shared source-file observation as bootstrap authority. Preserve the original content/code
fences, exact executable capture, target-directory lease, native cleanup and whole-directory
publication. This is one concrete generated-input slot, not a generic environment API, source
copier, Cargo dependency resolver or additional process owner. Record bounded parent work and
encoded input bytes through the existing Cargo owner; child/ambient work keeps its existing scope.

Prepare distinct A/B expectations before either native build, hold both immutable requests and
build serially in the same workspace/target. Verify the actual captured code and generated facts,
including rerun after a changed selection, no-input refusal with stale Cargo outputs, cross-bind
and installed cross-pack rejection. Relocate and execute a real source-free Player. Migrate the
three Two-Level runtime selections and independent renamed-engine consumers without restoring a
source-write fallback. Keep this prerequisite distinct from the later shared starter/Two-Level
cooked entry and its new external-game acceptance.

### Generated Input Implementation And Qualification

Implementation: `ebc50da`, qualified on November 18, 2026 for the Windows Rust 1.95.0 workflows
recorded below. The shared starter/external-game cooked entry remains the next delivery consumer.

`CargoBuildRequest::with_generated_rust_input` selects one canonical logical name and owns an
exact NUL-free UTF-8 source copy. The reserved invocation environment is removed for ordinary
requests and set from that owned copy for generated requests. No global environment is modified.
The 16 KiB transport ceiling is separate from total input bytes, input count and parent work
allowances. Current Player bootstrap constants contain only the bounded catalog digest/length,
target/profile and fixed pack name; their existing encoded representation is preserved.

The source has no temporary-file or dependency-routing owner. Cargo retains the existing target
lease and process custody. Build scripts use `rerun-if-env-changed` and write only `OUT_DIR`;
the three Two-Level wrappers include the resulting module and retain explicit ordinary-build refusal.
`GeneratedPlayerBootstrap::name` replaces its old physical-path API. Captured provenance and
the unpublished canonical v1 manifest record a separate generated fact and
`generated_input_name`; source-file matching is removed from bootstrap binding. Metadata digest
still represents actual declaration-only Cargo metadata. Trusted Cargo configuration must not
define or override `NARA_CARGO_GENERATED_RUST_SOURCE`; this remains a trusted compiler/build-script
contract rather than Rust inclusion, hermetic-build or hostile-config proof.

The owning Cargo/package tests pass 17/17, with 946 unrelated cases excluded, in
`378cb41e-0727-4e7b-9a05-57fa06df8d77`. Controls cover request byte independence, no-source-file
effects, command-local environment selection/removal, single-slot/name/transport rejection,
exact and one-short budgets, distinct generated-versus-file provenance and actual binder refusal
when only a matching ordinary file fact exists. Original target lease and failed-child cleanup
controls remain intact. The combined 600-file/16 KiB source case admits the full overlap before
file-fact allocation and rejects one short at that same preallocation point.

Independent review found and corrected the initially late combined-work check, a test-only empty
diagnostics construction error, three tuple-versus-object digest assertions and missing actual
Two-Level stale-output refusal coverage. The first owner run also exposed an invalid test tool
choice: nextest's multiply linked executable is correctly rejected by the original tool identity
owner, so the fixture now discovers the installed toolchain normally. These corrections do not
relax native build, source/current-content or cleanup contracts. Final independent/native
execution evidence follows below.

Independent real delivery passes 2/2 in `344cedaa-d039-41c4-a5e7-f0c3d0dac834`. The conventional
renamed-engine workspace prepares A/B requests before either build, then builds A/B/A with one
selection, workspace and shared target. The captured source-file facts stay equal while generated
facts and actual B code/content differ. Three original captures are published, relocated and run
after source/cache/tool removal. A and B each reject the other's installed pack; a subsequent
ordinary request refuses despite stale Cargo-generated output. The independent Palette cooker
and source-free Material-only Image consumer also pass through the same generated-input slot.

The initial two-consumer run `ecfc1fd0-ccbb-40ae-841c-0789207a8246` passes the Material consumer
and reaches cross-pack refusal in the A/B/A path. Its newly added assertion incorrectly expects
a catalog-digest error. The two profile strings produce different encoded catalog lengths, so
the original reader correctly rejects the pack header first. The fixed negative control checks
that actual length difference and requires exact `InvalidPackHeader` refusal in both directions.
All original valid gameplay, Deferred corruption and native cleanup controls remain intact.

The strengthened A/B/A consumer passes 1/1 in `d297efc8-4842-4dd1-ac78-2ba4f5682dc6` with a
parent-provided `compile_error!` value in the reserved environment. The original request replaces
that value for generated builds and clears it for the ordinary no-input build. This is actual
command/`OUT_DIR` behavior, beyond inspection of a constructed `Command`; Cargo configuration
override remains outside this trusted-environment guarantee. The separate Palette result above
does not need another execution cycle after this outer-test-only strengthening.

Two-Level's affected release cohort passes 36/36, with six native cases excluded, in
`eae2a5c4-a77d-4869-aa63-3e0c572d65e7`. It compiles all changed package-test helpers and covers
source agreement, actual profile preparation, content/code fences, wrong inputs and bounded pack
corruption. The existing headless/desktop/audio real package trajectories are qualified separately
after execution; this CPU result does not establish their native behavior.

The first three-Player native run, `9c27ff55-790e-4347-9fe6-32fc20cf87ea`, reaches actual audio
execution after its release build but fails the original exact-foreground-window guard during
physical input. The observed foreground belongs to another process. Keep that result as a failed
native prerequisite, not an audio or generated-input success; preserve the original assertion for
the separate rerun. Its desktop case later also fails the same original foreground guard after
multiple physical-input steps; it is not a successful desktop qualification.
The same run's headless package case passes, including the actual no-input
rebuild and refusal with old `OUT_DIR` bytes still present, followed by normal relocated gameplay,
checkpoint/fresh Restore and damaged-content controls. Later isolated results follow below.

The isolated physical rerun, `ad6f2e88-ebfb-4471-9722-11718a53291c`, fails both cases. Desktop again
loses the exact foreground to another process. Audio instead reaches its 30-second script bound
at the first Walk, with no held movement, one unsolicited travel and one Retry. The retained
diagnostic does not establish the event that changed those facts. Read-only inspection confirms
the observed foreign foreground PIDs belong to Edge and Explorer; the fixture does not restore
foreground before its original Player finishes. The first run reaches audio's later physical
test only after its complete scripted callback/PCM trajectory succeeds. These facts separate the
two failed native attempts; they do not turn either aggregate into a passing qualification.
Replay the retained source-free sealed worker before paying for another full Cook/build cycle,
and preserve its original foreground, gameplay, PCM and physical-cleanup assertions.

The retained audio sealed worker then passes 1/1 in 5.64 seconds without another Cargo invocation
or changed Player bytes. Its original script admits seven effects (six complete, one cancelled)
and verifies the captured PCM. A second original process performs physical D/E input and owned
window close, admitting two effects, and both prepublication corruption/mismatch controls reject.
Every original child records empty containment, stopped readers, drained output, destroyed window
and no termination intervention. The source/cache remain unavailable. This replay qualifies that
captured worker trajectory; the subsequent complete Cook/package rerun is recorded separately
below. No production input cancellation is weakened or bypassed.

Source inspection explains the stalled script's possible interruption: ordinary input is active
after initial reconciliation, and Travel or Retry correctly clears game-owned held movement. The
finite verifier sends Walk once and retains that dispatched state. An outside transition can
therefore leave it waiting for a pickup until its original diagnostic bound. The captured failure
does not identify the sender or exact command order. An idle desktop remains a prerequisite of
this live-input script; the observation does not justify changing gameplay cancellation or
silently reissuing a command to make the assertion pass.

The complete desktop package trajectory passes 1/1 in
`8fcf7748-25f7-4f10-9521-a7dfe1d908e6` (97.130 seconds). It recooks and publishes the actual
Player, relocates the complete installation and removes source/cache before the sealed worker.
Both hardware physical-keyboard sessions, authored region/material/TileSet and collision
controls, Save/fresh Restore, twenty further travels, software-DX12 first presentation, invalid
initial save refusal and all three finite scripted controls retain their original assertions.
All seven original windows/processes and pumps retire without termination intervention. This
passing rerun supersedes the desktop foreground failures for the current implementation; it
does not erase those failed-attempt diagnostics or qualify other platforms/performance.

The complete audio package trajectory subsequently passes 1/1 in
`f1ac0e74-023a-41b3-a446-e3c05d617c72` (84.516 seconds), including a fresh production Cook,
three-file publication/relocation and source/cache removal before the sealed worker. The actual
hardware-DX12/WASAPI script, full callback/PCM oracle, physical keyboard/owned-close process and
both invalid-package controls all pass through their original native owners. This is the full
outer delivery test, separately from the earlier retained-worker replay. Together with the
headless and desktop results above, it closes the selected three-Player migration on Windows
Rust 1.95.0. Physical human audibility, foreign execution, numerical performance and the shared
starter/external-game cooked entry remain unqualified or open as previously recorded.

Final correctness Clippy passes for the owning production library, the changed sample library,
cooker and package-test targets, and the independent consumer's complete target set. Root,
sample and independent-consumer formatting checks pass. Added relative document links and
whitespace checks pass; the fifteen paused Editor/Dialogue integration files retain their original
bytes. These checks and the actual native results above qualify the current implementation;
documentation closure does not require another unchanged product acceptance cycle.

Cargo's documented build-script output and environment dependency rules justify this seam.
`OUT_DIR` can retain old files across invocations, so activation follows the explicit request,
never file presence. Cargo configuration can supply/override environment values, which is why
the reserved-key constraint is explicit instead of an unsupported sandbox claim.

- [Cargo build-script outputs and change detection](https://doc.rust-lang.org/cargo/reference/build-scripts.html#outputs-of-the-build-script)
- [Cargo environment configuration](https://doc.rust-lang.org/cargo/reference/config.html#env)

## Complete Consumer

An independent ordinary workspace uses the same game recipe in a selected Host cooker and a
custom-named native Player. Production owners cook and publish one relocatable directory with
that captured Player, a complete processed-content pack and a bounded delivery manifest. The
Player runs from an unrelated working directory after authoring source, metadata and build cache
are removed and Cargo/rustc are unavailable. It exercises global physical Input, two Scene travels,
a Deferred destination, shared Image/TileSet content and an auxiliary compound Product.

Keep source-carrying Export available. Target compilation, package publication, native execution
qualification, Runtime Ready and executable last-good remain distinct results. A foreign artifact
continues to be build-only until a suitable runner separately qualifies it.

## Actual Gaps And Reusable Owners

The [explicit build consumer](../../tests/fixtures/cargo-build-consumer/README.md) already captures
custom Host/Player bytes and runs seven processed Input cases. Its
[shared fixture](../../tests/fixtures/runtime-runner/renamed-root/tests/support/processed_input_fixture.rs)
still assembles the catalog and supplies expectation arguments from the test process. Replace
those responsibilities with production owners in this unit.

- `VerifiedArtifactGroup` already retains complete exact member bytes, Product mappings, formats
  and dependencies. A valid old group does not prove that it is still the current cook selection.
- `RuntimeSceneCandidate::cook` and the Input Runtime codec already own their concrete encoding.
  They do not need to become cases in a new erased universal cooker.
- `RuntimeContentCatalogDefinition`, base admission and deep validation already check target,
  configuration, recipe, Schema, codecs and members. The missing production assembler must supply
  their complete inputs rather than create another parser or loader.
- The existing v1 pack writes exact admitted content. Its writer does not own directory staging,
  sync, atomic publication or failed-cleanup custody; the package publication owner must do that.
- Existing filesystem directory publication, Scene commit/residency, native runner and Editor
  generation/Ready owners retain their responsibilities.

## Selected Implementation Direction

First reconcile the bounded package contract with Accepted ADRs 0007/0033/0103/0110. Adopt only
the consumed slice of Proposed 0088. Its older dependency wording must follow the current rule:
Required and Reference edges both impose complete delivery obligations; Immediate/Deferred demand
controls current materialization. Soft alone permits absence. Deferred destinations must not be
stripped because their current resources are not prepared.

The request explicitly names Cargo selection/tools, content configuration and target, startup and
travel roots, addressable roots, concrete cook definitions and effective settings. Cargo profile
and content profile are separate fields. The code inventory and content observations also remain
separate: bind both to this request without requiring equal path sets or claiming a complete
workspace snapshot.

Prepare the complete cooked catalog and retain its exact members before building the Player.
Consume original verified artifact bytes rather than reopening mutable CAS paths. Track current
source/metadata/auxiliary and actual processed-input facts with the existing local-build fences.
Concrete target policy must affect actual output, such as the existing CPU Image transform.
Deep validation refuses a transform that invalidates atlas or Scene-cell constraints unless a
separately selected policy preserves those constraints.

For the first consumer, generate one finite Rust bootstrap input containing the expected catalog
digest/length, content target/profile and fixed relative pack path. The Player explicitly compiles
this input from Cargo's `OUT_DIR` and uses its own compiled recipe for compatibility. Select the
generated source on the original request independently of ordinary file inputs. The package binder checks its exact digest against the current
content preparation and captured provenance. Old Player A cannot bind catalog B even when their
startup Product or display label is unchanged.

```text
verified source/groups -> cooked catalog + exact members
                      -> generated Player expectation input
                      -> captured explicit Cargo Player
                      -> exact code/content binding
                      -> complete directory publication
                      -> separate native execution qualification
```

The catalog does not include the final executable digest, avoiding a hash cycle. A final delivery
manifest/receipt records executable, catalog and pack digests, selected build facts and provenance.
The manifest describes delivery; it is not a self-authorizing trust root. The Player never derives
its expected catalog digest from the catalog being admitted. A later reusable prebuilt-Player
mode would need its own independent expectation/trust contract; it is outside this first workflow.

Use a finite layout such as `player.exe`, `content.pack`, `package.json`, with the platform's real
executable name. The Player locates the pack relative to its executable, opens the original pack
and uses the existing processed Host startup. No runtime Cargo, importer, source manifest or cache
fallback is permitted. Additional native runtime libraries, if required, are explicit finite
members; never scan `target/` to guess them.

One package owner retains executable/content/preparation bytes and publication capabilities.
Account for overlapping groups, base, validation scratch, executable, pack writing and candidate
directory work under explicit limits. Reuse existing rejected/orphaned/unresolved cleanup and
durability outcomes. Partial pack writes never become published success. Preserved pack backing
retains its existing authenticated-availability guarantee; it is not silently upgraded to the
complete captured backing's stronger availability after file deletion or mutation.

## Ordered Work

1. Reconcile the consumed Accepted contracts and compare pinned Bevy/Godot. Select finite request,
   catalog assembly, generated expectation and exact package-binding facts before public APIs.
2. Implement concrete cooked-content preparation through existing tracked build/Scene/Input
   owners and complete catalog/deep validation. Refuse unsupported facets explicitly.
3. Generate the bounded expectation input, build the selected Player through the qualified Cargo
   owner and reject code/content/input mismatches before directory publication.
4. Stage, sync and atomically publish the finite package through existing filesystem authority.
   Retain cleanup custody and separate delivery evidence from native qualification.
5. Run the independent relocatable source-free consumer, targeted failure/retention/budget cases
   and affected source Export/Runtime/native regressions. Independently review, record actual
   limits, commit coherent replacements and locally fast-forward main. Keep the full Goal active.

## Acceptance And Limits

- Two clean content builds produce equal canonical catalog/member/pack digests. No Rust executable
  byte reproducibility is required. Every captured executable retains its own exact digest.
- Relocation and unrelated CWD succeed with only declared package members and normal OS runtime
  libraries. Startup, physical Input, failed travel and successful travel use production paths.
- Before publication, deep-validate the complete candidate and reject code A/content B, stale
  groups/bootstrap input, wrong target/profile/Schema/codec, duplicate Products, missing/corrupted
  candidate members and unsupported facets. Startup rejects catalog/target/extent and demanded
  content failures before Startup effects. A later corrupt Deferred payload rejects its acquisition
  and preserves current Scene/resources. Original-file backing does not promise perpetual availability.
- Deep-validation member coverage must equal the delivered member set. Extra compound siblings
  require actual roots/dependencies or reject; opening a valid catalog cannot substitute for
  decoding every candidate payload. Verify the original staged pack against the expected exact
  pack bytes. Define malformed-producer fixtures as pre-publication rejection, and inject later
  Deferred corruption only after complete publication to exercise demand-time failure.
- Bind the canonical generated-input name and digest/length from its distinct Cargo provenance and
  compare the whole requested selection. Trusted Player inclusion/use remains explicit; no binary
  inspection or Rust include-analysis proof is introduced. Keep original Host content observations
  and selection fences distinct from code inventory, including metadata and auxiliary reads.
- Interrupt cook, Cargo, staging and publication. No partial package is success; existing package,
  Runtime Scene/resources and executable last-good remain under their original owners.
- Retain base/acquisition A while building B and deleting cache. Escaped resources and subsequent
  acquisition obey A's original backing guarantee. Test exact/one-short allowances at new overlaps.
- Preserve the seven existing processed Input process cases. Qualify desktop presentation and any
  foreign runner separately. Headless execution alone does not prove server stripping; a claimed
  server mode needs its own selected recipe, member audit and executable capability evidence.

Pinned Bevy `85503941e488152711a4695bbe248e9ed9f453fc` separates processed loading from processor
execution in [asset composition](../../repo-ref/bevy/crates/bevy_asset/src/lib.rs). Pinned Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558` separates executable template preparation from content
export in [PC export](../../repo-ref/godot/editor/export/editor_export_platform_pc.cpp), and selects
target imported variants in [platform export](../../repo-ref/godot/editor/export/editor_export_platform.cpp).
These are responsibility references, not Nara execution or platform evidence. No new resolver,
universal cooker/build graph, remote service, patch/mount system, compatibility reader or migration
guide is selected. Reuse root target and serial Cargo; preserve vendor, concurrent work and refs.
