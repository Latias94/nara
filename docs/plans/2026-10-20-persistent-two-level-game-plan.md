# Persistent Two-Level Game And Content Lifecycle

## Outcome

Prove one small Rust-authored top-down exploration game with cross-level travel, pickups and
an explicit single-slot checkpoint save. It loads a second level, returns to the first, preserves
explicit game state across a fresh process, and runs from a source-carrying export outside the
checkout. Use ordinary public product interfaces. Preserve the existing Reference Game's wave
CLI, deterministic regression output and fixed candidate layout.

The user selected this exploration consumer rather than extending the wave-combat fixture.
Use a separate `samples/two-level-game` consumer after P1's focused public delivery tests.
Checkpoint saves reconstruct transient simulation; arbitrary-tick quicksave is a separate contract.

The user authorizes implementation and fearless refactoring. This plan does not reopen completed
foundation/locality units or create a new Goal. Root owns integration and serial Cargo/fmt/Git;
research and implementation lanes have disjoint live file ownership. Preserve untracked vendor.

## Opt-In Playable Round Flow

The next Runtime consumer adds an explicit game-owned flow without changing the default
exploration recipe or the Save/Resume wire formats. Boot freezes game time before the first
frame plan, then exposes a title, new round, playing, pause/continue, completion, return to title,
and exit. The round goal is one collected pickup in each main Meadow and Cavern occurrence.
Menu feedback and keyboard controls must be usable in an ordinary desktop window; this is a
small game presentation, not resumed Editor component development.

Keep `GamePhase` as the existing Scene preparation/reconstruction status. A separate opt-in
plugin owns round flow. It uses the original `RuntimeTimeSettings.time_scale`, retaining the
previous positive scale, and never changes the Host-owned `paused` flag. Always-enabled frame
actions drive menus even when there are no fixed ticks. Non-playing phases disable gameplay
input and clear held game intent. On continue, the first resumed original fixed transaction
ignores stale ordinary gameplay actions while retaining movement terminals and the original
command capture/acknowledgement. Only its completed boundary reopens gameplay input. No queue
replacement, extra paused step, synthetic acknowledgement, or second clock is introduced.

New rounds use the original `travel::begin` and Scene preparation/commit/reconstruction path.
Associate success with the new Scene occurrence, settled original request and successful
reconstruction. Reset round statistics only after that success. Preparation/preflight refusal
retains the old game and menu; post-commit reconstruction failure remains explicitly degraded.
Starting cannot be abandoned into an unrelated menu with an outstanding request. Exit always
uses the original App/native shutdown. The additional plugin and mapping lifetime remain
unsupported by exact reconstruction instead of borrowing the old eligibility proof.

Qualify the real game through public source startup, Input, time and command owners: frozen
title/pause, actual new Scene, zero-tick command backlog, fresh-press continuation, Host pause
independence, task progress, real pickup completion, successful reset and failed-start behavior.
Run one actual Windows desktop input/presentation/cleanup trajectory. Keep controller delivery,
general state-machine APIs, cooked menu delivery and Editor UI as separate successors. The
pinned Bevy State source (`03ed5a27c92715c8fa2188f31f04e454ec39942f`) supports separate game
transition policy; its World-owned schedule storage is not Nara's App-owned schedule authority.

### Round Flow Implementation And Qualification

The ordinary launch seam lands at `c2cf3dd`; the complete game flow and acceptance consumer land
at `ffa7303`.

The game now has an opt-in `flow::with_play_flow` recipe contribution, separate `FlowSnapshot`
and private transition state. Its independently enabled `play-flow` desktop profile composes the
existing Runtime UI and presentation owners; a game-owned HUD projects phase, goals and keyboard
instructions without adding an Editor component layer. The ordinary launch owner accepts an
explicit standalone profile while inherited supervised source/profile authority still wins.
The optional UI closure adds existing accessibility/presentation packages to the sample lock;
no existing package version or source is replaced. The common authoring bundle is refreshed
through the selected profile; additional UI metadata does not activate UI on the default game.

The first release CPU cohort passes 159/159 in `ad7d45ca-8cc9-4b7b-a931-10f7f5fd4c7f`:
152 library controls and seven complete source-game trajectories. These cover title freeze before
the first planned tick, original Scene replacement while scaled time is stopped, held-source
cancellation, zero-tick M/E/movement backlog followed by original consumption/acknowledgement,
Host/game pause independence, exact Resume refusal, real pickups in both main levels, full new-round
reset, preparation refusal and source repair. The startup-content recovery control starts with an
invalid checkpoint role, returns from Degraded to a persistent Title, repairs source and confirms
a new initialized Scene with actual moving animation. Review corrected both persistent-Title and
initialization-recovery paths. Menu binding installation errors fail through original Startup
cleanup rather than displaying a menu whose controls were not installed. The initial fallible
Startup system used the wrong ECS result type; it now returns the substrate's actual `BevyError`.

The selected profile's original `schema-write` and `schema-check` both pass. All seven existing
owners are preserved byte-for-value, with one additional native UI owner. The first UI release
rebuild after local target cleanup takes 26m02s; this is an observed build, not a frame-time or
edit-to-result budget.

Actual Windows DX12 acceptance passes 1/1 in `910aab4f-2975-4966-9112-e6d4e9199f98` on the
original NVIDIA GeForce RTX 4090, with the supervised-process cleanup receipt. A preceding direct
execution also passes the same full trajectory. Physical Enter, D, Escape, E, T and X operate
the original native window: title, a new Scene, held movement, stopped pause clock, release while
paused, stationary continuation, fresh movement, real pickup, Scene travel, title, a second fresh
round and normal exit. At each selected phase, the original native and UI submission facts agree;
the exact presented semantic text joins its own nonempty clipped Text paint with positive alpha
and actual submitted glyph quads. The final original HWND is destroyed, Runtime retires, and the
physical event loop returns. The ordinary `desktop --play-flow --verify-first-present` entry also
passes, exercising the standalone profile selection without an embedded test Host.

Two initial contained native attempts encounter Windows foreground activation refusal; both
complete original native/process cleanup without forced termination. The test now permits bounded
normal focus activation and records its HWND even on activation failure so cleanup remains
observable. It never injects Alt to force focus. Physical release custody records a successful
KeyUp once and never replays it after native close. Independent review corrected those test paths
and a transparent-text false positive before the passing run. No compositor scanout, physical
controller, screenshot pixel-oracle or numeric performance claim follows from these submission
facts. General state hooks, flow-aware exact reconstruction, controller menus and cooked flow
delivery remain outside this unit.

Final affected release regression passes 236/236 with no skips in
`e2679fd9-e385-482e-b781-5f4c4b188959`. It includes the library and flow controls plus original
source gameplay, authored Input, Resume, concurrent rooms, cooker source/runtime admission,
animation and live-command behavior against the refreshed common bundle. Minimal no-default
`cooked-player` plus all seven CPU flow controls pass 13/13 in
`d9939ee0-baf6-4b3c-a1d8-6ae0bb5b9acd`, without UI or development services. The omitted-option
default desktop also passes first-presentation verification. Round completion itself is covered
by the public CPU game trajectory, not claimed as a displayed native Complete frame. Correctness
Clippy passes for the changed sample library, entrypoints and tests, and the owning root Host/UI
feature closure. Existing advisory warnings remain; no warning cleanup or performance gain is
claimed. Both independent review lanes are closed with their concrete findings corrected.

Root/sample formatting and document link/heading, manifest/lock/Schema and whitespace checks pass.
All scoped worker/reviewer lanes are closed; the fifteen paused Editor/Dialogue files retain their
original byte hashes. No remote publication or branch movement is part of this unit.

## Source Agreement And Processed Material Kind Successor

Before extracting a shared cooked entry, repair the two confirmed delivery gaps from the
[external review reconciliation](../research/2026-11-18-engine-architecture-review-reconciliation.md).
The game cooker must use the manifest-selected Schema agreement already owned by the source
Host. Use public manifest ingestion, recipe resolution and `ProjectContentLoader::load_delivery`;
retain their actual member observations through the existing source fence and final package
publication checks. Remove the fixed Schema path from the game inventory. Keep game-specific
catalog expectations and codec/import selection with this consumer. This operation runs during
preparation, not during gameplay, and introduces no additional Schema validator.

At the existing processed-content decode boundary, join each decoded Material texture slot to
the selected Image codec role. A present Scene Product must not satisfy an Image requirement.
Use the original decoded value and work budgets, and report the Material, slot and target Product.
Qualify valid and missing Images, present wrong-kind targets, independently reopened encoded
content, valid alternate Schema paths, rejected missing/corrupt/divergent bundles and actual
source drift through publication. Preserve relocated source-free gameplay. Generated-source
isolation and a general starter/Two-Level cooked entry remain separate successors.

### Source And Processed Admission Implementation

The Material-kind correction lands at `0464be5`; source/cooker agreement lands at `2f4e4be`.

The cooker now compiles Scenes through the admitted source plan, then compares the completed
processed Schema with the original delivery receipt. Its raw observations include the actual
delivery members alongside importer inputs; `ContentFence` retains their identities and digests
through the existing build/publication checks. The original fixed game content list no longer
chooses a Schema path. Alternate-path qualification uses the full existing source-free Session,
Marker and concurrent-region workflow, rather than stopping at receipt construction.

The decoded Material check uses the existing typed acquisition and selected Image codec role.
Independent Material controls pass 4/4 in `91fb2f2c-458e-42a3-b43c-884d6ef82a51`, including a
directly encoded wrong-kind base admitted after its source and encoded directories are removed.
Both builder completion and explicit deep validation refuse its Scene-valued texture. Valid
source-free gameplay and missing-Image rejection remain intact. See the
[receiving record](../research/2026-11-18-engine-architecture-review-reconciliation.md#source-agreement-and-processed-material-correction)
for integration corrections and the complete qualification scope.

The selected release Cook/source cohort passes 31/31 in
`a3db0fe5-f855-4876-9fec-8b8ae01a2c7e`. It covers absent default bundle with a valid selected
alternate, public source-delivery agreement on missing/corrupt/divergent selections, preserved
bootstrap absence/bytes, post-preparation source/name drift, complete delivery/raw observations,
and the original Material/TileSet/Input rejection-and-repair controls. Valid prepared content
continues through the existing source-free Session, Marker and two-region workflow after source
and cache removal. This is CPU/source delivery evidence; actual package execution remains a
separately recorded result. The actual package trajectory passes 1/1 in
`0656cfc8-e6e4-4ab3-a630-3e75ed466319`, preserving the original Player/gameplay, checkpoint,
fresh Restore, damaged-content and physical-process cleanup controls. It first runs a real
rejected cooker for the corrupt selected bundle, then repairs and delivers from the alternate
path with the default absent. The relocated Player runs after source deletion with Cargo/rustc
unavailable. The receiving record retains the initial absent-bootstrap fixture correction.
The affected `runtime_content`/builder owners also pass 81/81 in
`19f860e9-49ea-4b1b-83d8-b71691d799c5`, preserving once-decoded work and original budget/lifetime
controls. No new desktop GPU or foreign-platform qualification is claimed by this unit.
An additional Desktop/AudioDesktop preparation control passes 1/1 in
`be516c23-a84f-4c28-97df-02d14cdabd84`, checking actual prepared content and source Schema
agreement without creating a generated bootstrap, Window or audio device.

## TileSet-Backed Concurrent Regions Successor

Replace the two region floor Sprites with authored Tilemaps using one shared TileSet and the
existing relic Image atlas. Preserve the room entities, collectible Material, primary game,
independent close and coordinated travel. The visible floor uses real cells and per-cell colors,
so TileSet loading and drawing are exercised by the ordinary game rather than a private fixture.

Keep publication authority in `TileSetAssets`. Add an explicit shared-input choice to its existing
finite prepare/commit path. Sharing requires the original declaration claim, complete matching
declaration, current value/state association and captured Image allocation. It returns the original
receipt without publishing another slot or version. Existing replacement and atlas-rebuild behavior
remain available for their original owners. The Host tracks TileSet use per region and pools only
its owned receipts; independent unload releases the final live user's declaration through that
same owner. Primary travel retires the primary/addition union in its original transaction.

Qualify source and processed loading, shared handle/version, changed live declaration refusal and
repair, invalid persisted cell membership, exact unload, retained old pixels and real source-free
delivery. Extend the existing cooked catalog by one TileSet with its Required Image edge. Use
the original source reader, codec, render capture and GPU Host, and preserve all Material, Save,
fresh Restore and native cleanup controls. Run checks at load/activation/publication; ordinary
gameplay gains no complete Scene or asset audit. Arbitrary native-service regions and full ADR
0089 stay open. This is functional Runtime delivery, not a performance experiment.

Godot's pinned `TileMapLayer` retains a `Ref<TileSet>` rather than giving each layer another
resource publication owner. Nara keeps that separation between map use and resource ownership;
its existing Host remains responsible for transactional activation and retirement. The source
reference does not establish Nara's transaction semantics or a performance comparison.
[Godot TileMapLayer](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/modules/tilemap/tile_map_layer.h)

### TileSet Region Implementation And Qualification

The original owner/Host changes land at `759daae`; the authored game, cooker and actual source-free
consumers land at `c32d2ad`.

The original `TileSetPublicationInput` adds an explicit `with_shared_receipt` choice. Preparation
checks the retained declaration claim, complete declaration including floating-point bits,
current TileSet/value association and captured Image. A shared input does not request a new
version or consume additional pixel retention. The Host tracks TileSet handles per region and
pools only newly owned receipts. Primary replacement uses the original primary/addition union;
independent unload retires a declaration only after its final live user. The existing atlas
rebuild barrier remains in effect. Source and cooked content use the same publication owner.

The first production check catches an unload closure whose inferred lifetime escapes the
original resource scope. Moving that concrete tail into `PreparedUnload::commit` resolves it
without cloning an owner or adding unsafe code. The Host selection then passes 75/75 in
`91c322e1-2ace-4a78-ac40-39c35329e19d`. The complete original Tilemap library selection passes
119/119 in `56feee97-cc7a-4361-923e-1e0e52f3cad0`, including the new source/processed sharing
controls, capacity reuse, unchanged slot/version and stale-baseline refusal.

The ordinary game replaces both floor Sprites with authored eight-cell Tilemaps using distinct
checkerboard colors and one shared TileSet/relic Image atlas. The collectible Material remains
unchanged. The cooked catalog has fifteen members: five Scenes, five Images, one Material, one
TileSet, Sound Bank, Sprite clips and startup Input; Meadow and Cavern remain the two travel roots.
Schema generation reports no change to native binding revision 6, and strict Check passes with
33 content members. Independent review catches a missing TileSet domain allowance in processed
additive admission. That whitelist now includes TileSet alongside Image and Material; other
unsupported domains still refuse. Bounded rereview reports no remaining P1/P2.

The source/cooked selection passes 42/42 with the hardware pixel case excluded in
`1131d79d-03a7-4548-a8f7-41377a979736`. It covers both close orders, unchanged TileSet slot/version,
escaped Image content, changed definition refusal and repair, invalid cell preflight, processed
dependency closure, primary travel and checkpoint restoration. The first hardware pixel run
`40426127-30de-405e-ac28-1371803d1266` reaches actual travel after its drawing checks, then fails a
new fixture assertion that assumes retiring TileSet residency removes its Product identity.
`AssetServer` keeps that original identity binding. The corrected control requires the same
binding alongside absent live TileSet value, claim and usable version; old captured pixels must
still replay. No production behavior changes for this fixture correction.

The corrected complete hardware DX12 trajectory passes 1/1 in
`5f19f462-e7dd-4432-81d0-9593cbfe37c4`. It checks both eight-cell floors' independently computed
checkerboard colors and their disappearance, preserves the relic Material controls, and replays
four original complete images after actual travel and Runtime close. Linear RGBA8 floor
expectations are north `[3,76,41,255]` / `[4,114,58,255]` and south `[3,67,70,255]` /
`[5,95,87,255]`. Original GPU resource retirement completes without retained bytes or cleanup
faults. This is captured image-output evidence, not frontbuffer scanout or a performance claim.

The actual source-free selection passes 3/3, with seven unrelated cases excluded, in
`dcd251d0-b92d-45a8-85de-7de30c8f933f`. It cooks, relocates and runs the headless, Windows hardware
DX12 desktop and WASAPI Players. Physical B/G opens the two eight-cell floors under one TileSet
version; pickup and independent close retain the surviving region. Coordinated travel releases
the declaration. Save, fresh-process Restore, the original software control, collision excursions,
twenty later travels and original native retirement remain intact. Each cooker reports complete
cleanup without termination in this run; Player cleanup retains its existing stricter contract.

The final affected release-profile selection passes 313/313, with seven excluded cases, in
`fced251b-26f3-430f-8899-63216b147548`. The final Host/public content selection passes 107/107 in
`3d0d7855-c08e-41e7-ad8d-d8c28b5ce7f4`. These include the corrected processed domain allowance,
source/cooked regions, configured content budgets, animation, collision, command recording,
checkpoint and exact-resume controls. They do not establish the absence of unrelated defects
identified by the separate November 18 external reviews.

Correctness Clippy passes for the Tilemap owner, selected root Runtime/tooling library and the
sample's all-target `cook,desktop,audio-cpal,cooked-player` closure. Existing advisory lints remain;
this is not a warning-free build claim. Root/sample formatting checks pass after formatting only
the new whitelist line and corrected fixture call. Local document links/headings, authored JSON,
TOML and whitespace checks pass. All fifteen paused UI/Dialogue files retain their original bytes.
Editor/UI work, arbitrary native-service regions, foreign-platform execution, numeric performance
budgets and full R9/R10/R12 remain outside this scoped unit. Runtime delivery takes priority over
small performance experiments; unchanged official implementations are not requalified here.

## Material-Backed Concurrent Regions Successor

Give both playable Meadow regions one authored Material Product and real Shader input. The relic
Sprites use the same immutable material definition and existing Images. Source and cooked play
must prepare, activate, independently unload and travel with those regions through the current
Scene, asset and render owners. A live material is shared only when its original slot, publication,
complete Shader/declaration/defaults and texture selections still match. A changed source while
another region uses the old material refuses the new activation without altering the incumbent.

Extend the current residency pool to Material publications, with per-region use and exact
last-live-user release. The primary Scene may share a material with its additions. Primary travel
must retire the union in its existing transaction, and a failed preparation or preflight preserves
the original group. Retained bindings keep their exact Shader and material data; retained render
inputs keep their captured texture values after the live publications retire. Reuse the Material compiler, Runtime codec and physical
submission owner; add no current-generation lookup or independent GPU authority.

The ordinary cooker publishes the Material's complete Required dependency closure, fences the
original WGSL read and delivers a source-free Player. Qualify the authored game, matching material
handle/version across both regions, conflict and recovery, independent release, coordinated
travel, and real GPU pixels from the captured material. Keep the existing physical play, Save,
fresh Restore and cleanup trajectory. Material admission work runs at load/activation/retirement,
not in ordinary gameplay ticks. This unit is functional delivery, not an optimization campaign.

This remains a concrete Image/Material additive contract. TileSet and arbitrary native-service
regions, explicit destination sets and full ADR 0089 stay separate.

Pinned Bevy uses an asset handle on `MeshMaterial2d` and resolves captured render assets through
the existing renderer. Godot's `ShaderMaterial` retains a `Ref<Shader>` and parameter values.
Those sources support keeping material data, Shader and rendering responsibility separate.
They do not establish Nara's transactional shared-definition or retirement guarantees.
[Bevy Material2d](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_sprite_render/src/mesh2d/material.rs) · [Godot ShaderMaterial](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/scene/resources/material.h#L95-L130)

### Material Region Implementation And Qualification

The Host implementation lands at `780ce6b`; the authored game, cooker and real consumers land
at `640024a`.

Both relic Sprites reference the authored `two-level-game.region` v1 Material, one static WGSL
body and a Meadow palette Image. Their component parameters override the asset defaults through
the existing persistent `nara.material2d` contract. Source and cooked recipes select the same
Material domain/Runtime codec. The catalog has fourteen members, five Scenes, five Images and
one Material, with two travel roots. The schema remains native binding revision 6; regeneration
reports no schema change and strict Check reports 31 content members.

The original Host now pools owned Material publications and records their use per addition.
An identical imported definition shares the original slot and version, including definitions
prepared independently. Both original authored references and their resolved Product bindings
must agree. Live defaults, Shader inputs or texture changes refuse before mutation. Prepared
primary travel includes the original primary/addition union. Pure retirement can release the
unchanged last-good Material after a failed reload, while ordinary sharing still requires its
unchanged publication state. Escaped bindings retain the original Shader, data and metadata
leases after the live publication ends.

The first Host selection passes 74/74 in `39400077-557c-468f-89f6-dc2c17a6fb34`. Independent
review finds that numeric equality would merge `+0.0` and `-0.0` defaults despite the existing
uniform encoding preserving their bits. The sharing comparison now checks those original bits.
The focused Material selection passes 12/12 in `eab15208-bdb1-4c45-9a3d-09d2ae1f4cbd`, including
same-bit sharing, changed-sign refusal, failed reload, subset retirement, union replacement and
escaped-owner release. These checks run within preparation and publication, not gameplay ticks.

The source-game room selection passes 8/8 in `6f30cbef-6a0b-49ba-b5c8-05d2f0fc3b52`. It proves
both close orders, handle/version sharing, changed live Shader/default refusal and repair,
independent collection, direct lower-level retirement, bad-destination preservation and travel.
The cook/content selection passes 26/26 in `fdcee8b6-0223-422b-b66d-7d09ccd0a583`. The cooker
uses the existing Material importer and `CookedContentBuilder::add_material`; it does not add a
Material build trait or rewrite authored texture Paths. The importer reads under the original
source capability, records the exact Material/WGSL bytes and resolves texture Products through
the admitted database. The original content fence preserves 29 fixed content inputs, raw
observations and WGSL included as code input, and rejects drift rather than rebasing later reads.

The initial hardware DX12 pixel test passes in `4e3acc91-d44c-4ee2-ab2a-f501b08f4e11`, then review
requires explicit negative pixels after pickup and independent close. The strengthened test passes
1/1 in `9e8dc933-1abe-4876-b78e-d92a920d157c`. It moves the actual player off the original relic
location, checks the authored floor after collection and the main background after independent
close, and retains the Lookout's positive graded pixels throughout. The original packets render
linear RGBA8 `[45, 127, 61, 255]` from independently decoded sRGB source/palette texels. Their
four complete images remain byte-identical when replayed after actual travel releases the live
assets and the Runtime closes. Original Host producer, native-use and resource retirement finish
without retained bytes or cleanup faults. This is hardware DX12 image-output evidence, not
frontbuffer scanout, numeric performance or foreign-platform qualification.

The actual source-free selection `68c650a8-2d6d-4242-a1ae-4d97e369a3d3` first refuses to build the
audible shipping Player because the shared desktop runtime uses the Image codec's trait constants
without importing `RuntimeAssetCodec`. That code is behind the generated shipping configuration,
so the development build does not compile it. The import is corrected without changing codec or
Player behavior. The headless and complete hardware DX12 desktop trajectories pass in that
selection. Physical B/G opens both rooms with Material version 1, identical Shader/interface
digests and no source inputs; north pickup and independent close retain the same publication,
and coordinated travel reports no resident region Material. Save, fresh-process Restore,
collision excursions and twenty further travels remain intact. The audible Player is re-run
against the corrected shipping source rather than counted from the failed build. That rerun
passes 1/1 in `976ce1f7-0810-4465-89df-138aba5d1bb1`, including actual WASAPI samples, physical
window Close and original native retirement. The actual desktop and audible Player receipts
retain their no-termination requirement. Cookers report their original complete cleanup facts
separately. The selected native scope is Windows hardware DX12 and its existing software control.

The final affected release-profile selection passes 305/305, with seven excluded tests, in
`18bf1ead-03ef-4faf-95bb-3fe7e2b29b8c`. It includes authored Input/Prefab/animation, collision,
checkpoint and fresh-process Restore, recording/replay, exact source reconstruction, all room
tests, cooked content and source fences. No checkpoint or resume wire format changes.

The final Host, public source boot and budget selection passes 107/107 in
`c2b3f4c4-0236-4d0c-9e53-40b0921fab92` with Runtime and tooling enabled. The original request,
publication, pre-native refusal, native retry and generation limits remain covered alongside
the new Material sharing and retirement controls. These cohorts overlap; their counts are not
additive coverage or a measured performance claim.

Correctness Clippy passes for the root Runtime/tooling library and all sample targets with cook,
desktop, audio and cooked-player enabled. Root and sample formatting checks pass. Changed local
document links, authored JSON and whitespace checks pass, and the fifteen paused UI/Dialogue
files retain their original byte hashes. Independent owner, cooker and pixel-consumer reviews
close their concrete findings. The feature leaves general additive TileSet/native services,
explicit destination sets and complete Foundation acceptance open.

## Concurrent Regions And Coordinated Travel Successor

Open the existing Meadow side room and a second authored Lookout in the same running World.
Each room has an independent occurrence and collectible. Both need the same relic Image and
Meadow Image. Closing either room releases only its lifetime. Shared publications survive until
their final live Scene user leaves. The rooms use normal deferred requests and reuse the original
single preparation slot.

Extend Host residency to own the union of the primary Scene and all committed additions.
Retain the existing source and processed authority checks. Ordered exclusive activation and
unload update the same owner. A prepared primary replacement joins all live addon receipts,
resource releases and catalog changes to its existing Scene product transaction. A rejected or
cancelled destination leaves the original regions playable. A successful destination removes
the original group and retains runtime-scoped time, progress and applicable services. The same
rule applies to the existing managed travel publication when it drives the staged transaction.
The retained-source Restart and exact game capture still require closed rooms.

Give the ordinary source recipe room for the main Scene, two distinct additions and a travel
candidate through the existing decoded-generation budget. The ordinary default remains two.
Cooked admission retains the original byte budgets. Room scope remains Image-only. General
scene-scoped native services and coordinated placement into two destination regions remain
separate ADR 0089 work.

Qualify both rooms through source and source-free cooked play. Prove shared Image identity and
exact release, bad-destination preservation, fresh occurrence identity, and real desktop input
and presentation across the handoff. Retain the game's original collision, checkpoint and
cleanup evidence.

Bevy spawns independent roots and uses strong asset handles for shared lifetime. Godot's PackedScene
instances can share cached Resources. Its SceneTree replacement detaches the old root and installs the
new root at frame end. Nara keeps the group handoff in its existing exclusive publication so observers
see a complete scene group at each boundary.
[Bevy instance activation](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_scene/src/resolved_scene.rs#L47-L65) · [Bevy strong asset handles](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_asset/src/handle.rs#L98-L121) · [Godot resource lifetime](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/doc/classes/Resource.xml#L6-L10) · [Godot scene replacement](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/doc/classes/SceneTree.xml#L42-L71)

### Concurrent Regions Implementation And Qualification

Implementation and game consumers land at `a3f6293`. The cooker/Player cleanup distinction lands
at `9bbea40`.

The Host retains each occurrence under the existing residency owner and pools only the Image
and catalog publications it owns. Activation checks an intersecting owned catalog record before
publishing another region. Unload counts live Scene users, including a region already ended by
the lower-level exact retirement API. A failed Image reload preserves the last-good publication
for pure retirement. It does not grant a new region permission to borrow a changed publication.

Prepared travel captures the region-set revision. A change before native work rejects and permits
discard. Once native retirement begins, that set is fenced until the original attempt completes;
an unload cannot strand its retry against a newer region set. A pre-native refusal leaves the
original game playable. A native failure retains the original publications and exact candidate
for explicit retry; it does not promise rollback of native effects.

`SceneProductRetirement<'a>::Instance(&receipt)` now borrows a receipt during preparation.
The Scene owner admits the complete membership count before retaining full receipt copies.
The Host joins its own borrowed receipts to the product selections in the same transaction.
The caller limit counts original members, including already retired members, and one unit for an
empty occurrence. This changes the public selection interface without adding a second retirement
owner. The source recipe selects four decoded generations; the default remains two.

The focused Scene selection passes 53/53 on Windows with Rust 1.95.0 in
`8b307419-a937-4f59-9379-beb9684628fa`. It retains the original tests for complete and partially
retired activations, empty-occurrence limits, stale receipts, resource mutation and retry without
replaying product configuration. The source schema regenerates at native binding revision 6;
strict Check reports 28 content members.

The final Host, source startup and budget selection passes 99/99 in
`94b53eaa-330d-4eb6-9606-da41aea5e82e`, including the tooling feature closure. It covers two
occurrences of the same Deferred Scene, insufficient retirement limits, changed region sets,
native failure followed by exact retry, and refusal to unload while native retirement is pending.
A failed Image reload after retirement preparation keeps the original publication eligible for
removal. Public source limits retain their default two-generation behavior; the configured
four-generation control releases capacity on the last owner.

The release-profile source and cooked-content selection passes 22/22 in
`82e78fdc-3cc0-41f7-8963-e0c0bfa53720`. Both room collection directions, independent close/reopen,
shared Image identity, removed-record refusal, last-live-user release after direct retirement,
and retained pixels use the actual game and public runtime interfaces. Save runs while one room
is collected and the other remains open. Bad destination and lifecycle preflight leave both
rooms intact; successful travel and later Restore retire them with the primary Scene.
The cooked game runs with its source unavailable, including both regions and coordinated travel.
The consumer's first compile reveals a test import of a private retirement predicate; its
replacement checks the public exact inactive-instance error and original-member tombstones.

The affected ordinary gameplay selection passes 276/276, with seven excluded tests, in
`cff9457e-5d6e-4737-8d83-a71a73abd003`. It covers the library, authored Input and Prefab markers,
animation, collision, checkpoints, recording/replay, source resume and Player entry contracts.
The separate-process headless save/fresh Restore and replay trajectories remain intact.

The actual relocated source-free headless and WASAPI Players pass in
`6e52b395-b232-47ae-a956-aded49255deb`. The same selection stops in the desktop physical-input
guard. Its retry `bfa60295-96bf-4458-b196-d3dd50fb5785` completes the new B/G open, north pickup,
independent close and coordinated travel trajectory before losing foreground during the original
movement workflow. An isolated retry `e81c71c5-0617-4f18-8eb3-f4c6af42cd1b` cannot acquire the
original HWND at startup: the reported foreground belongs to another process. The supervising
workers report complete containment, stopped readers and drained output without forced
termination. The exact-foreground assertion remains enabled. These interrupted attempts do not
qualify the complete path; the final successful run below supplies that evidence.

After that foreign process exits, `32c8c4fc-8ae2-48a6-906d-af8c235da272` exposes a separate fixture
error before launching the Player. The cooker exits successfully and publishes its package, but
the shared supervisor reaps residual descendants after its natural-exit grace. Its original
receipt correctly records `termination_requested=true` and `force_killed=false`. The fixture had
applied the game's natural-exit assertion to this build tool. Desktop and Audio now select their
concrete Cooker or PlayerWorker role. A cooker must exit successfully, complete containment,
readers and output, avoid loss and force-kill escalation, and report its unchanged receipt.
The PlayerWorker and every native Player retain the original no-termination requirement.
Independent review confirms this matches the existing shared process contract. A successful
build receipt does not establish natural exit of the whole compiler process tree.

The final Desktop and Audio selection passes 2/2 in
`1ecf03fe-5da2-404f-9bcd-372787dadff0`. The ordinary hardware DX12 Player receives physical B/G
to open both regions, W to collect the north relic, B to close only the north region, and E to
retire the remaining Lookout with its primary Scene. Original presentation receipts confirm
two, one and zero visible relic draws and the same Image publication while shared. The original
collision excursions, Save, fresh-process Restore and twenty further travels pass. The finite
software control, rejected initial save and scripted controls remain enabled. All four native
Player receipts show empty containment, stopped readers, drained output, destroyed windows,
no termination request and no force kill; the deliberate bad-save process exits unsuccessfully
as required. The WASAPI Player retains its sample comparison and original native retirement.
Both final cooker receipts exit successfully with complete physical cleanup and no termination
request. This qualifies the selected Windows workflows, not foreign native execution or physical
scanout.

Correctness Clippy passes for the root Runtime/tooling library, all serde-enabled Scene targets,
and all sample targets with development, cook, desktop, audio and cooked-player enabled. Root
and sample formatting checks, authored JSON, changed local document links and whitespace checks
pass. The fifteen paused UI/Dialogue files retain their original byte hashes. Independent review
closes the catalog, live-consumer, native-retirement fence, last-good Image and borrowed-receipt
findings.

Ordinary residency adds no per-frame scene-group audit. The desktop Player's opt-in `--observe`
mode joins room Sprite draws and Image publications to the original native presentation receipt.
That instrumentation does not establish production frame costs. General additive TileSet, Material
and native service lifetimes, foreign native execution and complete ADR 0089 remain open.

## Deferred Additive Region Successor

Add a playable side room admitted through a Deferred Scene reference from Meadow. The player
can keep moving while the original Scene request worker prepares the room. Its authored Sprite
and local interaction become visible together at an exclusive boundary. Closing the room ends
its exact occurrence and releases its own Image publication. The same room can then load again
with a fresh instance identity. Preserve the original level, simulation clock, input and game
progress around the complete operation.

Use the original hosted request slot, World, Scene materializer, asset catalogue and CPU Image
publication path in both source and cooked play. The deferred target belongs to the current
admitted closure. A product that is merely present in the pack grants no activation authority.
Preparation and cancellation keep their original physical task custody. An active addition
retains its own data and resource receipts. Identical Images may reuse the current level's
publication. A conflict on a live Image refuses before the first mutation.

The first consumer has one open side room. Close it before primary replacement or exact game
capture. Ordinary checkpoint restoration continues to reconstruct permanent game progress. Test
the cooperative load, cancellation or refusal with incumbent play, exact unload, fresh reopen,
and packaged execution without source files. This unit leaves coordinated multi-region travel,
scene-scoped native services and complete ADR 0089 acceptance open.

Godot separates threaded resource requests and polling from `PackedScene` instantiation. Bevy's
Scene spawn writes an admitted scene into the receiving World through its ordinary spawn path.
Nara uses those ownership boundaries with its existing task custody and product-scoped receipt.
[Godot resource loading](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/core/io/resource_loader.cpp#L887)
[Godot scene instantiation](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/scene/resources/packed_scene.cpp#L2582)
[Bevy scheduled Scene spawn](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_scene/src/spawn.rs#L690)

### Deferred Additive Region Execution Evidence

Implementation and the source-free consumer land at `81d1b6b`.

The focused host and source-loading selection passes 94/94 in
`2a9bee30-c617-4738-bf47-28b1423a7b22` on Windows with Rust 1.95.0. It covers physical cancellation,
an admitted Deferred source under `assets/`, cooked admission through a verified codec edge,
the last-good Image after failed reload, and asset cleanup after the original occurrence has
already retired. The existing public startup fixture uses the current borrowed codec input API.

The gameplay and cooked-content selection passes 240/240 in
`dfddf780-3179-4a87-88ac-4ea2c5faa7f8`, with seven explicitly excluded tests. This release-profile
run covers the sample library, source game, side room, cooked input and asset definitions, Sound
Bank, exact state capture and replay. The source game keeps movement and the original Scene
while the room prepares, refuses corrupt pixels and a conflicting shared Image, closes the room,
and reopens it with a fresh instance. The cooked in-process game opens and unloads the same room
after the source disappears. The retained pixel control keeps the old value usable after unload.

Independent review confirms the canonical source path is part of the decoded-generation identity.
Cooked admission retains Deferred targets reached through verified Scene or asset-codec edges.
The Host can finish exact unload after the previous Image reload attempt fails or after the
same Scene was fully retired through the lower-level operation. Invalid live facts continue to
refuse before publication. These checks run in the original load/activate/unload operations.

The actual release cooker passes the relocated headless and WASAPI Player trajectories in
`ea5fda6e-131c-4dcf-95f6-8c1b180cdd24`. The same selection first stops in the desktop scripted
control because the fixture requires the opt-in Sprite field on a process launched without
`--observe`. The physical side-room trajectory has already run at that point. The desktop rerun
restricts the Sprite assertion to observation-enabled windows and keeps the shared runtime checks
for every Player mode.

The final desktop trajectory passes 1/1 in `0debe7dd-483a-4ceb-ac36-9f13b64b7940`. The
ordinary hardware DX12 Player receives physical `B`, `W` and `B` from a relocated source-free
release. The opened room adds one visible relic Sprite to the stock draw batches joined to the
original native presentation receipt. Pickup hides that draw, and unload removes the relic Image
from live residency while keeping the same primary Scene instance. The same process continues
through travel and Save. A fresh process restores and continues the original game, including
twenty more travels. The existing software first-present and physical retirement controls pass.
This covers same-frame stock draw inputs and native submission on Windows. GPU scanout, broader
multi-region streaming, non-Image companion assets and foreign-platform execution remain open.

Scoped correctness Clippy, both formatting checks, the changed document links, and the authored
JSON/TOML checks pass. The work follows the Foundation
[runtime delivery priority](2026-10-23-foundation-architecture-rebaseline-plan.md#november-17-runtime-delivery-priority).

## Textured Prefab Runtime Successor

The marker's child declares a dedicated Image in the authored Prefab. The owning level declares
the marker Scene as an Immediate dependency. The Image enters the source and cooked closures
through the marker Scene alone. Exercise the ordinary resolved Sprite handle and visible texture
through repeated placement, retained-source Retry, travel and fresh checkpoint restoration.

Use the existing Host, content residency, Scene codec and renderer. The game prepares the bounded
marker topology and local reference from the compiled template. After publication, it records
the installed Sprite and Transform values for the existing degradation and Retry behavior.
This removes the temporary World that used to decode every template part. The resulting receipt,
geometry and game checkpoint remain the existing lifetime and reconstruction owners.

Verify the actual required catalog edge, shared texture across occurrences, distinction between a
retained value and changed disk bytes, damaged-resource refusal before replacing the current
level, and GPU readback of the marker during native reconstruction. The cook and fresh Player
must use the same resource and exclude the source after installation. Texture residency follows
the containing Scene and any retained render inputs. Independent persistent asset ownership and
asynchronous additive Scene loading remain open.

### Textured Prefab Execution Evidence

Implementation and consumer coverage land at `52a929d`.

The authored marker selection passes 5/5 on Windows with Rust 1.95.0 in
`5e68d8cf-a60c-441e-8002-66b8bfffb131`. The original game places repeated occurrences with the
same Image handle. A Retry after replacing the PNG uses the committed image. Fresh loading sees
the replacement. Damaged source PNG bytes reject travel while the original level and its markers
remain playable.

The affected release-profile selection passes 77/77 in `27262a36-f3d3-4603-90c6-54acbef8babd`.
The cooked catalog has ten members, two travel roots and five direct dependencies per level. Only
the marker Scene requires the badge Image. The in-process source-free consumer places a marker
through the original command queue and resolves its child reference to the Sprite bound to the
processed 2 × 2 Image. The Runtime, source fence, Save/Restore, replay and pixel oracle tests run
through their existing owners.

The Windows native release selection passes 5/5 in `9d75516a-fac3-453a-81ea-bf350a2b19cf`.
The production cooker builds the headless, desktop and audible Players and the tests run each
relocated pack with the source unavailable. The headless path rejects a deliberately damaged
badge member. The desktop path exercises physical input, travel, Save and fresh-process Restore.
The audible path compares actual WASAPI samples and completes native retirement. Both source
desktop reconstruction paths, with default and authored arrow bindings, read back the retained
marker texture through hardware DX12. The oracle checks its source sRGB texels and the expected
linear RGBA8 output `[255, 134, 13, 255]`. This qualification covers Windows and the sample's
Scene-owned Image lifetime. Foreign native execution and independent asset residency remain open.

Focused correctness Clippy, sample and workspace formatting, asset JSON parsing and changed
document links pass. The unit exercises the existing delivery and resource owners. The Foundation
[runtime delivery priority](2026-10-23-foundation-architecture-rebaseline-plan.md#november-17-runtime-delivery-priority)
keeps module timings as comparison evidence for future work.

## Authored Marker Instantiation Successor

This unit serves the active Foundation Goal and the Runtime delivery priority.

Replace the game's code-generated marker with a real multi-entity authored Prefab referenced by
a genuine wrapper Scene. Both levels declare that wrapper as an Immediate Scene data dependency.
Only the two levels are travel roots. Source preparation uses the already admitted dependency
candidate within the current content generation. Processed execution uses the original
residency-owned Runtime Scene acquisition. Both paths reuse the prepared value.

The original Host exposes scoped dependency inspection and explicit instantiation against the
complete expected current receipt and original component definitions. Normal Scene preparation,
reference remapping, hierarchy publication, World identity and occurrence ownership remain
authoritative. A caller explicitly places a dependency. The first game template uses untextured
Sprite data and internal entity references. The original asset residency contracts govern
resource-bearing consumers.

The game owns durable MarkerId/LevelKey facts and retains each full activation receipt and its
logical occurrence. Authored hierarchy, appearance and internal references come from the Prefab.
Placement adds the saved position to the authored spatial root without accumulating offsets or
overwriting authored rotation/scale. Retry restores the whole authored occurrence, including
missing ordinary components and legally retired members. The owner rejects stale Scene identity
left by raw ECS despawn.

Preserve rejection-before-mutation for ordinary current-level Restart and prepared travel. Extend
the existing product replacement transaction to accept both runtime-only entities and additional
complete activation receipts. Identity prepares one aggregate retirement and tombstone sequence;
Scene preflights the complete hierarchy/lifecycle/persistent-apply union and releases all original
occurrences in the exclusive commit tail. The transaction retains the authored activation
receipts and the original exclusive lifetime and native-effect contract.

The cooker consumes the actual wrapper Scene and Prefab through existing compilation and Runtime
Scene flattening. Shipping gains a ninth member and third Scene member while retaining only the
two level travel roots. Every level has its actual Required wrapper edge. Original source fences,
generated Schema/expectation, catalog and relocated source-free Player remain authoritative.

Acceptance covers repeated complete occurrences and internal-reference isolation, authored edit
visibility, multiple level instances, A-to-B-to-A, Retry, rejected replacement preservation,
Save/fresh-process Restore, original source-game resume and semantic recording/replay. The
source-free headless and desktop Players cover actual child geometry, pixels and original cleanup.
Original audio delivery receives its affected catalog checks. Focus verification on changed
owners and these gameplay paths. Full Foundation, R10/R12, foreign execution and numerical
performance budgets remain open.

### Authored Marker Implementation Checkpoint

The original Host now inspects and instantiates retained Immediate dependencies against the
expected current receipt. Source candidates share their original resolver/captured generation;
processed candidates reuse existing residency acquisitions. The product transaction now takes
explicit `SceneProductRetirement::{Entity, Instance}` values. Identity prepares one retirement
union, and Scene ends additional occurrences in its exclusive tail. The game retains whole marker
receipts and derives geometry/hierarchy/reference expectations from the original compiled template
using the receiving component registry. Native binding revision4 adds MarkerPalette/MarkerLink;
the existing project Schema command regenerates the actual authoring bundle.

The first production identity/Scene gate exposes one missing exhaustive diagnostic for the new
duplicate-instance error; the corrected gate passes. The sample library/headless compile gate
also passes. Initial game run `fdf1977c-d2cc-40dc-96ce-7b61312a083b` passes18/54: most failures
are the deliberately strict old authoring-bundle/native-Schema disagreement, before gameplay.
After refreshing the bundle and extending fixture inventories, the actual project Check succeeds
with twenty source-content members. An actual startup then exposes an empty-selection regression:
the additional-token helper demanded an installed identity domain while initial Scene publication
still owned its detached domain. Empty selections now preserve the original registration path.

Initial all-feature owner run `cc0cc62a-da5d-49bf-8855-54f19895455a` passes916/921. Corrections
preserve original wrong-World and projection error contracts, remove an invalid assumption that
the first ECS entity is the Scene root, and retain the aggregate owner's earlier lifecycle refusal
without another whole-hierarchy scan solely to preserve an internal diagnostic classification.
Independent review also identifies a removed root used as the new Scene's insertion anchor. The
hierarchy owner now preserves the primary Scene's slot relative to the nearest surviving root.
An interleaved additional-activation regression covers that case.

The first complete game cohort reaches 153/207. The marker camera check compared the authored
stable-ID selection to an explicit Product value, although both forms name the same primary
product. The game now accepts that exact primary identity in either form. Marker snapshot
collection queries the marker component directly and shares its graph check with explicit resume
capture. This removes the previous whole-World marker scan and a duplicate resume ID comparison.
The corrected ordinary cohort passes 206/207 in `78e99c49-99b1-4ddd-bd56-2b39725e4687`.
The remaining test used a success-only settling helper after an expected refusal. It now observes
the original movement, key release, level and marker receipts directly, then uses retained-source
Retry to reestablish the exact capture boundary. The cooker leaves tree shape to Scene compilation
and the same game-side admission used by source execution.

A final review identifies two different resource bounds and a hierarchy fact that can change after
staging. The Scene occurrence metadata records the original member count, even after partial
retirement. Admission projects the count and bytes retained after the complete retirement union.
The final composition uses current hierarchy placements, so a surviving child attached after
staging is promoted before its retiring parent is removed. Focused regressions exercise all
three cases.

Pinned Bevy `03ed5a27c92715c8fa2188f31f04e454ec39942f`
[`WorldSceneExt`](https://github.com/bevyengine/bevy/blob/03ed5a27c92715c8fa2188f31f04e454ec39942f/crates/bevy_scene/src/spawn.rs)
separates immediate resolve/spawn from dependency-aware queued spawning. Pinned Godot
`7a37606a433c0e7735525037cdf774f043146a21`
[`SceneState::instantiate`](https://github.com/godotengine/godot/blob/7a37606a433c0e7735525037cdf774f043146a21/scene/resources/packed_scene.cpp)
constructs the authored hierarchy and local ownership, checking structural failures at that
operation. These mechanisms support acquisition/preparation and explicit instantiation as
separate responsibilities. Nara's exact expected receipt, occurrence and aggregate retirement
are its own contracts. The game's bounded snapshot health projection checks its owned marker
graphs so it can expose degradation and allow Retry. No numerical cost has been established.

### Authored Marker Execution Evidence

The engine implementation is `6fd0135` and the game and delivery consumer is `895bb3c`.

The final ordinary sample selection passes 207/207 in
`9006314d-dc17-4813-b126-3c6f928055df`, covering gameplay, exact resume, recording and replay,
authored Input, cooker source and Runtime controls, and SoundBank content.

The final marker, native and delivery selection passes 13/13 on Windows with Rust 1.95.0
(`dfdfdf23-d651-4429-9da7-2c8b651f93f5`). All four authored marker cases pass, including
the corrected malformed-Prefab refusal. The actual release cooker produces the nine-member
catalog with three Scenes and two declared travel roots. The relocated headless and desktop
Players launch after the source and cook cache are removed, execute travel and Save/fresh
Restore, and return their original cleanup receipts. The headless case also exercises damaged
Immediate marker content. The desktop case covers Windows/DX12 physical game input and original
presentation. Two independent source desktop reconstruction paths verify marker-root and child
pixels, held input, reconstruction and physical close with default and edited arrow bindings.
The five audio oracle controls pass and the changed audio package fixture compiles. Hardware
audio execution was outside this selection. These are Windows workflow results.

The all-feature Identity and Scene library cohort passes 924/924 in
`bd823b52-657c-40ad-95fb-a6404b0f5630`. It covers first registration, ordinary and projection
replacement, native claims, interleaved root order, promotion after staging, original membership
limits and post-replacement occurrence budgets.

The focused public Host cohort passes 3/3 in `104bb9f1-96c5-43bb-b60c-17aac5e75724`.
The source and processed paths exercise repeated placement and occurrence reuse, with actual
resident document borrowing and independent local references. The first fixtures counted allocated
ECS indices and selected a minimal profile. Host startup requires a strict Schema agreement and
a semantic queue. The final fixtures count spawned entities, generate the actual bundle and
select the original local-headless profile. The Reference Game library and the sample's
all-target correctness Clippy gate pass. Formatting passes for the root, sample, Reference Game
and affected independent package.

The independent Scene, source-reference and managed-domain consumer selection passes 31/31 in
`a607b6b5-73f3-4a6f-9555-48cb9ca18a14`. The checked-in consumer lock predates the game's Sprite
animation and Rapier dependencies. An offline refresh in a temporary fixture copy adds those
dependencies while preserving existing package versions. The checked-in lock is outside this
unit's change scope.

## Existing Contracts And Pressure

- Strategy selects a Rust-first integrated production path, generalized from complete consumers.
- ADRs 0037/0117 distinguish asset identity, immutable published values, dependencies and retention.
  A copied handle does not acquire or retain content.
- ADR 0110 originally delivered exactly the startup closure. P1 extends that decision explicitly
  for a finite declared Scene-root set; a second Scene on disk alone is insufficient export authority.
- ADR 0027 keeps game saves separate from authoring Scene/Prefab files and backend-native state.
- ADR 0089 remains Proposed. Adopt only the concrete lifecycle behavior the consumer proves;
  do not implicitly accept its complete additive-loading or world-partition design.

At this plan's starting source, content loading is hard-wired to `startup.default_scene`. A generic selected-Scene
snapshot must bind its exact source and retain only its own dependency closure. Existing runtime
activation retains the startup source; changing a lower Scene receipt alone would leave Retry
and content retention pointing at the old level. Correct those owners before exposing travel.

## Ordered Units

### P1: Selected Scene Content And Exact Delivery

Declare project-wide `[content] scenes = ["level-b.scene.json"]`, relative to the effective
`paths.scenes`. Startup is an implicit delivery root. Bound the explicit list to 64 entries,
reject duplicate/invalid entries and preserve strict unknown-field admission. This is a finite
declaration ceiling, not a gameplay capacity or performance claim. Profiles do not independently
override the list.

Generalize `ProjectContentSnapshot` to one selected Scene and its dependencies. `load` selects
startup; `load_scene` uses an explicit admitted path without changing manifest bytes or settings
lineage. Rename startup-only data getters to Scene getters rather than preserving misleading
compatibility aliases. Keep startup attestation as a distinct proof available only for the actual
startup selection. Source/closure facts retain exact root, manifest, schema and member authority.

Produce a separately named delivery receipt from the sorted union of startup and declared roots.
Use the same loader/traversal for every root. Load and validate sequentially, retain only bounded
source facts after each snapshot is dropped, and keep the existing two-generation budget for
active plus prepared content. Charge union work and retained facts to the shared content budget.
Identical path/kind/digest facts deduplicate; conflicting facts reject. Export reopens these exact
members and does not discover dependencies or include arbitrary unused content. Build and startup
agreements remain distinct from the delivery union.

Acceptance: independent public consumer loads different roots under the same authority; failed
loads preserve prior snapshots; source-path/digest/schema mismatches reject; budgets release after
the owning snapshots/receipts retire. Export includes the second Scene's transitive Prefab/Image/
TileSet/metadata dependencies, excludes unused content, rejects changes before publication, and
loads both roots from the published tree under a different working directory.

### P2: One Committed Level Owner And Travel

Unify the currently committed source retention and Scene-instance receipt under one owner.
Startup is its first installation. Prepare the next admitted source without mutating the active
level, then commit at the existing owner/simulation safe point. Use the current protected Scene
transaction for lifecycle-free data and explicit product resources; do not allow arbitrary native
effects inside a rollback promise. Retry uses the committed level's source, not the original
startup source. Failure before commit retains the active level; later domain reconstruction has
an explicit degraded/retry result rather than pretending native rollback.

Retire the Host's decoded startup snapshot when its live ownership has transferred; retain
startup admission facts separately for Editor freshness comparisons. A legitimate current level B
must not make the Editor compare B's content revision against the original startup A. Products
must not maintain a second independently updated instance receipt for Retry.

Track decoded-generation lifetime with a shared, acyclic lease held by the snapshot, retained
Scene source and escaped Image values. Source-only closure/delivery facts do not retain it.
Old render packets can legitimately keep A alive after B commits: preparation of C must respect
the same two-generation ceiling until those consumers retire. Do not make Image retention hold
the containing snapshot or silently expand the generation budget. Scene replacement and resource
residency are distinct contracts; preflight identity/dependency changes and explicitly retire
only the old level's unshared resources under their existing owners.

Implement this in three cohesive slices: first the current Scene source/receipt owner, acyclic
generation retention and Reference Game Retry migration; then concrete project-content residency
preflight/commit/retirement; finally owned background preparation and the two-level consumer.
Keep data-only replacement usable without importing the filesystem or a graphics backend. Do not
expose full hosted travel after only the first slice, when destination resources cannot yet be
installed and retired correctly.

The residency slice uses one staged AssetServer for component decoding and destination resource
bindings. A narrow nominal Scene asset-bindings input may carry an otherwise empty component batch;
it must not accept unrelated staged component writes or expose scratch entities across calls.
Prepare Image versions, exact owned catalog changes, TileSet publication/retirement and aggregate
retention before the existing Scene commit. The protected exclusive operation then has no recoverable
resource-publication failure after the Scene changes. Do not implement World rollback or copy the
startup publisher's incremental reconciliation into travel. Native GPU/domain reconstruction keeps
its separate accepted-but-degraded semantics.

Keep a concrete current-level residency receipt, not a universal asset refcount service. Retire
only publications still matching that receipt, preserve foreign catalog records, and keep historical
AssetServer identity bindings. Empty destinations still retire old content. Shared identities may
keep their handles, but publish the destination's admitted decoded Image values so the current level
does not indefinitely retain its predecessor's generation. Preserve TileSet atlas-layout barriers;
old packets retain their own immutable values until physical retirement.

The owned preparation entry must use the original Host authority and shared loader budget, admit
only startup or declared delivery roots, and expose the prepared target read-only before commit.
Games validate logical checkpoint/progress references and build ordinary owned product state there,
then reuse the protected overlay writer. Do not resolve a second independent plan in gameplay or
convert project content into direct sources to bypass residency. Preparation completion is distinct
from playable reconstruction; deletion facts and dynamic objects become usable only after the
consumer's finite reconstruction succeeds.

Use current tasks only where blocking preparation requires them. Reject stale project/runtime/
request results and preserve cancellation and physical retirement. Keep the logical level path,
source retention and actual Scene identity distinct. Release is proven by owner/resource/packet
retirement, not by copied handle counts.

Acceptance: A -> B -> A; rejected B keeps A; Retry in B restarts B; repeated travel does not leak
identities or source budgets; two instances of one source remain distinct; pending preparation
across runtime close cannot publish late. Exercise actual product Hosts and ordinary game controls.

### P3: Versioned Game-Owned Save And Restore

Build the first format in the game consumer before extracting a general persistence interface.
Use ordinary private gameplay and save DTO/I/O modules behind a thin library and the supported
headless/desktop entries; existing local Cargo crates remain available when real reuse warrants
them. Run save/travel acceptance through exported headless arguments rather than an unpackaged
third probe binary. Test a deep gameplay-module change through Cargo without overstating the
separate closed five-file build-input observation as a complete source snapshot.

Capture an explicit selection of level identity, progress/resources, persistent dynamic entities
and deletion facts. Use durable identities and versioned migration. Do not serialize an ambient
World, runtime Entity values, task state or backend handles; do not infer save eligibility from
Scene/Inspector metadata or add an unproven universal derive flag.

Restore validates data and required content before committing game state. Native/resource state
is reconstructed from logical data under its existing owner. Preserve a usable current game on
preparation rejection and report reconstruction failures explicitly. Save I/O uses existing local
publication/conflict contracts, with application-owned location and bounded data.

Acceptance: save in B, terminate, restore in a fresh process, return to A, retain picked/deleted
objects and progress; cover two source instances, dynamic identity, old-format migration, corrupt
or missing content and interrupted/conflicting writes. Exported runs use the same game interfaces.

## November 13 marker reconstruction recovery

The existing two-level consumer exposes `Degraded` after a committed marker loses its runtime
representation or is legally retired. Retry previously reused live markers without rebuilding
their components, and retained dead tokens when allocating replacements. Both paths therefore
remained degraded without any failed Scene preparation to retry.

Repair this consumer's existing reconstruction owner. Validate each retained token's original
World identity domain and exact runtime-only live identity before reuse. Remove physically absent
owned tokens, reject ambiguous live identities, and restore checkpoint-backed Transform/Sprite
values and root placement through the existing identity and hierarchy writers. Keep one bounded
owned token set and validate the resulting all-World marker set. A missing authored Scene object
or native reconstruction failure retains its explicit failure behavior.
Later repair failure may retain partial game-owned component/entity/hierarchy effects and returns
the caller to `Degraded`; this consumer does not promise rollback.

Two complete managed HeadlessRun regressions first create markers across Meadow, Cavern 0 and
Cavern 1, return to Cavern 0, then remove its marker representation or legally retire the marker.
They observe natural degradation and issue the real Retry command. Recovery must preserve the
exact original committed `CurrentScene` receipt/source, Progress and ActiveLevel; restore exact
components, game identity and root membership; retain stable running frames and an unrelated
runtime entity; then travel again and retire the old marker. No replacement Scene transaction,
second runtime owner, checkpoint mutation or reconstruction of other levels is admitted.

The pre-fix release-mode run `1264b200-a5ff-405d-824a-5cb94e1875e1` passes all 30 existing tests
and fails both new cases at Retry remaining `Degraded`. This is a combined existing-control and
RED run, not a separate pristine-source baseline. Native physics/audio reconstruction, general
rollback, arbitrary missing authored components and other platform execution remain open.

The corrected release-mode run `acd1f0dc-0021-40c1-a41f-3e55b2f9719a` passes 32/32 with no
skips: both recovery trajectories and all original save, fresh-process restore, travel and
retained-source controls. It uses Rust 1.95.0, locked offline dependencies, the original root
target, one build job and serial nextest execution on Windows. Independent review finds no
confirmed P1/P2; foreign-token refusal, Parent-to-Root repair and later repair failure remain
source-reviewed branches rather than separately executed scenarios. This is CPU/headless
evidence, not native desktop or cross-platform qualification.
All-target correctness Clippy and sample formatting checks pass. Existing dependency and
headless-entry warnings remain; no warning-free or desktop-feature qualification is claimed.

## Source-Free Two-Level Player Successor

Status: implemented at `bb02fed` and locally qualified on Windows x64, continuing from
`a3ee53e`. Carry the actual
Two-Level Game through the existing cooked
content, captured Cargo Player and whole-directory publication owners. The current source
entry and reconstruction Host remain controls. Do not substitute another toy game, hand-built
catalog, test-only executable or manually copied pack for the ordinary game-owned cooker and
Player path.

Preserve the project's `ProjectLocation` Scene identities. Assign explicit complete shipping
Scene Products in the finite game level catalog; both levels are delivery/travel roots. Source
travel keeps its declared authoring paths; processed travel selects its complete Products from
the actual committed Scene representation. Use the existing Image importer, verified groups,
Schema compiler, reference resolver, Runtime Image codec and deep catalog validation. Do not
add another cooker, package format, loader, Scene or native-process authority.

The cooker and Player use one fixed game recipe with explicit Runtime Image codec selection.
The existing configured `RunOptions` values participate in the plugin-plan fingerprint, so a
Player must not reinterpret a fixed catalog with a different Save/Restore recipe. Admit its
finite launch intent through the original gameplay queue at the first fixed tick only, before
ordinary gameplay. Keep the original Storage/SaveSlot and close participant when opening the
explicit application save path; reject malformed, duplicate, late or incompatible intent.
Do not weaken configuration fingerprinting or expand the exact reconstruction inventory.

Generate the original independently expected bootstrap before building the Player, declare it
and the actual game/build inputs in Cargo's existing source inventory, retain content/source
observations through build and publication, and handle original rejected/orphaned/unresolved
package custody. A normal build without a generated expectation must refuse to run as a cooked
Player. No command-line/manifest override may authorize the opened pack. Reuse the original
root target, locked offline Rust1.95, one build job and serial Cargo.

Qualify the production cooker against both actual levels and Image assets. Publish and relocate
the exact three-file package. Remove only the test's owned copied source/cache/build-input tree,
run from an unrelated empty directory with Cargo/rustc unavailable, and execute the existing
Travel and Save trajectories. A fresh process restores the same checkpoint, independent level
instances, pickup progress and dynamic marker identities before returning to Meadow. Assert
actual processed resource content and original Scene receipts, not only decoded save DTOs.
Corrupt the later Cavern payload after a valid publication: startup must remain possible, the
actual travel must reject and preserve the original Meadow state, and no source fallback may
occur. Reject a foreign/corrupt catalog at startup and retain actual native exit/cleanup evidence.

Keep product launch policy, gameplay, checkpoint compatibility and finite packaging inventory
in this sample. The existing public production owners remain authoritative. Independently review
the launch/travel adapter, content/code fences and actual-process acceptance; run owning sample
and affected original processed/package controls. Record exact failures, platform/toolchain and
dependency scope before local Conventional Commits. This headless native delivery does not
qualify desktop presentation, server stripping, processed exact-state reconstruction, foreign
execution or full R7/R10/R12/Foundation. Editor/Dialogue product work remains paused.

### Cooked Player Implementation And Initial Controls

The sample now supplies ordinary `cook` and `player` binaries and one fixed `cooked_recipe`.
Its Image codec is explicitly selected as `two-level-game.cooked-image` revision one. The
shipping primary Scene Products are Meadow `624ebd5d-b7b1-4da9-94f8-86d689caeac1` and
Cavern `0c9fa5da-20ef-4c87-a559-c733bc231350`; original Image metadata and `ProjectLocation`
Scene ownership remain unchanged. The assembler consumes the actual saved Scene envelopes,
native Schema and real PNG imports, with exactly two Scene and two Image members. No engine
source, dependency or lockfile changes are required at this checkpoint.

Configured `RunOptions` do enter the original plugin-plan fingerprint; that comparison is not
relaxed. A versioned first-tick gameplay submission selects the finite Player scenario and
explicit application save path after fixed-recipe startup. It uses the original Storage Arc,
SaveSlot and registered Runtime close participant. Exact-state reconstruction remains source-only
and rejects this unrepresented processed placement. The source entry and development Host keep
their existing configurable recipes and command/lifecycle owners.

The first production library check passes. Original library controls, including the new launch
cases, pass 80/80 in `d5af7dc3-0fd0-4de6-8377-209a7c227c39` (5.380 seconds). Independent
review then identifies a real mixed-command rejection flaw: a late launch returns before normal
same-batch releases and saves. RED `9155fbd0-f56f-41e9-bb55-ddca429c2615` fails the new real
Headless trajectory with held movement still true, a changed second-tick pose and no save. The
receiver now rejects only launch intent, continues ordinary commands and preserves the launch
error after the batch. That regression passes in the first full package run.

The first full package run `d9bc2910-4cb0-40ff-af65-8b786e4ca9bd` passes 133/135, with four
actual-build/private-worker cases outside ordinary selection. Its two new fence tests correctly
expose comparison of identities from different capability sessions. The correction uses the
original live-directory `same_object` operation, then opens and compares file identities within
the retained original session. Code, build script and generated bootstrap identities/digests and
the exact code inventory remain checked through publication. Rebinding code does not replace
the earlier content baseline. The complete final selected-source run remains required.

Source review separately corrects publication failure reporting, unreaped Cargo custody and
Player output. An orphan now reports its exact original staging name/identity and cleanup
failure for explicit recovery; no guessed recursive cleanup occurs. Pending Cargo cleanup keeps
the original child and target lease until actual reaping, with a notice rather than a false
timeout completion. Player diagnostics and result writes use fallible `Write`, so a broken pipe
cannot replace an already captured original panic. The current resident Image set is observed
from actual `Assets<ImageAsset>` on every observation, separately from historical pixel proofs.
Independent review of the game adapter, cooker, Player and actual-process acceptance closes its
reported P1/P2 findings; source review is not actual generated-Player execution evidence.

The ordinary fallback Player compiles without a generated bootstrap and refuses execution.
The real generated branch is included only after the cooker writes its original expected bytes;
using `include!` permits formatting a clean source checkout without inventing placeholder
constants. `build.rs` is now part of the original development source fence, copied actual-build
workspace and declared Cargo inventories. Final native delivery, generated-branch compilation,
remaining checks and actual source/platform limits are still pending at this checkpoint.

### Cooked Player Native Qualification

The final ordinary selected package passes 135/135 in
`46b86a59-a52c-4fff-b9c8-69b2fdcc1270` (7.617 seconds), with the actual-build and private-worker
cases outside default selection. It retains all source gameplay, checkpoint, exact resume,
development Host and command/protocol controls alongside the new launch, source-fence and
fallback-Player checks. Broken stdout/stderr writes are exercised through the actual output
writer functions. These CPU results do not compile or execute the conditional generated Player.

The first actual production delivery run `89f5492f-f4e3-464e-bdb0-f3d021b72f13` fails before
Cargo: the Image importer receives the project root instead of the original assets directory
capability and correctly refuses absent metadata. The correction opens the admitted assets
subdirectory for physical import while retaining the unchanged logical `assets` scope and
original project-root content observations. No importer fallback or relaxed identity is added.

Actual release run `fca444e5-b81b-4847-b4f7-6c1035658d33` then passes 1/1 in 616.083 seconds,
including the initial explicit-target dependency/Player build. This executes the production
cooker and genuinely compiles the generated Player branch. It verifies exact captured code,
bootstrap/catalog/pack digests, the four-member closure and original three-file publication,
relocates the directory, then deletes only the checked owned source/cache/build-input copy.
The original engine source and shared target remain. Normal process, containment, output-reader
and drained-output receipts pass with no forced kill.

Six fresh sealed Player processes cover ordinary startup, complete Travel, Save, fresh Restore,
postpublication Cavern corruption and independent catalog corruption. Their actual executable
location supplies the pack, an unrelated working directory stays empty and an invalid PATH
makes Cargo/rustc unavailable. Actual processed Scene representations preserve the original
`ProjectLocation` source universe, six live entity receipts and level transitions. Both Image
Products retain their original 16x16 sRGB RGBA bytes and complete hashes; current residency is
distinct from the historical Image proofs. Save/Restore checks 30 points, three pickups, marker
IDs 1/2/3 in distinct logical level instances, next ID four and continued return to Meadow.
The later corrupted Cavern rejects only its demand: Meadow's original receipt, ten points and
marker one remain. Catalog corruption refuses before a gameplay snapshot. Neither failure can
fall back to the removed source.

This is Windows x64, Rust1.95, locked offline release, jobs one and the original target. The
616-second acceptance duration includes compilation and all scenarios; it is not a gameplay
frame time, startup distribution or build-performance improvement. Owning all-target
`cook,cooked-player` correctness Clippy and format checks pass; Clippy's clean-checkout Player
selection is the fallback branch, while the actual delivery separately compiles and runs the
generated branch. Existing non-correctness/dependency warnings remain. The original source
development Host's actual edit/adoption regression also passes 1/1 in
`135f346f-659f-481a-801d-48898c524a3b` (306.235 seconds, including a cold dev-profile dependency
build). It retains real body/private-structure edits, probe disposal, canonical restore, source
drift and compile-failure preservation, route exchange and original physical retirement after
adding `build.rs` to its copied/frozen inventory. Its intentional compiler rejection remains
part of that passing control, not a failed acceptance run. Final diff review removes one
redundant identical launch-skip check; affected launch/level controls pass 11/11 in
`53cf1c98-dcff-41e7-a5f7-b0c8086ea564` (0.459 seconds). Final selected correctness Clippy,
formatting, added local links/headings, manifest/lock/YAML parsing and Git whitespace checks
pass. These overlapping source selections are recorded separately rather than summed into an
aggregate test count. All ten scoped research/implementation/review agents are closed.

CI's existing ordinary Two-Level headless step now enables `cook,cooked-player` to cover these
CPU/fallback contracts. The expensive actual-delivery test stays explicitly selected by its
documented filter; its private ignored worker is not a standalone test. Remote CI has not run.
No dependency/lock version, engine source, runtime format or accepted owner changes. Desktop
processed presentation, server stripping, processed exact resume, foreign platforms and full
R7/R10/R12/Foundation remain open. Paused Editor/Dialogue work is preserved.

## Source-Free Desktop Player Successor

Status: implemented at `4267672`, with native-cleanup corrections `1fa730d`/`b92fd2e`, and locally
qualified on Windows x64/DX12, continuing from `cf68a77`.
Extend the actual cooker/Player game delivery above to a separately selected native desktop
placement. Retain the qualified headless path. Select
headless versus desktop explicitly in the game-owned cooker; Cargo features alone must not
silently choose a content configuration or change the fixed headless recipe. Desktop content
uses the project's actual desktop profile and the existing native game input plugin, Scene
materialization, Winit session and external WGPU Host. Code-first keyboard declarations remain
game-owned executable configuration unless a consumed authored Input asset requires the
original immutable Input codec. Do not add a second mutable mapping owner.

Use one fixed desktop cooked recipe in cooker and Player, preserving all fingerprint, source,
bootstrap, exact Cargo selection and whole-directory publication checks. The Player opens only
its compiled expected pack. Start the original DesktopRun initially paused, complete genuine
native presentation/paused maintenance, submit any finite first-tick launch intent through the
existing port, then resume through the original control ticket. Ordinary mode remains running
until normal native close. An explicit absolute save slot supports the same K/L actions.
Scripted Travel/Save/Restore and first-presentation verification are finite optional entry modes;
they do not replace the ordinary playable path or authorize a source fallback.

An opt-in bounded stdout observation stream may report actual post-Host game state and matching
Runtime/App/render/window/surface/device presentation facts for external qualification. It is
not another mutation channel, mandatory per-frame production telemetry, screenshot or GPU
duration proof. Correlate original facts rather than label an arbitrary current snapshot as an
older presented frame. Preserve original failure payload and physically retire Runtime, native
window, presentation resources and pump before terminal success. Incomplete cleanup remains
explicit; no independent window/GPU/input manager or Editor UI is introduced.

Qualify a package built by the production cooker, relocated with only its three declared files
and no copied source/cache/Cargo/rustc at execution. On Windows, use matched actual process/
thread/window authority for native key Down/Up and normal Close. Drive the ordinary running
game through movement/pickup, marker, travel/instance, checkpoint Save and fresh-process Restore;
prove restored independent-instance progress and continued play. Restore any injected holds and
the previously observed foreground window during success/failure cleanup. Do not operate on
unmatched external windows. Require genuine presentations after distinguishable game changes,
both exact processed Images and Scene receipts, normal original process containment/readers/
output retirement, and original native window destruction. Explicit adapter selection must fail
when unavailable. Separate these facts from any existing offscreen/source replay pixel evidence.

Retain malformed/bootstrap/fingerprint/source-drift refusal and the original headless delivery
control. Independently review recipe/configuration, startup/control and physical acceptance,
then run focused serial CPU and affected native controls, correctness Clippy, formatting and
document checks. Record exact executed backend, adapter, platform and remaining limits before
local Conventional Commits. This advances native game delivery without completing foreign
platforms, server stripping, processed exact reconstruction, UI/Dialogue or full Foundation.

### Desktop Implementation And Initial Controls

The game-owned cooker now selects `--runtime headless|desktop` independently of its compiled
Cargo features. `cooked_recipe()` excludes the native input plugin in either feature closure;
`cooked_desktop_recipe()` retains the existing compiled keyboard bindings and the same Runtime
Image codec. The desktop selection uses `player_desktop`, the actual `desktop` project profile,
`cooked-player,desktop`, and content profile `two-level-desktop-shipping`. Incompatible window
settings and an authored startup Input selection are refused before cooking. The original
content/code fence, generated expectation, captured build and directory publication remain
the owners of package agreement.

The new desktop entry retains the original initially paused DesktopRun, genuine post-Host
presentation receipts, first-tick launch command and exact Resume ticket. Optional observations
join original Runtime/App/render/window/surface/backend identities. Ordinary interactive mode
retains startup and the most recent Scene receipts in a bounded history; explicit finite
verification keeps its complete bounded trajectory. Diagnostic exhaustion stops optional
Observation records while reserving Ready/Result output. Recoverable gameplay operation errors
remain observable without closing ordinary play. Terminal output follows original Runtime and
event-loop retirement, and an original failure payload survives later cleanup.

Initial no-desktop-feature selection controls pass **2/2**, run
`6bc1ac89-302c-446d-91db-fa653f8070f5`, with Rust1.95.0 release/locked/offline and jobs1. This is
recipe/selection evidence only. Desktop feature compilation, generated branch execution and
the full physical source-free Save/fresh-Restore workflow still require the following native
qualification; no new desktop success or performance claim is inferred from this result.

The desktop-enabled entry/selection controls pass **17/17**, run
`891bde44-f97e-4adc-9fc1-5a68d2ec8884`. These include actual clean-checkout refusal for both
ungenerated Players, explicit feature-independent headless selection, and count/byte observation
exhaustion with usable Ready/Result output. The initial release build took11m41s in this local
cache state; this is not an edit-to-result or runtime cost budget. The generated desktop branch
remains outside that compile scope until the production cooker builds it.

Independent review identified and corrected three ordinary-session defects before native
qualification: observer-enabled Scene history could end play at the seventeenth activation;
diagnostic limits could end continued movement; recoverable K/L refusal could close the game.
The revised physical trajectory includes a missing-save Load followed by normal movement and
20 additional E travels after fresh Restore, retaining unchanged stored checkpoint bytes.
The cleanup review also found that an empty process tree plus `force_killed=false` did not prove
absence of earlier native termination. The original process owner now records all termination
attempts and skips automatic residual-tree termination when containment is already empty. This
desktop acceptance requires `termination_requested=false` as well as its game/native-close facts.

The shared process/output/log/wake controls pass **34/34**, with903 unrelated tests filtered,
run `17d41301-9f3c-4d78-b0dc-62f21357ecd9`. The new actual contained-process cases distinguish a
naturally exited parent/descendant tree, a successful direct exit leaving a live descendant that
requires native termination, and explicit termination/retry without force-kill escalation. This
qualifies the receipt fact on Windows x64/Rust1.95.0; the retained Linux WNOWAIT contract is
source-reviewed here and is not new Linux execution evidence.

The desktop-enabled sample's complete ordinary all-target cohort passes **153/153**, with13
native/actual-build cases skipped, run `2ccdc432-6f9b-4a41-9d07-7d22ea9f8b52`. The first attempt
stopped at four test-helper `json!([false;4])` macro syntax errors before executing tests;
explicit four-boolean arrays preserve the same held-key assertions. The corrected run retains
checkpoint, source-change preservation, complete semantic resume and both ungenerated Player
controls. It compiles all current native wrappers, while their ignored actual workflows require
separate execution. No generated desktop or physical-play success is inferred from this cohort.

The first production desktop delivery run, `93bef06e-0143-47e4-b846-fffef4bbedfd`, compiled the
real generated desktop Player and published its three files before stopping at a package-test
configuration comparison. The catalog canonicalizes the capability set order, while the test
compared the manifest's original vector order. Comparing complete configurations after the same
public `resolve()` validator preserves every effective setting and set membership without giving
declaration order semantic authority. Original plugin/Schema/codec fingerprints and later native
checks remain unchanged. The failed run does not establish physical play or source-free execution.

The second run, `2bb3668a-b47c-4938-8bf6-49403134538a`, passed package checks and source/cache
removal, then started the sealed Player through a real native presentation and Ready. It stopped
when Windows denied the background test thread's direct `SetForegroundWindow` call before input.
A subsequent temporary foreground input-queue join still failed actual activation. That attempt
was removed; the helper instead follows the checked-in Winit Windows implementation's matched
Alt transition before requesting foreground. Physical Alt must start Up and return to Up, and
the target retains actual matching foreground PID/thread/HWND/title checks before every game key.
Failure/normal paths release injected holds and restore the originally observed foreground.
No game-command substitution, production focus-policy change or desktop configuration change
is used. Failed attempts remain failed acceptance; their cleanup receipts retain any actual
termination intervention rather than treating absence of force-kill escalation as graceful exit.

Subsequent native iterations preserve all physical assertions. The ordinary retry after an
expected missing-save Load clears the recoverable error through gameplay before pickup assertions;
the original error is not erased by the observer. A libtest worker prefix required an explicit
newline before the machine report. The full hardware Save/fresh-Restore/20-travel trajectory and
software first-present control then completed, but the final CWD check found the driver-created
`NVIDIA Corporation` directory. With normal user profile variables deliberately absent, the
driver had fallen back to the working directory. The Player command now explicitly receives
test-owned local and roaming application-data directories for native driver caches; its PATH
remains invalid, source/cache absent, and unrelated CWD must still stay empty. The immutable
three-file installation is checked separately. This does not add a game filesystem fallback.

The sample build script now watches the existing `src/bin/player` directory and additionally
watches `generated.rs` only while it exists. Tracking a missing file alone caused Cargo to rerun
the script and rebuild sample binaries on every invocation. Both clean-checkout refusal and
real cooker-generated selection remain the acceptance for these cfg branches. The local change
does not establish a generalized build-speed or edit-latency budget.

### Desktop Player Native Qualification

Final ordinary sample qualification passes **155/155**, with13 actual-build/native cases skipped,
run `10766283-8c10-4877-97ba-8022ccb548ef`. This reruns the full affected cohort after the original
launch-result observation was added, including exact resume and both ungenerated-entry refusal
paths. The independently locked public process consumer passes **3/3**, run
`5d1d5ad5-25c3-4b14-af4f-c08127105ff6`: delayed protocol bytes survive diagnostic overflow,
exact handle-based product execution retains its sealed environment/EOF stdin, and original
exit/containment/readers/output cleanup now also requires no native termination request.

Final startup qualification also distinguishes ingress admission from application. The original
gameplay consumer retains its first launch result as a read-only diagnostic in the existing
Session; `player_launch_result(&World)` does not create mutation or resume authority. The desktop
entry joins this actual result at the first positive fixed tick, while Ready still requires the
original Resume ticket and initialized running state. It does not infer launch failure from a
generic snapshot error: an ordinary Save can be refused in the same first-tick batch. CPU controls
cover no-launch action refusal and preserving the first applied result after a later rejected
launch. The final native trajectory adds invalid initial save-path rejection and separately runs
the entry's finite Travel/Save/Restore modes, retaining all ordinary physical assertions below.

The complete production desktop delivery passes **1/1**, with the private worker excluded,
run `d1213e73-40d4-4f8e-bf10-3dba1b32a75a`, completing on November16 local time. Rust1.95.0,
release/locked/offline, jobs1 and the original root target are retained. The74.688-second actual
test includes its inner Cargo work; it is not a runtime or edit-latency measurement.

The actual cooker generates the desktop expectation and builds `player_desktop` with exactly
`cooked-player,desktop`. Package configuration is compared after full public validation, including
the desktop Window960×540 and WorldCanvas320×180, no authored startup Input, plugin/Schema/codec
fingerprints and original Scene/Image Products. The three-file package is relocated; only the
checked copied source/import-cache trees are removed. All execution uses unrelated CWD and invalid
PATH with no source or Rust tools; explicit test-owned local/roaming/common data directories keep
native driver caches out of both CWD and installation. The NVIDIA cache requires the common-data
projection on this machine. CWD remains empty and package bytes remain unchanged at completion.

Two ordinary Players execute on **NVIDIA GeForce RTX4090 / DX12 / discrete-gpu**. Physical scan-code
Down/Up events are accepted only while the exact process/thread/HWND/title is foreground. Missing
Load remains recoverable, Retry clears the error through gameplay, D moves and collects each near
pickup, release preserves idle position, and M allocates markers1/2/3 across Meadow0/Cavern0/
Cavern1. Q/E/R retain independently reconstructed progress and K publishes the actual checkpoint:
score30, three distinct collected facts, three exact recorded marker positions and next ID4.
The first original native window closes normally. A fresh process begins at Meadow score0; L
restores Cavern0, M allocates4, Q/E recover the other instance/level markers, then20 extra E travels
prove bounded history does not cap ordinary play. The stored checkpoint remains byte-for-byte
unchanged throughout continued play. Current Scene and historical receipts remain live six-entity
Runtime representations with original ProjectLocation identity; both complete1024-byte sRGB CPU
Images match their independent source golden hashes, and resident Images follow the actual level.

A third finite `--verify-first-present` process selects **Microsoft Basic Render Driver / DX12 /
cpu** with explicit fallback. These three processes observe112/148/6 genuine presented
frames respectively; those counts are fixture facts, not a frame-rate budget. Every original
Runtime and event pump retires, its HWND is destroyed, and the shared native owner reports exit0,
empty containment, stopped readers, drained output, no termination request and no force-kill.
Matched injected holds are released and the original foreground is restored. Optional observations
retain matching Runtime/App/render/window/surface/device identities and distinguish bounded history
sampling from complete finite history.

The same final run executes three additional hardware-backed finite Players: `--verify-travel`,
`--verify-save` and a fresh `--verify-restore` all complete the actual scripted progress at score30,
both Images, expected current-level markers and initialized/verified running state before normal
retirement. A seventh actual Player uses an invalid absolute initial save path: its original
launch is admitted but rejected by gameplay at tick1, `launch_accepted=false` and `ready=false`,
and exit1 is expected after complete native cleanup. All seven process receipts require empty
containment, stopped readers, drained output, original window destruction and no termination
request or force-kill. The earlier three-process native result
`74487dc3-1a02-49e5-a23c-fa69e23fd785` is superseded by this full final source/entry scope.

This is actual source-free native presentation and keyboard-gameplay evidence. It does not
establish frontbuffer pixels, physical scanout, GPU duration, numeric performance budgets, Vulkan,
Linux/macOS, authored Input products, processed exact-resume, server stripping, Editor/Dialogue
integration or complete R7/R10/R12/Foundation. The existing source/offscreen pixel proofs keep
their separate scope. Independent recipe, entry and physical/cleanup reviews are closed after
the corrections above. Final static and headless closure follows below.

The original actual headless delivery also passes **1/1**, run
`c6448aa2-c4d4-4493-8458-043d18f4fd4f`, through the same **desktop-enabled cooker** and its
default headless runtime selection. Its original compiled Player, three-file relocation,
source/cache removal, six sealed processes, Travel/Save/fresh Restore, Deferred corruption
preservation, catalog refusal and cleanup assertions are retained. The obsolete test-only
prohibition on a desktop-enabled cooker is removed because placement is now explicit; package
selection and fingerprint checks remain. This277.267-second test includes the newly compiled
inner feature closure and establishes no cross-cache compilation performance comparison.

The final headless delivery is repeated after the launch-result observation change and passes
**1/1**, run `2bfe241b-723f-4b68-b9d1-fee961c7e53c`, retaining all six original isolated Players.
Root library correctness Clippy with `runtime-2d,serde,project-tooling` and sample all-target
correctness Clippy with `cook,cooked-player,desktop` pass under Rust1.95.0/release/locked/offline.
Those lints run in the clean checkout's ungenerated cfg branch; actual generated branches are
compiled and executed by the production delivery tests. Existing non-correctness warnings remain.
Root/sample/independent-consumer `cargo fmt --check`, manifest parsing, local documentation links
and ordinary whitespace checks pass. No dependency/lock versions change. Local implementation
and qualification commits retain paused UI/Dialogue work separately.

A final full-installation digest check reuses the same physical trajectory and hashes every
executable/pack/manifest byte before and after all seven Players. Its first run,
`f12cc30b-111a-4af7-8473-0325f78110a5`, completed the worker and all Player assertions but refused
the outer cleanup receipt: Windows still reported a nonempty ancestor Job at the worker's exit
instant, so the supervisor attempted termination. The new receipt correctly exposed that
intervention instead of reporting graceful cleanup. The original owner at `b92fd2e` allows a fixed250ms
natural containment-drain observation after direct-child exit before terminating residuals.
Polling does not renew that deadline; genuine remaining descendants still enter the original
bounded terminate/escalation path. This race correction is qualified separately below; the
earlier successful seven-Player run remains evidence for its original source scope.

The corrected root process cohort passes **35/35**, with903 unrelated cases filtered, run
`b42372f2-dfbe-4acf-997d-6e924b175a8b`. Actual long-lived residuals are explicitly observed before
the fixed deadline, remain unterminated just before it, then require termination at the deadline;
the allowance does not renew. A separate short-lived descendant completes naturally with no
termination. That short-lived case does not synchronize first observation against the child's
exit and is not alone proof that the waiting branch executes. Independent reliability review
finds no P1/P2 in the selected correction. Final native/public-consumer closure follows below.

The final strengthened seven-Player desktop acceptance passes **1/1**, run
`7e93d11f-69e0-42ff-a9f0-14b9111c51e4`, after `b92fd2e`. It retains every physical, scripted,
startup-refusal and cleanup assertion above, adds full before/after digests for the executable,
pack and manifest, and preserves the empty unrelated CWD. All seven original native receipts and
the outer worker complete without termination intervention. The hardware/software adapter scope,
112/148/6 ordinary/finite presented-frame observations and unchanged stored checkpoint remain.
The297.665-second actual test includes inner recompilation after the process-owner correction;
it is not a runtime performance sample. This supersedes the earlier desktop runs for the final
source and complete installation-immutability scope.

The independently locked process consumer is repeated against `b92fd2e` and passes **3/3**, run
`8bd9c181-8a30-4c1f-b964-07cfa076f969`, retaining exact launch, sealed environment, complete protocol
output and no-intervention cleanup assertions. Its feature closure is compiled separately from
the root and game; the recorded3m51s build is a local cache fact, not runtime cost evidence.

The final headless production delivery also passes **1/1** after `b92fd2e`, run
`8c169ec6-adcb-4f4d-9573-d8d481ad84d2`. All six original isolated Players, generated headless
code/content agreement, corruption preservation, catalog refusal and cleanup remain intact under
the desktop-enabled cooker's default headless selection. Its278.535-second execution includes
the separate inner feature-closure build and does not establish a performance change.

Final root library and sample all-target correctness Clippy, followed by root/sample/independent
format checks, pass against `b92fd2e` after these actual delivery runs. Selected document links,
anchors, manifests/locks and whitespace checks pass. All12 scoped research, implementation and
review agents are closed. No dependency/lock versions, UI defaults or desktop display settings
change. Runtime delivery advances while foreign execution, numeric cost budgets, processed exact
reconstruction, server stripping and the remaining Foundation requirements stay open.

## Development Tools And Cooked Runtime Feature Closure

Status: implemented at `20d47a2` and qualified on Windows/Rust 1.95, starting from `b275ccb`.
Full R7/R10/R12/Foundation remains active. Continue the runtime-first priority without resuming
Editor, component or Dialogue development. The preceding source-free Player inherited
`nara/project-tooling`, the development Host and `notify` through the game's unconditional
dependencies. Source-free filesystem behavior alone did not establish an appropriate shipping
dependency or compiled-interface boundary; this unit corrects that game-owned selection.

Keep one game implementation and its original Cargo, package, command, Scene, save and native
retirement owners. Add an explicit development feature for the source executables, rebuild Host,
continuity transport and source watcher. Default source development remains convenient; the
cooker selects each Player with default features disabled, exactly as before. Cooking itself
requires Cargo tooling but does not require the rebuild Host or watcher. Headless cooked play
must not enable development/Cargo tooling, a source watcher, desktop platform, UI or GPU renderer;
desktop cooked play adds only its declared presentation placement. This does not promise to
remove all authoring/schema code from the root Runtime feature closure or establish a complete
dedicated-server product.

Migrate exact development build selection and resolved-feature checks together. A source Host
accepts its explicit development placement, never arbitrary extra features or a shipping Player.
Test harness tooling may build and supervise a Player without becoming part of that Player's
selected production dependency graph. Gate development-only tests and binaries by the same
feature rather than importing disabled interfaces or creating another protocol owner.

Acceptance retains the actual generated Player compilation, package publication, relocated
source/cache-free Travel, Save/fresh Restore and corruption/catalog refusal trajectories. Run
the existing headless and desktop delivery consumers serially, plus focused development
build/adoption regressions and ordinary default/minimal/cook/desktop compilation. Inspect Cargo's
selected normal/build dependency and feature graph for the exact Player selections; do not infer
closure from manifest text, binary strings or a reimplemented resolver. Record actual graph and
native evidence, scope limits and any fixture corrections. No performance, binary-size or
foreign-platform claim follows without its own measurement.

### Feature Closure Implementation And Qualification

The game now defaults to `development`, which enables its rebuild Host, finite continuity
transport, source entry points, Cargo tooling and optional `notify`. `cook` separately enables
the existing Cargo tools. `cooked-player` adds no development capability; `desktop` independently
selects the original window/render placement. The original cooker already disabled defaults for
its captured Player build and retains that exact selection. Development source builds now request
`development` or `desktop,development`; Host admission validates the same exact requested and
resolved lists and continues refusing other products, default-feature activation and extras.

On November 16, actual Cargo `tree --locked --offline --target x86_64-pc-windows-msvc
--no-default-features --edges normal,build --prefix none --format '{p}|{f}'` reports:

| Selected game features | Exact selected Nara features | Relevant exclusions |
| --- | --- | --- |
| `cooked-player` | `runtime-2d,runtime-core,serde` | No Cargo/tooling feature, watcher, Winit/WGPU backend or UI package |
| `cook` | `project-tooling,project-workflow,project-workflow-cargo-facts,runtime-2d,runtime-core,serde` | No development Host, watcher, Winit/WGPU backend or UI package |
| `cooked-player,desktop` | `desktop-winit,presentation-2d,render-wgpu,runtime-2d,runtime-core,serde` | No Cargo/tooling feature, watcher or UI package |

The existing finite process fixture runs that same Cargo query on each actual copied project
before its production delivery build. It retains the original bounded stdout, exact exit,
containment/readers/output cleanup and failure custody. It checks package identities and feature
sets, rather than interpreting the WGPU repository URL as a backend dependency: the CPU Material
compiler's Naga source intentionally remains. No lockfile version/source changes are required.

Minimal release production compilation and the final minimal headless ordinary cohort pass
119/119 with no skips, run `d9946383-06fc-423d-9d95-b245db441679`. The all-feature ordinary cohort
passes 188/188 with 16 separately selected ignored cases, run
`978731c0-805d-4dd4-88a5-2835663cf525`. Cook-only source and pure pack controls pass 7/7 with no
skips, run `21d4ae76-3ebb-4f76-b28c-f6f9becf7c05`; the shipping desktop all-target release check
also passes. The four pure corruption controls remain available without the development/native
harness. Initial broad test gating omitted them from the first minimal 115/115 run; narrowing
only the native harness gate restores that coverage before final qualification.
These are overlapping compile/test selections, not additive feature coverage. Minimal root code
retains existing source/schema and CPU Image/Material implementation warnings; this unit neither
hides them nor claims complete server-code stripping. Actual generated Player and rebuilt-source
native acceptance is recorded below.

The actual Windows/native cohort passes **4/4**, with eight cases outside the explicit selection,
run `499a58a0-f88d-4be3-83f9-641e28cb186f` (978.496 seconds including nested builds). Both copied
shipping projects pass their original-target normal/build Cargo-tree guard before generated
Player compilation. The headless package retains six sealed source/cache-free executions,
complete Travel, Save/fresh Restore, original Image/Scene facts, Deferred corruption preservation
and catalog refusal. The desktop package retains two ordinary physical-keyboard sessions and
three scripted controls on DX12/RTX 4090, a DX12/Microsoft Basic Render Driver first-present
control, invalid initial-path refusal and twenty additional ordinary travels. Original windows,
pumps and process containment/readers/output retire without native termination intervention;
installation/CWD/checkpoint immutability assertions remain unchanged.

The same cohort also passes actual Rust function-body/private-structure rebuilding and exact
adoption through both headless and continuous desktop DevelopmentHosts. The continuous child
reports Vulkan hardware, resumes ordinary running play after route exchange, retains stale
candidate/source refusal and closes while Running. The headless trajectory preserves its
intentional compile-error, complete probe, foreign/consumed ticket, source-drift and postcommit
predecessor-cleanup controls. Those two tests validate the changed requested/resolved development
feature selection rather than inheriting confidence from a manifest check. Their compilation
durations are fixture facts, not current edit-to-result performance samples.

All-target/all-feature development correctness Clippy and all-target minimal desktop shipping
correctness Clippy pass. Cook-only all-target checking, the Runtime workload/metrics consumer
compiled with `desktop,cost-measurement` and defaults disabled, root/sample formatting checks,
manifest/lock/workflow parsing, selected document links and whitespace checks pass. CI adds
ordinary minimal headless tests, cook-only source/pack tests and a minimal desktop compile gate;
remote CI and ignored native execution are not implied. Scoped production/custody and separate
test/usage reviews close without P1/P2, and all five unit agents are closed. No root/domain Rust,
dependency version/source or lockfile changes are made. Shipping feature closure is qualified
for this actual game; broader root authoring/schema stripping, server delivery, foreign native
execution, numeric size/performance and the remaining Foundation requirements stay open.

## Follow-Up Triggers

The first required physics/audio domain proves its own native acquisition, teardown and
reconstruction workflow before a portable provider abstraction. Multi-document Editor tooling,
Tile authoring, new job kernels, broad async traits, dynamic native ABI and browser execution
remain outside these units. Extract touched private responsibilities when implementation warrants
it, preserving one authoritative lifecycle rather than distributing its state across modules.

CI correctness continues independently. Observe hosted results at implementation boundaries;
do not idle waiting for the complete matrix or infer success from an earlier source. Cache/profile
changes remain measured work under the separate CI plan.

After P1-P3, audit compiler deprecations against the supported Rust toolchain and inspect the
actual enabled dependency graph of `bevy_ecs`, `bevy_tasks` and `nara_tasks`. Measure build and
runtime costs and distinguish ECS-internal scheduling from product task custody before selecting
reuse, removal or a narrower implementation. In particular, investigate the reported atomic
`fetch_update` to `try_update` rename. Coexisting package names alone do not prove duplicate
execution or justify replacing a scheduler.

The [preliminary dependency and deprecation study](../research/2026-10-21-task-dependency-and-deprecation-preliminary.md)
records the supported rename and enabled Bevy feature graph. It does not establish actual duplicate
worker pools or select a replacement; measure those costs after the complete consumer is qualified.

## Sources

- [Strategy](../../STRATEGY.md)
- [Module/goal review](../audits/2026-10-20-module-goal-alignment-review.md)
- [ADR 0110](../architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md)
- [ADR 0037](../architecture/adr/0037-asset-load-request-cache-and-lifetime-policy.md)
- [ADR 0027](../architecture/adr/0027-save-game-and-runtime-persistence.md)
- [Proposed ADR 0089](../architecture/adr/0089-runtime-scene-instance-loading-activation-unload-and-travel.md)
- [CI repair](2026-10-20-ci-portability-repair-plan.md)
