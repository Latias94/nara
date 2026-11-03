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

Parley supplies visual order, bidi affinities, selection, hit testing and caret geometry. ICU
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

The [independent UI consumer](../../tests/fixtures/ui-module-consumer/README.md) exercises local
editing and field compilation without ECS, Scene, a window or a renderer. Owning tests cover
fallback masks and the shared GPU path separately; they do not qualify OS IME event delivery.
