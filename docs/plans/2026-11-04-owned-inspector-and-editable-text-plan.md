# Nara-owned editable Inspector

## Target and ownership

Implement the foundation plan's first R11 visual workflow: an editable multilingual/numeric
Inspector, a clipped live Scene image and the ordinary external semantic command. Nara owns the
controls, text buffers, interaction and drawing. The existing Editor Host keeps native windows,
event loop, device/queue, submission and physical retirement. Existing Application/Session owners
retain exact document admission, operation results, Undo/Redo and Save/Reopen.

Replace a complete selected Inspector panel after matching its predecessor's behavior. Dear may
temporarily supply the surrounding existing workspace shell, but not the replacement panel's
editable controls. Do not embed another UI runtime or create a second graphics owner. R11's
broader virtualization, native IME/accessibility/DPI and idle-cost gates retain their full scope;
an internal text prerequisite alone does not complete the visual workflow or full R11.

## Text and control decisions

Reuse the pinned Parley 0.11.0 layout/editor primitives for cluster movement, selection and hit
testing. Constrain navigation, pointer placement and deletion with ICU grapheme segmentation:
Parley can subdivide a shaped ligature into scalar components. Keep Nara's bounded text owner and
resource-complete glyph snapshots. Introduce an
explicit finite font set shared by shaping and rasterization; every glyph key identifies its
actual admitted face. Never rasterize a fallback glyph using the primary face. The Editor Host
selects its font policy; unsupported glyphs remain diagnostic instead of silent ASCII substitution.

Keep three separate facts: semantic property identity, live field incarnation and current focus
lifetime. Redraw/reorder may preserve an existing field's state; retirement/recreation cannot
authorize an old field's queued input merely because the label/property is equal. Composition
messages and commits carry the exact admitted text-input lifetime. Native adapters must cancel
and reestablish IME ownership on focus/endpoint retirement; backend injection and actual OS
candidate behavior have separate evidence.

Typing changes the field buffer, not source data. Preedit changes only the visible composition;
the committed buffer and document/history remain unchanged. Cancel/focus loss restores the
original buffer and selection from before composition. An accepted IME commit makes one local
edit. Validate UTF-8 cursor offsets and text/history/layout bounds before adopting changes.

Focused Ctrl+Z/Ctrl+Y operate on the active text buffer. Explicit document-history commands keep
their own meaning and cannot be dispatched as a side effect of text routing. Escape first cancels
composition; a subsequent Escape may explicitly discard the field draft as in the predecessor.
Focus loss preserves an unapplied draft. Explicit Apply and the existing non-composing Enter
shortcut use the same exact document basis and result matching. Typing, composition completion
and focus loss must not implicitly publish. Preserve explicit Discard and conflict recovery.

## Execution and qualification

1. Add bounded multi-face shaping/rasterization and Parley-backed editable text with actual
   selection/caret/IME geometry. Prove invalid proposals preserve the original state and that
   fallback keys retain exact face identity through UI packet preparation.
2. Add Nara-owned field controls with focus-lifetime admission and local Undo/Redo. Consume them
   in the real Inspector using existing property drafts and Application results; remove the
   superseded scalar Dear controls after equivalent complete workflows pass.
3. Render the panel through the existing Host image/composite path alongside the live Scene.
   Preserve clipping, input mapping, retained physical resources and exact presented authority.
4. Exercise multilingual/string and numeric edits, preedit/cancel/commit, stale replacement input,
   source conflict, external semantic tool, Apply, document Undo/Redo and disk Save/Reopen through
   public and actual affected native consumers. Do not substitute a headless text test for panel
   presentation or injected IME events for actual OS candidate placement.
5. Independently review, run serial nextest and relevant native qualification, reconcile owning
   ADRs/guides/ledger and create intentional local Conventional Commits. Keep unsupported platform
   and broader R11/R12 requirements explicit.

Use Rust 1.95.0, root `target`, `CARGO_BUILD_JOBS=1` and one Cargo invocation at a time. Keep
production changes on the current branch, preserve concurrent edits and do not publish remotely.

## Shared checkpoint scope

This unit reopens ADR0102's shared checkpoint for the named live-field input and glyph-instance
invariants before the Editor adapter changes. Its independent public consumer is
`tests/fixtures/ui-module-consumer/tests/editable_text.rs`; the neutral layout, interaction and
presentation galleries retain their existing assertions.

The changed ordinary inventory comprises finite-font constructors on `TextEngine`, `UiCompiler`
and `UiPresentationPlanner`; collection-face admission; the bounded `TextEditor` action/result
and geometry vocabulary; `UiTextField` and its exact `UiTextInputToken`; and the TextInput semantic
role with empty committed values. Existing single-face constructors delegate to that configuration.
Grapheme admission, the default variable-font instance and actual selected-face diagnostics belong
to the same text owner. This is not a generic form schema or a platform text-input capability.

The field and text prerequisites can have a verified commit boundary. The complete selected
Inspector still requires ordinary Application results, actual Host presentation and native input;
the broader R11/platform gates remain open after that boundary.

## Native integration checkpoint

The first adapter consumes the shared primitives in the existing workspace's `Nara properties`
Inspector tab. It shares the original field drafts and exact Application submission results.
Strings and numeric scalars have local editing, composition, explicit Apply/Discard/Reapply/Reset,
document history and persistence. Other field shapes remain visibly read-only here; the predecessor
is retained until equivalent complete behavior is qualified.

The original Host draws the panel into a retained image using the existing device/queue and admits
input only after matching image, occurrence and native-window composite receipts. Equivalent frames
reuse their exact CPU/GPU output; text paint changes can preserve equivalent pointer routes, while
behavior changes, native mapping changes and presentation loss cancel gestures. Native backend
injection and actual OS IME delivery retain separate evidence boundaries.

Complete selected-panel replacement, clipboard and keyboard navigation, structured/reference
editing, a Nara-owned clipped live Scene/tool visual, virtualization, measured idle costs and the
remaining native platform gates are still open. The optional scalar tab is a checkpoint toward
those workflows, not completion of R11.
