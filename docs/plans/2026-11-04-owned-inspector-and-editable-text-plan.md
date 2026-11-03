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
document history and persistence. Boolean fields use explicit value buttons with the same exact
submission and recovery paths. The reference and scrolling successor below adds root reference
editing and a bounded picker; the structured successor adds supported nested value controls.
The predecessor stays until equivalent complete behavior is qualified for the selected panel.

The original Host draws the panel into a retained image using the existing device/queue and admits
input only after matching image, occurrence and native-window composite receipts. Equivalent frames
reuse their exact CPU/GPU output; text paint changes can preserve equivalent pointer routes, while
behavior changes, native mapping changes and presentation loss cancel gestures. Native backend
injection and actual OS IME delivery retain separate evidence boundaries.

Complete selected-panel replacement, keyboard-only shell entry,
a Nara-owned clipped live Scene/tool visual, virtualization, measured idle costs and the
remaining native platform gates are still open. The optional scalar tab is a checkpoint toward
those workflows, not completion of R11.

The keyboard successor adds logical-key editing, enabled-control navigation and explicit button
activation inside the active Inspector image. Its shared Host clipboard uses a fallible direct
port so rejected Cut/Paste preserves the original buffer. Composition and queued submission
guards remain intact, and modifier state survives transfer to the predecessor controls. The
corresponding native trajectory uses the production logical-key entry and an isolated memory
clipboard; physical OS keyboard/clipboard delivery and keyboard-only shell entry remain open.
This advances the clipboard/navigation part above without completing the selected-panel replacement.

The boolean successor uses the existing controlled Button and shared `FieldEditors` candidate.
The explicit value command has keyboard focus without a text-input lifetime. Original presented
basis, admission, exact completion, conflict recovery and the shared draft budget remain the
same as other fields. Capacity is checked at activation as well as compilation, so a previously
enabled control cannot exceed the budget after another edit fills it. No shared widget or platform
interface changes are required by this private Editor consumer.

## Boolean qualification checkpoint

The boolean integration working tree is based on `45f3b5c`. Verification uses Windows 11,
Rust 1.95.0, root `target`, `CARGO_BUILD_JOBS=1` and serial Cargo invocations.

| Gate | Result |
| --- | --- |
| Final field/font controls and affected native workflows | 19/19, `8e0c9dec-f272-4788-89b5-bc75a3764fb1`; 15 CPU controls plus the owned pointer/IME and keyboard/boolean trajectories and two predecessor field regressions |
| Ordinary Editor library | 303/303, `accb405b-c24f-4678-90ce-9f10fdea10d4`; 39 existing ignored cases excluded |
| Correctness Clippy | Editor all targets/all features pass `-D clippy::correctness`; existing warnings remain, and the introduced large-enum warning is removed |
| Editor build and formatting | `cargo build --locked -p nara-editor` and `cargo fmt --all -- --check` pass |
| Independent review | No correctness findings; the native test contribution is separately integrated and executed |

The ordinary library run precedes the final private text-state boxing, which removes the new
large-enum warning without changing editing behavior. The final 19-case cohort runs after that
change. The earlier nine native Host trajectories retain their separately recorded keyboard
checkpoint scope; unchanged cadence, detachment and recovery tests are not requalified here.

The new CPU control qualifies boolean refusal recovery, exact result identity, stale-frame and
read-only rejection; the late-capacity control now also proves an old enabled boolean cannot
create another dirty draft after all 64 slots fill. The native trajectory preserves the original
clipboard, composition, modifier, pointer-cancellation and document assertions while adding
pointer/Enter/Space boolean commands, repeated pre-paint refusal and an actual saved/reopened
`true` value. Test setup corrections add the CPU owner's required fresh-paint boundary before
read-only compilation and follow the native fixture's actual field order for keyboard traversal.
No product admission rule is relaxed.

The keyboard trajectory still uses production logical-key ingress and an isolated memory
clipboard. Physical OS input/clipboard, IME candidate placement, complete selected-panel parity,
the Scene/tool visual and the broader R11/R12 platform requirements retain their open scope.
The reference and scrolling successor continues below. The corresponding predecessor controls
remain until complete selected-panel parity.

## Reference and scrolling integration

Entity and asset references reuse the original typed scalar candidate and exact Application
submission owner. Source-local choices share ID/label projection with the predecessor. The owned
picker creates at most 32 live choice controls per page, with separate widget incarnations across
close/reopen and page replacement. Captured menu/page state, field basis, draft and current target
membership guard selection. Choosing a target retires the old text controls while retaining one
unpublished canonical candidate; explicit Apply/Reapply retains ordinary result matching.

The private Inspector viewport clips one absolute content tree through the existing Host image.
Wheel admission requires its confirmed image, native window and actual pointer ownership. Logical
line and physical-pixel deltas accumulate within captured content bounds. A changed view cancels
the active pointer gesture immediately, including a scroll away/back before repaint. Content or
viewport changes clamp the offset with at most one corrective compile; unchanged output reuses
its retained frame and image. This is wheel scrolling, not generalized UI virtualization.

Field metadata and text-input admission are separate. A visible button retains the field's exact
draft/current/view facts even when the text input is fully clipped. Focus, text tokens, clipboard,
composition, point input and IME geometry still require the text input's own visible semantics.
The short-viewport Apply and paged-choice regressions protect this distinction.

The structural replacement projects the existing `RootDraft`/`ValueDraft` through
individually incarnated leaf controls, with captured field and child identity. Preserve pending
Optional/List/Map confirmation, cancellation, local invalid text and exact whole-field Apply
through real Scene/Prefab consumers. Do not add a parallel form-value store or a speculative
public schema/widget provider. Complete native parity precedes removal of the old field paths.

## Reference and scrolling qualification

The integration working tree is based on `a4c5dd0`. Verification uses Windows 11, Rust 1.95.0,
root `target`, `CARGO_BUILD_JOBS=1`, DX12 and the installed `simhei.ttf` fallback, with serial
Cargo invocations.

| Gate | Result |
| --- | --- |
| Final owner controls and affected native workflows | 32/32, `c25e9dff-50e8-457c-a6de-6af68afdde01`; 20 CPU controls and 12 actual native trajectories |
| Ordinary Editor library | 308/308, `f1171ab9-99c3-48a1-9daa-e47d61ed38fe`; 41 ignored native cases excluded |
| Correctness Clippy | Editor all targets/all features pass `-D clippy::correctness`; warnings remain |
| Editor build and formatting | `cargo build --locked -p nara-editor` and `cargo fmt --all -- --check` pass |
| Independent review | The clipped-input/visible-button finding is corrected; scoped re-review has no remaining findings |

The CPU controls cover typed asset publication, a 67-entity picker with at most 32 live choices,
retired menu/page/focus identities, source conflicts, exact submission results and hidden-input
buttons. Native additions exercise actual reference selection, cancellation, explicit Apply,
Undo/Redo and Save/Reopen, plus long-panel wheel admission, clipping, dirty-buffer retention,
pressed-button cancellation and shorter-content clamping. The remaining native cohort includes
the original owned pointer/IME and keyboard/boolean paths, predecessor reference/field controls,
Project IME, Canvas, pause/resume, Game input and native view lifecycle/recovery.

Review found that filtering all field metadata by text-input visibility disabled still-visible
Apply/Discard/choice controls. The correction retains metadata for visible associated buttons
and independently requires visible TextInput semantics for every text entry and geometry path.
Both the short Apply viewport and the scrolled second picker page reproduce that boundary.

Two fixture corrections preserve product admission: the asset case now supplies real PNGs and
valid Image metadata instead of treating a Prefab as generic media; the scrolling case captures
the unavailable-record manifest after entity/component creation and reacquires focus after the
old text lifetime is clipped. The final 32-case cohort includes these corrections. No source or
input validation is relaxed. Actual OS input/IME/clipboard, keyboard scrolling, scrollbar dragging,
virtualization, complete Inspector parity and the Scene/tool visual retain their separate scope.

## Structured field integration

The private adapter projects the existing candidate tree into controls for supported Optional,
Record and scalar-item List/Map shapes. Opening a field, choosing a boolean or empty value,
beginning/confirming/cancelling a child and removing a collection item only change the original
`RootDraft`/`ValueDraft`. Whole-field Apply/Reapply retains the original Application submission,
conflict, exact result and document history owner. Pending children and invalid local text remain
recoverable and cannot publish a partial value.

Each text leaf carries its field owner and a typed draft address. Collection and Optional child
addresses retain their original `ChildDraft` identity; visible labels and list indices are not
input identities. The field metadata authorizes whole-field buttons independently of visible leaf
TextInput semantics. Removing or replacing a child retires its controls immediately. Unaffected
leaves retain their own text buffers and local Undo/Redo even when a preceding row is removed.
Pending-to-accepted transitions deliberately establish a new input control lifetime. The map key
editor is addressed to its exact pending child and preserves an explicitly entered empty key.

The predecessor consumes the same Optional child owner and retains its existing behavior. No
public form schema, parallel value store or graphics owner is introduced. General keyed-record
collection controls and unsupported shapes remain outside the existing supported-shape contract.
Complete selected-panel parity, component/tree operations and the remaining visual/platform gates
still precede removal of the corresponding Dear Inspector path.

## Structured field qualification

The integration is based on `fa0ca2f`, using the same Windows 11, Rust 1.95.0, root `target`,
single-job Cargo, DX12 and installed CJK fallback configuration as the reference checkpoint.

| Gate | Result |
| --- | --- |
| Draft and field controls | 32/32, `ec1753fc-3aa6-4609-bb4d-9f0beca715bd`; typed addresses, pending admission, retired identities, original scalar/reference controls and two complete Application consumers |
| Ordinary Editor library | 316/316, `4e0431a7-e561-4bc7-aca9-8921690eed43`; 42 ignored cases excluded |
| Final owner/native cohort | 16/16, `15000473-89b6-4f66-8f56-86c8504fda20`; two field controls, Project IME backend input and 13 actual Windows/DX12 trajectories |
| Correctness Clippy | Editor all targets/all features pass `-D clippy::correctness`; warnings remain |
| Build and formatting | Editor locked build and `cargo fmt --all -- --check` pass |
| Independent review | No P1/P2 findings in the scoped production review; final integration retains all executed controls |

The new native trajectory applies four non-default structured fields in each of a real Scene and
Prefab. Invalid Optional/Record numbers and pending List/Map drafts refuse Apply without changing
source/history. Child confirmation, cancellation and removal remain local until exactly one
whole-field transaction. Toolbar Undo/Redo, typed saved values, original entity records, unknown
records and clean Reopen all pass. The predecessor's complete recursive trajectory also passes
after consuming the same Optional child identity owner.

The final field controls add a composing surviving leaf: sibling removal and whole-field Apply
refuse, composition cancellation preserves the candidate, and deleting the preceding row keeps
the surviving control identity, focus token and local Undo. The other control retires cancelled
and confirmed Optional inputs, refuses stale tokens and uses explicit Reapply after an actual
Application source conflict. Exact queued/pending/completion observations clear only the original
field. The final cohort follows these assertions, all fixture corrections and the linear focus
reducer traversal. It explicitly executes the latest composition assertions and corrected native
helpers; the ordinary library run uses the same production behavior.

The first native attempt exposed two test assumptions. Pointer events are processed by the owned
adapter without its text/keyboard consumed return value, so the test now uses the established
mouse helper and verifies actual state changes. A manifest captured before entity creation also
contains a different complete placement inventory. Both recursive fixtures now verify original
unknown entries, overlays and placement order across creation, then capture the complete field
editing baseline; Save additionally compares every original raw entity record. Missing clipped
semantics are located by bounded real wheel input through the confirmed image. No product source,
input or result admission is weakened.

These results qualify the supported value controls and selected Windows backend paths. Nested
entity-reference picker parity, complete component/tree controls, keyboard-only shell entry,
the Nara-owned Scene/tool visual, virtualization, measured idle work and actual OS IME/clipboard/
accessibility/DPI behavior retain their separate requirements. The predecessor and the broader
foundation Goal remain open.
