# Independent UI module consumer

With its default, empty feature set, this small project exercises only the public `nara_text`,
`nara_ui_kernel` and `nara_ui_widgets` engine interfaces. Its tests shape and rasterize a label,
compile an immutable status panel, and compile a controlled button with focus and semantic actions.
A fourth test constructs complete binary
persistent identities without Scene types: distinct contexts stay separate, rebuilding the same
root/local composition preserves keys, and both buttons retain focus and semantic actions. The
independent Image consumer compiles an opaque Product-shaped name and two provider-scoped external
tokens into a retained frame without importing Asset, ECS or renderer types. It creates no native
window or GPU surface and cannot manufacture presentation authority or persistent asset bindings.

The editable-text consumer creates a bounded field through ordinary public text/widget APIs,
cancels preedit without changing its committed buffer, rejects input from an earlier focus,
performs local Undo/Redo and compiles the exact committed semantic value. It does not admit native
focus or qualify an Inspector, OS input-method delivery or platform accessibility.

The font-reconfiguration consumers explicitly replace/reorder real font faces while keeping a
field's key, focus token, draft, selection, preedit and local history. They compile the resulting
fractionally scaled/clipped field, verify committed semantics and exact glyph/raster provenance,
then use its new geometry for point editing. Different resource limits refuse without replacing
the editor; matching limits permit a retry. Adding the fallback changes a missing intersection
glyph into supported output. These callers use public `UiTextField::focus`, not private journal
submission, so they qualify local field/compile consumption rather than presented input routing
or live native Editor font switching.

The separate Cargo workspace and lockfile keep this dependency graph independent of the engine
and the broader module-consumer project. Base and `presentation` builds must be checked separately
from those workspaces so their Runtime, Scene or native renderer features cannot unify into this
fixture's normal dependency graph. The fixture-local Lyon drawing dependencies remain in both
configurations.

The optional `presentation` feature adds normal library dependencies on `nara_asset`,
`nara_image` with `render-prepare`, `nara_render` and `nara_ui_render`, all with default features
disabled. It enables no `runtime` feature. Value-only Window identities can be transitive inputs;
App/ECS installation, filesystem/task services, game UI, Editor, WGPU, Winit and Dear are outside
this normal closure. The ordinary base configuration still selects only text/kernel/widgets.

`presentation::CapturedUiImage` is reusable library code, also imported by the existing native
`render-output-consumer/examples/ui_surface.rs` executable. `rgba8` creates procedural Image
content with an external retention owner, allocates a real typed anonymous handle, calls
`Assets::commit_loaded`, then uses `PreparedImageSnapshot::capture`. `capture` also accepts a
caller's existing typed store. `insert_into` transfers the complete snapshot to `UiPreparedImages`;
`authored_node` and `authored_root` author ordinary image visuals. The library retains the typed
handle and complete AssetVersion/nominal slot revision/content hash with the original pixel Arc
and lifetime owner. It never assembles arbitrary snapshot identities or installs a journal,
backend, device or Runtime. The finite `capture_swatch` helper supplies exact 2-by-2 sRGB RGBA8
red/green/blue/yellow input to both consumers.

Six feature-gated CPU tests consume the original `UiSurfacePresenter`, read-only journal and
caller-owned interaction session. They cover preparation without publication, original completion
rejected intact by a foreign owner, deferred retries with the same packet/backend generation,
abandoned permits and retirement with late presented/deferred/dropped outcomes. Resource cases
retain escaped packets through same-ID/same-loader-version slot replacement, removal/reinsertion
and store replacement, including changed pixel content, nominal revision changes and final-owner
release. Named identities and provider-scoped external tokens preserve the original missing and
sticky-ambiguity behavior. These tests explicitly simulate a trusted CPU backend through public
binding/completion APIs; they do not prove actual native drawing or mint engine-private receipts.
Actual window presentation remains the separate native consumer's qualification.

`keyed_editors` is an independently authored reusable row composite. Two fields share the
accessible label `Value` but retain state by parent/item/property identity. Reorder, insertion
and repeat composition preserve the exact draft, selection, preedit, focus token and local
history. The chosen filter policy evicts omitted fields; a restored row starts fresh and old
scoped input cannot reach same-position replacements. Binding/schema mismatch preserves the
previous membership until explicit replacement. The component retains no journal, GPU, document
owner or native input method. Local tests use public field focus; presented input routing is
separately exercised by the native output consumer importing this actual library.

The composite now uses the independently authored `MeasuredColumn` container. It measures the
actual title and each field subtree, then places children through scoped public layout access.
No Taffy identities/storage or engine-private node construction are used. Typography changes
alter the title's measured height and subsequent row position; compatible repeat composition
shares its immutable layout description and retained keyed state. Available space and exact
outer allocation are distinct, and omitted children must be explicitly hidden. The native
output consumer imports this same library, preserving the original full input/retirement proof.
This layout interface alone does not qualify custom paths, canvas motion/scroll, foreign panel
hosting, automatic invalidation or a layout performance improvement.

`CurveCanvas` is a separately authored drawing consumer. It uses fixture-local Lyon dependencies
to tessellate a cubic connection into neutral `UiMesh` facts, then composes ordinary node/port
buttons over that geometry in the same paint stream. Explicit pan changes move both geometry
and control bounds within a fixed clipped viewport; the caller owns selected state and button
reducers. Local tests inspect actual triangles, paint order, fractional mapping and matching
hit/focus/semantic facts. The native output consumer imports this same library. No path engine,
graph widget, renderer dependency or engine-private construction is required by the component.
This fixture retessellates on composition; it does not qualify a geometry cache, final-size
drawing callback, curve hit testing, a complete graph editor or performance.

`CurveCanvasState` retains bounded desired pan/zoom and one drag anchor for that viewport.
`authored_view` transforms both mesh and controls, keeping label fonts fixed. The viewport
declares motion, scroll, three capture buttons and `TargetGeometry` continuity; child buttons
retain their ordinary behavior. The reducer accepts only already admitted Target batches for
its exact viewport. It supports unclamped pointer positions during capture, matching release,
focus/capture cancellation, cursor-anchored zoom with distinct line/pixel sensitivity and Shift
horizontal pan. It owns no journal, receipt or domain history. CPU tests cover view admission,
geometry, identity and bounds; the native output consumer supplies actual routed drag/wheel
and cancellation evidence, including dragging across changed presentations.

`styled_property_row::StyledPropertyRow` is another independently authored **normal library**
component in the base dependency closure. It combines a body label, the original retained
`UiTextField`, a controlled Apply-style button and a caption status inside a bordered, rounded
`UiVisual`. Its public constructor takes a root, an explicit collection scope, a `StyledProperty`
with an item ID/label/initial value/action label/action ID, the effective `UiAppearance`, and the
caller's `TextEngine`. Its visual keys frame scope and item ID independently; equal labels and
reordered items do not supply identity. The caller retains each row in its original
`UiStateStore` by parent/item key and exact binding/schema. Selection and replacement keep the
store's existing membership rules. The component creates no document, command, journal,
native, ECS, renderer or GPU owner and adds no dependencies.

Use `set_appearance(effective_scope, engine)` between membership passes to adopt an ordinary
appearance value. Resolve an optional local palette with `local_override.unwrap_or(parent)`;
removing the override restores that parent value. There is no global theme mutation or override registry.
The row uses shared `UiAppearance` body/caption text, regular control metrics and secondary button
recipes; the field's solid brushes, padding, selection and focus outline are resolved from those
same metrics/palette. Reconfiguration admits typography and retained local history on the same
field before adopting the new appearance. Failed admission retains the previous editor and
appearance. Color-only changes retain compatible editing-layout provenance. Density changes
insets/gaps/padding; explicit user text scale changes font size and line height. Host/native scale
remains the original compiler/presenter transform.

`authored_node(outer_width, status)` takes an explicit logical width. Shared `UiPropertyRow`
measures all four actual child subtrees through `UiLayoutChildren` and places their exact outer
boxes. The field uses an intrinsic width preference and derives its viewport from final allocation;
the consumer no longer premeasures text or duplicates the property layout implementation.
Inline mode reserves eight body-size units plus input padding for the editable value
and aligns actual first-line baselines. The field supplies its actual outer measurement and
baseline through the same public child measurement, including negative line-block origins;
the component does not reconstruct field metrics from font size or padding.
When that inline budget does not fit, label and field each take the content width, followed by
the measured action/status footer. The footer wraps onto separate lines when the status's actual
min-content width cannot fit after the button. Labels/status wrap; the button clips its visual text
within its padded box and keeps its full semantic action/name. Widths too small for control
padding plus one content unit refuse explicitly. Changed captured geometry gets a new layout
description; compatible compositions share the previous one.

The label combines a caller-designed notched-bookmark `UiMesh` and ordinary text through shared
`UiIconLabel`. Its logical envelope, gap and top offset from the real label baseline are explicit;
the mesh contributes no intrinsic text metrics, extra action or hit target. The original mesh
renderer handles it. Color changes replace only the affected captured mesh and brushes, while
compatible icon/row layout descriptions retain their identities.

The parent must allocate the requested outer width, use the same ordered fonts for text and
compilation, and recompose after width or typography changes. The field clips its display at the
resolved viewport and keeps the entire admitted UTF-8 value in its buffer and semantic value.
Callers obtain point/IME viewport width from the field's actual presented rectangle, reduce focus
and edit through `field_mut()`, and reduce already admitted button batches through `reduce_button`.
`button()` exposes the original key/action. The generic store's direct-field focus/text helpers
do not discover this nested field; callers use its original field API explicitly. Input admission,
focus retirement, application of intents and domain history stay with their original owners.

The added `tests/styled_property_row.rs` CPU workflows cover two equal-label keyed rows with
reorder; draft/preedit/selection/focus and Undo/Redo across palette, density and text-scale changes;
palette override removal with exact sibling paint preservation; independent normal/compact,
user-text-scale and native-scale geometry; actual wide-row baselines and narrow overflow; a full
4,096-byte committed value containing CJK/combining text; updated Point/IME padding; and refusal
without replacing a retained editor. A real admitted Noto CJK fallback additionally checks exact
font provenance and baseline alignment through both densities, 1×/2× text scale, full 4,096-byte
history and active preedit. Allocation regressions verify full text, selection/caret/Point/IME at
actual widths; unchanged composition preserves the authored description, shared mesh storage and
warm compatible text layouts. Prepared-text containment uses its inherited clip while controls
retain raw-rectangle containment. The styled consumer's nine CPU tests pass in the current
integration; native presentation is
separately qualified. The finite default font policy still reports missing glyphs for other text:
complete CJK semantics alone does not prove complete CJK glyph coverage.
There is no text-measurement cache in this component, no virtualized membership, native input or
accessibility proof, measured performance improvement, or complete styled-library qualification.
The main integration's additional native evidence has its own scope.

Run from the repository root, using the shared `target` directory through `CARGO_TARGET_DIR`:

```text
cargo nextest run --manifest-path tests/fixtures/ui-module-consumer/Cargo.toml --locked --test-threads=1
cargo nextest run --manifest-path tests/fixtures/ui-module-consumer/Cargo.toml --locked --features presentation --test-threads=1
```

Run Cargo commands serially. The [fixture guide](../README.md) describes the other consumer projects.
