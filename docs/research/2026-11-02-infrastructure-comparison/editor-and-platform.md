# Editor And Platform: Authoring Semantics Before Toolkit Breadth

Date: 2026-11-02. Source-based comparative assessment; no implementation changes.

## Scope And Verdict

An editor is an authoring operating system: it arbitrates document truth, drafts,
selection, focus, commands, history, tools and native interaction. Drawing controls
is one client of that system. Nara should retain its existing semantic owners and
complete its chosen custom-drawn frontend around two consumers: an independent
Snap command and a multilingual Inspector beside a live Scene image.

Two conclusions dominate. First, the missing integration is ordinary command
availability and complete editable interaction, not another document store or Undo
kernel. Second, convenient trusted APIs and strict execution boundaries solve
different problems. Fyrox and Godot reduce ordinary tool-author choreography, but
their mutable contexts and retained executable operations do not provide Nara's
source-revision and prepared-publication guarantees for free.

All four local HEADs matched the requested revisions:

| Repository | Reviewed commit |
|---|---|
| Nara | `a2640a10973b6d1724176a506237bc458aa5f3bc` |
| Bevy | `ae6152152d6fb8308ad6038314b48ef295e80fbc` |
| Fyrox | `b5b151b603bba0549ac03db676568726f17e6807` |
| Godot | `95c0fa5fe8a019adc29d074c16b64469b04571f7` |

This review reads AGENTS, Strategy, the implementation ledger,
[R1](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r1-minimal-semantic-tool-and-exact-operation-outcomes)/[R11](../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r11-ui-content-association-text-admission-and-platform-qualification), and relevant
Accepted ADRs 0013/0015/0026/0031/0041/0047/0102/0107/0108/0118. It runs no Cargo,
network, native workflows or agents. The supplied 236 passing Input/UI tests are
prior evidence, not a fresh execution here. Finite endpoints, reserved terminal
credits and total UI outcomes are delivered foundations, explicitly confirmed by
the [ledger](../../architecture/adr/implementation-status.md). Build/restart,
checkpoint, delivery/CI and diagnostic-to-trace analysis belong to the other lane.

## Comparison Matrix

Costs below mean integration responsibilities and retained state, not benchmarked
latency or a ranking of entire engines. Each column addresses the same consumer.

| Responsibility | Nara now | Bevy at this commit | Fyrox at this commit | Godot at this commit |
|---|---|---|---|---|
| Truth, selection and drafts | Workspace owns authored identities; Scene/Prefab owns source and history. Frontend retains invalid input and exact revision conflicts. | ECS widgets expose state/events; the reviewed primitives do not supply Nara's complete document contract. | Plugins receive mutable Editor context; MVC synchronization and command discipline connect selection, model and widgets. | Object properties and Editor selection integrate with Inspector and history context. |
| Ordinary independent tool | Window-free Snap works. Its visual installer registers only a panel; menu/shortcut contribution remains R1 work. | Feathers provides editor-oriented controls; application observers still supply tool semantics. | Plugin callbacks live outside individual controls; routed messages reach plugin logic. | Tool-menu callbacks, docks and Inspector plugins have separate registration surfaces. |
| Undo ownership | Owner-produced forward/inverse, exact document basis, bounded retention and refusal without publication. | EditableText explicitly lists Undo/Redo as unimplemented; widget change events are not document transactions. | Command objects execute/revert/finalize; groups reverse execution order. Correct inverse behavior remains command-author responsibility. | EditorUndoRedoManager groups property/method operations; LineEdit separately owns local text history. |
| Editable text and IME | Dear currently supplies editing. Nara text uses Parley/Swash with one admitted face; display layout is not an editable-control contract. | Parley-backed editing, selection, clipboard and IME exist. Candidate positioning still targets PrimaryWindow. | TextBox offers immediate, focus-loss and Enter commit policies, feeding routed text/property messages. | LineEdit integrates local history, shaped caret/hit testing, window-specific IME and accessibility actions. |
| Layout, focus and accessibility | Runtime UI already uses Taffy and a bounded AccessKit publisher. These do not establish custom Editor accessibility or virtualization. | UI layout and accessibility projection exist; EditableText still lists AccessKit integration as future work. | Retained controls expose measure/arrange and a UserInterface owns focus/capture and message routing. | Control owns event acceptance/focus navigation; TextServer supplies editing geometry through RIDs. |
| Placement and retirement | Source-built panels and finite child proposals exist; installed execution routing deliberately has one active slot. | Compiled plugins and ECS observers offer composition, without proving safe resident native unload. | Game-plugin hot reload explicitly flushes scene command stacks and removes plugin property editors. | EditorPlugin exposes visibility, menu and Inspector hooks; this convenience does not imply arbitrary callback unload safety. |

Bevy's current source contradicts the obsolete blanket claim that it has no editor
infrastructure. [Feathers, `crates/bevy_feathers/src/lib.rs:1`][b-feathers] explicitly
targets editors/inspectors, while [EditableText, `crates/bevy_text/src/editing.rs:10`][b-edit]
documents implemented interaction and precise remaining gaps. Its controlled-widget
model deliberately exempts expensive text state
([`crates/bevy_ui_widgets/src/lib.rs:13`][b-widgets]). That is useful evidence for
Nara's draft ownership, not evidence of complete editor parity.

Fyrox's actual chain is `UiMessage -> handle_routed_message -> Inspector property
change -> selection/controller -> command`. See
[`fyrox-ui/src/lib.rs:2252`][f-route],
[`editor/src/plugins/inspector/mod.rs:591`][f-inspector] and
[`editor/src/command/mod.rs:83`][f-command]. Its short callback route lowers author
ceremony, but `execute`/`revert` return no transactional result. Command grouping
alone is not evidence of failure-atomic source publication.

Godot exposes separate menu and Inspector placement in
[`editor/plugins/editor_plugin.cpp:242`][g-plugin] and `:515`, and assigns operation
history through [`editor/editor_undo_redo_manager.cpp:107`][g-history]. Borrow the
separation of responsibilities; importing Node/Object/Variant, global services or a
stable native ABI is neither required nor justified by these mechanisms.
Control's event consumption and focus-neighbor APIs
([`scene/gui/control.cpp:2643`][g-control], `:3377`) explain how ordinary editor
controls share interaction policy without making their visible panel the command
owner. This is a useful responsibility boundary even with a different widget model.

## Trace 1: A Tool With Its Panel Closed

EnemyStats already demonstrates the essential ownership split. The Source Tool
panel starts closed
([`editor/src/workspace/mod.rs:71`](../../../editor/src/workspace/mod.rs#L71)).
The ordinary Inspector matches the activated owner/component/property and invokes
its integer proposal control independently
([`field_editor.rs:791`](../../../editor/src/frontend/imgui/field_editor.rs#L791)).
The control emits `IntegerProperty` carrying the original activation and target;
its review emits Accept for the exact request
([`source_tool.rs:186`](../../../editor/src/frontend/imgui/source_tool.rs#L186)).

Session owns the active invocation and prepared candidate. Acceptance checks the
request, activation and original query basis before consuming the retained
construction through the ordinary source owner
([`src/project_host/runtime/editor/source_tools.rs:681`](../../../src/project_host/runtime/editor/source_tools.rs#L681)).
Neither painting nor panel closure is execution authority. Application delegates
the same operation and returns the commit result separately from its drive update.
Disabling execution is distinct from retiring the panel and from physical child
cleanup; this is valuable existing infrastructure.

The native test source prepares the Inspector proposal before opening the tool
panel, verifies that the second observer sees the same patch/inverse, accepts it,
then hides the panel and performs Undo/Redo: health `100 -> 125 -> 100 -> 125`,
history depths `0 -> 1 -> 0 -> 1`
([`authoring_workflow_tests/source_tool.rs:884`](../../../editor/src/platform/imgui/authoring_workflow_tests/source_tool.rs#L884)).
This is an inspected existing assertion sequence, not a new run or proof of every
closed-panel interaction.

The counterexample is ordinary Snap placement. Its pure planner and public Session
consumer already edit an inactive document without disturbing selection, retain
one prepared candidate, reject stale work and use one Undo
([`tests/snap_local_position.rs:190`](../../../tests/fixtures/module-consumer/editor-panel-consumer/tests/snap_local_position.rs#L190)).
However, [`src/snap_local_position.rs:214`](../../../tests/fixtures/module-consumer/editor-panel-consumer/src/snap_local_position.rs#L214)
registers only a panel; its preview and pending observation are panel fields.
Frontend reconciliation drops hidden instances
([`workspace.rs:395`](../../../editor/src/frontend/imgui/workspace.rs#L395)).
There is no corresponding contributed menu/shortcut path in that implementation.
R1 explicitly leaves this consumer open. The missing owner is contribution-level
command availability, not semantic execution.

## Trace 2: Text Editing Through Document Undo

In the current Inspector, a field key excludes the monotonic revision; its draft
retains the original target/schema basis. An unrelated revision marks a dirty
draft conflicted instead of replacing its text
([`field_editor.rs:34`](../../../editor/src/frontend/imgui/field_editor.rs#L34),
`:182`). Dear's input widget edits the local candidate. Enter or Apply parses it
and emits one `ApplyInspectorCommand`; invalid text remains local, and conflict
requires explicit Reapply (`:381`, `:513`).

Workspace resolves the exact source and revision before applying that command
([`crates/nara_tooling/src/workspace.rs:1452`](../../../crates/nara_tooling/src/workspace.rs#L1452)).
Only the matching successful result clears the draft (`field_editor.rs:641`).
The toolbar's Undo carries document identity and current revision
([`authoring.rs:112`](../../../editor/src/frontend/imgui/authoring.rs#L112));
Workspace reaches the source history owner. Replay checks content-state identity,
prepares the candidate, restores the popped entry on rejection, and publishes the
document/reverse entry together
([`crates/nara_scene/src/authoring.rs:510`](../../../crates/nara_scene/src/authoring.rs#L510)).
This is already a real document Undo path.

The native draft test preserves Chinese preedit across an unrelated revision,
commits it into the retained draft, then explicitly reapplies through history
([`field_drafts.rs:170`](../../../editor/src/platform/imgui/authoring_workflow_tests/field_drafts.rs#L170)).
It does not establish OS candidate-window placement or a Nara-owned editing buffer.
The custom frontend must still define local typing Undo versus document Undo,
composition cancellation, grapheme/caret behavior and focus-loss policy. ADR 0026's
Begin/Preview/Commit-or-Cancel invariant is accepted; a universal gesture API is not.

Godot makes the two histories visible: LineEdit consumes `ui_undo` and restores
text/caret locally ([`scene/gui/line_edit.cpp:989`][g-line], `:1839`); Inspector
separately creates property actions with `MERGE_ENDS`
([`editor/inspector/editor_inspector.cpp:5774`][g-inspector]). Fyrox instead exposes
commit timing explicitly through [`fyrox-ui/src/text_box.rs:125`][f-text]. Neither
policy should silently replace Nara's explicit Apply/conflict semantics. Bevy's
current IME implementation excludes preedit from the committed value, but its
local Undo remains open: rich shaping alone does not finish editing.

## Four Decisions, Alternatives And Acceptance

### 1. Give semantic commands a contribution lifetime

Recommend a finite typed command contribution owned by composition/Session, with
menu, shortcut and optional panel invoking the same handler. Availability observes
an explicit document/selection context; invocation captures the exact basis.
Keep source preparation, commit and result ownership unchanged. Reconcile ADR 0118
before extending its panel-only contribution surface.

Alternative: keep a hidden panel alive or introduce a string command bus. The former
makes layout responsible for execution; the latter adds dispatch machinery without
solving target or lifetime semantics. Cost: moderate shell/API work, little domain
work. Fyrox's persistent plugin callbacks and Godot's separate menu registration
demonstrate the useful separation without requiring their broad mutable contexts.

Acceptance: install external Snap without private hooks; close its panel; invoke
menu and shortcut on an explicitly targeted inactive source. Each actual change
produces one exact result and one Undo, persists through Save/Reopen, and refuses a
stale candidate. Retired visual observers cannot cancel an already committed edit.

### 2. Define editable-state semantics before building more controls

Recommend distinct document/property identity, widget incarnation, draft basis and
composition lifetime. Keep typing/preedit outside document history; accepted field
commit makes one source transaction. Route text Undo to its active edit buffer and
document Undo through explicit document commands. Specify Escape, focus loss,
selection change and panel retirement; preserve current explicit conflict recovery.

Alternative: per-keystroke document patches with later merging. Godot supports
merged property actions, but reproducing that policy would change Nara's validated
draft behavior and amplify source work. Cost: high interaction/testing work, not
primarily rasterization. Nara already uses Parley/Swash
([`crates/nara_text/src/engine.rs:3`](../../../crates/nara_text/src/engine.rs#L3));
reuse mature editing primitives while owning the control and publication policy.

Acceptance: Chinese composition, combining marks, emoji clusters and mixed bidi
text; rejected numeric drafts; selection replacement; local Undo then committed
document Undo; focus transfer during composition; late commit after same-label
widget recreation. Verify actual OS candidate placement separately from injection.

### 3. Make geometry and native interaction agree in the first slice

Recommend one Nara-drawn Inspector plus clipped Scene image, with explicit
layout/paint/semantic invalidation, focus/capture arbitration and admitted coordinate
mapping. Select Editor font fallback policy separately from deterministic Runtime
single-face policy. Retain exact image leases and presentation outcomes; do not
make a UI image label own GPU lifetime.

Alternative: rewrite every panel first, or infer editing support from glyph output.
Both postpone the highest-risk integration. Godot's
[`servers/text/text_server.h:573`][g-text] exposes caret, selection and grapheme hit
testing on shaped-text RIDs; the transferable lesson is shared geometry, not a
requirement to copy RID ownership. Bevy's IME positioning still has a multi-window
TODO ([`text_input.rs:575`][b-ime]), illustrating the remaining platform cost.

Cost: high platform/integration work. Acceptance: real candidate placement, text
Delete versus viewport Delete, fractional DPI and clipped hit testing; keyboard and
assistive-technology operations on the same field; reject stale window events.
For a large reordered/filterable list, count live controls and layout work against
visible rows. Clipping alone is not virtualization. Remove each Dear path only
after matching workflow parity; detached-window support needs its own evidence.

### 4. Separate ordinary tool ergonomics from executable lifecycle

Recommend a direct trusted Rust path using bounded reads and owner-prepared edits,
plus the existing explicitly approved finite-child path where installation and
process separation are needed. Both publish through the same source/history owner.
An approved child remains trusted executable code, not a sandboxed extension.
Reduce repeated caller orchestration; retain meaningful basis and result checks.
Do not impose wire messages or hypothetical unload tokens on ordinary static tools.

Alternative: every tool becomes a package/process, or every plugin receives mutable
Editor/world access. The first taxes simple operations; the second shifts invariant
enforcement to every extension author. Fyrox's concrete hot-reload cost is decisive:
[`editor/src/lib.rs:3198`][f-reload] clears scene histories/selection and removes
plugin-owned property editors before reload. Nara's data proposals avoid retaining
that child's executable inverse, but do not prove safe resident-code unloading.

Cost: moderate contribution/routing work; native hot unload would be a separate,
larger lifetime project. Acceptance: two pre-admitted tools coexist; updating or
disabling A preserves B's valid review and Inspector route, rejects A's stale
events, and separately observes physical retirement. Amend the deliberate single
active-slot contract first; initially reject overlapping property facets. Compare
author-visible concepts, callbacks and actual calls for Snap before inventing a
universal plugin kernel. No latency advantage is claimed without measurement.

## Settle Now; Deliver Later

Settle now: semantic target and contribution scope; draft/widget/composition
identity; local versus document Undo; publication and stale-result rules; focus and
shortcut precedence; layout-to-text-to-hit-test coordinates; font policy; and the
difference between route revocation and physical retirement. These decisions shape
every subsequent control and extension.

Deliver later: broad widget catalogues, rich text, generalized recursive inline
UI protocols, sophisticated docking, full cross-platform detachment, multi-document
transactions and resident native hot unload. Accessibility implementation can be
incremental, but semantic identities, actions and geometry must exist from the first
editable slice. Nara's existing Runtime AccessKit publisher explicitly does not
prove OS-client observation
([`crates/nara_winit/src/accessibility.rs:68`](../../../crates/nara_winit/src/accessibility.rs#L68));
it cannot substitute for Editor qualification. These are scoped product successors,
not evidence that the completed Input/UI foundation needs rebuilding.

[b-feathers]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_feathers/src/lib.rs#L1
[b-edit]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_text/src/editing.rs#L10
[b-widgets]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_ui_widgets/src/lib.rs#L13
[b-ime]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_ui_widgets/src/text_input.rs#L575
[f-route]: https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-ui/src/lib.rs#L2252
[f-inspector]: https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/editor/src/plugins/inspector/mod.rs#L591
[f-command]: https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/editor/src/command/mod.rs#L83
[f-text]: https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-ui/src/text_box.rs#L125
[f-reload]: https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/editor/src/lib.rs#L3198
[g-plugin]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/editor/plugins/editor_plugin.cpp#L242
[g-history]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/editor/editor_undo_redo_manager.cpp#L107
[g-line]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/scene/gui/line_edit.cpp#L989
[g-inspector]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/editor/inspector/editor_inspector.cpp#L5774
[g-text]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/servers/text/text_server.h#L573
[g-control]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/scene/gui/control.cpp#L2643
