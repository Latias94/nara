# A Self-Drawn Editor for Nara

November 23, 2026. This is a product and architecture recommendation based on Nara `5896585`
and the owner's Design Kit v0.4. The target is a desktop tool its authors can use to make a
game in Rust. Nara owns the controls people interact with, the meaning of edits and their
appearance. I would use the maintained Open-GPUI framework for the next desktop implementation,
keep the engine Host's existing rendering integration, and judge the result by work an author can
finish in the ordinary Editor. The Dear ImGui implementation has served as a runtime test and
reference. Its most valuable deliverable for the transition is the working behavior it records.
[Nara strategy](../../STRATEGY.md#our-approach) · [Accepted UI decision](../architecture/adr/0107-shared-ui-foundation-and-independent-product-layers.md#L30-L65) · [Reference retirement policy](../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md#L142-L172)

## Start with the components Nara already has

The property sheet shows an intensity of `4.00` overriding a source value of `2.00`, an invalid
numeric draft, and an unsaved-document indicator. These describe three different things. A save
can clear the document indicator while leaving the override intact. An invalid draft can retain
keyboard focus while the last accepted value keeps driving the Scene. Multi-selection has an
equally precise rule. Editing only X changes only that axis on compatible targets, and one Undo
restores each object's own starting value. These are enough instructions to build a distinctive
Inspector, down to what the author sees after an edit. The 2D reference game can exercise these
rules with `Transform2d` and Prefab instances.
[Property sheet](../design/editor-reference/images/nara-components-properties-v0.4.png) · [Field states](../design/editor-reference/nara-editor-design-spec-v0.4.md#L1219-L1238) · [Selection scope](../design/editor-reference/nara-editor-design-spec-v0.4.md#L283-L299)

Some of this design already runs. The drawn workspace has the graphite/teal palette and shares
the Inspector's field drafts and edit handling. `UiPropertyRow` measures label, editor, action
and status, aligns their baselines, and stacks the row at narrow widths. Preserve these behaviors
when implementing a GPUI property row. Reuse the existing draft or layout code where it fits
directly. A layout implementation tied to a particular tree can be replaced at the component
boundary. Ordinary buttons and text fields can use the framework directly. Nara's custom code
earns its place in controls such as a Prefab property row, a reference picker or a curve editor.
[Workspace palette](../../editor/src/frontend/drawn/theme.rs#L4-L25) · [Drawn Inspector](../../editor/src/frontend/drawn/inspector.rs#L19-L44) · [Measured row](../../crates/nara_ui_widgets/src/property_row.rs#L6-L15)

## What Linebender changes about the decision

Raph Levien's November 21 article argues for separating design from execution. His concern about
immediate-mode productivity tools is concrete. Accessibility needs stable identities, text layout
is expensive to repeat, and a changing viewport can cause unnecessary work elsewhere. For Nara,
give an Inspector field a stable identity across layout updates and let a running preview request
its own redraw. Ask separately whether the window still needs compositing. Levien also describes
limits in the GPUI renderer he discusses and tradeoffs in caching layers. A Nara gradient editor
or curve canvas provides a real place to compare a custom paint operation with an engine image
composited into the UI. The article offers design reasoning. The maintained fork and the actual
graphics backend determine which rendering capability Nara needs to add.
[Levien on GPUI](https://linebender.org/blog/next-100-guis/#rust-gui-is-still-compelling) · [Immediate and retained state](https://linebender.org/blog/next-100-guis/#immediate-vs-retained-mode) · [Module boundaries](https://linebender.org/blog/next-100-guis/#clearly-defined-modules) · [Layer costs](https://linebender.org/blog/next-100-guis/#optimization-opportunities)

Nara has a particularly good example of where this reasoning leads. In a recorded Windows/DX12
workload with 10,001 source records, a local property edit spent 223.6 ms P50 materializing the
Scene preview. Indexing touched hierarchy relationships brought that phase to 85.3 ms P50 on the
matched workload, with the same UI draw and submission counts. Those are historical phase timings.
For a slow Inspector drag, measure input to visible response and attribute the delay to document
work, layout, paint or submission before changing the responsible implementation.
[Workload](../benchmarks/2026-11-04-ui-work-baseline.md#L874-L885) · [Attribution and matched results](../benchmarks/2026-11-04-ui-work-baseline.md#L1191-L1209)

## Where to put the implementation

| Responsibility | Recommended owner and existing evidence |
| --- | --- |
| Document content, edits and history | The Scene/Prefab or extension document accepts the edit and owns its result and inverse. The workspace manages open documents and saving. The panel resolves its target and issues a typed request. The existing `EditorPanelHost` already dispatches through the application. [Panel Host](../../editor/src/panels/host.rs#L67-L83) · [Document domains](../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md#L183-L198) |
| Nara controls, tools and workspace behavior | Nara supplies the Inspector, hierarchy, dock content and canvases, including selection context, draft handling and tool-specific gestures. GPUI can provide focus, text input, layout and paint. Reuse the same view occurrence when moving a pane. A second view gets its own camera or graph navigation and can bind to the same document. [Nara interaction spec](../design/editor-reference/nara-editor-design-spec-v0.4.md#L894-L911) · [Panel and view identities](../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md#L313-L330) |
| Engine viewports | Keep the existing engine-owned device and submission. The independent GPUI consumer records the UI with the acquired window encoder and samples the engine's original Scene image. A click uses the displayed image, camera mapping, scale and clip from that occurrence. Moving a splitter can change the mapping while leaving the document and its history intact. [Renderer](../../tests/fixtures/gpui-engine-consumer/examples/gpui_view/renderer.rs#L297-L314) · [Display and picking](../architecture/adr/0111-editor-scene-view-execution-and-picking-authority.md#L148-L176) |
| Game HUD and player input | The game owns its authored UI, player focus and timing. Share a font, texture or control mechanism when a game and an editor tool both use it. A headless game can keep its existing composition and the desktop workbench can evolve independently. [Runtime and Editor ownership](../architecture/adr/0107-shared-ui-foundation-and-independent-product-layers.md#L141-L157) |

Three implementation paths are visible today. `nara-editor` selects the ImGui reference by
default. `nara-editor-drawn` runs the independent source workspace with Hierarchy, Inspector,
Undo and Save/Reopen. The separately locked `gpui_view` consumer already displays real Scene
pixels, routes picking and edits properties through a real `EditorProjectSession`. Its recorded
Windows runs cover DX12 and Vulkan. Cross-monitor input, physical IME and switching the default
desktop Editor need evidence from the integrated product. A version detail matters for docking.
The fixture pins `f75c10b2`. The `DockspaceSession` API in the newer `428b460e` reference is a
possible upgrade. The fixture has no docking dependency at its pinned revision.
[Editor binaries](../../editor/Cargo.toml#L9-L27) · [Drawn workspace](../guides/editor-panels.md#toolkit-independent-semantic-panels) · [GPUI consumer](../../tests/fixtures/gpui-engine-consumer/README.md#L182-L209) · [Fixture manifest](../../tests/fixtures/gpui-engine-consumer/Cargo.toml) · [Pinned docking interface](https://github.com/Latias94/open-gpui/blob/f75c10b2a9bf7b4dcb2c8c2aa446203997601369/crates/gpui_docking/README.md#L8-L22) · [Newer dock session](https://github.com/Latias94/open-gpui/blob/428b460e6d33e246aeb2d740a0e9c6215381c658/crates/gpui_docking/README.md#L3-L7)

## The next authoring workflows

Blender offers a useful precedent for the first workflow. Its Move tool and its Location X
control reach the same object through different write paths. For Nara I would share the edit's
target, accepted result and Undo boundary across Inspector and gizmo, and give each tool the
input handling it needs. Nara can keep an uncommitted preview. Escape then discards the preview
even if a newer document edit arrived during the drag. This follows Nara's own revision model
and avoids restoring an obsolete value into the document.
[Blender source trace](#blenders-move-gesture-and-location-field) · [Continuous interaction in Nara](../architecture/adr/0026-editor-command-patch-and-undo-model.md#continuous-interaction-transactions)

1. **Make editing a 2D Scene an ordinary workspace operation.** Start with the designed Hierarchy,
   Inspector and Scene View. Select an entity, edit its local X in the Inspector, drag it with a
   gizmo, cancel a drag, then Undo a committed edit and reopen the file. Reuse the source-local
   Transform tool's existing prepare/commit path. One concrete refactor is already visible.
   `dispatch_gesture` accepts Begin on the active trusted UI Prefab path and commits with
   `ApplyPrefabPatch`. A Scene gizmo needs a Scene-owned candidate and the transform for its
   presented view. Share the preview/commit/cancel behavior while preserving both target checks.
   The actual canvas can keep its own hit testing and coordinate conversions.
   [Source-local Transform tool](../guides/editor-panels.md#a-source-local-snap-tool) · [Gesture admission](../../src/project_host/runtime/editor/actions.rs#L612-L649) · [Current commit route](../../src/project_host/runtime/editor/actions.rs#L695-L718)
2. **Use the existing Dialogue Graph for the second kind of editing.** Its typed commands,
   stable choice ports, one document history, file round trip and conversation preview already
   exist. The committed public test attaches two graph views to one source. The native Dialogue
   panel is an unqualified working-tree draft at the research baseline. When editor development
   resumes, finish a writer's task in the product window. Keep the Scene visible, edit a line
   with Chinese IME, change a choice, play that branch, Undo, save and reopen. Closing the graph
   should leave the Scene usable. That one task distinguishes two views of one graph from two
   independent documents with correct input and history. It also exposes which controls both
   tools genuinely share.
   [Dialogue workflow](../plans/2026-11-12-dialogue-graph-integration-plan.md#complete-consumer) · [Two-view public consumer](../../tests/fixtures/gpui-engine-consumer/tests/dialogue_document/external_source.rs#L105-L124) · [Recorded native scope](../plans/2026-11-12-dialogue-graph-integration-plan.md#L838-L861)
3. **Move the working view before building another window system.** Start in one window. Then
   move a view between two differently scaled displays, preserve its document and camera, and
   pick the object that was actually shown after the move. Godot's editor provides a second
   precedent for the data model. It saves the split arrangement and each viewport's own position,
   rotation and projection. A floating Scene tab can therefore preserve what the author was
   looking at without creating another document. The OS window, input focus and GPU output still
   need their own handoff.
   [Godot split and view state](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/editor/scene/3d/node_3d_editor_plugin.cpp#L380-L387) · [Per-view camera](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/editor/scene/3d/node_3d_editor_viewport.cpp#L5329-L5339) · [Nara native scope](../verification/native-editor-qualification.md#L45-L52)

I would put new product features into one self-drawn workspace. Once that entry point covers
the incumbent author workflows, make it the default and remove the displaced first-party UI
code and temporary bridges. A specific visual limitation warrants replacing the affected paint
primitive. Repeated need for two competing focus, hit-test or submission owners across both the
Scene and Dialogue tools would warrant reconsidering the host integration itself. Compare
the alternatives on those two authoring sessions, using the existing document behavior, file
results, input routing and measured response time to decide which implementation to keep.
[Frontend replacement contract](../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md#L142-L172) · [Independent execution choices](../architecture/adr/0107-shared-ui-foundation-and-independent-product-layers.md#L59-L88)

## Blender's Move gesture and Location field

At Blender commit `20a0565aa86c7ba6ab1e9537675a0cd8529337ef`, Move (`G`) previews a drag in Object Mode by writing to the selected object. The modal operator opts into undo, points its transform data at the object's `loc` array, and saves the starting coordinates in `iloc`. Each update writes the saved position plus the current displacement. [Move shortcut](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/scripts/presets/keyconfig/keymap_data/blender_default.py#L374-L385) · [Move registration](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/source/blender/editors/transform/transform_ops.cc#L853-L865) · [Starting coordinates](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/source/blender/editors/transform/transform_convert_object.cc#L221-L223) · [Translation write](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/source/blender/editors/transform/transform_mode_translate.cc#L148-L154)

Pressing `Esc` makes `transformEnd` restore the saved transform and return `OPERATOR_CANCELLED`. Pressing `Enter` keeps the current position and returns `OPERATOR_FINISHED`. Only the finished result reaches the window manager's completion handler, which requests one undo step for Move when the operator undo depth is zero. [Default keys](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/scripts/presets/keyconfig/keymap_data/blender_default.py#L6242-L6248) · [Modal state changes](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/source/blender/editors/transform/transform.cc#L1166-L1179) · [Transform exit](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/source/blender/editors/transform/transform.cc#L2381-L2401) · [Result dispatch](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/source/blender/windowmanager/intern/wm_event_system.cc#L2746-L2757) · [Undo request](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/source/blender/windowmanager/intern/wm_event_system.cc#L1304-L1313)

Dragging the X Location field in Object Properties writes to the same `loc` array through RNA, Blender's property interface. The panel binds the field to `Object.location`. The control sets one indexed float, and the UI calls `RNA_property_update` independently of its optional operator call. The property declares an object-transform notification. The UI can also push its own labeled undo step. [Panel binding](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/scripts/startup/bl_ui/properties_object.py#L36-L49) · [Indexed write](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/source/blender/editors/interface/interface.cc#L2985-L2992) · [UI dispatch](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/source/blender/editors/interface/interface_handlers.cc#L1187-L1202) · [Location definition](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/source/blender/makesrna/intern/rna_object.cc#L3240-L3245) · [UI undo](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/source/blender/editors/interface/interface_handlers.cc#L1262-L1271)

A workspace groups screen layouts, and each screen contains areas. Each area keeps its active editor first in `spacedata` and its active regions in `regionbase`. Two 3D areas showing the same object can use different shading because each has its own `View3D` space. In one area's Quad View, four window regions share that space's shading and each holds its own orientation and view distance in `RegionView3D`. [Workspace, screen, area and shading](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/source/blender/blenloader/intern/versioning_defaults.cc#L350-L359) · [Active space and regions](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/source/blender/makesdna/DNA_screen_types.h#L683-L693) · [Region view state](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/source/blender/editors/space_view3d/space_view3d.cc#L253-L265) · [Quad View split](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/source/blender/editors/screen/screen_ops.cc#L5970-L6008)

The object-transform notification from a Location edit tags each receiving 3D window region for redraw. The same region subscribes broadly to `Object` changes and subscribes to `RenderSettings.use_border` only in camera view. The area's draw loop skips hidden or untagged regions. These paths suggest that one undoable move can update two views while each keeps its own framing and redraw subscriptions. [RNA notification](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/source/blender/makesrna/intern/rna_access.cc#L2635-L2646) · [Transform listener](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/source/blender/editors/space_view3d/space_view3d.cc#L712-L729) · [Region subscriptions](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/source/blender/editors/space_view3d/space_view3d.cc#L940-L993) · [Region draw loop](https://github.com/blender/blender/blob/20a0565aa86c7ba6ab1e9537675a0cd8529337ef/source/blender/windowmanager/intern/wm_draw.cc#L1055-L1059)

## coss ui as a component reference

coss ui is Cal.com's React component library, built on Base UI and styled with Tailwind CSS.
Use it as a reference for desktop control behavior and composition. Its theme assigns distinct
roles to input borders, focus rings and error text. Nara's Design Kit supplies the graphite
surfaces, teal emphasis and regular/compact minimum control heights of 28/24 logical units.
[coss ui](https://coss.com/ui) · [Styling](https://coss.com/ui/docs/styling) · [Nara design guidance](../design/editor-reference/README.md#decisions-to-carry-forward)

The best first comparison is the Transform X field. coss combines typed input, step buttons
and an optional draggable label. Its `Field` associates the control with a label, description
and error. Nara's selection and history rules supply the meaning of the gesture. For two objects
at X = 10 and 20, typing 15 sets both to 15. Dragging by +1 from their original positions previews
11 and 21. Releasing records one edit and Undo restores 10 and 20. The existing `UiPropertyRow`
already handles the label, editor, action and status layout at wide and narrow widths.
[Number Field](https://coss.com/ui/docs/components/number-field) · [Field](https://coss.com/ui/docs/components/field) · [Nara numeric and vector rules](../design/editor-reference/nara-editor-design-spec-v0.4.md#L1242-L1249) · [Row layout](../../crates/nara_ui_widgets/src/property_row.rs#L6-L15)

At the Open-GPUI revision selected by Nara's fixture, `NumberInput` already handles keyboard
stepping with Up/Down and range endpoints with Home/End. It paints a text value beside two step
buttons. A native Inspector can build on that behavior and use a text editor and drag target for
direct entry and scrubbing. [Fixture revision](../../tests/fixtures/gpui-engine-consumer/README.md#L3-L9) · [Keyboard behavior](https://github.com/Latias94/open-gpui/blob/f75c10b2a9bf7b4dcb2c8c2aa446203997601369/crates/ui_components/src/number_input.rs#L238-L270) · [Rendered control](https://github.com/Latias94/open-gpui/blob/f75c10b2a9bf7b4dcb2c8c2aa446203997601369/crates/ui_components/src/number_input.rs#L485-L565)

Two other patterns transfer well. A searchable picker can reuse the Combobox's grouped results
and empty state. An empty search only describes the query. A missing assigned asset still needs
its identity and a repair action. A segmented control can share its appearance across tabs,
settings and filters. Switching between Assets and Console changes the visible panel. Choosing
a light's shadow mode edits a document property and belongs in Undo.
[Combobox](https://coss.com/ui/docs/components/combobox) · [Segmented Control](https://coss.com/ui/docs/components/segmented-control) · [Nara reference states](../design/editor-reference/nara-editor-design-spec-v0.4.md#L1225-L1236)

For in-game UI, settings menus can reuse labels, value controls and visible focus cues. The HUD's
appearance and input behavior come from the game. In Nara's reference game the terminal overlay
puts focus on Retry, keeps the HUD visible and makes its controls inert. The runtime owns that
transition and prevents a second activation while the first retry is pending.
[Reference-game UI states](../architecture/reference-game-ui-product-slice.md#state-matrix) · [Shared mechanisms](../architecture/adr/0107-shared-ui-foundation-and-independent-product-layers.md#L141-L155)
