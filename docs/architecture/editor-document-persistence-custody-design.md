---
title: Per-Document Editor Persistence Custody
date: 2026-10-23
status: Custody decision accepted; remaining R5c workflow Proposed
---

# Per-Document Editor Persistence Custody

## Problem And Scope

The prepared-edit consumer exposed a mismatch between independently owned Workspace slots and
the integrated Host's singleton Scene/Prefab file custody. Opening another source of the same
kind still follows the existing replacement policy. Merely keeping more tabs without their
concrete file Hosts would strand persistence and cleanup authority.

This design replaces that singleton assumption. It is the persistence-custody sub-unit of
[R5c](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md), not the whole projection,
durable-addressing, history-expiry or Prefab-overlay redesign. ADRs
[0047](adr/0047-editor-workspace-and-scene-document-state.md) and
[0118](adr/0118-editor-workspaces-frontends-and-creation-templates.md) adopt the bounded
per-document custody decision. Additive Open, complete alias convergence, multi-source refresh
and Play-retirement source continuity remain Proposed. The implementation ledger owns actual
qualification; neither prepared edits nor map storage alone establish public same-kind multi-open.

## Ownership And Alternatives

Keep four identities separate:

- A scoped source locator and retained parent/target capability identify the publication source.
  Current file identity detects aliases; it is not a durable document ID and can change on Save.
- `EditorDocumentId` identifies the Workspace slot. Its session owns history, selection and saved
  state. Reopen retains the document ID but replaces the source incarnation.
- Preview selection identifies the document/revision projected by the current preview Runtime.
  A persistence map's first item is never a preview policy.
- Project startup and the captured Play request determine executable startup, independently of
  tab activation, file-map insertion order or ordinary source Open.

Prefer two concrete maps keyed by `EditorDocumentId`, retaining the existing Scene/Prefab lane
types. A single map with a closed enum is possible but does not remove the real codec and
checkpoint distinctions. An erased store/trait would move those distinctions into downcasts
or a second generic protocol without a consumer benefit. Retain closed typed persistence dispatch,
extension-document maps and `HostCustody` registrations; do not create a Runtime per document.

The pinned Godot source (`81ab961f91066a63bcbf3ec340962bb1a1ada558`) separates its `EditedScene`
collection from `current_edited_scene` in `repo-ref/godot/editor/editor_data.h` and its associated
activation/history operations in `editor_data.cpp`. This supports separating retained documents
from active navigation; it does not establish equivalent Rust or file-capability guarantees.

At Bevy pin `85503941e488152711a4695bbe248e9ed9f453fc`, the relevant source representations
are `DynamicWorld` and `WorldAsset` under `crates/bevy_world_serialization`, not an assumed
older `DynamicScene` API. `WorldInstanceSpawner` separately tracks asset/instance membership
and instance/entity mappings. That distinction supports keeping source and live incarnation
separate, but Bevy's World-backed representations do not supply Nara's data-only document,
history, file-capability or saved-checkpoint contracts.

## Lifecycle

### Open And Source Exclusion

An already open exact source activates its existing slot without rereading disk or replacing
history. A new source prepares its document and concrete file Host, then rechecks request,
source/index/Schema admission, closing and source exclusion at owner publication. Do not capture
an unrelated same-kind document's revision as a predecessor. Editing A while opening B must not
invalidate B merely because A changed.

Use one private alias-checking helper over existing concrete lanes, not a second source registry.
Normalize to the Host's project-relative source domain rather than comparing raw locators from
different scopes. Compare current file identity through retained parent/target capabilities,
including all Scene/Prefab/extension combinations. Exact-source activation and other-alias
rejection remain distinct. Lock/read/identity failure cannot count as proof of non-aliasing.
Do not rebuild ambient absolute paths or lowercase every path. Atomic Save can break an old
hard link; subsequent admission must observe the current target, not retain an eternal alias.

Failed publication retires only the unpublished candidate and restores the original active
document. Workspace fallback-to-first-slot is not exact restoration of prior navigation.
This remains session-local admission under the existing cooperative/DetectOnly file protocol,
not continuous strong CAS against arbitrary external writers.

### Save, Reopen And Content Refresh

Keep a single global persistence operation slot: independent documents do not require parallel
file operations. Capture the exact document checkpoint, readmission basis and concrete lane
handle together. Completion, uncertainty, baseline updates and cleanup act on that retained
lane, never a currently selected lane of the same kind.

Reopen also binds its original Host, document incarnation/revision, source binding and Schema.
Native callbacks remain in owner admission; background work does not gain executable authority.
Use existing retained-handle identity rather than another public generation system.

Prefab source observations need one canonical source-domain key: anchored Open currently uses
Prefab-relative asset paths, while Browser slots may carry project-relative locators. Choose
the domain key at admission rather than copying an arbitrary Workspace locator into refresh
state. Content refresh must retain
each saved source's outstanding digest obligation. A small source-keyed collection is sufficient:
a later verified save may supersede the same source, not another source. Closing a tab does not
erase an outstanding saved-content obligation. Publish `Current` only after the successor closure
has satisfied every retained obligation; do not introduce a general dependency-graph framework.

### Close, Project Handoff And Preview

Only remove a slot/lane after its exact decision is admitted, affected Play ownership retires,
and its Host borrowers and temporary cleanup physically complete. Pending cleanup, failed Save,
Cancel and stale prompts retain both source occupation and file custody. Closing B must not
retire A's file Host or stop A's unrelated Play. Preserve Play's originating document through
Starting, cancellation and physical-retirement states, not only while Running.

Exit traverses all document decisions. Project replacement retains the complete predecessor
session until its actual physical handoff boundary. Existing custody registration transfer
already handles multiple Hosts; transfer it once at successful runtime replacement.

Preview policy must be explicit when multiple Scenes exist. Preserve the existing independent
Scene/Prefab view workflow rather than silently choosing a map entry. Compare active-Scene-only
with a retained last-explicitly-selected Scene binding before adoption; a closed preview source
must clear or deliberately rebind it. The current `Back to Scene` first-Scene search must either
use that explicit binding or yield to the existing document selector. Keep the current global
Scene Reopen-versus-Play exclusion until a separately qualified policy replaces it.

The recommended multi-Scene policy is a retained last-explicitly-selected Scene: active-only
preview would remove the existing Scene view while editing a Prefab or extension. Bind initial
startup and successful explicit Scene selection/Open; leave non-active query/edit/Save/Reopen
navigation-neutral. Clear only when the bound source actually closes, with no fallback to the
first map entry. This broader selection policy is not adopted by the storage-only slice, whose
private binding preserves the existing singleton Scene view. `Back to Scene` and presented-input
admission must move with the later policy, independently of executable startup.

Two further owner obligations remain explicit. The current content-refresh owner has one
pending/failed obligation, so a later source must not be allowed to erase an earlier source's
unsatisfied refresh. Trusted Play's cancelling/retiring states also need to retain their exact
source before document Close can use source-local isolation throughout retirement. These are
follow-up replacements, not claims that map storage has already fixed them.

## Ordered Change Set

1. Migrate concrete lane storage, captured operation handles and all cleanup/uncertainty routes
   while temporarily preserving the current Open policy. Cover both constructors and execution
   lanes. Do not mechanically replace `Option` access with `values().next()`.
2. Centralize scoped source/alias exclusion across Browser, anchored Prefab and extension Open.
   Bind preview independently and preserve exact active navigation on failed publication.
3. Make ordinary authoring Open additive; delete the same-kind predecessor-close state machine,
   obsolete Open phases and misleading UI text. Dirty explicit Close keeps its own decisions.
4. Retain multiple Prefab refresh obligations and exact Play source through retirement. Extend
   the independent consumer to two Scenes and two Prefabs, then qualify public/native adapters.

Primary owners are `runtime/editor.rs`, `document_open.rs` and its preparation module,
`persistence.rs`, `persistence/reopen.rs`, `extension_documents.rs`, `content_refresh.rs`,
`trusted_play.rs`, Tooling's document-open model and the Dear frontend's document controls.
Do not rewrite the concrete file publication protocol, project-replacement state machine or
physical `persistence/retirement.rs` registry merely because document storage changes.

## Acceptance And Deletion

- Two Scenes and two Prefabs retain distinct sessions, selection, history and usable file Hosts.
  Query/prepare/commit and Save/Reopen of non-active B leave A's state unchanged.
- Browser and anchor Open of one Prefab reuse its slot. Exercise alias combinations, supported
  case aliases, distinct case-sensitive files, directory rename and atomic-Save identity changes.
- Opening B tolerates unrelated A edits but rejects stale B source/index admission. A failed
  publication preserves original navigation and does not retire any predecessor Host.
- Captured Save/Reopen, cancellation, panic and failed receipt affect only their original owner.
  Later B edits remain dirty; A edits do not stale B's exact basis.
- Held B cleanup retains B's slot and source claim while other documents remain usable.
  Multi-document Exit/replacement checks later prompts, Cancel, failed Save and stale decisions.
- Save P, fail its refresh, then save Q: Q cannot clear P's obligation. Runtime replacement/drop
  retains every lane, held operation and failed temporary cleanup until physical return.
- Exercise Play source isolation through starting and retirement, plus relevant actual native
  authoring/presentation trajectories. Do not infer new trusted arbitrary-Scene Open support.

Replace old singleton/retarget tests with additive Open plus explicit Close tests while retaining
their index, source, cancellation and physical-return coverage. Remove displaced API/state/UI
branches instead of leaving compatibility aliases. No cross-document transaction, atomic Save All,
multiple simultaneous Play Runtimes, dynamic tool ABI or session recovery journal is implied.
