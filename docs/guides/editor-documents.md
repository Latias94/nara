# Source-built Editor documents

One independently compiled dialogue document demonstrates the non-Scene workflow. Its source is
`tests/fixtures/module-consumer/editor-panel-consumer/src/dialogue.rs`; the public Host tests are
`tests/dialogue_documents.rs`, and `tests/dialogue_panel.rs` covers actual ImGui panel retirement.
The external consumer tests cover the public workflow; execution evidence is recorded separately
in engineering memory.

## Domain And Host Responsibilities

Implement `nara_tooling::EditorExtensionDocument` for the domain owner:

- `Command` is a domain-specific Rust type. Validate before mutation; failed apply/Undo/Redo must
  leave content, history and content-state identity unchanged.
- `View` is an immutable `Debug + Send + Sync` projection, not a mutable owner reference.
- `content_state()` identifies committed content. Use `nara::core::DocumentRevision::initial()`
  for a fresh content identity clock, advance it for every forward edit, and retain earlier
  `DocumentContentState` values in history. Undo/Redo restores those states. Do not reuse the
  generation of an undone state for a new branch.
- `history_status()` reports domain-owned Undo/Redo availability. Bound retained inverse data.
- `encode` and `decode` are pure, bounded codecs. Encode is captured on the owner thread; decode
  for Reopen runs on an I/O worker. They must not do filesystem I/O or own thread-affine native
  resources. Decode constructs fresh content identity and empty history.
- Return checked `EditorExtensionDocumentError` codes and safe summaries. Do not put raw document
  content, absolute paths, or arbitrary parser errors into diagnostics.

The Workspace separately assigns its public `DocumentRevision`: this is the accepted-transition
clock used by actions. It remains monotonic through Undo/Redo. The content clock belongs to the
domain and is not substituted for the Workspace action revision.

## Register And Open

A trusted executable or embedded shell registers an owner on an open `EditorProjectSession` or
`nara_editor::EditorApplication` and opens the source explicitly:

```rust,ignore
application.register_extension_document::<Dialogue>()?;
let document = application.open_extension_document::<Dialogue>("dialogue/intro.json")?;
```

The Host retains the project-root capability, opens a bounded (16 MiB maximum) existing source,
decodes it and binds its actual file identity/digest before exposing the tab. Invalid portable
paths, duplicate owners/sources, pending replacement or close, and absent registrations are
rejected. There is no Save As, arbitrary external filesystem writer, file-extension discovery,
runtime native asset ownership, or project-provided executable callback. Initial explicit Open is
synchronous in this first slice: it is a trusted Host command, reads one existing source through
the capability, and caps input at 16 MiB before invoking the domain codec. This keeps initial
activation and Host binding atomic without introducing a second async request protocol. It is not
a frame-time guarantee: a codec can still consume caller time within its byte budget. Reopen and
Save use the existing async custody and exact-result paths. If real consumers need responsive
first Open for larger/expensive codecs, measure it and extend the existing document-open owner
rather than adding an extension-specific worker protocol.

Extension locators are relative to the project root. Scene and Prefab workspace locators retain
their Host-owned scopes, so a root-level extension source does not alias an equal Scene-relative
name. The Host rejects reserved paths and compares actual file identities before either Open
direction, including case aliases on case-insensitive filesystems and hard links. It resolves
existing targets through their retained publication capabilities, so parent-directory renames and
atomic Save replacements do not substitute a stale path or file identity. Distinct files on a
case-sensitive filesystem remain independent sources.

Registration is session-local. A new/replacement project needs explicit trusted registration and
opening again. Native composition does not infer document owners from panel descriptors. This
keeps the first consumer complete without introducing speculative installation/lifetime hooks.

## Edit And Observe

Resolve `frame.workspace().extension_document(document)` or
`context.model().extension_document()`, then call `get::<Dialogue>()` for the typed read-only view.
The accessor checks the Rust owner type, not just its public string identity. Submit typed edits
through the ordinary ordered action path:

```rust,ignore
let action = EditorAction::Extension(EditorExtensionCommand::new::<Dialogue>(
    document,
    projection.revision(),
    SetLine { index: 0, text: draft.clone() },
));
let submission = context.submit(action)?;
```

Queueing is not owner acceptance. Observe the ordinary exact panel dispatch callback and retain a
stale draft until explicit review/reapply. The existing `EditorDocumentMutation::Undo`/`Redo`
actions, navigation, tab history and dirty projection apply to extension documents. Rejected or
wrong-owner/stale actions do not change the document. No `Any` payload can invoke another service.

## Save, Reopen And Close

Use existing `EditorPersistenceAction::Save` and `Reopen` with frame/document revision facts.
An accepted request returns an `EditorPersistenceAttemptId`; its Pending and Terminal projections
carry that same identity. A published immediate rejection also returns its attempt. A `Refused`
outcome publishes nothing and has no new attempt; a Busy reason alone does not distinguish these
cases. Drive while Pending, then inspect the observed Terminal:

```rust,ignore
if let EditorPersistenceView::Terminal { attempt, result } = frame.persistence() {
    show_persistence_result(result);
    // Capture this observation's identity, including when dispatch is deferred.
    let dismiss = EditorAction::Persistence(
        EditorPersistenceAction::AcknowledgeResult { attempt },
    );
}
```

Acknowledgment returns `Acknowledged { attempt }` only for that still-current terminal. An old,
foreign or duplicate identity cannot clear another result. A redraw does not invalidate an
observation of the same terminal. Do not sample the latest attempt later to substitute for the
one actually displayed. Explicit retries have new identities; acknowledging uncertainty clears
its observation, not the file owner's reconcile requirement or physical cleanup responsibility.

Save captures exact content/identity before background publication. Only the existing verified
atomic receipt advances the saved checkpoint. Edits accepted after capture stay dirty; Undo back
to the saved content becomes clean. Encoding, publication, conflict and uncertain results never
forge a saved checkpoint. The platform/capability guarantees remain those of ADRs 0047/0070/0118.

Reopen reads with a read-only capability, decodes off-thread and publishes only if the original
document/source/revision still matches. A subsequent edit rejects that completion. A successful
Reopen resets history and issues a new incarnation, so old commands cannot alias it. A failed
read/decode leaves current content/history intact. Explicit Reopen can replace unsaved content;
it is not an unsolicited reload or an automatic rollback.

Request document close with `EditorWorkspaceIntent::CloseDocument`. The Host issues the ordinary
Save/Discard/Cancel prompt. Cancel retains content/history. Discard leaves disk unchanged. Save
waits for publication before retiring the document; a failed Save returns to the prompt. A panel
must not remove a tab itself. Whole-session close retires extension publication Hosts through the existing Host-custody shutdown participant before returning its Complete receipt. A failed document Close/Save retains the open document and its file lane for another decision. Session close retains file/task custody independently of panel
objects, pending operation results, and logical task cancellation.

For persistent panel targets use `EditorPanelTarget::Source` with
`EditorPanelSourceKind::Extension` and the project-relative locator. Hiding or closing the panel
retires its draft only; reopening builds fresh panel state over the same document/history.

## Focused Verification

From the repository root, serialize with other builds and reuse the root target:

```powershell
$env:CARGO_BUILD_JOBS = '1'
cargo nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml --target-dir target -p nara-editor-panel-module-consumer --test dialogue_documents --test dialogue_panel
```

Run these alongside the relevant Scene/Prefab persistence regressions. Offscreen ImGui coverage
does not claim native pixels, OS input, device recovery, Linux/macOS, or timing evidence.

```powershell
cargo nextest run -p nara_tooling --lib workspace::
cargo nextest run -p nara --features 'tooling,runtime-2d,serde' --test editor_persistence --test scene_file_workspace
```

For changes affecting native document widgets, run the focused hidden-window workflow on the
Windows/DX12 profile in the [native qualification procedure](../verification/native-editor-qualification.md):

```powershell
$env:WGPU_BACKEND = 'dx12'
cargo nextest run -p nara-editor --lib --locked --run-ignored only -E 'test(native_extension_document_widgets_save_reopen_and_retire_panels)' --test-threads=1
```
