---
title: Capability-Bound Editor Project Browser and Source Index v1 - Plan
type: feat
date: 2026-08-19
deepened: 2026-08-19
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: ce-plan-bootstrap
origin: STRATEGY.md
execution: code
depends_on: docs/plans/2026-08-18-2158-feat-standalone-generic-editor-project-open-and-recovery-plan.md
plan_id: editor-project-browser-source-index-v1-2026-08
unit_namespace: EPB
---

# Capability-Bound Editor Project Browser and Source Index v1 - Plan

## Goal Capsule

| Field | Contract |
|---|---|
| Objective | Make the standalone `nara-editor` browse the opened project's declared source roots, display a deterministic project tree and source facts, and open supported Scene/Prefab documents without compiling or loading project Rust code. |
| Means | Complete the reserved handle-bound directory enumeration primitive in `nara_fs`, build a bounded Host-owned source-index transaction and immutable generation, then project it through `nara_tooling` and a private egui Project Browser panel. |
| Product owner | `nara_fs` owns native directory enumeration and platform proof; the project Host owns scanning, classification, budgets, snapshot publication, and recovery; `nara_tooling` owns browser state and frame-bound actions; `nara_tooling_egui` owns presentation only; private `nara-editor` owns window integration. |
| Architecture authority | Accepted ADRs 0020, 0047, 0050, 0070, 0090, 0104, 0110, 0114, and the completed Standalone Generic Editor plan remain authoritative. ADR 0070 owns the filesystem primitive; ADR 0114 explicitly gates project-wide browsing on that primitive. |
| Execution profile | Six dependency-ordered units. Keep the first slice read-only and manual-refresh driven. Reuse the existing Authoring Project, workspace, persistence, shell, and egui action seams while adding the missing closure-external document admission seam. |
| Stop conditions | Stop the affected path if implementation requires ambient `std::fs::read_dir`, a second project/document authority, a RuntimePlan or native registry, project code execution, a generic async framework, a watcher dependency, unbounded recursive traversal, or a filesystem guarantee not present in the capability matrix. |
| Tail ownership | EPB-U5 updates ADR 0114/0070 implementation rows, the architecture map, the implementation ledger, a completed registration, and one verification record. Predecessor plans and evidence remain immutable. |

## Product Contract

### Summary

The standalone Editor can now open a project generically, preserve degraded Scene/Prefab data, and
show a source authoring workspace. It still cannot behave like a production Editor's project panel:
the user cannot browse the declared source tree, discover another Scene or Prefab, inspect an asset's
source facts, or open a document by selecting it from a project-wide view.

This plan adds the smallest product slice that closes that gap without weakening the generic Editor
boundary. The source tree is discovered only through manifest-declared roots and a capability-bound
filesystem enumeration primitive. The resulting index is a Host-owned, immutable, bounded snapshot;
the UI never receives a raw path or a filesystem handle. A failed or incomplete rescan is visible and
does not destroy the last-good project workspace.

The first slice is intentionally a browser and source index, not an importer, file manager, or Play
system. It lets a designer find and open source documents while preserving the existing strict
Runtime boundary.

### Problem Frame

ADR 0114 deliberately deferred project-wide browsing because `DirectoryCapability::read_directory`
is still an explicit `Unsupported` primitive. The current `nara_asset::ProjectAssetDatabase` is an
in-memory logical-record table, not an authorized filesystem scanner, and the Editor's Host snapshot
contains only the startup/dependency closure needed for authoring and Runtime agreement. Adding a
quick `std::fs::read_dir` call in the Editor would create ambient authority, bypass reparse/mount
proof, and make the project browser disagree with Save, Reopen, and Export.

The missing product is therefore a cross-layer contract, not an egui widget. Native enumeration must
first produce scoped observations from a retained directory handle; the Host must turn those facts
into a bounded source index; only then may the UI offer selection and Open actions.

### Key Decisions

- KD1. **The first Project Browser is read-only and source-oriented.** It can select, filter, inspect,
  and open supported documents, but it cannot rename, delete, create, import, execute code, or mutate
  the filesystem. This protects the source tree while the relative mutation/reclamation guarantees
  remain separately gated. Governs R2, R12, R16, R20, R24.
- KD2. **Only manifest-declared roots are indexed.** The default roots are `paths.assets`,
  `paths.scenes`, `paths.prefabs`, and `paths.scripts`, plus the manifest and declared authoring
  schema bundle as explicit top-level entries. `.nara`, `target`, Cargo metadata, and arbitrary
  sibling directories are not implicitly scanned. Governs R3, R4, R18, R25.
- KD3. **`nara_fs` is the only enumeration authority.** Domain crates consume observations and never
  reproduce platform traversal with ambient paths. Strict ordinary Editor roots use the existing
  Untrusted/Recovery proof policy; unsupported or unproven platforms report a typed browser failure
  while keeping the Editor alive. Governs R4-R8, R22, R26.
- KD4. **Scanning is incremental and synchronous, not async.** Each Editor update advances at most one
  move-only directory-enumeration session by deterministic item/work budgets. This prevents a wide
  directory or large tree from monopolizing a frame without introducing a second task runtime,
  cross-thread handle ownership, cancellation protocol, or watcher semantics. An async/task-pool
  design remains a future measured successor. Governs R9-R12, R23, R27.
- KD5. **A scan publishes one immutable generation atomically.** The existing
  `EditorActionBatch.frame_revision` binds UI actions to the exact immutable frame containing the
  project/index facts; the Host does not create a second stale-action protocol. A failed replacement
  retains the last-good snapshot and its diagnostics. Governs R10-R15, R21, R27.
- KD6. **Scene/Prefab Open deepens the Host's existing authoring core.** The browser never parses or
  publishes a second document copy. It emits one Host-owned navigation/open action containing the
  selected source identity; one new on-demand admission basis reuses the current nominal authoring
  decode/publication core, while existing persistence and workspace paths remain the sole document
  authority. Governs R15-R17, R21.
- KD7. **Asset metadata is visible but uninterpreted in v1.** Asset sources and `.meta` sidecars are
  deterministic InspectOnly entries. The browser does not parse stable IDs, infer source/meta pairs,
  update `ProjectAssetDatabase`, or offer repair. Import status belongs to a later asset workflow.
  Governs R7-R8, R16, R20-R21.

### Requirements

#### Product identity and lifecycle

- R1. The installed `nara-editor` remains the only ordinary Editor binary; a project-specific
  compiled companion is neither required nor loaded by the Project Browser.
- R2. A project that opens successfully in Generic Open can display a Project Browser even when its
  Authoring Project is Degraded or Runtime is unavailable.
- R3. The browser indexes only the manifest's effective source roots and explicit manifest/schema
  members. It does not infer roots from the process current directory, Cargo workspace discovery,
  environment variables, or arbitrary sibling folders.
- R4. Every enumeration operation is performed through the exact `DirectoryCapability` retained by
  the current project attempt. No browser, asset, or UI module may call ambient `read_dir`, path
  canonicalization, or a path-based fallback.

#### Filesystem and source facts

- R5. `nara_fs` exposes a bounded direct-child enumeration operation that returns each entry's
  validated relative name, `FileKind`, and optional scoped `FileIdentity`; it never returns a raw OS
  path or treats an observation as a durable asset ID.
- R6. Strict enumeration never follows reparse points, symlinks, cross-volume objects, or unproven
  identities. A safely isolated bad entry or subtree becomes a bounded Unsupported/rejected fact and
  does not hide safe siblings; a failure that prevents complete observation of the current directory
  fails the attempt. Native names that cannot become portable project-relative UTF-8 paths are
  reported without exposing raw OS paths.
- R7. The Host classifies entries into stable source kinds: ProjectManifest, AuthoringSchemaBundle,
  Scene, Prefab, AssetSource, AssetMeta, Script, Directory, Unknown, and Unsupported. Classification
  is based on project-relative path and bounded name/kind facts, never file contents or executable
  loading.
- R8. Asset sources and `.meta` sidecars retain their exact project-relative paths as read-only
  InspectOnly entries. This plan does not parse, generate, repair, or reconcile metadata records.
- R9. Enumeration and scan budgets bound entry count, directory count, depth, path bytes, retained
  snapshot bytes, rejected-entry diagnostics, and per-update work. Exceeding a budget yields a typed
  diagnostic and no partial published replacement.
- R10. Native enumeration support is truthful: v1 claims strict product support only for Windows
  x86-64 local NTFS with native evidence. A Linux implementation may land behind `Unproven`, but it
  cannot be promoted until a dedicated native Linux/filesystem proof records handle-bound mount,
  symlink/magic-link, hard-link, and identity-drift behavior. Other combinations remain
  `Unsupported` or `Unproven`.

#### Snapshot, refresh, and actions

- R11. The Host owns a `ProjectSourceIndexSnapshot` with project-root identity, scan generation,
  effective roots, ordered entries, diagnostics, budget usage, and completion status. The snapshot is
  immutable and shareable by `Arc`; the UI cannot mutate it.
- R12. Initial indexing and manual Refresh advance through a bounded state machine. A refresh never
  enumerates an entire wide directory in one update, blocks the native event loop on an unbounded
  tree, or spawns a generic async task. While scan or query work remains, the application exposes a
  narrow pending-work signal so the native `ControlFlow::Wait` loop schedules the next redraw.
- R13. A completed scan replaces the previous snapshot atomically as Ready or
  ReadyWithDiagnostics. Isolated unsupported entries/subtrees are pruned with bounded diagnostics so
  safe siblings remain browsable. Root/capability drift, an incomplete directory listing, canonical
  path ambiguity, or budget exhaustion fails the attempt, retains the optional last-good snapshot,
  and exposes a structured retry reason without closing the project. Accepted project replacement
  retires the old scan state with the old project; it is not modeled as scan cancellation.
- R14. Browser selection, expansion, and filtering are UI-neutral tooling state keyed to the current
  immutable snapshot; only Refresh and Open cross into the Host through the existing
  `EditorActionBatch` and its exact `EditorFrameRevision`. The Host resolves each ingress action
  against the immutable browser facts in that accepted frame; stale frames, paths absent from that
  snapshot, non-Openable entries, or identity drift fail closed without changing the workspace.
- R15. Selecting a Scene or Prefab and activating Open delegates to the existing Host Authoring
  document-admission and persistence owners. Closure-external documents are decoded with the same
  exact authoring schema bundle and Adapter receipts as Generic Open. Retargeting the single Scene or
  Prefab lane preserves source identity and uses the existing dirty Save/Discard/Cancel flow.
- R16. Selecting a non-document source displays facts and diagnostics only. It does not decode image
  pixels, execute scripts, invoke importers, build a RuntimePlan, or create an ECS World.
- R17. The browser can filter by path substring and source kind without rescanning. Filtering is a
  cached UI-neutral projection over the immutable snapshot, not a second index or query authority.
  A changed snapshot/query rebuilds the row projection in bounded batches while the last complete
  projection or explicit progress state remains renderable.
- R18. Ordering is deterministic: directory entries and final flattened rows are sorted by canonical
  project-relative UTF-8 path, with kind/status as explicit tie-breakers. Native enumeration order
  never leaks into the product.
- R19. The browser shows scan progress, last-good generation, and bounded diagnostics through a
  closed safe projection containing only code, phase, optional relative source, fixed summary,
  impact, repair hint, and numeric budget facts. Raw errors, absolute paths, OS handles, native
  identities, arbitrary metadata contents, and secrets never cross the UI boundary.
- R20. A browser action cannot rename, delete, create, move, import, or auto-fix source files in this
  plan. Those operations require a later capability-specific plan and separate ADR admission.

#### Boundaries and maintainability

- R21. `nara_asset::ProjectAssetDatabase` remains the logical asset-record authority and is not
  modified by browsing or Refresh. It does not gain ambient filesystem access or become the scan
  owner.
- R22. `nara_tooling` owns the UI-neutral browser model and typed actions; `nara_tooling_egui` owns
  layout and input translation; `editor` owns only platform/window wiring. No browser state is stored
  in `editor/src/platform.rs` or duplicated in the egui Adapter.
- R23. The plan introduces no Tokio, async-std, generic background task abstraction, or watcher
  dependency. Existing manual source-change APIs remain untouched.
- R24. The implementation includes focused native filesystem tests, Host transaction tests, tooling
  action tests, egui projection tests, and one real standalone Editor browse/open trajectory. It does
  not add a platform-by-feature Cartesian matrix or screenshot parity suite.
- R25. Declared source roots are normalized before scanning. Missing roots remain visible as bounded
  unavailable placeholders; non-directory/unproven roots do not block unrelated valid roots; exact
  duplicate or ancestor/descendant root paths are scanned once with all logical roles retained;
  identity-alias or canonical-path ambiguity fails the affected candidate rather than duplicating it.
- R26. The Windows support claim is a per-attempt runtime proof, not only a static platform-matrix
  flag. Every root and descendant directory handle admitted to a strict scan must prove local NTFS
  semantics and the required identity/resolution tier; ReFS, network, removable, reparse-backed, or
  otherwise unproven volumes remain visible as unavailable and are never silently upgraded.
- R27. A successful scan performs a bounded final lineage revalidation of every traversed root and
  directory identity before publishing its replacement snapshot. A rename cannot redirect the live
  enumeration session, but the declared relative path must still resolve to the same identity at
  final revalidation; rename, replacement, disappearance, mount/volume drift, or an incomplete
  cursor therefore invalidates publication and retains the last-good snapshot. This is not a claim
  of byte-level point-in-time consistency for in-place writes.

### Actors and flows

1. **Open and initial index:** the existing Generic Open transaction publishes an Authoring Project;
   the Host creates an idle source-index state from the same root capability and effective manifest
   roots; the Editor shell remains usable while scan steps advance.
2. **Browse:** the user expands a root or searches the current snapshot. The Adapter reads only the
   immutable `EditorProjectBrowserView` and emits no filesystem calls.
3. **Open document:** the user selects a Scene/Prefab and presses Open. The action carries source
   path inside an exact frame-bound batch; Host finds the row in that frame's immutable source index,
   reopens/revalidates it, and admits the document through the retained schema/Adapter basis. If the
   same-kind persistence lane is occupied, the existing dirty replacement flow resolves it first.
4. **Refresh:** the user presses Refresh. The current snapshot remains visible while a new bounded
   attempt scans the same root. Success publishes a new generation; failure preserves the old one.
5. **Project replacement:** the existing shell replacement flow retires the browser snapshot with
   the old project and constructs a fresh state for the accepted replacement only.

### Acceptance examples

- AE1. On Windows local NTFS, open a generic project whose `assets`, `scenes`, `prefabs`, and
  `scripts` roots contain mixed directories, Scene/Prefab files, images, `.meta` files, and an
  unknown file. The browser shows all declared entries in deterministic order without compiling the
  project or scanning `target`/`.git`.
- AE2. Select a closure-external Scene, open it, then select a closure-external Prefab. Each is
  decoded with the Generic Open schema/Adapter basis and bound to its existing persistence owner; a
  dirty same-kind lane enters Save/Discard/Cancel, while the other lane remains intact.
- AE3. Inject a safely isolated reparse/symlink entry during Refresh. The new generation contains a
  bounded rejected diagnostic and safe siblings remain visible. Replacing a scanned directory so its
  complete contents cannot be proven fails the attempt, causes no path escape, and retains the
  previous snapshot.
- AE4. Add an asset source and a `.meta` sidecar with arbitrary contents, then Refresh. Both remain
  visible as independent read-only entries; the browser does not parse, generate, repair, or use the
  metadata to mutate the Asset Database.
- AE5. Start a scan over a bounded large fixture. Each Editor update advances a finite amount of
  work, the shell remains responsive, and no thread/task/watcher is created. A budget overflow
  produces a structured failure and no partial replacement.
- AE6. Submit an Open action after a newer scan or project replacement. Host rejects the stale
  generation without mutating active document, selection, dirty state, or persistence state.
- AE7. On an unsupported macOS/BSD or unproven filesystem adapter, the Editor remains open and the
  browser displays a truthful unavailable diagnostic rather than falling back to ambient traversal.

### Success criteria

| Signal | Target |
|---|---:|
| Ordinary Editor project-specific binaries required for browsing/open | 0 |
| Ambient directory enumeration in production browser/asset code | 0 |
| Partial scan published as a source-index snapshot | 0 |
| Stale browser action mutates workspace | 0 |
| Deterministic flattened ordering across repeated scans | 100% identical for equal source facts |
| Browser scan/query work per Editor update | bounded by declared item/work limits |
| Supported native proof | Windows x86-64 local-NTFS product evidence; Linux and other combinations remain truthful unsupported/unproven until separately proven |

### Scope boundaries

Included:

- handle-bound direct-child enumeration and capability-matrix updates;
- bounded recursive source-index transaction over manifest-declared roots;
- Scene/Prefab/asset/script/unknown classification and read-only `.meta` entries;
- immutable Host snapshot, progress, manual Refresh, stale-action rejection;
- on-demand generic Authoring admission and single-lane Scene/Prefab persistence retargeting;
- UI-neutral browser model/actions and a private egui Project Browser panel;
- one standalone Editor trajectory proving browse, filter, open Prefab/Scene, Refresh failure, and
  last-good retention.

Explicitly excluded:

- file rename, delete, move, folder creation, metadata generation, or source mutation;
- importer execution, image decoding/thumbnails, artifact cache changes, reimport, or cooking;
- OS file watchers, live auto-refresh, debounce policy, Tokio/async-std, or generic background jobs;
- external supervised Play/Run, live code reload, animation, shader/material authoring, or plugin
  hosting;
- arbitrary project roots, Cargo workspace indexing, `.git`/`target`/`.nara` scans, package catalogs,
  cooked/source-free export, or a general-purpose query language;
- a second Asset Database, document parser, Scene/Prefab workspace, or Editor state machine.

### Dependencies

- Completed Generic Editor Open/Recovery and Authoring Project snapshot (`a86aee1`).
- Existing `nara_fs` capability/session/identity and platform proof machinery.
- Effective manifest paths from `nara_project::ProjectPathsManifest`.
- Existing logical `ProjectAssetDatabase` boundary, which this plan does not mutate.
- Existing Host Authoring Scene/Prefab decode/publication and persistence cores. Their current
  startup/dependency-closure routing is intentionally insufficient for arbitrary indexed documents;
  EPB-U3 adds the missing on-demand admission and retargeting seam.
- Existing `EditorFrameModel`, `EditorProjectView`, `EditorActionBatch`, and egui shell composition.

### Outstanding questions

All questions below are explicitly deferred and do not block this implementation-ready plan:

- Which importer-specific thumbnails and previews should be admitted after the read-only browser
  proves its source-index contract? Deferred to an asset-preview plan.
- How should Rust/script entries open in a configured external editor without leaking raw paths or
  weakening process-launch authority? Deferred to an external-source integration plan.
- When should `.meta` parsing, stable-ID collision diagnostics, and import/reimport actions enter the
  browser? Deferred to an asset-import-status plan.
- Should an OS watcher request a manual rescan or apply per-entry invalidation? Deferred until a
  measured live-reload workflow requires it.
- Should project-owned ignore files or package mount roots participate in indexing? Deferred until
  package/source layout authority is accepted.
- Should rename/delete use a separate capability transaction or remain external tooling? Deferred
  to a dedicated persistence/reclamation plan.

## Planning Contract

### Key Technical Decisions

- KTD1. **Replace the placeholder with one bounded child-enumeration session.** A root capability has
  no valid empty `RelativePath`, and the current scanner needs only direct-child enumeration. Expose
  one move-only enumeration session from the current `DirectoryCapability`; callers open descendant
  directories through the existing handle-relative API before enumerating them. Remove the obsolete
  `read_directory` placeholder rather than supporting two equivalent public entry points.
- KTD2. **Use platform-native handle enumeration without sharing a mutable directory cursor.** The
  session owns an independent stream/handle and returns bounded batches plus a completion flag.
  Windows derives an independent exact-self handle-relative cursor from the retained directory
  handle (the implementation uses an empty-name `NtOpenFile` relative to that handle; a future
  adapter may use `ReOpenFile` only when the platform proves that directory form). It then uses the
  handle-bound `NtQueryDirectoryFileEx`/`FILE_ID_BOTH_DIR_INFORMATION` cursor. The first batch
  explicitly resets that private cursor; later batches continue only through the move-only session.
  If the platform cannot prove an independent exact-self handle, the operation remains Unsupported
  rather than sharing `DirectoryInner.file`. No platform uses `std::fs::read_dir` on a path. Unix
  adapters remain unchanged Unsupported/Unproven in this plan.
- KTD3. **Keep filesystem observations scoped and non-durable.** `DirectoryEntryObservation::identity`
  remains an optional process/session-scoped fact. The browser derives no stable asset ID from it;
  selected entries are reopened and revalidated through the current root capability before Open.
  A Scene/Prefab row is Openable only when the scan obtained an exact child-open `FileIdentity`;
  otherwise it remains visible as InspectOnly with a typed reason.
- KTD4. **Make the Host scan a private state machine.** Add a Host-private
  `AuthoringSourceIndexAttempt`/`ProjectSourceIndexSnapshot` module rather than a generic `Indexer`
  trait or a new public crate. It owns a FIFO directory frontier, canonical sorted entry facts,
  rejected-entry diagnostics, budget accounting, and atomic publish.
- KTD5. **Use item/work budgets, not wall-clock deadlines.** The initial Host defaults are
  `16,384` total entries, `2,048` directories, depth `32`, `2 MiB` logical path bytes, `256`
  diagnostics/`256 KiB` diagnostic text, `16 MiB` retained snapshot bytes, one active enumeration
  session, and per-update work of at most `256` entries or `32` directories, whichever comes first.
  These defaults are Host-owned
  constants rather than project-controlled manifest values. This makes tests deterministic and
  avoids pretending that a machine-dependent time slice is a security boundary. The platform event
  loop remains responsive because each `advance` consumes only the per-update work allowance.
- KTD6. **Keep project-wide index separate from Runtime closure.** The index uses the same exact root
  capability and manifest lineage but is not added to `StartupContentClosureReceipt.content_digest`
  and cannot make Runtime/Export accept more content. It has its own bounded attempt receipt and
  last-good snapshot.
- KTD7. **Do not turn the browser into an Asset Database updater.** `nara_asset` does not depend on
  `nara_fs`, open paths, or own scan progress. The v1 index retains only read-only source/meta path
  facts and cannot update `ProjectAssetDatabase`.
- KTD8. **Use one UI-neutral browser state and the existing frame-bound action ingress.**
  `nara_tooling` owns the copied selection key, path/kind filters, expanded relative paths, and cached
  bounded row projection. Selection/filter/expansion update that model locally; only Refresh and Open
  are Host actions inside an `EditorActionBatch`, whose accepted frame is their sole
  freshness/context authority. egui retains only widget focus, scroll, and other non-semantic control
  transients.
- KTD9. **Use existing document navigation and persistence.** Add the smallest typed navigation
  variant needed to request a Scene/Prefab source by index fact, then call one shared Host authoring
  document-admission basis. V1 keeps one path-bound Scene lane and one path-bound Prefab lane;
  retargeting either lane uses the existing Save/Discard/Cancel replacement protocol. Do not add a
  second parser, parallel persistence authority, or browser-local dirty/reopen workflow.
- KTD10. **No async in this slice.** Async becomes justified only when a measured scan or importer
  workload exceeds the bounded per-update model and a concrete owner can define cancellation,
  generation, close, and retained-handle semantics. This plan intentionally does not create those
  semantics prematurely.
- KTD11. **Atomic publication does not claim a point-in-time filesystem snapshot.** A successful
  attempt proves that one bounded traversal completed and each Openable document fact was validated
  through the retained capability. Files may still change between observations; action-time reopen
  and identity comparison remain mandatory. In-place writes to the same file object are intentionally
  read as current content by the existing document opener rather than treated as a byte snapshot.
- KTD12. **Open arbitrary Scene/Prefab sources from the retained authoring basis, not only startup
  closure members.** Extract the existing bundle/Adapter-backed document decode/publication logic
  into a Host-private admission basis owned by the Authoring session. Each Open gets a fresh bounded
  per-document attempt and exact source identity; Runtime registry/plan and project Rust remain absent.
- KTD13. **Normalize declared roots once, before traversal.** Convert the small fixed set of
  effective manifest roots into one ordered capability-relative root/role set. Missing roots become
  explicit unavailable facts; non-directory/unproven roots become scoped diagnostics; duplicate and
  ancestor/descendant roots are scanned once. If paths that normalize to the same canonical/alias
  key resolve to different native identities, or if their canonical spelling is ambiguous, reject
  the affected candidate instead of guessing. Distinct declared roots normally have distinct
  identities and are not an error.
- KTD14. **Prove filesystem class at the handle boundary.** Static capability matrices describe what
  an adapter can attempt, while the retained root/child handle supplies the runtime local-NTFS,
  same-volume, reparse, and identity facts required by strict browsing. The Host never treats a
  matrix enum as proof for a particular project root.
- KTD15. **Publish only after lineage revalidation.** The scan records every traversed directory's
  relative identity and native proof token. Before publication it reopens/rechecks those handles in
  bounded batches and compares the recorded facts. This closes directory replacement and mount drift
  without inventing a watcher or promising a point-in-time byte snapshot.

### High-Level Technical Design

These sketches describe ownership and protocol shape, not exact Rust APIs.

#### Ownership and data flow

```text
DirectoryCapability (exact project root)
  -> move-only nara_fs child-enumeration session
  -> AuthoringSourceIndexAttempt (one active cursor + bounded FIFO)
  -> ProjectSourceIndexSnapshot { root identity, generation, entries, diagnostics }
  -> Arc<EditorProjectBrowserView> + nara_tooling browser state
  -> EguiProjectBrowserPanel (render/input translation only)
  -> EditorActionBatch { exact frame revision, browser action }
  -> Host re-open/revalidate -> existing Scene/Prefab Authoring opener
```

The scan is source-oriented and independent of Runtime content closure. A source entry may be
visible while its native provider, importer, or Runtime codec is unavailable. Only document Open
crosses into the Host's shared Authoring document-admission basis; all other selection remains
read-only.

#### Scan state machine

```text
NotRequested
  -> Scanning { candidate, last_good? }
      -> Ready { snapshot }
      -> ReadyWithDiagnostics { snapshot }
      -> Unavailable { last_good?, failure }

Unavailable --Retry/Refresh--> Scanning
Ready/ReadyWithDiagnostics --Refresh--> Scanning { last_good }
any state --accepted project replacement--> retired with old project
```

Only `Ready` and `ReadyWithDiagnostics` contain newly published snapshots. `Unavailable` is an outer
session state and may carry no last-good snapshot on the first failed attempt.

#### Entry classification and opening gate

```text
native child observation
  -> portable project-relative name?
       no  -> bounded diagnostic; do not omit silently
       yes -> classify kind/status
              -> Scene or Prefab?
                   no  -> InspectOnly or None
                   yes -> exact child-open identity proven?
                            no  -> InspectOnly + reason
                            yes -> Openable { expected identity }
```

#### Open sequence

```text
egui input
  -> nara_tooling browser action enters exact EditorActionBatch frame
  -> Host finds the relative source in that frame's immutable index snapshot
  -> Host reopens selected child from retained root capability
  -> Host compares exact expected FileIdentity
  -> shared Authoring Scene/Prefab admission basis
  -> existing workspace/dirty/replacement outcome
```

### Module map

| Layer | Owner | Planned location | Must not own |
|---|---|---|---|
| Native enumeration | `nara_fs` | `crates/nara_fs/src/capability.rs`, `src/platform/{unix,windows}.rs` | recursive scan policy, asset classification, UI state |
| Source index transaction | Project Host | `src/project_host/authoring/source_index.rs` and `authoring/mod.rs` | OS enumeration algorithms, RuntimePlan, Editor widgets |
| UI-neutral model/actions | Tooling | `crates/nara_tooling/src/project_browser.rs`, `frame.rs`, `lib.rs` | egui, raw paths, file handles |
| egui Adapter | Toolkit | `crates/nara_tooling_egui/src/project_browser.rs`, `editor.rs`, `lib.rs` | Host mutation, document parsing, platform IO |
| Product/platform wiring | Private Editor | `editor/src/app.rs`, `editor/src/platform.rs` only at composition points | scan algorithm, duplicate browser state |
| Evidence | Tests/docs | `crates/nara_fs/tests`, `tests/`, `editor/tests`, ADR/ledger | fixture-only claims presented as product proof |

### Data contracts

The planned private/public-at-tooling shapes are intentionally narrow:

```text
DirectoryEnumerationLimits { max_total_entries, max_name_bytes, max_rejected_facts }
DirectoryEnumerationSession (move-only, independent native cursor)
DirectoryEnumerationBatch { entries, rejected_entry_facts, complete }

ProjectSourceIndexSnapshot
  root_identity
  scan_generation
  effective_roots
  ordered entries: SourceIndexEntry
  diagnostics
  budget_usage
  completion: Ready | ReadyWithDiagnostics

SourceIndexEntry
  relative_path
  kind
  scoped_identity
  availability/status reason
  open_capability: Scene(expected identity) | Prefab(expected identity) | InspectOnly(reason) | None

EditorProjectBrowserView (owned immutable Arc-backed frame value)
  status: NotRequested | Scanning | Ready | ReadyWithDiagnostics | Unavailable
  generation/root_identity
  scan progress and last-good generation
  ordered entries
  selected_path
  path/kind filters and expanded paths
  bounded diagnostics
  pending_work: None | Scan | Query

SourceIndexDiagnosticView
  code / phase
  optional relative source
  fixed safe summary / impact / repair hint
  numeric budget facts only

EditorProjectBrowserAction
  Host: Refresh | Open { relative_path }
  Tooling-local: Select | SetPathFilter | SetKindFilter | ToggleExpanded
```

The exact Rust names may change during implementation, but the ownership and move-only/generation
properties are binding. `SourceIndexEntry` is not a `FileCapability`; the Host reopens the selected
relative path at the action boundary. The surrounding `EditorActionBatch.frame_revision`, not fields
duplicated on each action, binds the request to the root/index facts used for membership and identity
checks.

### Risks and mitigations

| Risk | Mitigation and owning proof |
|---|---|
| A wide directory monopolizes the event loop | Move-only enumeration sessions return bounded batches; U1 proves multi-batch completion and U2 allows only one active cursor. |
| The filesystem changes during or after a scan | The snapshot claims a completed traversal, not byte-level atomicity; Open rechecks frame membership, kind, and exact identity before reading. |
| One hostile entry hides the rest of a project | Safely isolated entries/subtrees become bounded rejected facts; only incomplete directory observation or global integrity/budget failure rejects the attempt. |
| A maximum-size index causes permanent UI jank | Tooling rebuilds cached row projections in bounded batches and egui renders only a hard-limited visible window. |
| Diagnostics leak local paths or native facts | Host publishes only the closed, quota-bound `SourceIndexDiagnosticView`; tests inject hostile raw errors. |
| The browser becomes a second document/asset authority | Existing Authoring open/persistence and Asset Database ownership remain unchanged; the only new publication is the source-index snapshot. |
| Platform claims exceed evidence | U1 closes only Windows x86-64 local-NTFS support; all other combinations retain explicit Unsupported/Unproven status. |
| Static matrix is mistaken for a filesystem proof | U1 requires a runtime local-NTFS/identity gate on every retained handle; ReFS, network, removable, and unproven roots stay unavailable. |
| A directory is replaced after traversal | U2 performs final bounded lineage revalidation before publication and retains last-good on drift. |
| `ControlFlow::Wait` sleeps while scan work is pending | U2 exposes an explicit pending-work signal; app/platform request the next redraw or immediate fair wake until the state machine advances. |
| Manifest roots are missing, overlapping, or ambiguous | U2 normalizes a role-bearing root trie and distinguishes missing/non-directory/unproven/alias facts before traversal. |

### Implementation sequencing

1. U0 establishes the accepted filesystem/indexing contract and updates deferred tests without
   changing Editor behavior.
2. U1 implements native Windows enumeration and proves the local-NTFS matrix while keeping other
   adapters truthful.
3. U2 builds the Host source-index transaction and its private projection seam.
4. U3 adds closure-external Scene/Prefab admission and retargetable persistence lanes.
5. U4 adds the UI-neutral browser state/actions, integrates the immutable view into the Editor frame,
   and composes the egui Project Browser.
6. U5 proves the complete standalone workflow, simplifies the final diff, reviews it, and closes
   architecture/ledger/memory evidence.

## Implementation Units

### EPB-U0. Admit the capability-bound browser boundary

**Goal:** Establish one durable contract for project-wide source browsing before behavior changes.

**Requirements:** KD1-KD3, R1-R4, R20-R23.

**Files:**

- `docs/architecture/adr/0070-capability-oriented-filesystem-substrate.md`
- `docs/architecture/adr/0114-standalone-generic-editor-authoring-and-execution-boundary.md`
- `docs/architecture/adr/implementation-status.md`
- `docs/architecture/README.md`
- `docs/architecture/open-questions.md` only if an existing question is refined rather than copied

**Approach:**

- Record that the next reserved U11 consumer is the Editor/authoring source index.
- State that the browser is read-only, manifest-rooted, incremental, manual-refresh driven, and
  independent from Runtime closure.
- Keep the existing ADR 0114 sentence about project-wide browsing truthful by adding the successor
  plan/activation relation; do not turn this plan into a generic asset-import or watcher roadmap.
- Define the supported proof target as Windows x86-64 local NTFS, with Linux/macOS/BSD and unproven
  filesystem combinations remaining explicit Unsupported/Unproven.

**Test scenarios:**

- Architecture search finds no browser permission to use ambient directory enumeration, async
  framework, watcher, or source mutation.
- ADR 0070's U11 completion rule, ADR 0114's deferred-browser sentence, and the new plan agree on
  one owner and one boundary.

**Verification:**

- `rg -n "read_dir|async|notify|rename|unlink"` over the planned production owner list is reviewed
  as a boundary check, not a blanket repository ban.
- Documentation link and status checks pass.

### EPB-U1. Implement handle-bound directory enumeration

**Goal:** Make `nara_fs` stream bounded direct-child observation batches through one retained,
independent Windows directory-enumeration session.

**Requirements:** R4-R6, R9-R10, R18, R22, R26; KTD1-KTD3, KTD5, KTD11, KTD14.

**Files:**

- `crates/nara_fs/src/capability.rs`
- `crates/nara_fs/src/directory_enumeration.rs`
- `crates/nara_fs/src/error.rs`
- `crates/nara_fs/src/proof.rs`
- `crates/nara_fs/src/platform/windows.rs`
- `crates/nara_fs/src/platform/unix.rs`
- `crates/nara_fs/src/platform/unsupported.rs`
- `crates/nara_fs/src/platform/mod.rs`
- `crates/nara_fs/src/lib.rs`
- `crates/nara_fs/tests/filesystem_contract.rs`

**Approach:**

- Replace the Unsupported placeholder with one move-only child-enumeration session. Its cumulative
  limits default to `4,096` entries, `1,024` bytes per name, and a bounded rejected-fact count; each
  `next_batch` consumes only the caller's remaining per-update allowance and returns completion.
- Windows creates an independent exact-self handle-relative enumeration handle and uses a
  fixed-buffer, resumable `NtQueryDirectoryFileEx`/`FILE_ID_BOTH_DIR_INFORMATION` loop for
  names/kinds. The current adapter opens an empty relative name with `NtOpenFile` against the
  retained directory handle; `ReOpenFile` is not assumed for directory objects. If that exact-self
  reopen cannot be proven, the capability remains Unsupported rather than sharing a cursor. A
  directory record's 64-bit FileId must not be fabricated into Nara's
  128-bit Windows `FileIdentity`; exact identity is attached only after the existing child-open/facts
  validation can prove it.
- Exclude `.`/`..`; represent safely isolated reparse/symlink/special/unportable names as bounded
  rejected facts without following them. A native enumeration error that prevents knowing whether
  the directory is complete, or any cumulative limit overflow, fails the session rather than
  returning a partial-complete listing.
- Update the platform enumeration proof only for native Windows local-NTFS evidence. Unix and
  unsupported adapters keep their existing truthful status. Add an operation-specific runtime proof
  over the retained handle; do not widen the existing general identity semantics that may admit
  local ReFS for other operations.
- Keep the Unix adapter's new enumeration symbol as an explicit typed Unsupported/Unproven stub in
  this plan, so Unix builds remain exhaustive without claiming a platform implementation.

**Test scenarios:**

- Root and child directories return mixed regular/directory entries in a bounded result.
- A directory wider than one batch completes across several Editor-sized batches without a single
  full-directory allocation or event-loop stall.
- Repeated enumeration through shared capabilities returns the same set instead of observing a
  consumed shared directory cursor.
- Replacing the path name after the directory handle is opened does not redirect enumeration.
- Reparse/symlink and cross-volume entries fail closed or produce the documented non-openable fact.
- Non-UTF-8/unportable names become bounded rejected facts; incomplete native enumeration and
  cumulative limit overflow are typed failures.
- Windows x86-64 local-NTFS tests prove the supported matrix; Linux and other Unix keep the existing
  Unsupported/Unproven result.
- A local ReFS, remote/network, removable, or injected unproven handle is rejected for enumeration
  even when the static Windows adapter reports that the native primitive exists.

**Verification:**

- `CARGO_BUILD_JOBS=1 cargo nextest run -p nara_fs --test filesystem_contract --test-threads=1` with the new enumeration filters selected after implementation (do not invent a second test target just for platform code).
- `cargo fmt --all`.
- `rg -n "std::fs::read_dir|read_dir\(" crates/nara_fs/src crates/nara_asset/src src/project_host editor/src` confirms no ambient production traversal.

### EPB-U2. Build the Host-owned bounded source-index transaction

**Goal:** Scan declared roots incrementally, classify source facts, and publish an immutable
last-good snapshot without coupling the Runtime closure or document authority.

**Requirements:** R3, R7-R14, R18-R19, R21-R23, R25, R27; KTD4-KTD7, KTD11, KTD13, KTD15.

**Files:**

- `src/project_host/authoring/source_index.rs` (new)
- `src/project_host/authoring/mod.rs`
- `src/project_host/authoring/project.rs`
- `src/project_host/runtime/editor.rs`
- `src/project_host/runtime/editor/model.rs`
- `src/project_host/runtime/editor/open.rs`
- `src/project_host/runtime/editor/persistence.rs`
- `src/project_host.rs` or `src/project_host/runtime.rs` only for the narrow frame projection seam
- focused tests under `src/project_host/runtime/tests.rs` or a new `tests/editor_project_browser.rs`

**Approach:**

- Construct the scan from the existing Authoring Project root capability and effective manifest
  paths. Normalize roots once into an ordered role-bearing capability-relative set: missing roots
  become visible unavailable facts, non-directory/unproven roots become scoped diagnostics, duplicate or
  ancestor/descendant roots are scanned once, and identity/canonical aliases fail the affected
  candidate rather than being guessed. Never reopen the root through an ambient path.
- Keep v1 index freshness explicit: initial scan and manual Refresh are the only generation
  triggers. Save and Runtime content refresh do not mutate or retire the source index; Open
  revalidates selected facts, and a manual Refresh publishes the successor generation.
- Use a FIFO frontier containing only relative directory paths and expected identities. The attempt
  owns at most one active enumeration session; each `advance` consumes a fixed item/work allowance,
  and each queued directory is reopened/revalidated before its session starts.
- Classify Scene/Prefab and other source kinds by project-relative path only. Document bytes are
  opened by the existing Host path only after an explicit Open action; `.meta` contents are not read.
- Treat duplicate canonical/case-folded paths, unsupported entries/subtrees, and budget exhaustion as
  structured facts. Isolated rejected entries are pruned into `ReadyWithDiagnostics`; root drift,
  incomplete directory observation, canonical ambiguity, or budget failure rejects the candidate.
- Require every admitted root/child handle to pass the runtime local-NTFS and strict identity gate;
  the static platform matrix is only an adapter capability declaration. ReFS, network, removable,
  reparse-backed, or otherwise unproven handles remain unavailable.
- Before publishing, reopen/revalidate every traversed directory lineage fact in bounded batches.
  Replacement, disappearance, mount/volume drift, or an incomplete cursor rejects the candidate;
  in-place writes to the same file object retain the existing non-snapshot semantics.
- Keep the scan receipt separate from `StartupContentClosureReceipt`: it shares root identity and
  manifest lineage but does not change Runtime content digest or Export membership.
- Add a single `ProjectSourceIndexState` to the Authoring session: no attempt, scanning, ready, or
  unavailable-with-optional-last-good. Publish `Arc<ProjectSourceIndexSnapshot>` only as Ready or
  ReadyWithDiagnostics when the complete candidate passes global budget and identity checks.
- On project replacement, drop the old state only after the new Authoring receipt is accepted. On
  manual Refresh, retain the old snapshot while scanning and replace it atomically on success.

**Test scenarios:**

- A mixed fixture yields stable lexical ordering and correct source-kind/availability facts.
- A scan over a large bounded fixture advances in finite batches and never publishes a partial
  snapshot.
- Asset and `.meta` sidecar entries are visible independently without metadata parsing or database
  mutation.
- Root identity/generation drift, directory replacement, unsupported enumeration, and budget
  overflow preserve the last-good snapshot and produce structured diagnostics.
- A first scan containing one safely isolated rejected entry publishes safe siblings as
  ReadyWithDiagnostics; a native error that leaves a directory incomplete publishes nothing.
- Runtime closure digest and Runtime/Export acceptance are unchanged by adding unreferenced files to
  the browser's declared roots.
- A scan with pending work exposes a narrow Host-owned progress signal that remains true until the
  attempt reaches a terminal state. Native `ControlFlow::Wait` scheduling is verified when U4 wires
  this signal into the frame/app/platform composition.

**Verification:**

- Focused Host/browser tests with `cargo nextest` and `--test-threads=1`.
- `rg -n "RuntimePlan|ComponentRegistry|World|std::fs|read_dir|tokio|async_std|notify"` over
  `src/project_host/authoring/source_index.rs` and its callers confirms the boundary.
- `git diff --check` and `cargo fmt --all`.

### EPB-U3. Admit closure-external Authoring documents and retarget persistence lanes

**Goal:** Open any indexed Scene or Prefab through the same generic schema/Adapter authority as
Generic Open, without requiring it to belong to the startup dependency closure.

**Requirements:** R1-R2, R14-R16, R21-R23, R25; KTD9, KTD11-KTD12.

**Files:**

- `src/project_host/authoring/document.rs` (new)
- `src/project_host/authoring/mod.rs`
- `src/project_host/authoring/project.rs`
- `src/project_host/runtime/editor/document_open.rs` (new)
- `src/project_host/runtime/editor.rs`
- `src/project_host/runtime/editor/model.rs`
- `src/project_host/runtime/editor/persistence.rs`
- `src/project_host/persistence.rs` only if the existing Scene/Prefab hosts need one shared binding
  constructor
- focused tests under `src/project_host/runtime/tests.rs` and `tests/editor_persistence.rs`

**Approach:**

- Extract one Host-private immutable `AuthoringDocumentAdmissionBasis` from the accepted Authoring
  session: exact root/manifest/bundle lineage, authoring schema bundle, Adapter receipts, and file
  limits. It is not a second project snapshot and exposes no Runtime registry or plan.
- Admit a selected Scene/Prefab only after the caller supplies an index member fact from the accepted
  frame. Reopen the file through the exact declared root, require matching kind and expected
  `FileIdentity`, perform a fresh bounded read/decode/classification attempt, and publish no document
  on mismatch or budget failure.
- Reuse the existing nominal authoring decode/publication core for startup Scene, dependency Prefab,
  and on-demand document admission. Do not copy codec, migration, unavailable-record, or preservation
  logic into the Editor action handler.
- Refactor the parallel `scene_title`/`source_document`/Scene persistence fields into one explicit
  path-bound Scene lane; keep one analogous optional Prefab lane. Opening the currently bound source
  activates it. Opening another same-kind source first completes the existing
  `CloseDocument` Save/Discard/Cancel protocol, then retargets that lane; cancellation or persistence
  failure keeps the old lane and discards the pending request.
- Save and Reopen continue to use the lane's exact source binding/identity/digest. Closing a lane
  releases its persistence owner; the Authoring admission basis remains reusable for a later Open.
- Keep simultaneous multi-Scene or multi-Prefab persistence outside v1. Scene and Prefab lanes remain
  independent, so retargeting one does not close or rebind the other.

**Test scenarios:**

- Open a Scene that is not the startup Scene and a Prefab that is not in the startup dependency
  closure; each preserves Complete/Degraded authoring classification and exact source binding.
- A forged index request, wrong document kind, missing identity, replaced source, malformed payload,
  or per-document budget overflow publishes no workspace document or persistence lane.
- Retarget a clean Scene lane, then a dirty Scene lane through Save, Discard, and Cancel; only Save or
  Discard admits the successor, while Cancel and failed/uncertain persistence retain the exact old
  document and lane. Repeat one representative path for Prefab rather than a full Cartesian matrix.
- Save, close, and reopen a closure-external document; the current file identity/digest and preserved
  unavailable-record manifest remain authoritative.
- Opening a Prefab does not disturb the Scene lane, and opening a Scene does not disturb the Prefab
  lane.

**Verification:**

- Focused Authoring document-admission and Editor persistence tests with `cargo nextest` and
  `--test-threads=1`.
- Boundary search confirms the on-demand path uses the retained authoring bundle/Adapter receipts and
  imports no RuntimePlan, frozen native registry, World, project crate, or ambient filesystem API.

### EPB-U4. Add UI-neutral browser actions and egui Project Browser

**Goal:** Let the Editor browse, filter, select, refresh, and open supported documents without
duplicating Host state or filesystem access.

**Requirements:** R12, R14-R19, R22, R24; KTD8-KTD9.

**Files:**

- `crates/nara_tooling/src/project_browser.rs` (new)
- `crates/nara_tooling/src/frame.rs`
- `crates/nara_tooling/src/lib.rs`
- `crates/nara_tooling_egui/src/project_browser.rs` (new)
- `crates/nara_tooling_egui/src/editor.rs`
- `crates/nara_tooling_egui/src/lib.rs`
- `crates/nara_tooling_egui/src/tests.rs` or new focused browser tests
- `src/project_host/runtime/editor/actions.rs`
- `editor/src/app.rs` only for frame/action composition
- `editor/src/platform.rs` only to consume the unified scan/query pending-work redraw signal

**Approach:**

- Add an owned, cheap-to-clone `EditorProjectBrowserView` to `EditorProjectView`/`EditorFrameModel`;
  it carries `Arc`-backed immutable source-index facts rather than borrowing from the mutable Host.
  Add ordinary browser variants to the existing frame-bound action algebra and its exhaustive Host
  dispatcher in `runtime/editor/actions.rs`. Do not duplicate frame freshness, root identity, or
  index generation in every action.
- Keep selection, path/kind filters, expanded paths, and the cached row projection in
  `nara_tooling`. Rebuild a changed query in bounded batches keyed by snapshot generation/query
  revision; expose query work through the same pending-work signal used by scans so
  `ControlFlow::Wait` cannot strand a half-built projection. egui retains only widget transients and
  performs no filesystem reads or source sorting.
- Compose the Host scan and tooling query signals once in the frame/app boundary; platform requests
  bounded follow-up redraws only for Scan or Query work. Document Open and dirty-close decisions
  remain in the existing workspace-intent state and never create a redraw spin.
- Implement `EguiProjectBrowserPanel` as a separate module with a bounded tree/list, kind/status
  badges, search field, kind filter, Refresh button, and Open button. Use virtualized visible-row
  rendering with a hard per-frame row limit; do not instantiate controls for the full snapshot.
- Project Host diagnostics through a closed `SourceIndexDiagnosticView`; never forward raw `FsError`
  display/debug output, `PathBuf`, native identities, or arbitrary file contents to egui.
- Scene/Prefab Open actions enter the existing Host action batch or product open path. Non-document
  entries display read-only facts and a clear reason when no operation is available.
- A stale batch uses the existing `StaleFrame` rejection. For accepted frames, Host looks up the
  relative path in that exact frame snapshot, verifies Openable kind/expected identity, then reopens
  it through the current root before any document bytes are read.

**Test scenarios:**

- The same snapshot produces the same flattened rows and filter results regardless of input order.
- Refresh and Open use the exact Host frame revision and contain no raw OS path or duplicate
  stale-generation fields. Selection, filters, and expansion remain tooling-local and are keyed to
  the snapshot/query revision rather than entering the Host action batch.
- An Open submitted from an old frame batch is rejected as `StaleFrame` without changing active
  document, selection, dirty state, or persistence state.
- A forged `.git`/`target`/unindexed relative path and a same-frame leaf replacement are rejected
  before document bytes are read.
- Egui tests separately cover NotRequested, Scanning, Ready, ReadyWithDiagnostics, and Unavailable,
  plus search/kind filtering, keyboard focus, Open Scene/Prefab, and read-only unsupported asset
  facts.
- A maximum-size model proves query rebuild is bounded and visible-row rendering never walks or
  creates widgets for all snapshot entries in one frame.

**Verification:**

- `cargo nextest run -p nara_tooling -p nara_tooling_egui --test-threads=1` with focused filters
  for browser model and Adapter tests.
- `rg -n "std::fs|read_dir|DirectoryCapability|FileCapability" crates/nara_tooling crates/nara_tooling_egui editor/src` confirms no filesystem authority leaks.
- `cargo fmt --all` and `git diff --check`.

### EPB-U5. Prove the standalone browse/open trajectory and close the lane

**Goal:** Demonstrate the complete product flow on a real standalone Editor path and record durable
architecture/evidence without expanding scope.

**Requirements:** R1-R27, AE1-AE7, and KTD10, KTD13-KTD15.

**Files:**

- `editor/tests/application_contract.rs`
- `editor/tests/generic_open_recovery.rs` or new `editor/tests/project_browser.rs`
- `tests/project_content_boot.rs` only for a source-index/runtime-closure isolation assertion
- `tests/module_consumer_boundary.rs` or a focused boundary fixture if required
- `reference-game/tests/editor_ui_refresh.rs` only if the existing real project trajectory is the
  smallest consumer; otherwise use a clean temporary project fixture
- `docs/architecture/adr/implementation-status.md`
- `docs/architecture/README.md`
- `docs/knowledge/engineering/verification/` (new record)
- `docs/knowledge/engineering/registry/` (new completed registration)
- `docs/knowledge/engineering/current-state.md` via the normal memory render workflow

**Approach:**

- Use one real project fixture with declared roots, one closure-external Scene, one closure-external
  Prefab, an image/meta pair, an unknown file, and a safely isolated unsupported entry.
- Drive the native standalone Editor far enough to show the Project Browser, filter an entry, open a
  closure-external Scene, open the closure-external Prefab without disturbing the Scene lane,
  request Refresh, inject a scan failure, and verify last-good retention.
- Keep native evidence bounded to the existing Windows path; use Host/tooling tests for the rest of
  the state algebra. Do not add a screenshot matrix or external Play requirement.
- Run simplification and code review after the trajectory is green. Remove abandoned scan/index
  experiments and any compatibility shim not required by an Accepted contract.
- Close the plan only after the implementation ledger and architecture map state that external Play,
  import/reimport, watchers, source mutation, and extension hosting remain separate successors.

**Test scenarios:**

- Real `nara-editor` browse/filter/open trajectory opens both closure-external Scene and Prefab
  without project-specific compilation and preserves their independent persistence owners.
- Repeated equal scans publish equal ordered facts and distinct monotonic generations.
- Refresh failure retains the prior visible tree and reports a bounded diagnostic.
- Runtime Check/Play/Export behavior is unchanged when unreferenced files are added to browsed roots.
- Raw errors containing an injected absolute path/native identity never appear in the public browser
  diagnostic projection.
- Boundary scans find no ambient traversal or async/watcher dependency in the new browser path.

**Verification:**

- Focused `cargo nextest` suites from EPB-U1 through EPB-U4, serially.
- One native Windows standalone trajectory documented with exact command and observed result.
- Architecture/ledger/memory consistency checks and `git diff --check`.

## Verification Contract

### Required gates

- Never run Cargo commands concurrently in this checkout; use `CARGO_BUILD_JOBS=1` for substantial
  checks.
- Run `cargo fmt --all` once the relevant unit is complete, then inspect the diff before staging.
- Run focused `cargo nextest` targets for `nara_fs`, Host/browser, `nara_tooling`, and
  `nara_tooling_egui` with `--test-threads=1`. Use the exact target names present after implementation;
  do not invent a package-wide matrix when a focused test exists.
- Run the standalone Editor trajectory on Windows only after the focused Rust tests pass. No WSL,
  Linux shell, or cross-platform screenshot gate is required for this plan.
- The documented native command is `cargo run -p nara-editor -- <project-root> --profile desktop`;
  the trajectory must use PowerShell on Windows and must not rely on WSL.
- Run boundary searches for ambient filesystem APIs, Runtime/native imports, async/watcher
  dependencies, raw absolute paths, and duplicated browser authority.
- Run `git diff --check`, inspect `git status --short --branch`, and verify that user-owned
  `.codex-research/` content is untouched and unstaged.

### Evidence requirements

- `nara_fs` evidence must distinguish native adapter support from strict runtime proof and must record
  unsupported/unproven platforms truthfully.
- Host evidence must show no partial snapshot publication, exact frame-to-snapshot membership and
  identity checks, and last-good retention after scan failure.
- UI evidence must show that egui emits typed actions and never calls filesystem APIs.
- Product evidence must show one installed generic Editor, no project Rust compilation, and existing
  Scene/Prefab document ownership after Open.

## Definition of Done

### Global

- All R1-R27 and AE1-AE7 are satisfied. Only the separately listed Outstanding questions remain
  successor work.
- `nara_fs` is the only native enumeration owner; no ambient traversal or fallback exists in the
  browser path.
- The Host has one bounded source-index transaction and one immutable last-good snapshot authority.
- Tooling and egui have one frame-bound browser model/action path; no duplicate parser, asset
  database, document slot, or application state machine exists.
- No async runtime, watcher, importer execution, source mutation, external Play, animation, shader,
  or extension code is introduced by this plan.
- All focused verification gates pass, including the Windows standalone trajectory and boundary
  checks.
- Abandoned experiments, dead compatibility shims, duplicate state, and unused public exports are
  removed before closure.
- ADR/ledger/architecture/memory evidence is updated only after code and tests are stable; predecessor
  evidence remains immutable.

### Per-unit

- EPB-U0: authority and scope are reconciled before production behavior changes.
- EPB-U1: bounded handle-bound enumeration works on the claimed matrix and fails closed elsewhere.
- EPB-U2: scans are bounded/incremental, source facts are deterministic, and snapshot publication is
  atomic with last-good retention.
- EPB-U3: closure-external Scene/Prefab admission reuses one authoring basis and exact persistence
  lanes without Runtime or project-code execution.
- EPB-U4: browser UI is a pure projection with exact frame-bound actions and existing document
  Open ownership.
- EPB-U5: one real standalone trajectory proves the product flow and the durable records state the
  remaining successor boundaries truthfully.

## Appendix

### Research breadcrumbs

- `docs/architecture/adr/0070-capability-oriented-filesystem-substrate.md` reserves U11 for
  handle-bound directory enumeration and prohibits domain copies of platform algorithms.
- `docs/architecture/adr/0114-standalone-generic-editor-authoring-and-execution-boundary.md` says
  project-wide browsing is a separate successor and keeps Authoring independent from Runtime.
- `docs/architecture/adr/0020-project-layout-and-package-format.md` defines conventional manifest
  roots without making the Editor-owned database the source-layout authority.
- `docs/architecture/asset-import-host-interface-design.md` requires project-source queries to use
  an immutable committed index and keeps native enumeration separately gated.
- `docs/knowledge/engineering/standalone-generic-editor-product-model-research.md` identifies the
  project-wide source index as the next product layer after Generic Open/Recovery.
- Current source anchors: `crates/nara_fs/src/capability.rs`,
  `crates/nara_asset/src/database.rs`, `src/project_host/authoring/project.rs`,
  `src/project_host/runtime/editor.rs`, `crates/nara_tooling/src/frame.rs`,
  `crates/nara_tooling_egui/src/editor.rs`, and `editor/src/app.rs`.

### Non-goal rationale: why not async yet?

The browser's first workload is bounded source-name and directory facts, not image decoding or
importer execution. A deterministic per-update cursor gives the user a responsive shell while
keeping all capability handles on the Host's known execution context. Async would add a task owner,
cross-thread handle lifetime, cancellation, stale-result, shutdown, and error-recovery contracts
before the product has measured a need. If a real project fixture later exceeds the item/work target,
the successor should introduce an owner-specific task contract with an evidence-backed generation
and close model rather than a generic `async` abstraction.
