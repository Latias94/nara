---
title: Source-Owned Preparation In Registered Editor Panels
date: 2026-10-25
status: Bounded source-built workflow and selection-independent query facade qualified; shared-owner locality and full R2 remain open
---

# Source-Owned Preparation In Registered Editor Panels

## Consumer And Responsibility

R2 needs a separately compiled, stateful panel to discover a non-active source, read a value,
prepare a computed edit, retain and inspect the actual unpublished candidate across paints,
and submit that same candidate. Construction must similarly preserve transaction-local
references and the fixed preview-to-success ID mapping. A test host preparing the candidate on
the panel's behalf does not establish this workflow.

The source owner already creates the candidate, inverse and history admission. The missing
boundary is access and transport, not another transaction implementation. ADR 0118 adopts the
finite source-built interface; this document explains its selection and acceptance, not its
implementation status.

The [scoped qualification](../knowledge/engineering/verification/2026-10/2026-10-25T024527Z-prepared-panel-complete-review-and-scoped-workflow-qualification-49e27d9779054a5b8fda1142add06519.md)
records the actual complete review, proof corrections, independent follow-up and serial public,
Editor and affected-existing-native evidence. It does not qualify installable tools or interactive
preparation performance.

## Selected Interface

| Alternative | Consequence | Selection |
| --- | --- | --- |
| Expose mutable Application or Session during paint | Bypasses ordered submission, origin filtering and drive ownership | Reject |
| Transport preparation and owned candidate through every action/outcome | Requires one-shot extraction through currently cloneable observations without an asynchronous consumer requirement | Keep outside this unit |
| Borrow read-only preparation, then consume the candidate through existing actions | Matches current synchronous preparation and ordered mutation responsibilities | Select |

Application splits its panel-workspace borrow from finite, read-only preparation access. Panel
context forwards to existing Session entry points, including project/domain/lifecycle admission,
and exposes the same named-document entity/authoring query facade used by the qualified
direct-member workflow. It never exposes that Session publicly. Preparation from a query observation checks the complete
document incarnation/revision/Schema basis through Workspace's existing resolver, then invokes
the ordinary preparation path. That check is not a revision reservation or another candidate
validator. Construction drafts retain their own captured source/Schema and local scope.

Panels retain `Option<PreparedEditorDocumentEdit>` or
`Option<PreparedEditorDocumentConstruction>`, not a candidate ID into a Host registry. Their
preview reads the existing patch/inverse and, for construction, fixed local mapping. Dropping
the candidate before enqueue cancels without publishing source state or another Undo entry.
Runtime preview publication is a separate operation, not a side effect of inspecting this value.

Two concrete consuming actions reuse existing ordered dispatch:

```rust,ignore
EditorAction::CommitAuthoringEdit(prepared_edit)
EditorAction::CommitAuthoringConstruction(prepared_construction)
```

`EditorAction` and `EditorActionBatch` are move-only. Do not preserve blanket `Clone` or
`PartialEq` by cloning candidates, comparing only patches, or introducing an interior-mutable
one-shot container. Historical construction mappings and successful receipts may be cloned;
their equality and resolution retain exact construction-scope affinity and resolved IDs.

The typed panel commit methods preserve the original candidate on queue refusal. Successful
enqueue transfers ownership to the frontend candidate; the panel cannot promise cancellation
afterward. Existing origin/incarnation, supersede, project replacement, filtering and exact
`PanelSubmissionId` completion rules remain in force. No callback is manufactured for a queue
entry that was refused. An observer retired after publication cannot roll the document back.

Commit-only actions use source-candidate authority rather than a geometry frame. A batch with
navigation or other frame-sensitive work retains whole-batch frame admission. The Session
consumes the candidate directly; extracting its patch and preparing a replacement would violate
preview/commit identity. Success is the actual owner report, or the construction outcome carrying
the owner's committed mapping. `Dispatched` alone is not success. Several candidates in an
ordered batch are not one transaction; two from the same basis may accept then reject stale.

Use a Workspace-target panel for a named non-active document. This unit does not change the
activation requirement of existing Source-target built-ins, introduce a remote preparation
protocol, deliver inline Inspector customization, or qualify installable tools.

## Verification And Stop Conditions

- A real registered panel discovers component/root IDs, reads and computes a replacement,
  prepares through context, cancels one candidate, retains another across paints and commits it
  on non-active Scene and Prefab sources. No host-side preparation substitutes for panel access.
- Retained previews, cancellation, stale Schema bases and queue refusal publish no revision,
  dirty, selection, history or disk changes. Queue refusal returns the same usable candidate.
- One successful multi-operation candidate creates one Undo. A second same-basis candidate is
  rejected, with no automatic rebase or result inferred from a later frame.
- Panel construction creates two locally referenced entities; preview IDs equal the successful
  mapping through Undo/Redo and Save/Reopen. Receipt clones retain scope identity; failed
  dispatch does not fabricate a successful mapping.
- Existing lifecycle, stale-frame, supersede, hide/retarget/reset/project replacement and close
  tests must still cover candidate-bearing work. Retired instances do not receive old callbacks.
- Serial owner/Editor/public consumer checks and appropriate native regression evidence precede
  qualification. Compilation is not native presentation or performance evidence. Measure
  preparation costs separately from read-only point queries and ordinary Application drive.

Stop and deepen the existing source owner if this requires tool-built inverses, another history,
candidate registries, unsafe borrow splitting or mutable document access during paint. Do not
solve a source-built synchronous consumer by requiring a universal asynchronous SDK.

## Pinned Mechanism Comparisons

The [bounded cost comparison](../benchmarks/2026-10-25-prepared-panel.md) keeps synchronous
preparation, typed enqueue and consuming dispatch/drive separate. The measured source includes
real entity references and preserved unknown components. Its shared preparation cost still needs
owner phase attribution; the transport contract does not establish interactive latency or close
R2's locality work. Built-in result observers borrow original-slot outcomes; only an owned panel
completion clones its historical result. This does not introduce another report or candidate store.

Read-only Bevy `85503941e488152711a4695bbe248e9ed9f453fc` defines `Command::apply(self, &mut World)`
and `CommandQueue::push(command)` by value in
`repo-ref/bevy/crates/bevy_ecs/src/system/commands/command.rs` and
`repo-ref/bevy/crates/bevy_ecs/src/world/command_queue.rs`. The useful principle is consuming work
ownership, not blanket command cloning. Bevy's command queue is not a prepared source transaction,
history owner or proof of Nara's exact panel result/lifetime contract; no queue implementation is
copied into Nara.

Godot `81ab961f91066a63bcbf3ec340962bb1a1ada558` exposes explicit create/do/undo/commit history
operations in `repo-ref/godot/editor/editor_undo_redo_manager.h`. That makes undo ownership
visible, but does not require Nara panels to construct their own inverse operations. Nara's
document sessions already validate and invert semantic source edits, so the panel transports
their candidate rather than reimplementing Godot's callable-based history adapter.

## References

- [Foundation plan](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR 0118](adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [Read projections](authoring-read-projection-design.md)
- [Retained construction](authoring-construction-design.md)
- [Panel context](../../editor/src/frontend/imgui/workspace.rs)
- [Session preparation](../../src/project_host/runtime/editor/prepared_edit.rs)
- [Source candidate](../../crates/nara_scene/src/authoring/prepared.rs)
