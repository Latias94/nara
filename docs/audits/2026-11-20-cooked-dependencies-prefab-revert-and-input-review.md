# Cooked dependencies, Prefab Revert and input review

The fifth review covers Nara at `8b4d327`, following `a8ee1ce`. Seven failures reproduced on Windows. The release blocker affects a single-Scene game whose importer first declares a required Image during Build. Six other findings affect cooked Sprite startup, effective Prefab editing and player input. The previous round's minimal startup and embedded-PNG repairs passed their original controls. [Previous review](2026-11-20-runtime-startup-cook-and-presentation-review.md)

## P1 · Cook cannot resolve an Image first requested by a built Product

Use the independent badge consumer with `ImageDelivery::LinkedAtBuild`, one startup Scene and the existing PNG and sidecar. Project Check succeeds. Building the badge twice gives `Built` and `Hit`. The group declares an `Immediate` dependency on the Image. The stock Cook callback sees zero stock Images, and assembly returns `MissingDependencyProduct`. Using the same Scene and badge group, explicitly adding the Image built from that PNG produces a three-member pack. A fresh source-free Headless Player starts, executes a frame and reads `[24, 120, 220, 255]` from the decoded Image. [Importer](../../tests/fixtures/module-consumer/asset-domain-consumer/tests/support/project_cook_badge.rs#L139) · [Public Player control](../../tests/fixtures/module-consumer/asset-domain-consumer/tests/project_cook_runtime_selection.rs#L333)

Cook takes its Image candidates from `snapshot.images()`. It records a required Product missing from that inventory as `Output::Demanded`. The replay predicate only picks up a previously encountered `Output::Stock(_, StockState::Demanded)`. The existing two-Scene test supplies the Image in an earlier Scene, so it exercises that second path. Repeating the same single-Scene inventory will still yield zero candidates. [Candidate inventory](../../src/project_tooling/project_cook/content.rs#L405) · [Missing demand](../../src/project_tooling/project_cook/content.rs#L486) · [Replay predicate](../../src/project_tooling/project_cook/content.rs#L696) · [Two-root control](../../tests/fixtures/module-consumer/asset-domain-consumer/tests/project_cook_runtime_selection.rs#L104)

Resolve new required Product IDs against the admitted Project sources when visiting the built group's dependencies. Retain the source and sidecar observations used to select and build each payload. Acceptance should cover the single root, the existing two-root case, stale PNG and sidecar rejection, and the embedded badge that ships with just two members. Godot's selected-Scene exporter follows dependencies from each selected root. [Godot export traversal at `b070e63b161a`](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/editor/export/editor_export_platform.cpp#L674)

## P2 · A deeply validated pack can fail to start its textured Sprite

Keep the Scene, PNG and `ImageRuntimeCodec` fixed and change only the registration call. With `add_runtime_image_codec`, the Player starts with one resident Image, runs a frame and closes successfully. With `add_runtime_asset_codec::<ImageDomain, ImageRuntimeCodec>`, Check, stock Cook, selection revalidation, pack reopening and two-member deep validation all succeed. Direct asset decoding returns the same RGBA bytes. `HeadlessSession::start()` then reports `scene.invalid-component-payload` with `invalid-asset-ref` for `nara.sprite.Sprite`, and close reports `Failed`. [Codec roles](../../src/product.rs#L462) · [Sprite preparation](../../crates/nara_sprite/src/lib.rs#L712) · [Player publication](../../src/runtime_content/prepared.rs#L218)

`CookedContentBuilder::finish()` already invokes deep validation. Its Scene pass collects Tilemap and Material requirements. The Sprite Image binding adds no corresponding native-publication requirement. Have the Sprite binding contribute that requirement to pack validation. A Sprite bound to a CPU-only Image should fail at assembly with the offending binding identified. Keep direct CPU Image consumption and successful native Sprite startup as controls. [Builder validation](../../src/project_workflow/cooked_content.rs#L785) · [Scene requirement collection](../../src/runtime_content/deep.rs#L150)

## P2 · A sibling note blocks Revert of a scalar Prefab override

Mount the same base twice and pin `score = 20` and a separate `note` on the left instance. A registered panel reads `score` and its exact containing claim in both a Scene and a Prefab. With a 64-byte note, `PrepareRevert(score)` reaches `Review`. With a 140,000-byte note, the action is accepted and ends in `Failed(SourceRejected)`, with diagnostic `scene.authoring-prefab-budget-exceeded`. The failed attempt preserves the containing revision and file bytes. On those same large documents, source-owned Revert with a 1 MiB allowance prepares and commits. Save, Undo/Save and Redo/Save preserve the entire note and the other mount in both document kinds. [Effective action](../../src/project_host/runtime/editor/prefab_inspection.rs#L467) · [Source Revert](../../crates/nara_scene/src/authoring/prefab_declarations.rs#L368) · [Existing round-trip fixture](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/prefab_effective_inspection.rs#L661)

The effective action fixes the allowance at the 256 KiB default. Its underlying `SetPrefabInstance` patch charges two complete anchor declarations before cloning, so the sibling text alone accounts for 280,000 bytes. The Host also turns the typed budget refusal into `SourceRejected`. Give the effective workflow a bounded, selectable allowance and return `BudgetExceeded` when capacity runs out. Preserve its live dependency checks through Commit. If large-instance edits are a product target, give the source transaction owner a patch operation for one exact claim and its inverse. That would keep the cost of editing a scalar independent of sibling payloads. [Limits](../../crates/nara_scene/src/authoring/prefab_declarations.rs#L73) · [Double charge](../../crates/nara_scene/src/authoring/prefab_declarations.rs#L382) · [Error conversion](../../src/project_host/runtime/editor/prefab_inspection.rs#L683) · [Transaction contract](../architecture/adr/0026-editor-command-patch-and-undo-model.md#L124)

## P2 · The stock gamepad D-pad selects the wrong UI actions

Start at the center of a five-button cross. Gilrs Up, Down, Left and Right focus down, left, right and center respectively. Clicking the right stick focuses up. Pressing South next records the corresponding wrong action in `UiActionJournal`. The four keyboard arrows select their expected buttons in the same layout. With a nonmodal menu, D-pad Right also leaves gameplay's `move-right` action held. The probe injected the Gilrs adapter's actual button codes into the presented Runtime UI and observed the gameplay state. It used no physical controller. [Gilrs codes](../../crates/nara_gilrs/src/controls.rs#L48) · [Bridge](../../crates/nara_gilrs/src/bridge.rs#L238) · [UI mapping](../../crates/nara_ui/src/interaction/timeline.rs#L275) · [Unhandled routing](../../crates/nara_ui/src/interaction/timeline.rs#L207)

The adapter assigns `13..=16` to the D-pad. UI interprets `12..=15` as directions. Put the standard button identities in the input contract shared by producers and consumers, retaining the numeric representation where required by stored bindings. Verify the four directions, South activation, right-stick click and gameplay pass-through in one integrated menu. Bevy's Gilrs converter maps each named direction to its corresponding `GamepadButton`. Godot binds `ui_up` to the named `JoyButton::DPAD_UP`. [Bevy converter at `779770720e6a`](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_gilrs/src/converter.rs#L20) · [Godot action binding at `b070e63b161a`](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/core/input/input_map.cpp#L505)

## P2 · The first fresh movement key after refocus can disappear

Hold W, lose window focus, release W in another window, regain focus and press W again. A hidden Windows Winit session emitted fixed-step `move, stop` up to the focus loss. The first fresh, nonrepeat W press after refocus left the semantic action released and emitted no command. Releasing and pressing W inside the game then emitted the next `move`. The probe posted the key-up to a second HWND in the same process and drove the actual Winit message pump. The reference game binds W to movement. [Focus gate](../../crates/nara_winit/src/lib.rs#L1054) · [Winit event admission](../../crates/nara_winit/src/lib.rs#L1758) · [Reference binding](../../reference-game/src/input.rs#L36)

Loss of focus inserts W into `suppressed_keys`. Ordinary refocus changes the focus flag. The key-up delivered to the other window never clears the entry. Reconcile physical key state on return, or use platform evidence that a real new press began. Test a release while away followed by an immediate fresh press. Also hold W continuously across the switch and verify that synthetic presses and repeats keep movement stopped. Physical foreground Alt+Tab remains a separate native check. [Suppression and release](../../crates/nara_winit/src/lib.rs#L1065)

## P2 · Moving over a modal underlay can activate the old modal button

On one presented frame, hover the modal's Retry at `(220,130)`, then move to the enabled underlying button at `(20,20)`. The move returns `TargetOutsideModal`. A subsequent primary press and release with no coordinates reuse `(220,130)`. Retry receives `Pressed` and `Released { inside: true }`, and the stock button reducer emits one activation. Supplying `(20,20)` on the otherwise identical press and release yields no activation. The previous disabled-control regression still passes. [Motion routing](../../crates/nara_ui_kernel/src/interaction/pointer.rs#L76) · [Position resolution](../../crates/nara_ui_kernel/src/interaction/session.rs#L507) · [Rejected candidate](../../crates/nara_ui_kernel/src/interaction/session.rs#L219) · [Button reducer](../../crates/nara_ui_widgets/src/button.rs#L174)

The candidate carries the new position. The outside-modal rejection discards it before the previous target can receive a hover exit. Treat an admitted same-epoch motion into a modal underlay as an observed location with no eligible hover target. Verify that both forms of the next button event leave Retry idle and that an actual click on Retry still works. [Modal scope check](../../crates/nara_ui_kernel/src/interaction/routing.rs#L83) · [Disabled-control regression](../../crates/nara_ui_kernel/tests/interaction/pointer_input.rs#L439)

## P2 · A zero-pixel button intercepts a visible button's click

Place a solid foreground button at `x = 19.9`, width `0.2`, over a solid background button at 1× scale. The CPU paint packet contains only the background rectangle. The foreground has an empty physical rectangle, no focus entry and no semantic node, yet a primary click at `(20,20)` activates it. At `(21,20)` the background receives the press and release. Changing the foreground width to `1.2` yields two paint primitives, one foreground pixel column, and agreement between focus, semantics and the click. [Hit registration before the visibility check](../../crates/nara_ui_kernel/src/compiler/root.rs#L368) · [Paint packet lowering](../../crates/nara_ui_render/src/planner/lowering.rs#L223) · [Hit resolution](../../crates/nara_ui_kernel/src/interaction/routing.rs#L14)

Apply the existing surface-coverage result when registering pointer hit regions. Cover a zero-column foreground over a live background and a one-column positive control. Keep explicit invisible hit areas with real geometry usable. The measurements here are CPU paint packets and widget intents. [Surface visibility contract](../architecture/adr/0108-ui-surface-presented-session-and-atomic-interaction.md#L284)

## Repair order and verification

1. Resolve newly required Products from admitted Project sources and validate their use by the stock Sprite. Keep the embedded, source-linked, build-linked and CPU-only cases as distinct controls.
2. Unify gamepad identities across Gilrs and Runtime UI. Correct the focus-loss gate and pointer motion/coverage rules at their respective owners. Each regression should observe the final gameplay action or widget intent.
3. Make effective Prefab Revert's capacity and its diagnostic explicit. Use the two tested document kinds and preserve exact history and dependency revalidation if the patch representation changes.

| Windows verification in this review | Result |
| --- | --- |
| Public `project_runtime_boot` release target, Rust 1.95.0 | 24/24 passed, including checked minimal startup, explicit startup commands and paused capture faults. |
| Independently locked badge Cook and panel release tests, Rust 1.100.0 | 7/7 selected passed in nextest `27ac341a-de9f-4442-b0bf-91f8800f3670`. Three Cook cases cover embedded, source-linked and two-root build-linked delivery. Four panel cases cover stored and effective claims on named Scenes and Prefabs, including Undo/Redo and Save/Reopen. |
| Existing UI interaction release suite | 66/66 passed, including moving onto a disabled button. |
| Existing 4K Editor Scene preview hardware control | 1/1 passed with explicit Vulkan on an RTX 4090, reading the rendered pixels and comparing the picked entities. |

The seven new findings used temporary Rust 1.95.0 probes and release libraries. Cook reopened real packs. Prefab tests used registered semantic panels and real project files. The D-pad and pointer tests used CPU presentations. The focus test used injected Windows messages. GPU evidence above belongs to the existing 4K picking control. Native audio output, physical controller input and a visible Editor session were outside this verification.

## Repair status

### Build-declared Image dependencies and Sprite codec admission

Commit `6be6521` fulfills the first-build Image dependency through the Project Content loader.
The loader uses earlier provenance for a known typed Image and discovers new identities through
its bounded catalogue. PNG and sidecar observations reach the original publication fence. Two
review controls check the exact combined file limit and successful path-only delivery with an
unrelated invalid sidecar. A mixed acquisition control decodes the known and newly discovered
Images in both Product orders. The source-free consumer reopens the relocated single-Scene pack
after the original project path disappears. [Independent Cook consumer](../../tests/fixtures/module-consumer/asset-domain-consumer/tests/project_cook_runtime_selection.rs)

Commit `c696d1f` makes deep validation check that a stock Sprite with an Image binding selects an
Image publication codec. The diagnostic names the Scene, entity, field and target Product. The
ordinary consumer verifies a fresh Headless startup, direct acquisition through the CPU codec,
and null and omitted Sprite bindings. [Sprite publication regression](../../tests/cooked_sprite_publication.rs)

The Cook plan records verification of the two repairs, including relocation and startup of the
source-free one-Scene pack. [Cook qualification](../plans/2026-10-31-cooked-player-delivery-plan.md#build-declared-image-and-sprite-qualification)

### Effective Prefab Revert capacity

Commit `9c61130` makes `PrepareRevert` carry `PrefabDeclarationLimits`. The Host preserves the
source owner's budget diagnostic and reports `BudgetExceeded`. The existing patch charges the
complete expected and replacement declarations. The independent registered-panel consumer
proves both real-file workflows with a 140,000-byte sibling note. The default refusal preserves
the full file, document basis and history. The 1 MiB allowance permits Review, Commit, Save, Undo,
Redo and Reopen, preserving the sibling and other mount. The tests also change a live dependency
before preparation and after Review to check the same publication fence. Windows release
nextest `e173f1b3-84ac-4a64-8ed3-d6a9ae9d5e2b` passes 14 selected panel tests, with one outside
the run. The Engine Host cohort `3c65302f-9731-44cf-91a5-168c50508fe6` passes 10 tests, including
dependency drift between dispatch and the drive that consumes the request.
[Panel regression](../../tests/fixtures/module-consumer/editor-panel-consumer/tests/support/prefab_effective_inspection.rs)

### Gamepad and presented pointer routing

Commit `6250e24` shares `GamepadButton` between Gilrs and Runtime UI and retains the original
binding codes. The presented menu checks every D-pad direction, the resulting South activation,
suppression of the gameplay Right edge, and right-stick click delivery to gameplay.

Current-presentation modal motion records the observed position and clears hover over the
underlay. The regression checks coordinate-free and explicitly positioned clicks, then
activates Retry at its actual location. Hit registration and dispatch now share the existing
visible pixel coverage used by focus and semantics. The tests check zero and one foreground
pixel column, an intentional unpainted hit area, and a historical press against a successor
that lost its pixel coverage. Windows nextest `17cd1bde-3a9a-4678-ad20-53751788d82f` passes
519 tests across Input, Gilrs, Runtime UI and UI Kernel, with one outside the run. The three
new integrated Runtime UI cases failed on the prior implementation in
`2eadfd6f-93d2-480e-959c-6d9f79b80b03`. Two-Level release nextest
`795344af-2ce9-4728-bfb4-d54d0334f413` passes 17 tests, including normalized D-pad movement and
the controller-only round. Windows WGI initialization and retirement pass in
`987f6fc8-a20a-4995-99c2-5aed1deb1a09`, with zero connected controllers.
[Presented UI regressions](../../crates/nara_ui/src/adapter_input_regression_tests.rs)

### Ordinary Windows refocus

Commit `551db20` admits a fresh nonrepeat physical press after ordinary Winit focus loss on
Windows, including when key-up went to another window. Synthetic focus key events and repeats
stay suppressed. A paused handoff retains its physical reconciliation and admission requirements.
Windows release nextest `938396cd-3ea2-4b99-a31d-86f990def774` passes all 99 selected Winit tests.
The separately selected hidden-HWND test passes in `e5c5463a-d365-4f8c-a4c0-6b7352bce2c8`. It posts focus and
key messages to the original and receiver HWNDs, exercising fixed-step Move and Stop through
the actual message pump. The D-pad regressions use normalized input. Physical controller input
and physical Windows foreground switching remain unqualified. [Windows regression](../../crates/nara_winit/src/native_tests/reusable.rs)

All-target, all-feature correctness Clippy passes for the changed owner crates. Formatting
checks pass for the root workspace and the independent panel package.
