# Bounded editable text

`nara_text::TextEditor` owns a single-line local buffer. `nara_ui_widgets::UiTextField` adds a live
field identity, focus lifetime and clipped drawing. Neither type applies document changes, owns
a native input method or grants presentation authority.

## Font configuration

Construct an ordered immutable font set and give the same selection to text editing, UI compilation
and packet preparation:

```rust
let fonts = vec![primary, fallback];
let mut text = nara_text::TextEngine::with_fonts(fonts.clone(), Default::default())?;
let mut compiler = nara_ui_kernel::UiCompiler::with_fonts(fonts.clone(), Default::default())?;
let mut planner = nara_ui_render::UiPresentationPlanner::with_fonts(fonts, Default::default())?;
```

The first face is primary. Each glyph key retains the actual selected face; rasterization uses
that face, including its original collection index. A planner missing a referenced face refuses
before changing its atlas. Retained paint packets own their glyph masks independently of the font
engine. A missing shaped glyph reports its original scalar or complete multi-scalar source
sequence. Sequence diagnostics identify incomplete output without guessing which normalized
character failed. No system font discovery occurs inside these modules.

`FontFace::from_bytes` selects one face from an OpenType/TrueType container.
`FontFace::from_shared_bytes` permits several selections to share immutable container bytes.
Admission permits at most eight distinct faces, 32 MiB per container and 64 MiB across selected
faces. The aggregate conservatively counts the full container for each selection, even when bytes
are shared. The Host decides how to obtain fonts and which scripts its font policy supports.
Faces use their default variation instance; private family registration suppresses automatic
weight, width and slope synthesis. Custom variable-axis selection is outside this interface.

## Local editing

Create one owner and keep it across compatible redraws:

```rust
let mut draft = nara_text::TextEditor::new(&mut text, "12", 18.0)?;
draft.edit(&mut text, &nara_text::TextEdit::SelectAll)?;
draft.edit(&mut text, &nara_text::TextEdit::Insert("34".into()))?;
assert_eq!(draft.text(), "34");
draft.edit(&mut text, &nara_text::TextEdit::Undo)?;
assert_eq!(draft.text(), "12");
```

Parley supplies visual order, bidi affinities, selection, hit testing and caret geometry. Its
complex-script segmentation data is enabled without system font discovery. ICU
grapheme boundaries constrain movement, pointer placement and forward/backward deletion. Shaped
ligature components do not become independently editable characters. An insertion which joins
neighboring characters leaves its caret at a complete grapheme boundary.

The editor rejects controls and line separators. Each buffer remains within its original
`TextLimits`, with at most 32 Unicode scalars per grapheme and 32 Undo/Redo entries in total.
The grapheme ceiling is checked against the complete proposed text before entering the pinned
shaper, including when insertion or deletion joins neighboring text. Every operation adopts its buffer,
selection, layout, composition and history together after complete validation. A rejected
operation preserves the previous observations and history. Another engine may drive an existing
editor for ordinary actions only with the currently configured ordered font identities and
original complete limits and adopted typography.

`TextEditor::reconfigure(&mut replacement_engine, typography)` explicitly changes the ordered
font selection and validated `TextTypography`. Logical size and optional absolute logical line
height are distinct; an automatic height uses the actual selected fonts' metrics. An absolute
height controls line spacing and may be smaller than the glyph ink. It does not scale glyphs or
define their clip. `reconfigure_fonts` retains the adopted typography. The replacement engine
must keep the original limits. Reconfiguration reshapes and
admits layouts of the current display, composition origin and every retained Undo/Redo state
before adopting anything. Refusal preserves all editor observations, history, cached paint and
the previous font association. This complete-state operation can reshape up to 34 bounded
buffers. Reusing the same font set/order, typography and limits is a no-op and preserves paint identity.
Glyph raster/target-scale admission remains separate and may refuse later painting; successful
layout reconfiguration alone grants no draw or presentation authority.

Success retains committed text, preedit, selection/affinities and local history without recording
an edit. Paint receives new font provenance even if the changed fallback is unused. Earlier paint
snapshots stay immutable and can still be consumed under their original ordered font selection.
`UiTextField::reconfigure_fonts` adds the same operation while retaining its live field key,
focus token and enabled state. The caller must coordinate matching compiler/planner fonts and
publish/admit the resulting geometry through its existing Host; this local operation does not
replace that presentation or input handoff. The current native Editor font policy is still fixed
at construction; this interface alone does not qualify live Editor font switching.

Pinned Parley 0.11 retains its construction-size margin for an empty buffer's IME context even
after a FontSize change. Nara retains Parley's layout-derived vertical geometry and resolves that
three-character horizontal exclusion margin from the adopted logical size. Reconfiguration tests
cover the empty-buffer case rather than assuming visible text proves candidate geometry.

`UiTextField::reconfigure(engine, typography)` retains that same field/focus/history while
adopting validated size and line spacing. `set_appearance` adopts one value containing padding,
foreground, field visual, selection and caret brushes. Drawing, Point and IME use the same adopted
padding. Resolve the new typography before publishing its dependent geometry; an older frame
does not authorize Point with a successor field's layout.

`Preedit` changes `display_text()` and its layout while `text()` retains the original committed
buffer. Its optional cursor endpoints are UTF-8 byte offsets relative to the proposed preedit.
`CancelComposition` restores the original text and selection. `Commit` replaces that selection
once and records one local edit. Other navigation/editing actions cancel composition before
operating on the committed buffer. Local Undo/Redo does not invoke document history.

For intrinsic measurement, `TextLayoutRequest::with_min_content()` asks the original shaper for
its minimum content width and breaks that same shaped layout at that width. Natural, explicit
finite wrapping and min-content remain distinct request/cache identities. Selecting a finite
wrap width replaces min-content mode. This performs one shaping operation under the existing
source/glyph/line limits; an intrinsic empty/space result may internally have zero width without
widening public finite-wrap admission.

Use `UiLayoutChildren::min_content_width(child)` when a custom container only needs a subtree's
minimum width before choosing its allocation. Wrapped text in that scoped query uses
`TextEngine::measure_for_width_with_glyph_limit`, an observation with real dimensions/baseline
and no drawable glyphs, raster keys or output authority. Its intermediate line storage remains
bounded by admitted source bytes and shaped glyphs, while final output line admission is deferred.
Direct `TextEngine::layout` and regular child measurement retain their original line limits.
The compiler separates observation and drawable cache entries, then resolves final placement and
paint under the original limits. A paragraph that fits the chosen width can therefore survive
an exploratory min-content query without admitting an over-budget final layout. Source/glyph and
candidate layout-work accounting still applies to both kinds, including compatible cache hits.

## Field and Host responsibilities

An independently authored native frontend can use `nara_ui_render::UiSurfacePresenter` for one
Host-allocated, non-reused surface lifetime and fixed render target. Keep its interaction session
and fields outside this presenter. `prepare(frame, images)` returns the original backend frame;
it leaves the read-only journal unchanged. With WGPU, use `WgpuUiSubmission::capture`,
`submit_for_display` and that submission's designated `display()`. After the exact composite
completes `submission.presented(composite)`, return each original outcome and call `drain`.
Only `UiSurfacePresentationEvent::Presented` supplies the new routing receipt. Merely planning,
submitting an offscreen image or drawing another mirror occurrence cannot publish that route.

Resolve pointer/key actions against this journal and reduce its focus facts before admitting
field text events. On a successful new presentation, reconcile the caller's live interaction
state; never restore a whole interaction snapshot captured during preparation. `retry()` retains
a deferred attempt's original identity and backend requirement. `retire()` immediately revokes
the surface's routes; original outstanding/dropped outcomes must still drain until
`is_quiescent()`. Render Host output/native-use retirement remains a separate physical boundary.
Attempt faults preserve a previous valid route; target/device faults withdraw its history and
close admission. The installed Slot remains until the Host explicitly retires it with its own
retirement generation.
The [independent native consumer](../../tests/fixtures/render-output-consumer/examples/ui_surface.rs)
demonstrates this bridge, including a modal popup and retired-route/token refusal. It is not
the whole Editor Host, platform IME delivery or a neutral presentation dependency split.

`TextEditor::paint_snapshot()` publishes an opaque immutable `TextPaintSnapshot` with the complete
display source, ordered font identities, logical size and positioned editing layout.
Its `first_baseline()` exposes the actual original first line without exposing layout storage;
old snapshots retain that observation after the live editor changes.
`TextEngine::prepare_window()` admits that complete source under the receiver's byte, line, glyph,
diagnostic and scale limits and returns only glyphs whose actual raster placement intersects the
local clip. `prepare_window_at()` additionally takes the exact logical draw origin and physical
content origin used by the renderer; target-scale size and subpixel phases are rebuilt from those
baselines. Both methods preserve full metrics and original visual order/coordinates without
reshaping substrings. Raster extent policy applies even to excluded glyphs. A returned layout
owns only its window glyphs and reindexed runs, with no retained complete-layout resource.

`UiNode::with_prepared_text(UiPreparedText::new(snapshot))` measures the snapshot's exact dimensions
and prepares its paint window from final inherited/content geometry. Typography cannot be
overridden independently. The compile candidate charges the complete display bytes once per
published snapshot identity, sharing the ordinary source-admission ceiling, but reuse performs no
new shaping. Every draw charges its output glyphs, including repeated draws of one snapshot,
against the same combined glyph ceiling as ordinary text. Each prepared draw scans at most one
receiver-admitted complete glyph set and rasterizes at most one mask per distinct target key;
only placement metadata is reused within that call. Candidate node limits bound draw count. This
is bounded raster/scan work, not a claim of constant-time scrolling or cached raster preparation.
Snapshot publication retains one bounded copy of the editing layout until its display changes.
Field selection, pointer mapping and IME continue to use the complete editing layout.

`UiPreparedText::with_draw_offset` shifts that immutable layout's drawing origin in authored
logical units while retaining the content clip and intrinsic measurement. Window preparation
and final raster placement use the same shifted origin. This lets a field include negative line
block origins under short line spacing without shifting its clip or reshaping a substring.
Non-finite offsets are refused at normal style admission.

`UiTextField::authored_node` draws its text, selection and caret within one content clip. Its third
argument is the preferred intrinsic outer width, not a promise about final allocation. Min-content
reserves padding plus positive content width; exact parent placement recomputes viewport, scroll,
selection and caret at the allocated width. Read the actual accepted field rectangle and pass its
logical outer width to pointer mapping, `ime_area()` and `layout_measurement()`.
The candidate area is local logical geometry; the Host translates it to the actual native window
and scale. Semantic values contain committed text, including an empty string, while preedit stays
out of both the field value and its internal text semantics.
`layout_measurement(width)` exposes the same geometry's minimum outer size and actual first
baseline without shaping again. The authored field's custom layout publishes that baseline
through ordinary `UiLayoutChildren::measure`; a parent must not derive it from font size or
padding. Automatic height uses the actual font line blocks plus padding; pixel height retains
at least that minimum. Exact parent/percentage allocations must accommodate it or explicitly
own their clipping policy.
The field retains immutable field/viewport layout descriptions keyed by their captured metrics,
preferred width and selection/caret inputs. Unchanged composition reuses them; changed captured
inputs replace them while old snapshots retain their original descriptions. Colors alone do not
change layout identity. Complete prepared text remains part of the child description, so equal
geometry never hides changed content. Exploratory measurement cannot change the retained editor.

## Scoped appearance and measured controls

`nara_ui_widgets::UiAppearance` is a caller-owned effective value containing a semantic palette,
`UiDensity` and independent user text scale. Pass the nearest intended scope explicitly and
replace a local override with its parent's value to reset it. Resolving a recipe performs no
domain action, listener registration or document mutation. Other libraries can use their own
roles and drawing while consuming the same low-level interfaces.

Use `text(role)`, `control_metrics(size)`, `button(...)` and `field(...)` to share typography,
padding and state visuals. Body, caption and title use absolute logical size/line-height pairs;
the original Host applies device scale once. Compact density changes spacing independently of
text scale. Focus, selection and validation retain separate cues with unchanged hit geometry.
Pointer capture preserves a held press during drag-out, while the pressed appearance requires
the pointer to remain over the button. Reentry restores that appearance; release outside emits
no action. Focus and selected-state cues remain independent of the held press.

The current built-in recipes resolve these values before device scaling. They are the first
dense desktop defaults, not calibrated physical units or universal game-UI sizes. The
[implementation](../../crates/nara_ui_widgets/src/appearance.rs) owns their exact values.

| Recipe | Normal | Compact | User text-scale behavior |
| --- | --- | --- | --- |
| Body size / line height | 14 / 20 | 14 / 20 | Multiply both values |
| Caption size / line height | 12 / 18 | 12 / 18 | Multiply both values |
| Title size / line height | 16 / 24 | 16 / 24 | Multiply both values |
| Small / Regular / Large minimum control height | 24 / 28 / 34 | 22 / 24 / 30 | Grow to fit scaled body line height plus vertical padding |
| Small / Regular / Large minimum vertical padding per edge | 2 / 4 / 7 | 0 / 2 / 5 | Remains logical padding; larger minimum height may add extra padding |
| Control horizontal padding per edge | 10 | 8 | Remains logical padding |
| Group gap / panel inset | 6 / 12 | 4 / 8 | Remains logical spacing |
| Control radius / border stroke | 4 / 1 | 4 / 1 | Remains logical geometry |

These heights are recipe minima. Measure the actual child and field baseline through the layout
interface; do not reconstruct it as a fraction of font size or assume every fallback run fits
inside the primary face's metrics. A declared short line box controls spacing, not glyph ink
clipping. Resolve from the original logical recipe on every scale change rather than repeatedly
scaling rounded device geometry. The native Host owns the final logical-to-device mapping.

`UiAppearance` currently scopes palette, density and text scale. Font family/weight/features,
icon sizing, forced palettes, motion and persisted preferences are not implied by that value.
Palette admission checks finite channel ranges; it does not certify contrast for arbitrary
overrides. `UiColor` stores linear-sRGB RGB and straight alpha; `rgba` accepts those channels
directly. Use `from_srgba8` (or opaque `from_srgb8`) for encoded sRGB bytes such as design HEX.
Only RGB receives the transfer conversion; alpha stays linear and RGB is not premultiplied.
Image source encoding remains a separate texture declaration. Qualify
text, focus, selection and error cues against their actual surfaces.
Other component libraries can own additional typed roles and recipes, compose these controls,
or provide their own visuals without extending a central widget enum. Metric overrides use
`UiControlMetrics`; text buffers, input lifetimes and domain history keep their original owners.
Before adding a recipe, specify its combined states, actual minimum/baseline, overflow, scope
reset, resource failure and invalidation, then exercise it with a complete consumer. See the
[design inventory](../architecture/ui-foundation-design.md#style-layout-and-visual-component-readiness)
for the remaining capability and qualification requirements.

The [user-supplied Editor reference](../design/editor-reference/README.md) records proposed
future visual values separately. In particular its 13/20 body, system-font policy, encoded HEX
colors and 2-unit focus stroke are not the current implemented defaults above.

`UiControlGrid` measures and baseline-aligns children, then wraps them according to available
width. Its declared minimum cell width is a lower bound: actual child min-content width under
the admitted typography determines when to wrap. A cell wider than the available allocation
keeps its explicit overflow policy. `UiAnchoredPopup` grows from its preferred width to actual
child min-content width before fitting available bounds above/below the captured logical anchor;
its caller supplies oversized-content scrolling, dismissal, modal scope and focus restoration.
Neither utility owns a window, input route or domain action.

`UiPropertyRow` measures four arbitrary child subtrees in label/editor/action/status order. Wide
rows reserve natural label/action/status widths and give the editor the remainder. Narrow rows
stack label/editor and wrap the footer according to actual min-content width. Measurements use
actual baselines and remeasure at exact allocated widths before placement. Authored widths inform
natural sizing; final slots override width while preserving each child's height and overflow policy.
The row owns no property value, draft or action reducer. Retain its immutable `UiCustomLayout`
while its minimum editor width/gap stay compatible.

Feedback that changes during a pointer gesture needs an owner-chosen stable size. The Inspector
measures its finite status labels with the admitted fonts once per typography and reserves their
maximum envelope, while authoring only the current caption. Canceling preedit therefore cannot
resize nearby hit geometry before a Copy release. Generic property rows still measure their
supplied children; they do not own a global status vocabulary or relax input-capture continuity.

For custom containers, `UiLayoutChildren::measure(child, constraints)` preserves the child's
authored width; definite available space supplies a wrapping hint and percentage basis.
`measure_at_width(child, outer_width)` overrides that width to obtain the actual height/baseline
before an exact placement at the same width. Use it when shrinking/growing an allocated column,
including children with pixel/percentage widths. Authored height and overflow remain child policy.
It neither selects final placement nor grants a side-effect phase; the same root-local poisoning,
request accounting and candidate-wide work limit apply. The shared row, icon label, control grid
and anchored popup use this path for final allocated-height measurement.

`UiIconLabel` accepts an ordinary decorative icon node followed by a text-bearing label. Supply
the icon's logical size, gap and explicit top offset from the actual label baseline. Captured
`UiMesh` geometry uses the original compiler/renderer; the recipe neither rescales the mesh nor
adds an icon font, action or hit target. Its owner controls asset readiness, missing-icon fallback,
optical inputs and overflow. The independent property consumer uses both recipes without a new
widget registry or document model.

The compiler retains at most `min(max_nodes, 256)` successful plain-text requests within its
original immutable font policy, additionally bounded by its source-byte and glyph limits.
Drawable layouts and intrinsic-width observations share those bounds but cannot satisfy each
other's requests. Exploratory geometry does not enter the final layout cache.
Complete requests include text, typography, wrap mode/width, alignment and raster scale; brush
colors do not change shaping. A warm compatible color change can reuse layout. Every candidate
still charges full source/layout-glyph admission and every draw charges output independently.
Eviction or a new compiler rebuilds equivalent output. These bounds describe retained content,
not measured heap allocation or a native frame-time result.

The [independent styled property row](../../tests/fixtures/ui-module-consumer/src/styled_property_row.rs)
combines retained text with a label, action and status through these public interfaces. The
[styled workspace plan](../plans/2026-11-05-styled-source-workspace-plan.md) records first-party
integration and actual qualification separately from the complete future component library.

## Field input lifetimes

A field incarnation has a unique live key even when its semantic property is equal to an earlier
field. After admitting focus, the Host obtains a `UiTextInputToken` through `focus()` or by reducing
the presented interaction's focus facts. Queue each text event with the token admitted at ingress.
Do not obtain a new token when draining an old event. Blur, disable and field replacement revoke
that lifetime; reenable can restore logical focus with a fresh token.

The Host retains exact presented-frame admission, native focus and IME ownership. It must revoke
native input ownership when a field, panel, endpoint or window retires. Clipboard, keyboard
shortcuts, native candidate placement and platform accessibility remain adapter responsibilities.
The current AccessKit bridge maps the TextInput role, label, committed value and focus; it does not
advertise text selection or SetValue actions.

An Inspector separately retains semantic property identity, exact document/schema basis and
submission results. Typing, composition completion and focus loss must not automatically Apply.
Explicit Apply, conflict recovery, document Undo/Redo and persistence use the existing Application
owner. These primitives alone do not qualify the complete
[Nara-owned Inspector](../plans/2026-11-04-owned-inspector-and-editable-text-plan.md).

## Keyed composite fields

`nara_ui_widgets::UiStateStore` keeps local fields and arbitrary component state by exact
root/parent/explicit-child identity. The store belongs to one frontend instance. Labels and row
positions are presentation data; use domain item IDs for child keys. A `UiStateBinding` supplies
an opaque model binding and schema generation, while the store checks the Rust state type.
Use a stable binding across compatible revisions; changing a document revision alone need not
replace every draft. Type, binding or schema changes refuse ordinary reuse. Explicit `replace`
starts a fresh incarnation after its complete initialization succeeds.

Start a membership pass with `begin()`, derive child paths from `pass.root()`, and obtain a typed
handle through `try_state(key, binding, initializer)`. `pass.read(&handle)` exposes existing or
staged state immutably for node construction. `finish()` adopts the selected membership and
evicts omitted state. Duplicate keys, capacity/initialization failures and incompatible reuse
refuse the pass, even if a caller catches the individual error. Dropping an unfinished pass
keeps previous membership. Repeating compatible composition does not rerun initializers or
mutate drafts. Initializers construct local state only; document commands, tasks and subscriptions
belong to explicit updates under their existing owners.

Outside composition, use `get_mut` for explicit local updates. For directly retained `UiTextField`
values, `reduce_text_focus` consumes the Host's admitted focus dispatch without a caller-owned
field map. After current presentation/focus admission, `text_input(&handle)` captures a
`UiStateTextInput`; queue this exact value with an incoming text event. `edit_text` validates both
the store/field incarnation and the focus token. Obtain a new token only for a new ingress event,
never while draining an older one. Its `TextEditOutcome` tells the frontend whether dependent
text/layout/paint/semantics must rebuild. `point_text` uses the same scoped input and the field's
local logical viewport for caret placement/selection. No automatic redraw or document Apply is
performed.

The [independent row composite](../../tests/fixtures/ui-module-consumer/src/keyed_editors.rs)
uses the same `Value` label for distinct keyed fields and no central widget enum. Its tests
preserve drafts, selection, preedit and Undo through reorder/insert and repeated composition.
It deliberately evicts filtered rows: their drafts and preedit are discarded, and restoring a
row initializes fresh state. A form that preserves drafts while controls are hidden must declare
a longer-lived owner instead. This prerequisite is not a virtualized-list pinning policy.

State adoption is separate from compiler and presenter acceptance. Before accepting further
events, the Host releases/reconciles native focus, capture and IME according to its existing
contracts. An evicted field's earlier pixels grant no input to a replacement. Destroying an
arbitrary state value cannot cancel a task or release a native input method on the Host's behalf.
The store bounds live slots and exact key-path bytes; its old-plus-staged slot peak is at most
twice the configured slot limit. Arbitrary payload memory is not bounded by that count. Direct
text-focus reduction scans retained slots; no constant-time dispatch or performance gain is
claimed. The interface retains native-process type identity, not a hot-reload state codec.

## Native Editor integration

The ordinary Scene/Prefab Inspector's default `Document` tab uses Nara controls. Its string and numeric
fields use Nara drawing and local editing. Boolean values use an explicit `Set true` or `Set false`
button. Entity and asset references use bounded text editing; `Choose target` also offers the
current document's source-local entities in pages of at most 32 choices. Labels include each
entity's exact ID to distinguish duplicate names. The same picker is available for entity-reference
leaves inside supported structured fields. Choosing a target changes only that leaf in the retained
draft; explicit whole-field Apply publishes it. Supported Optional, Record and scalar-item List/Map fields use
`Edit value` to open a structured draft. Unsupported shapes remain read-only. The displaced ordinary
Dear Inspector and competing optional tab are removed; specialized UI properties and the workspace
shell retain their separate consumers while those complete workflows are migrated.

Select a field with the pointer, drag to select, or use arrows, Home/End and Ctrl+A. Within the
active Inspector image, Tab/Shift+Tab move through enabled fields and buttons; Enter or Space
activates a focused button. Ctrl+C/X/V copy, cut and paste Unicode selections through the same
clipboard owner as the workspace shell. A failed clipboard write leaves a Cut's original text
and selection intact. Multiline or oversized paste is rejected with a field error, without changing
the buffer or local history. Ctrl+Z/Ctrl+Y change the focused buffer. On macOS the implementation
uses Command for these shortcuts, with native qualification still required on that platform.
Explicit Apply and non-composing Enter submit that field through the
ordinary document owner. Escape first cancels composition, then can discard the draft. Losing focus
or switching Inspector tabs preserves unapplied input. A locally queued or admitted submission
locks the captured draft until its exact result returns; a refusal preserves it for correction.
Finish or cancel preedit before pointer selection or Apply. Pointer placement uses the text origin
from the displayed frame and refuses an unpresented replacement buffer.
Reapply is explicit after a source conflict. Toolbar Undo/Redo and Save/Reopen continue to affect
the document.

Activating a boolean button submits its explicit value through the same document owner. The
button supports pointer and Enter/Space activation without acquiring text or IME ownership.
Repeated activation while its exact submission is queued or pending preserves the captured value.
A refused submission retains the proposed boolean for explicit Apply, Discard or conflict Reapply;
document Undo/Redo and Save/Reopen include accepted boolean edits. The retained-draft capacity is
checked again at activation, including when other edits fill it after the displayed frame.

Use the wheel over the Inspector image to reach fields below the viewport. Line and physical-pixel
scrolling stay within the panel's content bounds. A scroll cancels a pressed button before release,
including a scroll away and back before another paint. Scrolling a text field out of view revokes
its active text/IME lifetime and preserves its draft; select the field again after returning.
Visible Apply/Discard and target buttons remain usable when their field's text input is above the
viewport. Resizing or selecting shorter content clamps the scroll position. Keyboard scrolling,
dragging a scrollbar and generalized virtualization retain their separate implementation scope.

Structured child controls keep unfinished values in the field draft. Confirm or cancel a pending
Some, list item or map entry before applying the whole field. New children require explicit values;
empty strings, keys and collections have explicit confirmation controls. Invalid numbers and
duplicate map keys retain the input for correction. Removing one row preserves the other rows'
text editing and local Undo/Redo; it revokes the removed row's old input lifetime. Child confirmation
and cancellation do not change the document. Explicit Apply/Reapply, document Undo/Redo and
Save/Reopen retain their ordinary whole-field meaning.

A choice for an unfinished Some, list item or map entry still needs that child's confirmation.
Cancelling or replacing the child retires its picker and text input together. Choosing one leaf
preserves the other leaves' text buffers and local Undo/Redo. Reference menus page independently;
closing, reopening or changing a page refuses queued events from the earlier menu view.

Use `Copy retained input` to recover a whole draft, including invalid text and unfinished children.
It remains available when the field is incompatible, temporarily unprojected or waiting for an
exact submission result. It copies committed candidate data through the Host clipboard; IME
preedit is excluded. A clipboard failure is shown beside the control and preserves the draft,
validation/submission error, local history and source. Editing the candidate clears old copy
feedback. Retained rows identify their original entity, component and field.
Values refused by the text control use a bounded read-only preview; Copy still recovers the
complete retained input.

An admitted source-tool integer facet replaces only its matching field's input with the declared
proposal parameter. Local editing and Undo/Redo affect that parameter; Enter or `Prepare property
proposal` requests a review, and Escape resets the parameter after cancelling composition. The
property and ordinary Source Tool panel observe and accept the same Session candidate. Ordinary
dirty source input remains separate and retains Copy/Discard and explicit conflict recovery when
the facet is disabled. See the [source-tool guide](finite-source-tools.md#panel-and-integer-property-facet).

At the retained-draft limit, `Discard retained field drafts` releases the displayed set of
unsubmitted drafts. It preserves queued and pending edits and their original completion IDs.
A changed draft set refuses an earlier button event; review the current panel and retry. This
local recovery action does not change the document or Undo/Redo history.

The Host uses the bundled primary face and, when configured, one fallback from
`NARA_EDITOR_CJK_FONT`. During coexistence with the workspace shell, use a standalone TrueType file
accepted by both font backends, such as an installed `simhei.ttf` on Windows. The shared Nara font
API independently supports collection face selection; that does not extend the shell's font loader.
An absent fallback does not authorize character substitution or a CJK rendering claim.

The Inspector image uses the existing graphics Host and exact native composite receipt. Unchanged
fields reuse their compiled frame and retained image. Native focus or presentation loss cancels
input; focus reacquisition establishes a new text lifetime. Control navigation can retain keyboard
ownership after a text token retires. Keyboard shortcuts use logical keys, distinguish AltGr text
from Ctrl shortcuts, and preserve held modifiers when handing input back to the workspace shell.
IME candidate geometry is mapped through
the displayed image into its owning window. Backend-injected composition tests establish draft and
history behavior. The keyboard trajectory calls the same logical-key entry used by Winit ingestion
and replaces the clipboard backend with memory, leaving the user's actual clipboard untouched.
Actual OS key/clipboard delivery, candidate placement/event delivery, keyboard-only entry from the
surrounding shell, detached-window/DPI behavior and text accessibility retain separate qualification.

The [independent UI consumer](../../tests/fixtures/ui-module-consumer/README.md) exercises local
editing and field compilation without ECS, Scene, a window or a renderer. Owning tests cover
fallback masks and the shared GPU path separately; they do not qualify OS IME event delivery.
