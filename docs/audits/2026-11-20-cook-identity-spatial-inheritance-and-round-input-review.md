# Cook identity, spatial inheritance and round input review

The sixth review covers Nara at `05ecc98` and the unopened-source Editor workflow at `cb67a9b`.
The two highest priorities are a Cook that accepts conflicting Image identities and a reference-game
input sequence that overwrites the previous round's checkpoint. Five further findings concern
camera inheritance, cooked UI startup, controller handoff and the sample's native audio verifier.
All seven have Windows reproductions. [Previous review and repairs](2026-11-20-cooked-dependencies-prefab-revert-and-input-review.md)

## P1 · An earlier Scene can select the wrong pixels for a later Image Product

Give `badge.png` and `unused.png` distinct pixels, correct path fields in their sidecars and the
same source UUID. The first Scene embeds a PNG by path. The second Scene's badge links to the
Image Product by ID at Build. Change just the embedded path in the first badge. The later badge's
build-group digest and linked Product ID stay identical. Each five-member pack reopens and passes
deep validation. After moving the source project away, fresh Headless Players enter the second
Scene and read these pixels through the resident `ImageAsset` handle. [Independent two-Scene fixture](../../tests/fixtures/module-consumer/asset-domain-consumer/tests/project_cook_runtime_selection.rs#L240)

| First Scene's embedded input | Later badge's Image in the Player |
| --- | --- |
| `badge.png` | `[24, 120, 220, 255]` |
| `unused.png` | `[232, 64, 24, 255]` |

`with_image` skips catalogue discovery when the receipt already names a path for that Product.
Only the catalogue path reaches `AssetDatabase::insert`'s duplicate-ID check for the two records.
A separate single-Scene control started with a unique ID and completed Cook. Changing only an
unused sidecar to claim the selected ID left `ProjectCookSourceSelection::verify()` successful.
A fresh Cook on the same files returned `project.content.asset-identity-conflict`. Giving the
unused sidecar an unrelated ID passed both checks. Package publication calls this same selection
verifier. The late-change probe exercised the verifier directly. [Path reuse](../../src/project_content/delivery_images.rs#L70) · [Catalogue insertion](../../src/project_content/catalogue.rs#L350) · [Duplicate identity rule](../../crates/nara_asset/src/database.rs#L323) · [Selection verification](../../src/project_tooling/project_cook/package.rs#L138) · [Publication callback](../../src/project_tooling/project_cook/package.rs#L521)

Resolve every ID-based demand to a unique source in the admitted roots, including when the
candidate path is cached. Recheck that ownership before publication. Both embedded path choices
should reject a duplicate. A duplicate introduced after preparation should also reject, while an unrelated-ID
change and path-only embedding should keep their existing behavior. Godot's editor checks whether
a known UID belongs to another still-existing path when adding a file. Nara already has its own
conflict diagnostic for this case. [Existing Cook controls](../../tests/fixtures/module-consumer/asset-domain-consumer/tests/project_cook_runtime_selection.rs#L225) · [Godot UID handling at `b070e63b161a`](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/editor/file_system/editor_file_system.cpp#L917-L928)

## P1 for the Two-Level Game · Pressing Pause before Save can overwrite an older checkpoint

With a checkpoint slot configured, save 10 points in Meadow. Return to the title and start a
second round at 0 points. Tap `Start` and then `LeftShoulder` in the same host frame. Both
traces record the pause action's `Started`
before the save action's `Started`. Both frames end on the `Paused` HUD. Restoring the slot
exposes a one-millisecond difference in the outcome. [Round controls](../../samples/two-level-game/README.md#L167) · [Checkpoint binding](../../samples/two-level-game/README.md#L156) · [Configured fixed interval](../../samples/two-level-game/nara.toml#L30)

| Input delivery in round two | Fixed steps in the input frame | Writes | Score restored from disk |
| --- | ---: | ---: | ---: |
| Pause then Save, 19 ms | 0 | 0 | 10, original bytes |
| Pause then Save, 20 ms | 1 | 1 | **0, previous checkpoint overwritten** |
| Complete Pause, then Save on a later frame | 0 | 0 | 10, original bytes |

Round flow applies Pause in `Update`. The earlier `FixedUpdate` command consumer still sees
`Playing` and calls `Storage::save`. A frame with no fixed step leaves the command pending until
the flow can discard it. Hidden DX12 runs also obtained a `Paused` presentation receipt for the
actual input frame in each timing case. Give the round menu and checkpoint commands a shared
input ordering decision before a fixed step can write. Test zero, one and multiple due steps,
then Restore the old score. A standalone Save during play should still publish once. Bevy's
once-per-frame pre-fixed-loop set illustrates where a game can resolve decisions needed by
fixed simulation. [Nara stage order](../../crates/nara_app/src/lib.rs#L135) · [Round flow](../../samples/two-level-game/src/gameplay/flow.rs#L258) · [Fixed consumer](../../samples/two-level-game/src/gameplay/mod.rs#L301) · [Gameplay gate](../../samples/two-level-game/src/gameplay/travel.rs#L67) · [Write](../../samples/two-level-game/src/gameplay/travel.rs#L163) · [Bevy scheduling at `779770720e6a`](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_app/src/main_schedule.rs#L402-L439)

## P2 · Disabling a spatial parent moves an enabled child camera by 100 world units

Place a parent at `x = 100`, its child camera at local `x = 3`, and an independent red sprite
at world `x = 103`. Toggle only Bevy's `Disabled` on the parent. The hierarchy stays valid, the
camera retains its parent and view identity, and both local transforms stay in place. The
presented camera center moves `103 → 3 → 103`. Registering and toggling a custom gameplay
disabling component produces the same result. A control that actually removes and restores the
parent's `Transform2d` establishes the intended boundary for spatial inheritance.

On a hardware Vulkan target of 320 × 180, the 8 × 8 sprite covers columns `156–163`, then
`256–263` while the parent is disabled, then `156–163` again. The same sequence occurs with the
custom marker and with actual transform removal. All 57,600 pixels in each of the seven frames
matched the predicted image on an RTX 4090. The enabled camera remained the sole extracted view.
[Camera extraction](../../crates/nara_render/src/runtime.rs#L800) · [GPU readback fixture](../../tests/fixtures/module-consumer/sprite-render-consumer/tests/support/readback.rs#L129)

Transform propagation observes local transforms with an ordinary filtered query. A missing row
in that query makes the child a spatial root. ADR 0100 defines composition across a continuous
chain of stored transforms, and Hierarchy already enumerates stored relations independently of
gameplay filters. Apply that rule when collecting the transforms needed for a completed spatial
snapshot. The enabled child's camera should stay centered at 103 through either gameplay toggle
and move to 3 on actual transform removal. Bevy documents how default filters preserve stored
relations. Godot's `CanvasItem` composes the selected parent item's global transform with the
child's local transform. [Nara contract](../architecture/adr/0100-runtime-structural-hierarchy-and-completed-2d-transform-projection.md#L66) · [Hierarchy enumeration](../../crates/nara_hierarchy/src/validation.rs#L207) · [Transform query](../../crates/nara_transform/src/propagation.rs#L41) · [Parent selection](../../crates/nara_transform/src/propagation.rs#L318) · [Bevy default filters at `779770720e6a`](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_ecs/src/entity_disabling.rs#L1-L9) · [Godot canvas transform at `b070e63b161a`](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/scene/main/canvas_item.cpp#L201-L213)

## P2 · A deeply validated UI background can prevent Player startup

Make `menu-icon` in the startup Scene reference a PNG only through `UiVisual.background.image`.
Keep the Scene, PNG, codec ID and `ImageRuntimeCodec` fixed. Registering it through either
`add_runtime_asset_codec` or `add_runtime_image_codec` passes Project Check, Cook, source
revalidation, pack reopening and two-member deep validation. Direct decoding returns
`[24, 120, 220, 255]` in both cases. With the generic registration, `HeadlessSession::start()`
reports `scene.invalid-component-payload` / `invalid-asset-ref` for `menu-icon`. With Image
publication registered, the first frame resolves the widget's handle to the expected pixel and
the session closes successfully. [Codec registration](../../src/product.rs#L462) · [UI Image resolution](../../crates/nara_ui/src/codec.rs#L228) · [Player Image publication](../../src/runtime_content/prepared.rs#L218)

The previous round added a Sprite-specific publication check to the deep Scene pass. UI requires
the same handle from another binding. Have the binding owner declare the required publication
role so pack validation and Player materialization use the same fact. A generic-only codec with
this binding should fail `CookedContentBuilder::finish()` with the Scene, entity, field and
target in the diagnostic. Keep the publication-capable UI, a solid-color background and direct
CPU decoding as controls. [Deep Scene checks](../../src/runtime_content/deep.rs#L188) · [Assembly check](../../src/project_workflow/cooked_content.rs#L785) · [Sprite regression](../../tests/cooked_sprite_publication.rs#L61)

## P2 · A fresh controller or keyboard press can fail to take over movement after travel

Hold Right on controller A through entry into Cavern. Once playing, press Right on controller B,
then disconnect A. The semantic action remains held, yet `Session.held[1]` is false. No new
movement command reaches the game and the character stays at `x = -120` for five fixed steps.
Swapping only the disconnect and the fresh press produces one `Started` edge, `x = -116.8`
after the first step and approximately `-104` after five. Keyboard `D` gives the same results.
Staying in Meadow or pausing and resuming there permits movement in the original handoff order.
[Controller contribution rules](../../samples/two-level-game/README.md#L175)

Travel clears the game's held-direction array. Semantic Input retains A's contribution and
combines active sources with OR. Adding B, then removing A keeps the aggregate true throughout.
Gameplay rebuilds its array only from `Started` and `Released`. Make travel and semantic input
agree on which held sources survive the transition. If travel requires a fresh press, suppress
the previously held sources until release while admitting B's new press. Check both arrival
orders, controller-to-keyboard handoff and the last contributing source's release. [Travel reset](../../samples/two-level-game/src/gameplay/travel.rs#L276) · [Aggregate transitions](../../crates/nara_input/src/semantic.rs#L299) · [Movement bindings](../../samples/two-level-game/src/gameplay/input.rs#L140) · [Fixed movement](../../samples/two-level-game/src/gameplay/mod.rs#L538)

## P2 · The stock source audio verifier exits before its first frame

The documented source `audio_desktop --verify` command creates a private project by copying a
literal list. The list omits `input/explorer.actions.json` and `assets/animations/explorer.sprite-animations.json`
with its `.meta`. The stock release executable exited with code 1, zero presentations and no
callback evidence. Adding only the input map exposed `project.content.stable-asset-reference-unresolved`.
Adding only the animation files left `open-directory` / `not-found`. Supplying both groups to
the verifier's private project let both trajectories finish on Vulkan and WASAPI. They accepted
five and seven effects, compared the captured PCM with zero maximum error and retired their
devices. [Documented command](../../samples/two-level-game/README.md#L290) · [Copy list](../../samples/two-level-game/src/bin/audio_desktop/workflow.rs#L990) · [Input declaration](../../samples/two-level-game/nara.toml#L26) · [Animation reference](../../samples/two-level-game/scenes/meadow.scene.json#L307)

Build the verifier's private copy from the admitted project inputs, including the sound bank's
raw dependencies, then apply its deliberate failure and repair edits to that copy. An ordinary
run from the checked-in game should reach both trajectories. Keep the individually missing
input map and animation as targeted startup failure controls.

## P3 · The audio verifier compares authored sounds against compile-time WAVs

With the startup dependencies present, halve the PCM16 amplitude of `pickup.wav` without
rebuilding the executable. Pickup at tick 14 scores 10 and its 5,760-frame callback output
matches the modified WAV sample by sample. The verifier reports `independent PCM mismatch`.
Rebinding `pickup` to `travel.wav` in the descriptor plays the correct 11,520 frames, which the
verifier checks against the original 5,760-frame boundary. The importer follows the project's
descriptor while the verifier embeds the two repository WAVs. This limits what the source
qualification can establish about an edited sound bank. [Source importer](../../samples/two-level-game/src/sound_bank/source.rs#L77) · [Embedded WAVs](../../samples/two-level-game/src/bin/audio_desktop/oracle.rs#L9) · [Frame boundary](../../samples/two-level-game/src/bin/audio_desktop/oracle.rs#L214) · [PCM comparison](../../samples/two-level-game/src/bin/audio_desktop/oracle.rs#L318)

Capture the raw WAVs selected by the project's descriptor before playback and decode those
bytes independently for comparison. A temporary implementation passed both trajectories with
the original, half-amplitude and rebound inputs. Deliberately changing one captured channel
sample by `0.25` still produced a frame- and channel-specific mismatch. The checked-in WAV
hashes can separately verify the identity of the distributed sample. [Distributed audio assets](../../samples/two-level-game/assets/audio/README.md#L8)

## Regression coverage and limits

Fifteen existing release cases passed serially from the shared build artifacts. They cover six
independent badge Cooks, the Sprite publication requirement, two effective Prefab Revert round
trips, two unopened Scene/Prefab round trips, three presented UI routing cases and one hidden-HWND
refocus case. [Cook](../../tests/fixtures/module-consumer/asset-domain-consumer/tests/project_cook_runtime_selection.rs#L96) · [Revert](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/prefab_effective_inspection.rs#L655) · [Unopened sources](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/unopened_authoring.rs#L401) · [UI routing](../../crates/nara_ui/src/adapter_input_regression_tests.rs#L43) · [Windows refocus](../../crates/nara_winit/src/native_tests/reusable.rs#L688)

An additional registered-panel control inspected value 17 in a real Scene and Prefab. An
equal-length in-place write of 18 before requesting Open rejected both promotions. When the
request had already been dispatched and the next Editor drive was delayed by 160 ms, the same
write allowed both documents to open on the observed 17. Editing to 19 and requesting Save then
returned `TargetChanged` and preserved 18 on disk. This matches the documented split between the
worker's body check and Save's conflict check. The delay controlled the drive timing. The public
probe observed the terminal results without a worker-return receipt. [Inspection contract](../architecture/authoring-read-projection-design.md#L434) · [Body check](../../src/project_host/runtime/editor/document_open/preparation.rs#L111) · [Open publication](../../src/project_host/runtime/editor/document_open.rs#L390)

The new probes used Rust 1.95.0 and existing Windows release libraries. Cook exercised real packs
and fresh Headless sessions. The spatial test used an offscreen hardware target and full pixel
readback. Round input used normalized events in headless and hidden DX12 runs with zero physical
controllers connected. The audio runs used actual WASAPI callbacks. Workspace-wide tests, a
fresh standalone Player executable build, physical controller operation and audible speaker
output were outside this review.
