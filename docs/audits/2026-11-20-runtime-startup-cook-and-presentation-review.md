# Runtime startup, Cook and presentation review

Two engine defects block supported workflows. A project using the default runtime preset passes Check but cannot start even with zero gameplay commands. A typed importer can produce a valid self-contained asset that Project Cook cannot package. Five further engine findings concern paused capture, source path spelling, TileSet retirement, UI pointer routing and Scene View picking.

This fourth review started at `a49de6a` and checked the affected source paths again at `a73a220`. The [previous review](2026-11-19-engine-consumer-contract-review.md#repair-status) records repairs for all thirteen of its findings. Twenty-one related Windows regression tests passed individually in this review. Two further tests passed for the new stored Prefab property queries. The measurements and game-specific findings below have their own scope.

## P1 engine findings

### The default runtime preset passes Check and cannot start

Use the repository's real-file `TestProject::with_prefab_startup`, with its Scene, Prefab, Image, `runtime-2d` capability and built-in schema. Start a `HeadlessSession` with `ProductRecipe::new()` and zero submitted commands. The following four runs differ only in preset or plugin selection. [Fixture](../../tests/support/project_content_fixture.rs#L40) · [Default preset](../../crates/nara_project/src/capability.rs#L165) · [Preset selection](../../src/product/profile.rs#L113)

| Manifest preset | Extra gameplay plugin | Project Check | Start and first frame |
| --- | --- | --- | --- |
| Omitted | No | Passed | Start failed with `project.run.command-queue-missing` |
| `minimal` | No | Passed | Same failure |
| `minimal` | Yes | Passed | Both passed |
| `local-headless` | No | Passed | Both passed |

`materialize_runtime` requires `GameplayCommandQueue` before allocating the Scene and acquires it again to iterate the empty input list. Preserve a command-free startup path. Require the queue when the Host has commands to deliver or the selected gameplay plugin owes the World that resource. Keep the regression for a selected plugin whose queue disappears during setup. Bevy's minimal plugin group also supplies a runnable schedule independently of gameplay input. [Startup check and submission](../../src/project_host/runtime.rs#L1799) · [Selected-plugin failure control](../../src/project_host/runtime/tests.rs#L7572) · [Accepted manifest presets](../architecture/adr/0035-project-manifest-and-runtime-settings-authority.md#L198) · [Bevy minimal group](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_internal/src/default_plugins.rs#L165)

### Cook forces a source Image into a pack after the importer has baked it into another Product

A Scene uses a typed badge asset. Its source importer reads a 1×1 PNG. The badge can retain a Runtime Image dependency or embed the PNG bytes. Both configurations passed public Check, built once and then hit the cache. The linked badge cooked successfully with three members. The embedded badge's codec reported zero dependencies, but Project Cook still added the Image and failed with `UnreachableMembers { reachable: 2, total: 3 }`. [Image inclusion](../../src/project_tooling/project_cook/content.rs#L353) · [Contribution order](../../src/project_tooling/project_cook/content.rs#L548) · [Reachability check](../../src/project_workflow/cooked_content.rs#L776)

Packaging the same Scene and verified badge group as two members passed deep validation. A fresh headless Player ran a frame and its World contained the exact 70-byte PNG. Editing the PNG produced `RawInputChanged` and a rebuilt badge with the new bytes. Select final pack members by walking the built Products' runtime dependencies from the Scene and global input roots. Keep the PNG in the build observations and pre-publication revalidation. Preserve strict rejection of truly unreachable pack members. [Typed build contract](../architecture/adr/0103-concrete-domain-owned-asset-import-execution.md#L278) · [Cook contribution API](../../src/project_tooling/project_cook/content.rs#L139)

## P2 engine findings

### A fault during paused capture can still return a successful checkpoint value

Start and pause a healthy `local-headless` Session. In the capture callback, synchronize with a worker holding the original `RuntimeFaultReporter`. Have the worker publish `RequiredService` and finish before the callback returns `42`. `HeadlessSession::capture` returned `Ok(42)` although `state()` was already `Faulted`. The session first exposed the diagnostic on close, which returned `Failed`. Reporting immediately before capture returned `Err(NotPaused)`. With no fault, capture succeeded and close completed. A game could persist the successful value before calling close. [Capture gate](../../src/project_host/runtime/action/session.rs#L280) · [World observation checks](../../src/project_host/runtime.rs#L772) · [Game capture consumer](../../samples/two-level-game/src/gameplay/resume/capture.rs#L113)

Check the original Runtime's fault on entry and after the callback, alongside the existing schema checks. Preserve and report the original fault. Exercise an actual worker reporting between those checks and verify that a healthy completed paused frame still yields its value. The command-submission path already accounts for faults arriving during an operation. [Command result handling](../../src/project_host/runtime/action/session.rs#L364) · [Paused capture contract](../architecture/adr/0076-play-runtime-debug-control-and-observation.md#L350)

### Windows Check accepts auxiliary file spelling that Build and Cook reject

Rename the Two-Level Game's actual `pickup.wav` to `Pickup.wav`, leaving the sound bank's spelling as `audio/pickup.wav`. Check passed and source import returned the same 5,760 PCM frames as before. Both a fresh build and a warm-cache replay failed with `ExactSpellingMismatch { operation: OpenFile }`. Matching the descriptor spelling to the disk name restored a successful Cook with identical decoded samples. [Sound Bank dependency](../../samples/two-level-game/src/sound_bank/source.rs#L78) · [Auxiliary source read](../../src/project_content/domains.rs#L730) · [Source open](../../src/project_content.rs#L2407) · [Build open](../../src/asset_build/execute.rs#L371)

Using both spellings for the same WAV also gave the default and desktop Check receipts two paths with equal digests. Creating the second file through Export's staging operation then returned `AlreadyExists`. This control reached staged file creation. Make the source reader verify exact spelling when it first records an auxiliary dependency, and retain that error's path through Check. Cover one wrong spelling and two aliases of the same file. [Build replay](../../src/asset_build/execute.rs#L616) · [Export staging](../../src/project_workflow/export/stage.rs#L141)

### A revoked TileSet continues to produce new draw batches

Give a Sprite and a Tilemap the same Image. Remove only the TileSet's catalog record on frame 2. Without a source notification the TileSet becomes `Removed` and the map disappears. Also deliver the public `AssetSourceChanges::removed` notification and the state becomes `Failed("asset.reload-record-missing")`, with no usable version or current publication receipt. Nevertheless, each of frames 2–5 produced a **new Tilemap batch** referencing the old pixels. The independent Sprite remained correct in both runs. [Reload invalidation](../../crates/nara_asset/src/reload/resolution.rs#L468) · [TileSet reconciliation](../../crates/nara_tilemap/src/content.rs#L806) · [Extraction](../../crates/nara_sprite_render/src/extract.rs#L98)

The invalidated state revision makes the TileSet owner yield to a possible other writer. Extraction then accepts the surviving typed value and checks its atlas Image. Require a usable publication for the TileSet itself when extracting managed content. Preserve compatible last-good sampling when an actual usable TileSet version exists, and test an independent Sprite sharing its Image. The probe observed stock CPU batches after injected notification. [TileSet publication contract](../architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md#L271)

### Moving onto a disabled button can leave the previous button hovered and actionable

Move from enabled Play at `(40,30)` to disabled Retry at `(140,30)`. The stock Runtime continued to paint Play's hover color. Vulkan readback at `(16,44)` remained `[25,184,209,255]`. Moving into empty space or losing window focus restored `[255,0,0,255]`. On the public UI-kernel route, sending `Pressed(None)` and `Released(None)` after the move focused and activated **Play**. Giving the events Retry's coordinates produced zero actions, as did the ordinary Runtime click on Retry. With Retry enabled, the position-free route activated Retry. [Pointer motion](../../crates/nara_ui_kernel/src/interaction/pointer.rs#L79) · [Rejected dispatch](../../crates/nara_ui_kernel/src/interaction/session.rs#L219) · [Coordinate fallback](../../crates/nara_ui_kernel/src/interaction/session.rs#L507)

Commit the observed position and exit from the old hover target even when the new control cannot activate. Keep disabled activation refused. Regression coverage should assert hover pixels, focus and commands separately for position-bearing and position-free events. Godot records the new viewport position before updating the hover hierarchy. [Godot pointer position](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/scene/main/viewport.cpp#L3311) · [Godot hover change](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/scene/main/viewport.cpp#L3366)

### Scene View can draw an opaque pixel that its own picker considers degenerate

At 4096×2160, a 1×1-pixel marker produced an opaque red pixel at `(2048,1080)`. Picking its center returned `None`. Widen it to 1.5 pixels and the same red pixel selects the marker. At 1920×1080 an 11-pixel-wide, 1/64-pixel-high marker painted eleven red pixels, and all eleven center picks missed. Doubling only its height gave byte-identical pixels in the sampled 32×3 rectangle and eleven successful picks. These were Vulkan offscreen readbacks on an RTX 4090. [Geometry predicate](../../src/project_host/runtime/editor/preview.rs#L65) · [Editor pick](../../src/project_host/runtime/editor/presentation.rs#L267)

The 1×1 marker's clip-space determinant was `1.13028e-7`. The fixed threshold is `f32::EPSILON`, about `1.19209e-7`, so preview resolution alone can exclude a visible quad. Evaluate the captured axes in double precision and reject non-invertible geometry. Retain the actual rasterized-pixel control: the current CPU fake applies the same threshold as the picker and would hide the discrepancy. [CPU fake](../../tests/support/fake_preview_toolkit.rs#L125)

## Hierarchy cost and diagnostic cursors

**Ordered hierarchy edits scale with the width of the edited sibling list.** Inside a managed Update, place 32 transient entities at the end of the root list and remove them in reverse order. Compare 64 individually committed operations with two atomic batches of 32. Both paths restored the exact original root sequence. Each cell gives the range of medians from two Windows release runs, each using nine measured repetitions after one warm-up. Timing covers the writer operations only. [Single-operation entry](../../crates/nara_hierarchy/src/writer.rs#L353) · [Reference game projectile placement](../../reference-game/src/systems.rs#L1017) · [Projectile retirement](../../reference-game/src/systems.rs#L1326)

| Existing roots | Individual edits | Two batches |
| ---: | ---: | ---: |
| 65 | 0.133 ms | 0.011 ms |
| 1,025 | 1.222–1.231 ms | 0.044 ms |
| 8,193 | 12.463–12.687 ms | 0.387–0.407 ms |

Keeping the same 8,192 static entities under one unrelated parent reduced the individual-edits median to **0.055–0.056 ms**. The writer indexes every sibling, materializes the ordered vector and rebuilds root membership for each transaction. Batch same-tick projectile retirements where their ownership and failure semantics permit it. For producers that must commit immediately, an append/remove-last path can preflight the affected links and update the existing order in place. Preserve prior-order checks and failure atomicity. Bevy's relationship hook updates the target's existing collection directly. [Sibling index](../../crates/nara_hierarchy/src/writer.rs#L588) · [Vector materialization](../../crates/nara_hierarchy/src/writer.rs#L728) · [Membership rebuild](../../crates/nara_hierarchy/src/relation.rs#L61) · [Bevy relationship insertion](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_ecs/src/relationship/mod.rs#L199)

**Tracing cursor ownership matters across Runtime replacement.** A host kept its public `RuntimeTracingCursor` after observing three entries and closing the first Runtime. In a fresh Runtime, the retained cursor emitted `0, 0, 0, 1` for four successive new diagnostics. A cursor created from the new bus emitted `1, 1, 1, 1`. The bus restarts numbering at zero, and the cursor contains only the next sequence. This is a public integration hazard. Document the need to recreate cursors with their bus, or give them a bus identity and report a mismatch. The in-repository tracing tests use one cursor per bus. [Cursor representation](../../crates/nara_diagnostic/src/runtime.rs#L637) · [Sequence initialization](../../crates/nara_diagnostic/src/runtime.rs#L688) · [Emission](../../crates/nara_diagnostic/src/runtime.rs#L1017)

## Game and integration findings

- **P1 for the Two-Level Game's resume workflow.** Start the Meadow character in an accepted pose at `(-30,-12)` with rotation `0.01` radians. Move diagonally down and right in 20 ms fixed steps. Step 8 remained capturable. A replay of the step 9 sweep reported zero collision callbacks. The game wrote `(-12.836445,-32.380024)`, putting a corner 2.2531 world units through the inner wall. Continuing movement made steps 9–19 fail `capture_resume`. Releasing movement after step 9 left capture unavailable through step 36. At zero rotation all 36 steps could be captured. Validate the proposed endpoint with the same character shape and overlap predicate used by reconstruction. Retain a valid pose when the sweep result overlaps, and verify that sliding still makes progress. [Sweep](../../crates/nara_rapier2d/src/character.rs#L89) · [Writeback](../../samples/two-level-game/src/gameplay/collision.rs#L273) · [Resume acceptance](../../samples/two-level-game/src/gameplay/collision.rs#L291)
- **P2 in the independent authored-collision adapter.** Reparent a stationary floor between two identity-transform parents while 16 boxes sleep on it. The four world-space vertices remained bit-identical. On fixed step 181 the adapter recreated both native handles, contact count went `16 → 0 → 16` across reconciliation and solve, and all 16 boxes woke. Keeping the parent preserved contacts and sleep. Lowering the floor by four units correctly removed contacts after solve. For fixed bodies, compare effective world geometry and the collision definition when deciding whether to replace the native pair. The adapter lives in the motion execution fixture and has a separate rendering consumer. [Preservation predicate](../../tests/fixtures/motion-execution/physics/src/authored.rs#L192) · [Pair removal](../../tests/fixtures/motion-execution/physics/src/authored.rs#L280) · [Contact consumer](../../tests/fixtures/render-policy-consumer/tests/support/authored_collision/scene.rs#L482)
- **P2 in Collect Game movement.** The authored speed is specified in world units per second. With speed 200 and a 20 ms tick, one Right command moves four world units under an identity Prefab anchor. Scaling only the anchor by two makes it move eight. Rotating only the anchor by 90° turns the same command into four units along world Y. Compute the desired endpoint and bounds in world coordinates, then transform it into the effective parent's space for local writeback. Verify the result against the published global pose. [Speed contract](../../samples/collect-game/gameplay-plugin/src/lib.rs#L54) · [Movement](../../samples/collect-game/gameplay-plugin/src/lib.rs#L253) · [World-space observation](../../samples/collect-game/gameplay-plugin/src/lib.rs#L308)
- **P3 in Collect Game pickup detection.** Give `gem-left` a radius of one world unit and leave the shipped movement settings intact. At x=−40 the direct walk collects it. At x=−38 the player goes from −40 to −36 in one tick, passes through its center and leaves it visible and uncollected. Only the authored X coordinate changed. Evaluate the swept player segment against the pickup radius. Keep teleport and Reset separate from ordinary movement so a reset cannot collect every gem it passes. [Pickup test](../../samples/collect-game/gameplay-plugin/src/lib.rs#L281)

## Verification scope

Rust 1.95.0 on Windows x64. Public CPU and headless probes used temporary source projects and the shared build dependencies. The two presentation findings additionally used Vulkan offscreen readback. The TileSet probe reached freshly extracted CPU draw batches. The WAV alias check reached Export staging. Native window input, physical watcher delivery, complete Export publication for that alias case, and non-Windows behavior remain outside the evidence here.

The 21 repair controls covered indexed Undo, persisted floats, disabled hierarchy edges, Step retirement and resize, wide-integer Cook, stable Image/Material relocation, UI overflow, camera subviews, Inspector reference text, unchanged placement, TileSet sharing, projectile targeting, pickup opacity, additive unload, fixed-clock presentation and audio observation. The two newer Prefab query controls covered bounded pagination and continuing past an oversized header. All 23 ran from existing release test binaries. The review made no workspace-wide build or timing qualification.

## Repair status

### Runtime startup and paused capture

In `3861f45`, the Host requires a queue when the frozen plugin plan includes the gameplay command
plugin or when the caller supplies commands at startup. Source Projects with an omitted preset
and an explicit `minimal` preset both pass Check, start without the queue and complete a fixed
tick with the authored Prefab, Sprite and Image resident. Adding the plugin explicitly and using
`local-headless` each make the queue available. Commands without a queue still reject startup.
The existing control that removes an expected queue during plugin setup still rejects the
candidate and retires the acquired owners once. [Startup admission](../../src/project_host/runtime.rs)

Paused capture returns the Runtime's original fault diagnostic and discards the value when a
worker reports a fault during the callback. A subsequent capture with the same fault skips the
callback. The original session still physically closes and reports failure. A completed healthy
pause yields the requested value. [Public startup and capture regressions](../../tests/project_runtime_boot/admission.rs)

The pre-fix run `d0ea044e-84a3-44c6-8bc3-29487bf973e6` logged the missing queue and successful
`Ok(42)` before the test processes aborted on assertion failure. After the repair, Windows x64
Rust 1.95.0 nextest passed all 25 selected public boot and owner cleanup tests in
`cbe55dd9-88e7-44c5-afe1-2e5f44e1fe7e`.

### Runtime members and build observations

In `2727116`, Project Cook chooses the stock payloads required by the encoded Scenes, global
Input and verified artifact groups. Embedded image bytes remain in the typed Product, and the
source PNG still participates in cache invalidation and publication revalidation. Linked image
Products add the actual Image payload. The original builder still rejects unreachable explicit
members.

The independently locked consumer writes and reopens real packs and starts a fresh Headless
Player. It verifies two members for an embedded badge and three for a linked badge, checks the
actual decoded bytes or pixels, rejects the stale group after a same-length PNG change, and
rebuilds with `RawInputChanged`. Its two-root case delivers exactly five members and visits the
earlier Image source once more after a later build introduces the dependency. Each Scene's
contribution runs once. Both the complete delivery digest and ordinary startup stamp are checked
before adopting the second traversal's receipt. Temporary selection indexes and the first
delivery receipt release their budget before the phases that no longer need them.
[Independent consumer](../../tests/fixtures/module-consumer/asset-domain-consumer/tests/project_cook_runtime_selection.rs)

Windows x64 Rust 1.95.0 nextest `625c3202-4d05-495b-ac53-f20a403b4cd2` passes all 11 selected
tests across Project Cook, compound source workflows and processed Headless Host. The first
consumer attempt exposed a missing Image codec in the fixture. The final recipe explicitly
selects that codec and both linked cases pass through deep validation.

The final engine cohort passes 64/64 in `01553836-c8b7-435a-b20f-5d2c268e2ea8`, covering the
public Runtime boot and content delivery targets, the startup retirement control and the Cook
owners. Releasing the transient metadata charge lets final validation pass at the exact assembly
work limit. Setting the limit one byte lower still rejects final validation.
The Two-Level Game's `cook_runtime` and `cook_source` targets pass 37/37 in
`cac0a5f9-d2d0-4105-881f-83ef7ac3143a`. They cover shared atlas and animation delivery, typed
Material and TileSet payloads, source-free decoding, selected Schema, and source drift before
publication. The repair runs cover Windows Headless execution and pack I/O.

Scoped engine correctness Clippy, strict Clippy for the independent Cook test, and root and
consumer format checks pass. The three independent Cook cases pass again in
`3123949d-7b9b-41b9-bde7-d94053385fdf` after the fixture's scoped large-error lint allowance for
the original public error type.

### Two-Level Game rotated wall contact

The public Headless reproduction failed at the reported ninth step in
`a913648d-2c13-400e-8a52-b888a2d7cd21`. In `6d48055`, the game checks the exact candidate pose
with the same native shape intersection used for resume placement. On overlap it retries one
sweep with the inward component removed using the wall's native contact normal. If the retry
cannot be admitted, the last accepted pose is retained.

The repaired public sequence stays capturable for all 36 ticks, continues sliding and rounds
the south end of the wall. A second Runtime restored at step nine produces the same canonical
state on each later tick. The source gameplay, collision and recovery cohort passes 29/29 in
`f5ab2166-be91-4a87-87c8-559870848054`. The existing native DX12 reconstruction and the two
source-free Cook and Player controls pass in separate runs.
[Reproduction and repair evidence](../plans/2026-11-04-authored-collision-derivation-plan.md#rotated-character-pose-admission)

### Typed source file spelling

Windows run `ff7d4772-882c-4c69-bd5c-31f5ce5d6648` reproduced both incorrect Check receipts.
In `ddabcab`, the first read of a typed source path verifies its stored spelling through the
existing Assets capability. The typed source cache only holds verified paths. The public
diagnostic preserves the requested `assets/audio/pickup.wav` and the file operation on mismatch.

Check rejects the wrong pickup spelling and rejects a second alias after reading the correct
name. Correcting the input passes Check and Cook, and the cooked Sound Bank decoder produces
bit-identical PCM for both cues. The Two-Level cohort also covers source gameplay, collision and
resume, plus source-free Cook and Player controls.
[Sound Bank regression](../../samples/two-level-game/tests/cook_source.rs)

The original source reader now reserves handle and name-query work together and refunds the
handle charge when a subsequent encoded-read reservation is refused. The budget regression
exercises both refusal points and then reads the original file in the same context. The final
engine cohort passes 143/143 in `fd3e0484-a354-46e4-bf42-e5992bda6f88`. Two-Level passes 68/68 in
`e2f42a30-4d88-4f25-be8b-b2fdec88fb57`, and the independently locked asset-domain cohort passes
15/15 in `e8e11514-238a-45a3-9ccf-80f1a926df0a`. Scoped correctness Clippy passes for the
engine and sample targets. Root and sample formatting and changed document links pass.

### TileSet draw publication

Commit `34a5070` makes stock extraction check the usable TileSet version and its exact
published slot once per map before admitting its captured Image. Code-first TileSets without a
managed Image snapshot continue through their existing path. Earlier draw batches keep their
captured Image.
[Extraction](../../crates/nara_sprite_render/src/extract.rs)

The source regression removes the catalog record and sends the normal public removal event on the
same frame. The first Windows run, `9080d8b9-15f8-4e1a-abd3-279115f59b95`, found two batches
after revocation where only the Sprite should draw. The repaired run,
`9abdfa57-5b38-40f0-88e7-05cda3afce35`, passes 61/61. It observes four fresh frames with only
the Sprite and the original retained packet with both draws. A separate control refuses a
processed TileSet value copied into a foreign store while the original version reports usable.
The existing tests cover last-good rendering when a newer atlas fails or hits its retention budget.
[Source regression](../../crates/nara_sprite_render/tests/tileset_content.rs) ·
[Foreign slot regression](../../crates/nara_sprite_render/src/tests/source_refresh.rs)

The independently locked consumer passes 14/14 in `71836144-7fad-4bf9-93cb-e5cb9f2001f9`.
It covers source-free startup and travel, primary and auxiliary products, native resource
retirement, and retained captures. The WGPU offscreen readback passes 1/1 in
`5480e777-f67d-4ae9-9f68-1e3a0459855e`, comparing exact pixels for the primary and auxiliary
products after the original typed stores and owner retire. These runs use Windows x64 and Rust
1.95.0. Sprite renderer correctness Clippy and format checks pass.
[Independent TileSet consumer](../../tests/fixtures/module-consumer/tests/runtime_tileset_delivery.rs) ·
[WGPU readback](../../crates/nara_render_wgpu/src/readback_tests.rs)

Godot queues a TileMapLayer update when the selected TileSet changes. Bevy's render asset
cache retires prepared allocations on last use. Nara's current TileSet publication governs new
draws, and already captured packets retain their exact pixels.
[Godot TileSet change](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/modules/tilemap/tile_map_layer.cpp#L2008) ·
[Bevy render asset extraction](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_render/src/render_asset.rs#L331)

Independent source review identified two existing recovery gaps. A source notification can
supersede the owner's state revision and leave a retired atlas held in the typed slot. The
original claim can then block Scene replacement after the catalog record returns. A terminal
load attempt that preserves the old usable version can also make a later plain source request
lose last-good on dependency failure. The [recovery repair](#file-backed-tileset-recovery) below
exercises both paths through the original owners.
[Reconciliation](../../crates/nara_tilemap/src/content.rs) ·
[Scene replacement](../../src/project_host/runtime/content_residency/tilesets.rs)

### File-backed TileSet recovery

Commit `454d7c3` retains the preceding state revision when the resolver rejects a notification
before routing. Consecutive rejections preserve that origin. An independent state write ends
continuity. The TileSet owner can still honor its exact declaration claim when the resolver
changed the state and the original value slot still matches. A prepared mutation captures the
current exact revisions and checks them at commit.
[Resolver](../../crates/nara_asset/src/reload/resolution.rs) ·
[TileSet owner](../../crates/nara_tilemap/src/content.rs)

Windows run `018bcdf0-73a8-45b8-94e3-35bf52249856` reproduced the lost usable version after a
failed source notification and a subsequent request with an unavailable Image. The first combined
run, `90aa7e05-da3b-423f-a637-7a0532135cce`, reached 364/365. Its removal regression read the
frame-scoped diagnostic after a later frame cleared it. The corrected fixture observes each
frame. The final resource, TileSet and Sprite renderer cohort passes 368/368 in
`ab28cc38-86ac-4aa9-86dd-ba3305c8a40b`.
The regressions carry forward the authorized last-good atlas until its successor publishes. A
revoked value is retired at reconciliation. A freshly admitted request stays Loading while its
Image is pending, whether the old TileSet or its old Image was revoked. An explicit decode
rejection publishes its diagnostic after the resolver has suspended the earlier declaration.
[Public rendering regressions](../../crates/nara_sprite_render/tests/tileset_content.rs)

The Project Host and startup cohort passes 100/100 in
`14778edc-af32-4223-9345-46c10cfec356`. The recovery test removes the actual TileSet file and
catalog record, delivers the published source notification, and verifies release under a one-atlas
budget. The public Scene request rejects an occupied stable identity, then restores the catalog
and publishes changed pixels under the original handle. A second removal permits travel to a
Scene that has no dependency on the TileSet. Restoring the file permits re-entry in the same World.
[Project Scene recovery test](../../src/project_host/runtime/tests/content_residency.rs)

The independently locked source-free consumer passes 14/14 in
`c2747b75-7158-4dc5-be21-629c1116aca3`. It covers primary and auxiliary TileSet products,
original retirement, and scene travel using a cooked pack. WGPU offscreen readback passes 1/1 in
`786efb36-5608-4f0e-9b8b-dd8fb22b7dcf`, preserving exact retained pixels for both selections.
Correctness Clippy and workspace formatting checks pass. The qualification uses Rust 1.95.0 on
Windows. The source removal is injected at the public notification queue. Native filesystem
watcher delivery and window presentation have separate qualification paths.
[Independent TileSet consumer](../../tests/fixtures/module-consumer/tests/runtime_tileset_delivery.rs) ·
[WGPU readback](../../crates/nara_render_wgpu/src/readback_tests.rs)

Bevy records a failed load on the asset and its dependents without replacing the stored value in
that handler. Godot can fall back to the resource already in its cache after a failed load. Nara
retains the failure result alongside any still-authorized publication, and requires a fresh source
admission after revocation.
[Bevy failed load](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_asset/src/server/info.rs#L685) ·
[Godot cache fallback](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/core/io/resource_loader.cpp#L620)

### Collect Game world movement and pickups

Windows run `4d1c302b-57c7-4e22-a6d7-709f69ccfbd4` reproduced both gameplay defects through the
independently locked sample. At speed 200 with a 20 ms tick, a twice-scaled Prefab anchor made
Right move 8 world units. A radius-one gem at `(-38, 0)` stayed visible after the player moved
from `(-40, 0)` to `(-36, 0)`.

The game reads the effective parent from the fixed input snapshot, computes walking and its
limits in world units, and writes only the local translation after applying the parent's inverse.
Pickup contact uses the completed fixed position and this tick's walking segment. A reset sets
the origin for later movement in that tick. Pickups that inherit the player's movement use
their final relative distance. Godot applies the parent's affine inverse when setting
`Node2D` world position and computes contact from the nearest point on a finite segment.
[Gameplay owner](../../samples/collect-game/gameplay-plugin/src/walking.rs) ·
[Godot world position](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/scene/2d/node_2d.cpp#L298) ·
[Godot segment geometry](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/core/math/geometry_2d.h#L108)

A second regression, `56f9cb27-8839-4304-8a2e-f7a9a9eb06da`, reproduced an incorrect
landing at `(-96, 3.999999)` and a false pickup under a rotated, highly nonuniform parent.
The game checks the candidate local `f32` translation against the intended world endpoint
before writing it. The control with the same scale and an axis-aligned parent still walks
four world units up.

The independently locked sample passes 13/13 in `14407d51-8aae-4e50-9b1f-9e57e923dd92`
and 13/13 with `desktop` in `73f28055-3b48-49b4-a51f-698924116ce9`. The gameplay cases use
real Scene and Prefab files. They cover transformed and nested singular parents, world bounds,
a crossed pickup and a near miss, and Reset with and without a following command in the same
tick. The headless commands complete all three pickups at tick 62 and verify one reset by
tick 66. The Windows desktop command verifies the first presentation and exits. Correctness
Clippy and sample workspace formatting checks pass on Windows with Rust 1.95.0.
[Sample regression](../../samples/collect-game/tests/gameplay.rs)

### Fixed floor contact continuity

The independent Rapier adapter keeps the original fixed pair when the same live entity and
collision definition yield the same native pose and local hull. The repaired Windows run
`746bc6fb-a978-49d6-902b-8cd8aa8d3a7e` passes all 21 selected tests. An equivalent reparent and
reflection both preserve 16 contacts and 16 sleeping bodies. Moving the floor removes the old
contacts and lets the same bodies fall. A local hull change whose world debug points round to the
same coordinates also replaces the pair. The independent offscreen render consumer passes 11/11
in `2fb9f395-238a-4fe2-96b5-b56fefb3416c`, including two hardware DX12 trajectories.
[Collision qualification](../plans/2026-11-04-authored-collision-derivation-plan.md#fixed-collision-continuity)

### Disabled controls and pointer motion

A move over a disabled hit records the observed pointer position and exits the old hover target.
The disabled control still rejects a press. The public journal regressions move from an enabled
button to the disabled control, then check both coordinate-bearing and position-free presses and
releases. Neither route focuses or activates a control. An enabled target still focuses and
activates through both routes. The Windows interaction suite passes 66/66 in
`6f9dfd17-7633-475a-bdf7-4a5d8389a71e`.
[Pointer regressions](../../crates/nara_ui_kernel/tests/interaction/pointer_input.rs)
