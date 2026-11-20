# Camera interpolation, game recovery and authoring review

The seventh review started at `0d3e76f` and included the prepared-property preview through
`1d657a2`. Eight findings reproduced on Windows. Two warrant first attention. Valid camera
transforms can fault the running game, and a failed Restore can authorize overwriting another
session's checkpoint. The other findings concern recovery snapshots, Project Cook, menu input
and the size at which a small Editor property change exceeds the read or edit limits. A
concurrent Editor change closed the first measured threshold during this review.
[Sixth review and repair qualification](2026-11-20-cook-identity-spatial-inheritance-and-round-input-review.md#repair-status)

## P1 · Interpolation between two invertible transforms can fault the Runtime

Parent an enabled `Camera2d` to a transform with unit scale. Enable fixed transform interpolation.
At the first 20 ms fixed step, flip the parent's X scale to `−1`. Driving 30 ms yields an
interpolation fraction of `0.5`. The simulation camera's determinant is `−1`, while its
presentation determinant is zero. View extraction faults the Runtime. Driving a further 20 ms
returns the same fault at tick 1. A root camera and a parent changing from `+1` to `+2` both
advance to tick 2. [Local interpolation](../../crates/nara_transform/src/presentation.rs#L315) · [Parent composition](../../crates/nara_transform/src/presentation.rs#L264) · [Camera extraction](../../crates/nara_render/src/runtime.rs#L800) · [Singular view rejection](../../crates/nara_render/src/runtime.rs#L859)

The problem affects frames on either side of the singularity. With the default 16,666,667 ns
step and a 24.8 ms drive, a stationary marker at world `x = 8` projected to `x = 493.33` on a
320-wide canvas. Full 960 × 540 Vulkan readback found `144 → 0 → 144` red pixels across three
frames. With interpolation disabled, the marker moved from canvas `x = 168` to `152` and kept
144 pixels in every frame.

Both axes can also flip together. From `(1, 1)` to `(−1, −1)`, both endpoint determinants are
`+1`, yet the half-step again faults. Expressing the same endpoint as a half-turn with unit scale
keeps the determinant at `1` and the Runtime running. Define an invertible presentation path for
equivalent same-handed transforms and a discontinuity policy for a change of handedness. Keep
the error for an actually singular camera. Godot switches to the current basis while
interpolating translation on a handedness change and otherwise interpolates the decomposed 2D
transform. The double-axis case is an acceptance test for Nara's signed-scale representation.
[Godot physics interpolation at `b070e63b161a`](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/core/math/transform_interpolator.cpp#L36-L46) · [Godot 2D interpolation](https://github.com/godotengine/godot/blob/b070e63b161aa816afac71dcfc9d60859d38f220/core/math/transform_2d.cpp#L263-L272)

## P1 for the Two-Level Game · A failed Restore clears a real save conflict

Session A saves 0 points in Meadow. Session B subsequently saves 10 points in Cavern to the
same slot. A's normal Save detects the external change. Temporarily break the Cavern Scene and
let A request Restore. A stays in the original Meadow instance with `restores = 0`. Repair the
Scene. A third Runtime can now restore B's Cavern checkpoint and all 10 points.

| What A does before its next Save | A's successful save count | Actual slot after Save |
| --- | ---: | --- |
| Skips Restore | `1 → 1` | Cavern, 10 points, original bytes |
| Attempts the Restore that fails | `1 → 2` | **Meadow, 0 points** |
| Completes a successful Restore | `1 → 2` | Cavern, 10 points |

`SaveSlot::load` accepts the newly read file identity and digest as the basis for the next
write. The game still has to validate and install the destination Scene. A later scene
rejection leaves the old game state paired with the new write permission. Stage the observed
slot binding with the decoded checkpoint and adopt it when Restore commits. Both an immediate
Save and a pending Save executed on Continue reproduced the table. Both should preserve B's
bytes after a rejected Restore and permit saving after a successful one.
[Slot binding](../../samples/two-level-game/src/save/io.rs#L124) · [Save conflict check](../../samples/two-level-game/src/save/io.rs#L143) · [Restore request](../../samples/two-level-game/src/gameplay/travel.rs#L158) · [Scene commit](../../samples/two-level-game/src/gameplay/travel.rs#L376)

## P2 · Rejected travel can produce a recovery snapshot that a fresh Runtime refuses

Collect `gem-near` in Cavern, return to Meadow, rename that pickup in the Cavern source, and
attempt to travel back. The game rejects the destination because the saved pickup is absent.
The incumbent remains playable with 10 points and two completed travels. Restore the exact
original Cavern file and place a marker in Meadow. `capture_resume` succeeds, but the new
snapshot contains the rejected candidate's Cavern digest (`4c70dc5e2862…`). A fresh Runtime
refuses it with `resume content compatibility rejected Cavern`. The pre-attempt snapshot
(`90a581602941…`) still restores against those same files.

`validate_content` updates `ResumeControl.levels` before progress validation and scene commit.
Capture checks the active level, then serializes every level's stamp. A syntax error fails
earlier and preserves a restorable snapshot. Repeating the pickup rename against the current
Meadow instance instead causes capture itself to refuse. Stage the candidate stamp until
commit. After either kind of rejection, a fresh Runtime should recover the post-attempt score
and marker. [Stamp update](../../samples/two-level-game/src/gameplay/resume.rs#L210) · [Progress validation](../../samples/two-level-game/src/gameplay/travel.rs#L348) · [Active-level check](../../samples/two-level-game/src/gameplay/resume/capture.rs#L155) · [Snapshot contents](../../samples/two-level-game/src/gameplay/resume/capture.rs#L208)

## P2 · A global action map cannot also supply raw bytes to an importer

An independent text importer reads `controls-a.json` to build menu text. Give the project a
byte-identical `controls-b.json` and select B as global Input. Check, Cook and Player startup
pass. Change only the manifest selection to A. The text build hits the same cached group, but
Check and Cook return `project.content.closure-path-collision` before the Cook contribution
callback runs. Switching back to B restores the workflow. Direct assembly through the public
`CookedContentBuilder` with shared A produced the same catalog digest as B. Its three-member
pack passed deep validation, and the Headless Player read the same 922 bytes of text.

The closure records global Input as `InputActionMap` and a raw read as `AssetSource`. Its
one-kind-per-path rule treats the two observations as conflicting even when they describe the
same file and digest. Represent raw consumption as a use of an observed file. Keep digest and
incompatible typed-owner conflicts at the admission boundary. Sharing A should then work
without requiring authors to maintain two copies, and editing A should invalidate the
prepared source selection. [Input registration](../../src/project_content/input.rs#L246) · [Importer dependency registration](../../src/project_content/domains.rs#L720) · [Closure comparison](../../src/project_content/closure.rs#L521) · [Delivery contract](../architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md#L306)

## P2 · Pre-publication verification misses a new owner of a selected text Product

Cook a pack with four members and five Products, including an Image and a Scene's selected
text subproduct. After preparation, change only an unselected text sidecar's source UUID.
Giving it an unrelated UUID allows both selection revalidation and a fresh Cook. Giving it
the selected text source's UUID still lets `selection.verify()` succeed. Fresh Check, Cook and
source startup all report `project.content.asset-identity-conflict`. The original pack reopens
and yields `Start!`. Giving the very same sidecar the selected **Image** UUID makes the
original selection verifier reject immediately.

The initial Product lookup checks the reference catalogue. The publication callback's later
ownership check takes its selected IDs from the delivered Images. Retain the source owner
selected by each identity-based Product resolution, including compound assets, and use those
IDs for the existing bounded ownership check. A competing claim for the selected text source
should reject the original selection, and repairing the claim should make it usable again.
This probe called the same verifier used by package publication and reopened real packs.
[Initial lookup](../../src/project_content/catalogue.rs#L667) · [Ownership filter](../../src/project_content/delivery_images.rs#L84) · [Selection verification](../../src/project_tooling/project_cook/package.rs#L154) · [Publication callback](../../src/project_tooling/project_cook/package.rs#L537)

## P2 · Clicking a painted pixel can activate the button behind it

Place a five-unit-wide red button at logical `x = 20.4` over an opaque blue button at 1× scale.
The stock UI renderer paints the red button on physical columns 20 through 24. At `y = 20`,
dispatch a press and release at each integer cursor position and reduce both buttons' events.

| Physical cursor X | Actual Vulkan pixel | Button activated |
| ---: | --- | --- |
| 20 | Red foreground | Blue background |
| 21 and 24 | Red foreground | Red foreground |
| 25 | Blue background | Red foreground |

Moving the origin to `20.0` leaves the entire 320 × 180 rendered image identical, including all
150 red pixels, and corrects both edge clicks. Paint rounds the rectangle's edges to pixels.
The hit test uses the fractional logical rectangle after converting the pointer back to
authored coordinates. Give point membership in a painted rectangular control and its clip the
same physical-coverage convention as rendering. Preserve intentionally unpainted hit areas and
continuous coordinates for drag events. The previous zero-pixel fix already rejects empty
coverage; both buttons here have many visible pixels. [Edge rounding](../../crates/nara_ui_kernel/src/geometry.rs#L504) · [Pointer conversion](../../crates/nara_ui_kernel/src/geometry.rs#L346) · [Routed hit test](../../crates/nara_ui_kernel/src/interaction/routing.rs#L31) · [Stock rectangle lowering](../../crates/nara_ui_render/src/planner/lowering.rs#L51)

## P2 · A one-pixel layout change redirects Right from Settings to Quit

Use three 80 × 40 buttons. Resume starts at `(20, 10)`, Settings at `(140, 10)` and Quit
at `(20, 125)`. From Resume, Right followed by activation emits the Settings intent. Change
only Quit's X to `21` and the same input emits **Quit**, 115 units lower. A separate two-button
horizontal menu produced another wrong-direction result. From Settings, Up then activation
emitted Resume. Reversing only declaration order kept both rectangles in place and changed the action
back to Settings.

The directional search ranks the distance along the requested axis before all perpendicular
distance. When that search finds no candidate it selects the first focusable control. Prefer
an aligned neighbor within the requested direction and preserve existing focus when no neighbor
exists. Initial focus and explicitly cyclic Tab navigation can keep their own rules. Bevy's
directional navigator considers alignment and rectangle-edge distance and represents a missing
neighbor explicitly. [Nara candidate score](../../crates/nara_ui_kernel/src/interaction/focus.rs#L150) · [No-neighbor fallback](../../crates/nara_ui_kernel/src/interaction/focus.rs#L86) · [Bevy candidate selection at `779770720e6a`](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_input_focus/src/navigator.rs#L70-L135) · [Bevy missing neighbor](https://github.com/bevyengine/bevy/blob/779770720e6ad916c1de38cf66a67d0e079ed1e3/crates/bevy_input_focus/src/directional_navigation.rs#L417-L436)

## P2 architecture · A keyed list can be displayed after its row selector stops working

Give an editable keyed list two `u64` fields per row, the ID and the editable value. In both an
admitted Scene and a Prefab, use the durable selector for the last row's value. At `1d657a2`, with 85 rows,
changing it to `123456`, Undo and Redo all succeed. At 86 rows the same read succeeds, but
preparing the edit returns `scene.authoring-field-patch-unavailable`. The revision stays
unchanged and a separate scalar root on the same component still commits. The same distinction
holds at 4,096 rows.

The guide documents the limit on the final root patch. Its practical threshold here is 85
rows because the replacement root has `1 + 3 × rows` value nodes and the field-patch limit
is 256. The property edit clones the candidate root into `SetField`, so adding an untouched
row can prohibit a scalar change. During review the working tree gained `ReplaceProperty`
for an existing descendant. Rebuilding those sources and rerunning the same public probes
allowed both document kinds to commit, Undo and Redo at **86 and 4,096 rows**. The eight
existing page tests passed in nextest `d799515f-638c-4e01-8e5e-3c7917921dc7`.
[Property lowering](../../crates/nara_scene/src/authoring/property.rs) · [Field limit](../../crates/nara_scene/src/authoring_decode.rs#L631) · [Public editing contract](../guides/persistent-components.md#L951)

The rebuilt version still has a separate first-lookup cost. Reading the first or last scalar
charged 8,196 scan entries at 4,096 rows. At 32,766 rows it used exactly 65,536 entries, the
public maximum. One more row made both selectors return `ReadBudgetExceeded`. At 32,768 rows,
relative positional reads still returned `1` and `32768`. A write through the row selector and
a write to a separate scalar root on the same component both refused. A direct `SetField` on
that other root also returned `scene.authoring-field-edit-format-invalid`. These observations
identify separate read and mutation limits. If collections of this size are supported for
editing, reuse the admitted source's validated key-to-row lookup for first reads and check
the mutation limits independently. Retain duplicate-key checks, stale revision refusal and
exact Undo. [Key scan](../../crates/nara_reflect/src/property.rs) · [Read ceiling](../../crates/nara_tooling/src/authoring_query.rs#L29)

## Verification and repair order

1. Correct transform interpolation and the Restore write baseline. Verify the exact half-step,
   the near-half-step rendered pixels, and the checkpoint recovered by an independent Runtime.
2. Publish recovery stamps with committed Scene changes. In Project Content, retain each
   selected source's identity and allow compatible uses of one observed file.
3. Align pointer input with rendered edges and directional focus with screen geometry. Use
   the emitted action as the assertion. For larger collection editors, check the first lookup
   and the edit on the same admitted row as well as subsequent display pages.

All 53 distinct Windows release controls passed. They covered the ten Image Cook cases, both Image publication
bindings, fixed-input inheritance and the disabled-parent GPU readback, eight audio-oracle cases,
eight property-page cases, two registered-panel round trips, ten Two-Level round cases and
seven Reference Game cases. Four audio verifier cases covered stock playback, edited and rebound
WAVs, a short cue on hardware Vulkan/WASAPI, and missing inputs before startup.
The page regressions include prepared previews from `60633ca`.
[Cook consumer](../../tests/fixtures/module-consumer/asset-domain-consumer/tests/project_cook_runtime_selection.rs) · [Camera regression](../../tests/fixtures/module-consumer/sprite-render-consumer/tests/geometry.rs) · [Prepared previews](../../crates/nara_tooling/tests/support/property_value_pages.rs) · [Native sound verifier](../../samples/two-level-game/tests/native_audio.rs)

Temporary Rust 1.95.0 probes linked existing release libraries. The new Cook cases used real
packs and CPU Headless Players. Game recovery used real checkpoint files and separate Runtime
instances in one process. Menu activation used normalized events and controlled button
reducers. Camera and menu pixels were read from Vulkan offscreen textures on an RTX 4090.
The new Editor cases exercised the public Workspace with serialized and admitted documents.
The package publication callback was inspected. A complete directory publication, physical
input, swapchain scanout and the full workspace suite were outside these probes.

## Repair status

### Camera interpolation and game recovery

`65bd956` handles local handedness changes in fixed transform presentation, retaining the
current basis between samples and interpolating the translation. An exact invertible endpoint
keeps parent and child samples paired, including an extreme reciprocal-scale case found in
review. A double sign flip uses an equivalent half-turn and preserves the existing shortest-arc
policy. The selected CPU cohort passes 28/28 in run `df269d98-2912-413b-9ffb-4586a98f0a17`,
including sticky Runtime fault and original frame retention for a genuinely singular camera.
Two independent Vulkan cases pass in `397018cd-9e5b-47bf-b94f-15e5ea5a8b54` on a discrete NVIDIA
GeForce RTX 4090. The mirror case checks six complete 960 × 540 outputs, including the exact
fixed step, interpolation fractions 0.45, 0.5 and 0.55, and continued execution through tick 2.
These captures are replayed after the original Runtime retires.

`6491618` stages the exact checkpoint binding and observed level stamps in the game-owned
travel request. Only successful Scene replacement publishes them. The Two-Level Windows
suite passes 166/166 in run `8606d075-bb23-43ec-92c2-e0c7956f8384`. It covers direct and pending
Save after a rejected Restore, external replacement after the read, successful Restore as the
new write basis, and a fresh Runtime restoring a snapshot captured after rejected travel. The
cases exercise real files and independent game instances through the original source Host.

### Cook source selection

In `ba85439`, the closure builder and delivery merger accept a raw read and a typed use of one
path when the observed digests agree. The typed role is retained. A Schema bundle read as raw data
continues to contribute to the Runtime content digest. Different bytes or competing typed uses
still refuse publication. The Cook fence carries the selected owner of each delivered source,
including an intrinsic Input identity and the source of compound subproducts. The original
bounded catalogue verification looks for new competing claims in the configured source roots.
A typed build contribution must publish only Products of its selected source. The common
artifact store still supports explicit multi-source groups for other consumers.
[ADR 0110](../architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md)

The final engine and public source/Cook cohort passes 134/134 in
`5e49b09f-ef40-45f9-8852-25ae28cf6b1a`. The shared action-map case produces the same cooked
catalogue as the two-file control, installs the Input definition in Headless, and decodes the
original raw bytes' digest through the imported compound Product. Changing either a selected
file or its unique owner rejects the old selection. A raw read of the startup Scene sidecar
retains the delivery metadata role. Both supplied and cache-hit typed groups reject foreign
source Products. The first engine run had one fixture missing its explicit processed Input
codec. The final case registers that codec and runs the actual Player.
The independently locked Cook consumer passes its ten build, relocation, Player, and source
identity cases in `f1ac29d3-7e09-459d-8f73-6270c65ce1e7`.

### Presented menu input

In `6b9b5d2`, the UI kernel and stock packet planner use the same snapped content and clip bounds. Hit
membership uses the intersection in physical coordinates. The original authored points remain
available to reducers for drag and other continuous gestures. Directional focus prefers overlap
on the perpendicular axis, then distance. A direction with no destination retains current focus,
while an old frame cannot consume a direction against a newer focus assignment. Retained visuals
without published hits do not change another root's pointer capture.
[ADR 0108](../architecture/adr/0108-ui-surface-presented-session-and-atomic-interaction.md)

The final UI kernel, Runtime UI, and packet planner cohort passes 410/410 in
`2ead686f-9182-490a-afa1-b4e2ff22e049`. It includes fractional physical scales, ancestor and
Surface edge rounding, unchanged routed coordinates, capture across retained visuals, and
directional activation before and after a one-pixel change in an unrelated button's position.
The independent Vulkan stock UI cohort passes 2/2 in `1b1a49e8-7507-445a-ab3d-a2437b08d8ee`.
The two origins, 20 and 20.4, yield the same complete image. Actual red and blue pixels and
the immutable frame's hit results agree at both rectangle boundaries. Press/release routing
and keyboard actions are covered in the CPU cohort.

### Selective collection edits

`c4f78de` covers the reviewed 86-row failure through the registered panel's 4,097-row Scene and Prefab
trajectories. History retains the selected value and its inverse selector, including key edits.
The original file format limits still bound the final source. This initial repair kept the
65,536-entry key scan. The subsequent indexed read and encoding repair addresses the
32,767-row refusal.
[Owner design and verification](../architecture/authoring-read-projection-design.md#selective-edits-in-authored-collections)

### Large keyed lookup and file publication

In `06ce6a0`, direct source reads and prepared previews use the positions retained by validation of the
exact Scene or Prefab record. Canonical defaults use their Schema's shared positions. The
candidate still resolves ordered writes against its evolving state and rejects duplicate keys.
The current file cap remains 16,777,216 bytes. For the 32,768-row two-`u64` Scene, canonical RON
with four-space indentation used 23,604,986 bytes. Two-space indentation in `617b920` uses 13,577,490 bytes.
The Prefab uses 13,577,491 bytes under the same setting. Both formats pass publication checks,
and RON round-trips each complete document.
[Owner design](../architecture/authoring-read-projection-design.md#indexed-reads-in-admitted-keyed-collections)

The independent registered-panel run `a62d78c6-1985-44eb-863d-b255ccc53f72` passed 20/20.
Both document kinds complete the public workflow at 32,768 rows, including a separate scalar
edit, ordered key and value changes, duplicate-key refusal and exact persisted history.
The shared Reflect, Scene and Tooling cohort passed 1,382/1,382 in
`546bba23-4246-4ad4-aaa2-5cb79aa185f1`. Its numeric last-row page starts with five scan entries.
The original Drawn workspace passes its native DX12 regression in
`80e0981f-6db6-4f36-b9c3-2886cbf06357`. The owner design records the linker retry and measurement scope.

### Qualification scope

The repair runs used Windows x64 and Rust 1.95.0. The affected engine libraries pass correctness
Clippy. Formatting passes for the root, sample, stock composition consumer and affected module
consumer packages. The module-consumer formatting check runs package by package because the
combined Windows command line exceeds the OS limit. GPU evidence covers offscreen Vulkan
readback and the hidden native Drawn Editor on DX12. The full workspace and other platforms
remain outside this qualification.
