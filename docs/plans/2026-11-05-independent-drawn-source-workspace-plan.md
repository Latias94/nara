# Independent Drawn Source Workspace

**Status:** Initial source workflow qualified on Windows/DX12; complete successor remains in progress
**Owner:** Editor frontend, native Host and existing Application/Session owners
**Parent:** [Foundation R11b](2026-10-23-foundation-architecture-rebaseline-plan.md#r11b-public-panel-hosting-and-component-composition)

## Complete consumer

Launch a separately selected Nara-drawn Editor composition without Dear dependencies or
initialization. Its first complete workflow opens an ordinary data-only project, displays open
source documents, selects a Scene or Prefab, uses the existing ordinary source Hierarchy and
specialized UI Hierarchy/tree/forms, and runs exact Undo/Redo and Save/Reopen/acknowledgment.
Source structure, Schema, selection, dirty state, history and persistence retain their existing
owners. All UI is drawn through the shared kernel/presenter and original WGPU Host.

This is the first complete workflow inside the independently runnable successor, not the
complete replacement matrix or a change of default. Inspector, Scene/Game, Browser/import,
project-manager/recovery breadth, docking and the remaining specialized tools are subsequent
workflows in the same composition. Unsupported panel kinds are explicit. Existing reference
controls and their full behavior remain available until complete successor acceptance.

## Responsibilities

- Extract pure Hierarchy controllers and shared source-label/admission helpers from the Dear
  adapter. Both compositions use the same implementations; no copied controller or alternate
  source lowering, identity allocation, history or result dispatcher.
- The new private frontend owns its shell controls, document toolbar, local panel state and
  Nara input adapter. The public semantic panel Host retains exact candidate/result custody.
- The public native image Host owns original instance admission, complete-window image routes,
  platform focus/capture/modal/IME/clipboard/cursor/refresh. The new composition supplies real
  native events and immutable presented snapshots, with original field-token validation.
- The original WGPU Host owns devices, queues, target producers and all native/image submissions.
  The existing UI presenter/journal owns exact UI acceptance. Multiple visible panel images
  adopt together only after the exact complete native result.
- Composition/preparation may repeat. Input and exact action results perform explicit updates;
  preparing UI never mutates source or publishes effects.

## Integration corrections

The current native input lock protects ordinary authoring while a workspace close decision is
pending. The successor shell needs a separate exact decision scope over the original accepted
image. Only the current prompt/result can retain that route; ordinary Hierarchy routes remain
withdrawn. The shell's captured bindings and the original Session still admit each decision.
This is a private first-party consumer before any general public lifecycle-input policy.

Presentation reconciliation revalidates retained field tokens before refreshing native text
services. Escape, hidden controls and projection replacement cannot leave an old IME scope
pointing at a retired field. Original pending display/snapshot pairs survive deferral; after
completion, a following redraw compiles state that advanced while that pair was pending.

A terminal render/input failure pauses drawing immediately and reports its original diagnostic.
The native title and keyboard remain available without the failed renderer: F5 reconstructs
graphics, while S/D/Escape resolve the displayed exact ordinary close prompt and Enter
acknowledges its result. No automatic dirty-data discard is permitted. Graphics reconstruction
retains compatible controllers and Session history while giving native presentation fresh
Surface identities. Final runner results use the shared reference implementation to preserve
original failures, physical graphics cleanup, failed Open/Session receipts and Open Host Tasks.
Injected failure controls and actual device-loss recovery remain separate evidence scopes.

The source chooser consumes the existing Browser snapshot and request lifecycle through bounded
pages, rather than using test-only source-open choreography. Complete source paths and snapshot
generation/index/capabilities remain in captured bindings even when their labels are bounded.
Source-admitted UI structure also corrects an obsolete compiled-only frontend capability gate:
the complete original UI projection plus source Schema/readiness permits closed recipe lowering,
while original source construction and dispatch still validate every component/property/topology.
Continuous field/Canvas authority remains separate. The native consumer retains all four recipes,
history/persistence and source-preservation assertions rather than narrowing to read-only UI.

The source projection cache also retains whether the original complete UI projection matches
the safe authoring projection. Selection and maintenance publication reuse that result rather
than rebuilding the source projection on every frame. Schema adoption and source/gesture
changes keep their original cache invalidation. Degraded sources keep recipe construction and
movement where admitted, while deletion exposes the original Complete/no-Prefab-anchor refusal
before lowering. Direct owner submission retains the same independent final deletion guard.

Native pointer release does not claim keyboard ownership after projection replacement. Text
services refresh from the actual accepted endpoint after presentation. Specialized form buttons
explicitly use existing target-geometry capture continuity: composition-blur validation rows may
disappear without losing an unchanged Cancel press. Changing only the scroll extent retires old
wheel observations, while source, form, behavior and actual target geometry changes still retire
their pointer actions. CPU and native trajectories keep press/present/release coverage.

Both compositions use one extracted frame-maintenance deadline calculation. Source Open,
Browser and other actual owner work retain maintenance wakes until idle; cancelled exit state
comes directly from Application instead of a frontend boolean. Graphics retirement is polled
without consuming the original owner. Diagnostics do not establish physical completion, and
unfinished retirement cannot install a replacement. An initialization failure leaves an explicit
F5 retry available even when the predecessor has completed and no live graphics exist. Normal
close keeps the event loop alive while the original GPU owner retires; the final cleanup tail
also retains that owner until completion rather than dropping it after a timer. No finite
completion-time guarantee for arbitrary driver failure follows from this custody rule.
Display-frame identity belongs to the surviving native workspace, so a fresh GPU Host continues
the window's monotonic sequence instead of replaying frame one against an accepted native route.
Recovery still forwards native focus/geometry and IME lifecycle acknowledgments through the
original image Host, including periods with no live graphics. Disabling a previously ready
IME must receive its acknowledgment before a recovered field requests a new scope. Ordinary
text and pointer events remain withdrawn while paused. A wheel observation invalidated by a
local filter/form update is an expected input refusal; only that typed refusal becomes a no-op
with redraw, while invalid input and actual backend errors retain their failures.
Normal process Exit can remove the Session before an already requested redraw arrives. A
missing authoring frame under the Application's actual pending Exit is inert while retirement
continues; it is not a project-open failure. Genuine Open recovery retains the original
diagnostic codes and summaries in the runner's error rather than replacing them with a generic
frontend message. The executable startup/ordinary-close check exercises the real event-loop
ordering separately from the injected native workflow's direct retirement driver.

The Image recipe uses an actually imported fixture product, prepared through the original
Browser/import owners before the interaction trajectory. Canonical metadata alone does not
establish a current imported choice. This setup does not qualify drawn Browser/import controls.
The Shell exposes an explicit source refresh over the existing Browser action. An atomic Save
can replace the source file's native identity while the old index snapshot remains visible.
Reopening that closed document must first refuse the old identity, preserve data and acknowledge
the exact failed request, then refresh and admit the successor. The frontend never bypasses the
index/source admission checks or silently retargets a captured Open button.

Creation/movement tests choose actual offered placement declarations. `Last` may be represented
by a deduplicated `After` choice, and chained sibling declarations can make a later reparenting
invalid. The positive subtree workflow uses independent `First` declarations. A separate actual
pointer trajectory creates a dependent sibling chain and requires the original
`scene.patch-local-layout-invalid` refusal, unchanged history/source and explicit form cancellation.
The frontend cannot silently rewrite other authored placement predicates to make that move succeed.

## Verification

1. Existing controller/Editor CPU workflows continue after relocation; no-default compilation
   proves the extracted controllers do not require Dear. Preserve forms, capacity and stale
   projection/selection/Schema refusal cases.
2. Launch the actual new executable on Windows/DX12. Through presented pointer/key/text controls,
   select source declarations, create/move/delete, switch documents, Undo/Redo, Save/Reopen and
   acknowledge exact attempts. Compare complete source bytes/decoded source and one-history-entry
   semantics, including unknown records. Exercise specialized UI creation recipes and cancellation.
3. Two simultaneous shell/content images have original composites. Close with text/preedit and
   a candidate pending; late input/output cannot reach replacement panel incarnations. Actual
   original producer/device retirement must complete without abandoned submission obligations.
4. A separately locked build/dependency consumer excludes every Dear package. No official
   external-toolkit extension or mixed first-party composition is required.
5. Run affected reference native trajectories, independent consumers, correctness Clippy,
   executable builds, formatting and local document links. Record exact scope and remaining
   platform/cost gaps rather than treating this workflow as complete R11b/foundation acceptance.

## Qualification checkpoint

The final November 5 Windows/DX12 production-composition trajectory passes in run
`978dc758-eeaf-470a-b163-14d7a33d4180` (79.830 s). It retains all ordinary Scene/Prefab,
unknown-source, four-recipe, subtree, history/persistence, projection and instance retirement
assertions. The Text recipe commits all 4,096 UTF-8 bytes through injected native IME, with a
Chinese Button label as a second field control. It also keeps source-order and stale-source
identity refusal, explicit refresh, dirty Cancel/Discard, retained original graphics during
delayed/error close polls, initialization failure and successful explicit reconstruction.
Two exact failure diagnostics remain recorded after recovery; successful physical cleanup is
checked independently rather than treating those injected diagnostics as missing obligations.
The final review adds a filter edit followed by wheel input before any successor paint,
the IME disable acknowledgment during pause, focus notifications with no live graphics and
post-reconstruction Chinese input without another disable acknowledgment. All original source,
history, controller-state and physical-retirement assertions remain. The earlier run
`045b2630-27a2-4228-a77e-500da1eb25b4` passes before the final inert post-Exit redraw assertion;
`f6140bb7-ae5f-4ba2-a1e4-7b2b086828c0` is a separate pre-review checkpoint. These debug
durations are correctness observations, not an attributed performance comparison.

Earlier failed iterations exposed pointer-release keyboard ownership, Cancel capture across
composition blur, display-frame continuity across reconstruction and missing source refresh.
Fixture corrections preserve real imported choices, owner-offered placements, canonical source
encoding and component Schema owners. The tests now retain negative controls for conflicting
authored sibling declarations and old source identity instead of weakening source admission.

CPU scopes are separate: the final reference library and selected public integrations pass
472/472 in run `42e1031e-88ea-4b8d-ac4d-22de9638c4fa`, while the final no-reference library
passes 220/220 in run `d12b7f72-4b8f-4d65-9b70-723716570fda`. Earlier checkpoints pass
471/471 and 219/219 before the final Shell/control
refinements; the Shell separately passes 19/19 including Refresh. These scopes overlap and
do not represent additive coverage.

The source prerequisite is committed as `6cc1cd8`. All 21 selected root all-feature tests pass
in run `26cca64a-d612-4b8c-84b5-8ed0c645cdeb`, including the complete four-recipe source/history/
persistence consumer, missing-template/wrong-Schema refusal, protected/mixed/incomplete source
and affected authoring-image/Canvas/continuous-gesture regressions. Root all-feature library
correctness Clippy also passes. Earlier fixture failures required actual Image import and the
existing degraded-document deletion refusal; neither failure justifies relaxing owner checks.

The affected reference native cohort initially passes 30 of 31 cases in run
`6d16bbb2-f90c-401f-8089-a600d51fde9f`. External cadence stops before its quiet-running
assertions because the Reference Game's independently locked graph omits the Editor's
`arboard` and `wgpu` dependency edges. An isolated locked/offline metadata command reproduces
that refusal. The corrected root, Reference Game, UI Host, module-consumer and stock-consumer
locks change dependency edges only, without adding, removing or upgrading package versions.
Reference Game and stock-consumer metadata then pass. The isolated native cadence retry passes
in run `a02d19a2-0c14-4e1e-82d9-ddbd1c711101` (486.234 s), including the actual supervised
build, child presentation, quiet maintenance and natural-exit presentation. The 30-case cohort
and isolated retry are separate runs. The original supervised Play preparation/admission
path is unchanged; the added failure diagnostic retains the original owner result/output.

The separately locked UI Host consumer builds all three examples with `native`. Its CPU
suite passes 14/14 in run `bfc66a4a-30ed-41b2-b353-e53dba14c8b0`, and its two actual
Windows/DX12 native consumers pass 2/2 in run `023a582c-ae85-48bd-92b7-48f5aa4068bb`.
The Nara adapter retains six presentations and five journal commits; the independent software
frontend retains 18 presentations and seven exact source/history/persistence results without
adopting a Nara widget tree. Their internal assertions retain complete source preservation,
native admission, stale input/output refusal and original physical GPU retirement.

The root no-reference production binary and separately locked public `drawn_editor` entry
both launch against independently owned temporary copies of the ordinary known-unbound
project. Each remains running before receiving ordinary `WM_CLOSE`, exits zero and preserves
all 11 original source files. The first root close attempt exposed the queued-redraw/Session
retirement race described above; the corrected root and public entries both pass. These are
actual executable/event-loop checks, not test-only success modes. A separate normal/build/
feature dependency inspection excludes every Dear/ImGui package, `imgui-reference` and
`editor-imgui` from the public entry's selected no-default graph.

The independently locked module panel consumer passes 46/46 with four ignored native cases
in run `c65af37a-f76f-4918-b87b-dc7fd9e0f155`. Its separately selected embedded public Host
passes the real hidden-window startup/paint/ordinary-close trajectory, 1/1 in run
`74b8824d-dd1e-4737-8104-d6bf5bcf64d5`, preserving the fixture's original source bytes.
These public-consumer results retain their reference/toolkit-independent composition scopes.

After the final shared scroll-refusal correction, the four affected reference input trajectories
pass 4/4 in run `4143bf22-f45c-42ea-a083-8c1cd32646c8`: ordinary Scene/Prefab structure,
1,000-declaration filter/wheel/text lifetime, Inspector keyboard/clipboard and the full specialized
UI/Canvas workflow. The earlier broader 30-case pass plus isolated cadence retry retain their
separate original runs. The final default binary pair and no-reference production binary build;
Editor all-target correctness Clippy passes with all features and without reference features.
Existing non-correctness warnings remain. Root/independent formatting and changed-document
file links pass. No Linux/macOS, actual OS IME/accessibility, arbitrary driver-loss recovery,
finite driver-close time or performance/default-switch qualification follows from this unit.

The final independent correctness rereview has no remaining P1/P2 findings after the IME
acknowledgment, expected wheel refusal and pending-Exit redraw corrections. The subsequent
[styled-workspace prerequisite](2026-10-23-foundation-architecture-rebaseline-plan.md#styled-source-workspace-and-component-library-prerequisite)
is a separate design/implementation unit. Current engineering controls do not qualify a mature
theme/typography system, reusable visual recipes or responsive page layout. The complete
foundation and successor Editor work remain active.
