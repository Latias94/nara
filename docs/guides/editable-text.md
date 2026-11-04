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
original complete limits; the logical size stays fixed for that editor.

`TextEditor::reconfigure_fonts(&mut replacement_engine)` explicitly changes the ordered font
selection. The replacement engine must keep the original limits. Reconfiguration reshapes and
admits layouts of the current display, composition origin and every retained Undo/Redo state
before adopting anything. Refusal preserves all editor observations, history, cached paint and
the previous font association. This complete-state operation can reshape up to 34 bounded
buffers. Reusing the same font set/order and limits is a no-op and preserves paint identity.
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

`Preedit` changes `display_text()` and its layout while `text()` retains the original committed
buffer. Its optional cursor endpoints are UTF-8 byte offsets relative to the proposed preedit.
`CancelComposition` restores the original text and selection. `Commit` replaces that selection
once and records one local edit. Other navigation/editing actions cancel composition before
operating on the committed buffer. Local Undo/Redo does not invoke document history.

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

`UiTextField::authored_node` draws its text, selection and caret within one content clip. Pass the
caller's resolved logical viewport width to drawing, pointer mapping and `ime_area()` consistently.
The candidate area is local logical geometry; the Host translates it to the actual native window
and scale. Semantic values contain committed text, including an empty string, while preedit stays
out of both the field value and its internal text semantics.

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
