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
editor only with the same ordered font identities and complete limits; the logical size stays
fixed for that editor.

`Preedit` changes `display_text()` and its layout while `text()` retains the original committed
buffer. Its optional cursor endpoints are UTF-8 byte offsets relative to the proposed preedit.
`CancelComposition` restores the original text and selection. `Commit` replaces that selection
once and records one local edit. Other navigation/editing actions cancel composition before
operating on the committed buffer. Local Undo/Redo does not invoke document history.

## Field and Host responsibilities

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

## Native Editor integration

The existing Editor workspace offers a `Nara properties` Inspector tab. Its string and numeric
fields use Nara drawing and local editing. Boolean values use an explicit `Set true` or `Set false`
button. Entity and asset references use bounded text editing; `Choose target` also offers the
current document's source-local entities in pages of at most 32 choices. Labels include each
entity's exact ID to distinguish duplicate names. The same picker is available for entity-reference
leaves inside supported structured fields. Choosing a target changes only that leaf in the retained
draft; explicit whole-field Apply publishes it. Supported Optional, Record and scalar-item List/Map fields use
`Edit value` to open a structured draft. Unsupported shapes remain read-only. The complete
predecessor Inspector remains available under `Document` during this incremental integration.

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
