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

These results qualify the supported value controls and selected Windows backend paths. The nested
entity-reference picker successor follows below; complete component/tree controls, keyboard-only shell entry,
the Nara-owned Scene/tool visual, virtualization, measured idle work and actual OS IME/clipboard/
accessibility/DPI behavior retain their separate requirements. The predecessor and the broader
foundation Goal remain open.

## Shared reference picker integration

Root and structured entity references now consume one private picker owner. Each leaf picker is
addressed by the existing typed `DraftInput`, retaining the original field and child identity.
The shared source-local projection formats only the current page's labels, with at most 32 choice
controls per picker. Map key inputs and asset-reference text do not receive entity pickers.

Each displayed picker action carries its original menu revision. Closing/reopening and page
replacement advance that revision, retire old choices and reset pressed state. Even a compatible
Toggle control whose key survives a close/open round trip cannot consume an earlier view's input.
Presented field metadata includes every current picker revision, so an equivalent-looking menu
cannot reuse a compiled frame with obsolete action bindings. Activation checks the captured view
before changing menu or draft state. Reduction still synchronizes focus for an identical live key
when an earlier view delivers navigation; its disabled descriptor suppresses old pointer state
and activation without losing the kernel's current focus fact.

Choosing rechecks the original field/draft/current projection, exact leaf and live choice,
composition and pending state, capacity and current source-local membership. It writes a canonical
reference through the same candidate parser, then retires only that leaf's text and picker.
Unrelated text controls retain their local history. Pending child confirmation and whole-field
Apply/Reapply keep their original meanings and exact Application result routing. A visible choice
can still operate when its text input is fully clipped; it does not acquire hidden text authority.

The old root-only menu flags and field-operation variants are removed. This is a private Editor
consumer refactor; shared UI controls, source formats and public field APIs are unchanged. Full
component/tree parity, the clipped Scene/tool visual and broader R11/R12 gates remain separate.

The retained-input copy and capacity successor below addresses the predecessor's
`FieldEditors::show_requested_recovery`/`show_recovery` recovery paths. Keep their original
unavailable and incompatible input semantics when removing the corresponding field path.
The source-tool property integration below supplies the ordinary finite facet consumer of the
same submission/history owners. The clipped live Scene retains its separate complete visual gate.

## Shared reference picker qualification

This integration is based on `18c8faa`, using Windows 11, Rust 1.95.0, root `target`, serial
single-job Cargo, DX12 and the installed `simhei.ttf` fallback.

| Gate | Result |
| --- | --- |
| Ordinary Editor library | 321/321, `6bd8313c-fba4-4ce3-956b-8b72e1e19696`; 43 ignored cases excluded |
| Final owner/native cohort | 44/44, `9e5c9b3a-a660-4a7d-8eb3-75376d788ad1`; 29 CPU controls, Project IME backend input and 14 actual native trajectories |
| Independent review | Two focus-state P2 findings corrected; final scoped re-review has no remaining findings |
| Correctness Clippy | Editor all targets/all features pass `-D clippy::correctness`; warnings remain |
| Build, formatting and documents | Editor locked build, `cargo fmt --all -- --check`, changed local Markdown targets and `git diff --check` pass |

Both final cohorts include the same latest production source and focus corrections. The native
addition chooses all seven reference leaves across Record, pending/accepted Optional, existing/new
List and existing/new Map paths in an ordinary Scene and Prefab. Duplicate names retain exact IDs.
Menu and whole-field cancellation, explicit pending confirmation and refused partial Apply leave
the source/history unchanged. One final whole-field Apply, Undo/Redo, typed saved references,
original raw source records, unavailable manifests, clean Reopen and unchanged project code pass.

CPU controls additionally prove exact original Application completion, per-leaf token retirement,
unrelated local Undo preservation, cancelled/replaced Optional choice refusal and bounded second
page selection with the input fully clipped. Root controls retain their earlier conflict, stale
choice, pagination and hidden-input assertions. The first nested controls assumed every leaf was
visible in the finite panel. They now locate controls through bounded ordinary scrolling and
explicitly place the direct input above the clip for the hidden-input assertion; no production
visibility or input admission is relaxed.

Review first found that view replacement cleared the focused state of stable Toggle/Next buttons.
It then found that simply rejecting an old view's entire batch also discarded a real focus loss
when the user navigated before repaint. The correction preserves stable-key focus on view change
and reduces old-view batches with a disabled descriptor. It updates focus and clears pointer
state without producing intents; direct activation still refuses the old view. A real kernel
FocusTarget/ActivateFocused regression drives that sequence through the actual Inspector reducer.
A separate control establishes nonempty pressed/hovered state before view replacement and verifies
pointer cancellation with focus retained. The final ordinary and native runs follow both fixes.

The first 36-case combined run passed before the final focus-loss correction. The final 44-case
run supersedes it for this integration. This evidence does not qualify actual OS keyboard/IME/
clipboard delivery, native accessibility, keyboard-only shell entry, virtualized control counts,
idle costs or the Nara-owned live Scene/tool visual. Complete selected-field/component parity and
the broader foundation Goal remain open.

## Retained input and capacity recovery

The owned panel copies the existing `Candidate::copy_text` representation through a synchronous,
fallible callback to the Host's existing clipboard. Copy has a presented button and exact field,
draft and control incarnation, without requiring editable source data or a live text/IME token.
Invalid and unfinished structured candidates remain lossless under the existing recovery format.
Queued/pending, incompatible, unprojected and text-limit-refused drafts retain this read path.
Composition preedit is never adopted or copied as committed input. The Host result changes only
control feedback; it cannot overwrite a candidate, validation/submission error, operation result,
selection, source or document history. Editing the candidate clears the old copy feedback.

At capacity, a panel-local recovery button captures the exact dirty, non-in-flight draft set.
Activation compares that set before deleting any entry. A changed basis, candidate, queued/pending
state or newly eligible draft refuses the whole earlier action. Matching recovery removes only
those drafts and their controls; clean unrelated controls and original queued/pending completion
IDs survive. The compiled recovery snapshot participates in frame reuse and pointer invalidation.
Recovery and Copy remain reachable when the document-submission queue is full. No clipboard queue,
replacement value store, generic effect framework or document transaction is introduced.

Copy follows the existing Apply/Discard/Reapply/Reset footer in keyboard order. Retained rows name
their original entity/component/field instead of appearing to belong to the new selection. These
recovery controls advance selected-field parity; the source-tool facet successor follows below.
The clipped live Scene, complete component/tree behavior and broader native/idle/platform
requirements remain open.

Copy invalidation follows the original field incarnation and committed draft snapshot. Cancelling
preedit or changing the source-edit buttons' enabled state cannot invalidate an unchanged Copy
operation. The shared pointer coordinator compares ordered hit identities and geometry separately
from enabled state; exact captured-target interactivity and the existing route, mapping, modal and
explicit invalidation checks still apply. Disabling the captured target cancels its gesture.
Retained rows share an entity/component heading and avoid repeating a scalar candidate already
shown in its text control, keeping the existing finite glyph budget unchanged. A candidate
refused by the text control uses a bounded UTF-8 preview instead of sending its complete
text back through the same compiler limit; Copy retains the original complete representation.

## Recovery qualification

This successor is based on `5b58527`, with Windows 11, Rust 1.95.0, the root `target`,
single-job serial Cargo, DX12 and the installed `simhei.ttf` fallback.

| Gate | Result |
| --- | --- |
| Ordinary Editor library | 326/326, `8640c504-d7e2-4342-af6d-aa551150399c`; 44 ignored cases excluded |
| Shared UI, kernel and widgets | 240/240, `58b71fd8-078b-4683-a494-90a2eac4f874`, including unrelated-enable and captured-target-disable controls |
| Independent UI consumer | 7/7, `410dcc94-91f3-4772-9991-00a6ee2d39e6`, without ECS, Scene or renderer dependencies |
| Selected native trajectories | 17 passing cases in `0ad06ba2-238d-416b-b990-26f458242cd0`, plus the corrected ordinary Scene/Prefab case 1/1 in `11a7e984-8b56-4a0b-9f69-9d8f7f0be4ac`; 18 distinct trajectories qualified |
| Independent review | Composition/Copy cancellation corrected; final bounded production and regression review has no remaining P1/P2 findings |
| Correctness Clippy | Editor and UI kernel, all targets/all features, pass `-D clippy::correctness`; warnings remain |
| Build and formatting | Editor locked build and `cargo fmt --all -- --check` pass |
| Documents | Changed local Markdown targets and `git diff --check` pass |

The five recovery CPU controls are included in the ordinary Editor result. They cover invalid
scalar and oversized retained input, preedit exclusion, fallible feedback isolation, read-only
Copy, pending Optional/List/Map input, removed components, stale controls and exact recovery of
the 64-draft budget. Old snapshots refuse atomically; clean controls, queued work and two original
pending IDs survive, while recovered controls and text tokens retire.

The new native trajectory uses actual pointer/IME ingress to create all 64 dirty fields in a
65-field schema. It clicks the displayed recovery button, refuses retired input and edits the
previously disabled field without source, history, selection, unavailable-record or project-file
changes. Its other workflow copies Chinese and invalid numeric input, excludes preedit, refuses
a clipboard write, retries, preserves local Undo/Redo and recovers unprojected and removed fields.
Both use the isolated Host memory clipboard. The final native cohort additionally executes the
earlier owned field/keyboard/reference/scrolling/structured paths, predecessor field and recursive
paths, Project IME, pause/Game/view lifecycle and retained Scene/Canvas controls.

Initial qualification exposed duplicated retained-value rendering exceeding the existing glyph
budget, then composition cancellation re-enabling sibling buttons and cancelling Copy's press.
The final source removes redundant rendering, bounds the refused-input preview and corrects the
owning input comparisons without widening resource limits. The added raw-pointer regression
initially expected a widget activation from the kernel; it now checks exact routed release and
cancellation, with the actual native Copy proving the widget/Host effect.

The ordinary native case had captured its complete manifest before authorized entity creation.
It now verifies original unknown entries, overlays and placement order across creation, then
captures the complete editing baseline. Existing exact Save/Reopen manifest and original raw
entity checks remain. Only that test changed after the 17 passing native trajectories, and its
focused rerun passes; the full 18-case invocation itself had one fixture failure.

These results cover the recorded Windows backend paths and existing finite model limits. They
do not qualify actual OS IME/clipboard delivery, candidate placement, accessibility, DPI transfer,
keyboard-only shell entry, virtualization, idle costs or the Nara-owned Scene/tool visual.
Complete component/tree parity, R11/R12 and the broader foundation Goal remain open.

## Source-tool property integration

The selected Nara property row consumes the already admitted finite integer facet. Its parameter
uses the existing local text owner and remains separate from the ordinary source-field draft.
Invalid parameters and composition cannot prepare a proposal. Explicit Prepare captures the
original activation, complete property target and parsed i64 parameter. Exact Session review
rows are borrowed for presentation; Accept retains only the original request identity.

Field and source-tool actions share the existing panel queue and ordered Authoring submission
ledger. The existing source-tool controller retains offered/pending admission and exact result
feedback. No prepared candidate, inverse executor, result bus, universal control schema or
replacement source-value store is introduced. The ordinary tool panel can accept the same
candidate prepared through the Nara property, and the Nara property can accept that shared review.

Activation, source, Schema and target replacement retire the parameter control and its text
lifetime. A hidden ordinary dirty draft keeps its original basis, Copy/Discard and conflict
recovery; it never becomes a tool parameter or publishes automatically when the facet retires.
Nonmatching fields retain the ordinary Nara editor. This completes another consumer of the
selected property path; complete component/tree parity, the clipped Nara Scene visual and the
broader R11/R12 gates remain open.

The actual child fixture uses distinct declared IDs and storage paths: `health`/`armor` identify
property proposals, while `wire_health`/`wire_armor` locate their visible Nara rows. The first
native attempts confused those facts, then observed terminal Session completion before the
following confirmed Inspector image. The fixture now uses the actual Schema paths and the
established finite presentation pump after terminal observation. Production identity, admission
and presentation checks are unchanged by these fixture corrections.

## Source-tool property qualification

The successor is based on `69cb29c`, using Windows 11, Rust 1.95.0, root `target`, serial
single-job Cargo, DX12 and the installed `simhei.ttf` fallback. The actual .NET 10 Host and
EnemyStats v1 child build successfully into the existing target directory.

| Gate | Result |
| --- | --- |
| Ordinary Editor library | 330/330, `80d32adc-5392-4f21-ad20-e931e4d01afa`; 45 ignored cases excluded |
| Selected native cohort | 17/17, `e6812078-6e2f-45f6-b4fe-145aafe48a80`, including both actual C# property/panel consumers |
| Independent review | Scoped source-tool, parameter, queue, review and control-retirement review has no P1/P2 findings |
| Correctness Clippy | Editor all targets/all features pass `-D clippy::correctness`; warnings remain |
| Build and formatting | Editor locked build and `cargo fmt --all -- --check` pass |
| Documents and runner | Changed local Markdown targets, `git diff --check` and runner Python syntax pass |

Four owner controls exercise invalid and canonical i64 parameters, exact original activation and
property target, queued-input refusal, composition, failed input isolation, retired controls,
activation/source replacement and ordinary dirty conflict recovery. The full library run includes
the pre-existing exact Application result, clipboard, structured-field and capacity controls.

The new actual native workflow first retains an ordinary `health=137` draft without changing
source/history, then activates the installed tool. Its Nara parameter prepares a real `+25`
proposal while the ordinary armor field remains editable. The exact Session patch/inverse is
observed from the property and ordinary Source Tool panel, accepted once, and follows one
Undo/Redo. A second proposal is accepted through the Nara button. Actual replacement and disable
retire old tokens and reject late IME; fallback restores the original dirty draft with explicit
conflict recovery. Discard changes only that draft. Save, physical Close, clean Open, original
project code and public Rust materialization all pass. The final cohort also executes predecessor
property/panel, owned scalar/keyboard/reference/structure/recovery/scrolling, Project IME,
pause/Game/view-lifecycle and retained Scene/Canvas trajectories.

The runner's `--native-editor` selection now includes both property consumers; its Python syntax
is checked. This integration rebuilds the actual Host/child and runs the recorded native tests
directly. It does not rerun the runner's entire language/module/catalog matrix or establish a new
language/Host support claim. Shared UI production code is unchanged in this successor; the
preceding recovery unit retains its separately recorded shared/independent consumer results.

The complete selected Inspector, component/tree controls, clipped Nara Scene visual,
virtualization, idle measurements and actual OS IME/clipboard/accessibility/DPI qualification
remain open. These concrete property workflows do not complete R11/R12 or the foundation Goal.

## Owned Scene integration

The selected Scene replacement uses the existing hosted panel surface for Nara's compiled
toolbar, status and viewport geometry. Reset, picking and bounded pan/zoom belong to that
control owner. The native adapter retains the exact Scene output beside the panel frame;
neither `UiImageId` nor a CPU image snapshot receives GPU custody.

Choose direct sampling at the original window compositor. Emit the opaque panel chrome and
the Scene image inside its reserved, non-overlapping viewport in the original window draw-list
position. Intersect the viewport clip with the parent window clip. This preserves the existing
popup and secondary-window ordering without a readback, a nested image-permission protocol or
another graphics owner. Arbitrarily interleaved overlays are outside this two-placement packet;
they would need an ordered drawing packet rather than silently flattening overlapping layers.

The Scene sample has its own exact display occurrence and external binding. Keep its source
output, local layout/UV/clip and native destination snapshot together. A changed placement
retires that occurrence before reestablishing input. Scene preview acceptance, content stamps,
revision-bound picking and physical completion remain with their original owners. Confirm the
panel and Scene samples in the actual same-window presentation before routing viewport input.
The neutral preview owner rebinds the same immutable output to its new nonzero occurrence and
mapping with a fresh token, rejecting every changed output fact and the old token. A previously
sampled passive source cannot dispose a newer accepted candidate while its panel image is still
being prepared.
Viewport pointer admission checks the current confirmed native sample and its exact clip before
the reducer can queue a pick. Rebinding refuses that event immediately; it cannot borrow a later
receipt even when the CPU frame, source and local mapping remain equal. The chrome toolbar keeps
its independent route, and a consumed control event does not fall through to the shell or Game.

Qualification must include real Windows/DX12 source edits and Undo, UV pan/zoom/reset, clipped
and outside-content refusal, stale mapping/receipt refusal, retained old output pixels, source
and panel retirement, and the existing Scene/Canvas priority controls. Record actual allocation
and additional-pass behavior separately from any unmeasured performance claim. This integration
does not close full Inspector parity, virtualization or the remaining R11/R12 platform gates.

## Owned Scene qualification

Qualified on Windows/DX12 on November 4 with Rust 1.95.0, one Cargo invocation at a time,
`CARGO_BUILD_JOBS=1` and the root `target`. The ordinary source-tool cases reused the actual
prebuilt .NET 10 Host and EnemyStats v1 artifacts. CJK controls used the configured standalone
`simhei.ttf` fallback. No registry record or separate verification shard was added.

| Check | Result |
| --- | --- |
| Final Editor library | 341/341, 48 ignored; `efe1a825-d139-4e6c-b19a-33ee24deb9e5` |
| Public preview presentation consumer | 4/4 in `editor_persistence` with root all features; `3f0b9fb0-050b-47b6-b6e9-9202f5cee64e` |
| Selected native integration cohort | 21/21; `6c89ad6f-c94e-4576-bdfb-16a15b8b01cd` |
| Final native image-clip follow-up | 9/9; `424bd16d-ecdf-4ec5-9c23-a32bd273d1b2` |
| Independent review | The occurrence-rebind P1 and queued-pick P2 are corrected; final bounded production review has no remaining P1/P2 |
| Correctness Clippy | Editor all targets/all features pass `-D clippy::correctness`; warnings remain, including the existing shared error-size policy |
| Build and formatting | `cargo build --locked -p nara-editor` and `cargo fmt --all -- --check` pass |

The final library includes the toolkit-free Scene controls, exact retained-frame routing,
pointer-anchored and accumulated prepaint camera input, Reset, tiny panels, letterbox and
ancestor-clip refusal, keyboard non-picking and invalidated viewport behavior. The existing
default-layout test still requires both central panes to draw during preparation, before any
native panel image exists.

Three new real native trajectories establish original-output sampling through a distinct
placement occurrence and a same-window chrome receipt; native wheel, middle drag and Reset;
actual Sprite picking, ordinary edit/Undo, window resize and panel close; and same-sized panel
movement with a queued click during the refused rebind interval. The last control preserves the
exact CPU frame and independent toolbar route, refuses the old event before queuing, then
requires a new click after the replacement sample. GPU numeric observations verify actual source
pixels and immutable retained predecessors after edit, Undo, resize and retirement.

The 21-case cohort also covers ordinary Scene/Prefab persistence, all selected owned Inspector
controls and recovery, both actual C# source-tool property paths, Project composition input,
pause/Resume and exact Game input/Step, detached Game/native recovery, retained Scene/Prefab
binding, UI Canvas priority, image reimport and the independent multi-pass Scene effect. The
final nine-case follow-up reruns the three new Scene cases and the affected Game/lifecycle,
retained binding, Canvas, reimport and effect consumers after constraining native sampling to
the intersection of image content, viewport clip and parent window clip.

Review exposed a real ownership mismatch: redisplay previously required the entire old image
fact, including occurrence, although a changed placement requires a new occurrence. The preview
owner now accepts only the same immutable output facts with the confirmed fresh occurrence,
updates the exact image/mapping and replaces the token. The public consumer protects foreign,
zero, changed-output, stale-token and same-occurrence/moved-mapping refusals. Another correction
keeps a newer accepted candidate while the hosted panel still displays an older output.

Initial native qualification also exposed fixture assumptions: the generic Inspector project
did not declare the built-in Sprite/Transform schema owners, and several predecessor controls
compared actual samples against the producer's now-passive source occurrence. The fixture now
composes the real admitted owner snapshots; the tests observe the actual Nara placement and
use native input rather than Dear Scene item events. Image reuse waits for the finite hosted
chrome preparation to settle before asserting one native queue submission. These corrections
preserve the original source, exact output and native-presentation assertions.

The product adds a panel-sized chrome output when its compiled frame changes and samples the
Scene output directly; it does not read back, copy or upload Scene pixels. Unchanged CPU frames
and chrome outputs are reused, and native source-reimport controls verify settled image reuse.
Per-output retention remains charged to the original Host; explicit close and retained pixel
tests observe retirement without another GPU owner. This is not a measured idle, allocation-tail
or retirement-latency result. Complete Inspector/component/tree parity, virtualization, actual
OS IME/candidate placement, accessibility, Scene detached-window/DPI qualification and broader
R11/R12/foundation requirements remain open.

## Component controls successor

Replace the selected Inspector's component composition controls through the existing
Scene/Prefab actions and ordered Authoring submission ledger. Capture document kind, source
revision, Schema, entity, availability and local menu observation in the displayed controls;
refuse stale actions rather than retargeting them to the latest selection. Keep unavailable
creation reasons and read-only component diagnostics visible.

Retain that basis in the existing pending submission until the paint-time Authoring queue
boundary and revalidate it there. Creation-metadata adoption can change Schema without changing
the source revision; a raw composition action alone cannot preserve the old creation template.
Both Add and Remove consume their displayed operation observation before another event can
queue the same mutation. Invalidated pointer state is cancelled while stable keyboard focus
keeps its ordinary lifetime.

Folded fields retain their original drafts and compatible local text history while losing
focus, composition and displayed input authority. Removed components leave dirty fields in
the existing retained-input recovery path. No new mutation, history or candidate owner is
introduced. Qualify both Scene and Prefab Add/Remove, fold/unfold, explicit field Apply,
Undo/Redo and disk Save/Reopen, plus stale/read-only refusal and unknown-data preservation.
Complete tree/navigation parity, virtualization and the other R11/R12 gates remain open.

## Component controls qualification

Qualified on November 4 on Windows/DX12 with Rust 1.95.0, root `target`,
`CARGO_BUILD_JOBS=1` and serial Cargo. The native cohort uses the configured CJK fallback and
the existing actual .NET Host/EnemyStats artifacts for its source-tool property control.

| Check | Result |
| --- | --- |
| Final ordinary Editor library | 345/345, 49 ignored; `fb5acfa5-0251-4d5a-bf3d-0f100876e2a9` |
| Final selected native cohort | 13/13; `e72aecd1-7737-4222-a81f-42e66d74b312` |
| Independent review | Two queue/observation P2 findings corrected; scoped re-review has no remaining P1/P2 |
| Build and lint | Editor build and all-target/all-feature correctness Clippy pass; existing warnings remain |
| Formatting and documents | `cargo fmt --all -- --check` and diff checks pass; 1,022 relative links resolve |

The four component owner controls cover exact Scene actions, duplicate-creation and unknown/read-only
refusal, old/retired display rejection, incompatible retained drafts after removal, cancelled
hidden preedit, new focus lifetime and compatible local Undo after unfolding. A separate queued
Add control publishes a genuinely admitted different creation Schema with unchanged Scene
observations and proves that the final queue boundary refuses it. Repeated Remove is refused
before document drive. This specifically protects a creation-metadata transition that source
revision checks alone cannot detect.

The new native trajectory performs Add, dirty-field fold/unfold, explicit Apply, Remove,
Undo/Redo and disk Save/Reopen in both Scene and Prefab. It verifies typed saved values, original
unknown records, history/dirty state, retired hidden text focus and unchanged project code. Its
component controls use actual Winit pointer ingress and the confirmed hosted panel image, rather
than Dear component events or direct mutation substitutes. Entity creation, navigation and
persistence still use the existing surrounding shell.

The final 13-case cohort also protects ordinary source preservation, scalar/IME-injection and
keyboard/clipboard controls, root/nested reference pickers, structured fields, wheel clipping,
retained-input/capacity recovery, the actual C# property tool, and all three direct-sampling Scene
trajectories. Earlier 344-test and native passes preceded the review corrections; the final
345/13 results above cover their replacements. These are bounded Windows/DX12 results, not actual
OS IME/candidate placement, assistive technology, detached Scene/DPI or measured cost evidence.
Full Inspector/navigation and tree replacement and the foundation Goal remain open.

## Default ordinary Inspector replacement

Complete the ordinary Scene/Prefab Inspector's selected-entity workflow with authored placement
intent and revision-bound Open Prefab Source, then use the Nara controls as the default Document
tab. Remove the competing `Nara properties` tab and the displaced Dear whole-Inspector/component
painters. Retain source-built `edit_field` contributions and specialized UI/source-asset/Prefab
Intent panels under their existing owners; they are distinct consumers, not a fallback ordinary
Inspector. The existing shell continues to own document navigation/history/persistence and the
source tree until those complete workflows have replacements.

Migrate predecessor tests to the actual new CPU/native input paths and keep exact values,
defaults, structured/reference editing, source conflicts, component operations, local/document
history, tool replacement and unknown-record persistence assertions. Qualify default startup,
Prefab-source navigation, no-active/empty selection, read-only/replaced basis and native
retirement with the affected existing suite. Keep tree/workspace parity, virtualization,
measured costs and OS/platform evidence explicitly open after this replacement.

Independent review found that the public unattached `show` entry discarded Nara image candidates,
so merely changing its default Inspector would leave embedding permanently preparing. Reuse
the complete native owner as public `EditorNativeHost` with caller-loop events and an optional
caller-created primary window. The ordinary runner uses the same construction and cleanup tail.
Pass loop failures into consuming `finish`; ordinary close decisions remain explicit and an
unfinished Session cannot report success. Keep `show_contributions` explicit for source-built
panels without hosted Inspector/Scene outputs, and remove the ambiguous old entry. A separate
independent caller-loop consumer qualifies startup/paint/close; matched native authoring tests
use the same public Host construction for actual editing and retirement.

The independent process exposed a real default-main-thread stack overflow during ordinary
project opening, after native initialization. A local debugger traced it through by-value Open
progress and Session publication into authoring Runtime construction; the Session occupied
85,824 bytes in that debug binary. The Session now heap-owns its three largest operation hosts:
Schema-owner adoption, authored Schema upgrade and Prefab extraction. Starts and resets reuse
those allocations. The matched binary's inline Session size is 32,784 bytes; the operation states
remain owned heap allocations, so this is not a total-memory reduction claim. Public Open/Session
values, exact operation identity, cancellation and close custody retain their existing contracts.
The independent binary uses the ordinary process stack
and caller-created hidden window, preserving this regression beyond the larger test-thread stack.
The correction's independent static review has no P1/P2 finding. The process smoke qualifies
startup/painting/normal close, not all three operations' start/cancel/reset paths on that main
thread; their existing owner and native workflows retain their distinct test-stack scope.

## Default ordinary Inspector qualification

Qualified on November 4 on Windows/DX12 with Rust 1.95.0, serial Cargo,
`CARGO_BUILD_JOBS=1` and the root `target`. Native source-tool controls reuse the actual .NET 10
Host and EnemyStats v1 artifacts; CJK text uses the configured `simhei.ttf` fallback.

| Check | Result |
| --- | --- |
| Final ordinary Editor and selected public integrations | 355/355, 52 skipped; `b4fee414-d4f1-4db5-af35-50c7963dbcaa` |
| Final selected native Editor cohort | 35/35; `bc50941c-790e-42a8-8e13-e07e624e07b3` |
| Final independent panel/document/tool consumers | 46/46, four skipped; `8e4ddbfc-0c58-4625-96bc-46b57bca484e` |
| Independent caller-owned native Host process | 1/1; `57bb3cc5-790e-4a8e-9908-ba71b821b65c` |
| Session operation owners after heap-state correction | 63/63; `10d934d2-216c-47ea-a8cd-8917d389c255` |
| Build and lint | Editor build and all-target/all-feature `-D clippy::correctness` pass; existing warnings remain |
| Formatting and documents | Root/consumer formatting and diff checks pass; 1,050 relative file links resolve |

The ordinary run combines 346 library tests with nine selected public integration cases. CPU
interaction tests use the actual Nara reducer, local TextEdit state and a bounded hosted journal;
they do not claim GPU/OS delivery. Specialized UI fixture setup uses explicit public navigation,
while the native Canvas test opens its Prefab through the actual Nara Button and Winit ingress.

The native cohort includes all 22 ordinary authoring trajectories, Canvas, Game input, native
cadence, detached/recovered views, composed panel reconstruction, six retirement controls,
Scene reimport and the independent stock-composition effect. It preserves source values,
defaults/omitted fields, local/document history, conflict recovery, source-tool review, unknown
records and Save/Reopen. Two hosted Inspectors have independent compiled frames, surfaces and
confirmed placement occurrences; retiring one preserves the other. Game capture transfers to
the real owned text focus and resumes its later resize/exact-Step controls. The separate source-built
custom field panel protects the continuing `edit_field` consumer and keyboard handoff.

The independent hidden-window binary imports only public APIs and supplies its original Window,
forwards the caller loop into `EditorNativeHost`, observes three contributed-panel paints, sends
ordinary CloseRequested, and requires successful `finish` and unchanged fixture source bytes.
It uses neither a private graphics Host nor a larger configured process stack. This is
startup/painting/normal-close evidence; native field editing is established by the matched
authoring cohort using the same public Host construction.

Initial native qualification found three fixture regressions: a second Inspector hidden in the
same dock tab, a custom field window overlapping Source Tool review, and a removed Dear Sprite
field locator in the Game focus test. The replacements require both actual images, place the
explicit contribution away from review, and use real Nara pointer/text input. Earlier 24/27 and
the separate three-case repair run precede the final 35-case qualification above.

This closes the ordinary Scene/Prefab Inspector's default-panel replacement. Specialized panels,
tree/workspace replacement, keyboard-only shell entry, virtualization, measured idle/allocation
and retirement cost, actual OS IME/candidate placement, assistive technology and untested
platform/DPI combinations remain open. The complete R11/R12 and foundation Goal remain active.
