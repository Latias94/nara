# ADR 0047: Editor Workspace and Scene Document State

**Status**: Accepted
**Date**: 2026-07-09
**Last Revised**: 2026-07-28
**Refines**: ADR 0015, ADR 0026, ADR 0034, ADR 0038, ADR 0043

## Context

nara already has strong authoring foundations:

- `SceneDocument` and `PrefabDocument` are persistent data.
- `ScenePatchDocument` is the mutation/undo unit.
- `SceneAuthoringSession` treats the document as truth and projects into a managed live `World`.
- `SceneEditorState` owns Edit/Play/Paused flow and Apply Changes for a narrow safe subset.
- UI adapters render tooling models rather than owning scene mutation.

What is missing is the editor workspace layer. Mature editor behavior needs open documents, active
scene, selection sets, dirty/saved revisions, external file reload conflicts, per-document undo, and
multi-document operations. If this stays implicit, egui, future dear-imgui, future nara UI editor
panels, and AI agents will each invent their own workspace state.

## Decision

`nara_tooling` owns a UI-agnostic editor workspace model above individual authoring sessions.

```mermaid
flowchart TD
    Workspace[EditorWorkspace]
    Workspace --> Docs[OpenDocument slots]
    Workspace --> Active[Active document / scene]
    Workspace --> Selection[Selection set + top selected]
    Workspace --> History[Navigation and selection history]
    Workspace --> Conflicts[External reload conflicts]
    Docs --> Session[SceneAuthoringSession / Prefab session]
    Docs --> Undo[Per-document undo/redo]
    Docs --> Save[Saved revision / dirty state]
    Session --> Patches[ScenePatchDocument transactions]
    Workspace --> UI[egui / nara UI / AI adapter models]
```

Rules:

- The workspace is UI-toolkit agnostic and lives in `nara_tooling`, not in egui/dear-imgui adapters.
- Open documents are explicit slots with document kind, stable document ID, optional source path or
  asset reference, loaded document value, source revision, saved revision, dirty state, diagnostics,
  and external reload status.
- `SceneAuthoringSession` remains the per-document scene authoring/live projection boundary. The
  workspace coordinates many sessions; it does not replace patch validation or direct document
  ownership rules.
- Selection is workspace state: a selection set, top-selected item, selection history, and optional
  viewport/tool context. Selection targets use authoring identities and provenance, not runtime
  `Entity` values.
- Undo/redo is per document by default. Cross-document operations must be represented as explicit
  multi-document transactions with per-document patches and rollback diagnostics.
- Dirty/saved state is revision-based. A document is dirty when its current authoring revision
  differs from the last successful save revision.
- A successful save is proven by a non-cloneable, workspace-bound persistence checkpoint plus a
  consumed filesystem replacement receipt bound to the prior/candidate identity, captured
  revision, canonical content digest, post-publication content observation, and required guarantee
  tier. A UI/tooling command or an unhosted `EditorWorkspace` cannot advance saved state.
- External file changes enter as conflict state, not silent overwrite. The editor may offer reload,
  merge, save-as, or keep-local actions, but these are explicit workspace commands.
- Play Mode is anchored to a document snapshot and workspace mode state. Stop Play still discards
  runtime changes by default; Apply Changes flows back through document patches and normal undo.
- Workspace commands are explicit data values. UI adapters render workspace models and submit
  commands; they do not mutate documents or sessions through private paths.
- `ToolingPlugin` should eventually install the workspace resources and scheduling needed for
  tooling models. It should not remain a placeholder once editor workspace features ship.

## Workspace State Vocabulary

| Concept | Meaning |
|---|---|
| `OpenDocumentId` | Stable editor-session ID for one open scene/prefab/asset document |
| `OpenDocumentSlot` | Document value plus path/source identity, revisions, diagnostics, and conflict state |
| `ActiveDocument` | Document currently receiving inspector, viewport, and command focus |
| `SelectionSet` | Ordered selected authoring targets plus top-selected target |
| `DocumentRevision` | Monotonic authoring revision for dirty/saved/conflict checks |
| `ExternalReloadState` | Clean, changed-on-disk, deleted-on-disk, conflict, or reload-failed |
| `WorkspaceCommand` | UI/AI command that resolves to document patches, selection changes, mode changes, or save/reload actions |

## Alternatives Considered

### Option A: Let each UI adapter own workspace state

**Pros**: Fast for the first egui panel.

**Cons**: dear-imgui, nara UI, and AI adapters would diverge. Scene mutation and selection semantics
would leak into UI toolkit code.

**Decision**: Rejected.

### Option B: Use `SceneAuthoringSession` as the whole editor model

**Pros**: Reuses the existing deep module and avoids another layer.

**Cons**: A session is one document projection. It should not own multi-document workspace state,
selection history, save conflicts, or editor-wide mode/navigation concerns.

**Decision**: Rejected.

### Option C: UI-agnostic `EditorWorkspace` above authoring sessions

**Pros**: Preserves document-as-truth, supports multiple adapters, gives AI agents the same command
surface as editor UI, and keeps save/reload/conflict semantics centralized.

**Cons**: Adds a real tooling model before the visual editor exists.

**Decision**: Chosen.

## Success Metrics

| Metric | Target | Measurement |
|---|---:|---|
| UI adapter neutrality | egui and future nara UI editor panels consume the same workspace model | Adapter review |
| Multi-document readiness | Open scene/prefab documents have distinct dirty, saved, undo, and conflict state | Unit tests |
| Safe reload | External file changes never silently overwrite dirty local documents | Unit tests |
| Provenance safety | Selection targets use authoring identities/provenance, not runtime `Entity` | API review |
| Command consistency | UI and AI commands flow through workspace commands and scene patches | Unit/integration tests |

## Risks and Mitigations

| Risk | Severity | Likelihood | Mitigation |
|---|---|---:|---|
| Workspace becomes a hidden editor runtime | Medium | Medium | Keep it as UI-agnostic state/commands; runtime logic stays in app/world systems. |
| Cross-document transactions overcomplicate early tooling | Medium | Medium | Start with per-document undo and reject unsupported multi-document writes with diagnostics. |
| External reload merge is hard | High | Medium | Model conflict state first; merging can be explicit later. |
| Selection state duplicates inspector state | Low | Medium | Make inspector selection a projection of workspace selection once workspace ships. |

## Consequences

- `nara_tooling` should grow `EditorWorkspace` / `OpenDocumentSlot` before editor dogfooding expands.
- `SceneInspectorState` and `SceneEditorState` can remain useful, but they should become parts of a
  broader workspace model rather than independent editor singletons.
- `ToolingPlugin` should install meaningful tooling resources/systems when workspace state lands.
- Save/reload/dirty/conflict behavior should be tested at the tooling layer before a visual editor
  depends on it.

## Implemented Slice: RGF-U17 Known-Schema Product Loop

`EditorWorkspace` now remains the UI-neutral authority for document slots, selection, dirty/saved
revision, source digest, undo, and external-conflict state. The concrete root `EditorProjectSession`
owns filesystem capability, persistence receipts, and the single Play lifecycle; tooling and egui
hold no `World`, `RuntimeStartAttempt`, or `RuntimeInstance`.

Known-schema Save captures one linear checkpoint through the workspace's non-cloneable
`EditorPersistenceAuthority`. The concrete Host writes through `nara_fs`, then consumes a
non-cloneable `ReplaceReceipt`; the receipt distinguishes bytes accepted by the temporary-file
capability from bytes rebound and observed through the published target name. Only matching,
identity-bound post-publication content creates an `EditorPersistenceCommit`. Failure, conflict, or
ambiguous publication leaves the saved revision/digest unchanged and the document dirty; ambiguous
publication additionally blocks retry until explicit Reopen reconciliation.

Dirty Close and Exit require Save, Discard, or Cancel and retain the document until the selected
persistence action plus runtime retirement complete. Explicit Reopen may replace a dirty authoring
session only through the same workspace-bound authority after bytes decode and validate; failed
open, read, decode, validation, or workspace publication preserves the prior slot. This is a
single-document `DetectOnly` policy, not a strong compare-and-swap claim against a non-cooperative
writer that changes the file after post-publication observation. It does not admit unavailable-
schema authoring, recovery journals, or multi-document atomic publication.

### RGD-U11 API Migration

The former public `EditorPersistenceCheckpoint { document, revision, digest }` construction and
`EditorWorkspace::__apply_persistence_checkpoint` mutation path are removed. Embedding Hosts now:

1. create the paired workspace/authority through `EditorWorkspace::new_hosted`;
2. capture a document revision through `EditorPersistenceAuthority::capture`;
3. publish canonical bytes through a real `nara_fs` replacement operation;
4. consume its `ReplaceReceipt` through `EditorPersistenceCommit::from_publication`; and
5. advance saved state through the matching authority's `commit` method.

Ordinary UI/tooling consumers continue to use `EditorWorkspace::new` and cannot obtain persistence
authority. Opened-source digest binding and Reopen publication also require the paired authority;
`#[doc(hidden)]` is used only to de-emphasize the Host embedding surface, not as the capability
boundary.

## Open Questions

- Should the first workspace support prefab documents directly, or only scenes plus linked prefab
  source metadata?
- What is the minimal external reload conflict workflow before the editor has a full UI?
- How should project-wide asset documents participate in the same workspace model?
