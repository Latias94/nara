# Finite Source Tools

A trusted Host can explicitly enable one finite source tool in a Project Session. The tool runs
as a direct child process, receives copied component values and returns property proposals.
The Project Session owns activation, requests, review and acceptance. Scene/Prefab preparation,
generated entity IDs, inverse patches, history and persistence retain their existing owners.
No drawing toolkit or gameplay VM is involved.

Ordinary project Open never executes a tool. Code-free release capture, receipt-last artifact
publication and explicitly approved captured-image execution have separate concrete APIs below.
The project-local installed graph and desired-selection workflow follow the
[local lifecycle plan](../plans/2026-10-28-local-source-tool-release-plan.md) and
[installed-graph successor](../plans/2026-10-28-installed-source-tool-graph-plan.md).
Safe file reclamation and a public language SDK remain separate work.
Raw development command paths are trusted local configuration, not immutable installed release
identity.

The additive metadata operation below has bounded
[whole-Session and Windows consumer qualification](../knowledge/engineering/verification/2026-10/2026-10-28T080917Z-whole-session-installed-schema-owner-adoption-and-explicit-publication-recovery-5a02037cad1b4af49cbec2b24306524a.md)
under the [owner-adoption plan](../plans/2026-10-28-authoring-owner-adoption-plan.md). This does
not implement existing-owner migration or complete Package Manager controls.

## Prepare A Local Release

`PreparedEditorSourceToolRelease::capture(&source, expected_manifest_digest)` reads the fixed
`nara-tool.json` through a Host-issued strict source capability. Supply the manifest's raw BLAKE3
digest and length from the source observation you selected; the manifest does not approve itself.
Capture verifies each declared member and retains owned bytes plus the ordinary admitted Schema
owner snapshot. Editing the original directory afterward does not modify the candidate.

Format 1 requires explicit `package`, opaque `release` label, `host_target`,
`authority: "trusted_process"`, `protocol_version: 1`, `owner`, `component`, `catalog`, `entry`,
`requires` and `files`. An exact requirement has `package` and `manifest: {length, blake3}`; each
member has `path`, `length`, lowercase 64-digit BLAKE3 `blake3`, and `kind` (`tool` or `content`).
`catalog` and `entry` exactly name distinct nonempty tool members. The catalog uses Reflect's
existing JSON catalog envelope. This first capture form requires catalog generation one and has
no predecessor-lineage input; an already evolved catalog is not yet admitted even if only tool
code changes. Package identity and Schema owner identity are separate fields.
The first Host targets are `x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu`; capture rejects
a different current Host. The executable runtime/launcher remains an external Host prerequisite.

Paths are portable ASCII with at most eight components and 512 bytes. File and directory spelling
aliases, traversal, device names, manifest self-membership, and file/directory collisions reject.
Ceilings are 64 KiB manifest, 64 members, 16 MiB per member and 64 MiB aggregate payload; existing
Reflect catalog limits apply as well. Unlisted siblings are not read or included. Only the captured
closure may be staged by a future installer; ambient siblings cannot become implicit dependencies.

Preparation starts no child, writes no project file and changes no active tool or document Schema.
Content classification grants no project destination, overwrite or deletion authority. Exact
requirement declarations do not resolve or prove installed dependencies. This candidate is the
input to the subsequent [local lifecycle plan](../plans/2026-10-28-local-source-tool-release-plan.md),
not an installation receipt, executable approval or immutable launch proof. Installation and
execution need the later publication and runtime-custody boundaries before they can use it.

## Publish And Inspect A Local Artifact

Create `EditorSourceToolInstallationPlan::new(parent, destination, captured)` with a Host-issued
read-write parent capability and one validated destination component. `release()` lends the same
immutable candidate for preview; `cancel()` returns it without writing. `publish()` explicitly
confirms this artifact publication. It reserves an absent empty candidate directory, writes and
checks declared members, then commits `.nara-installation.json` with no-replace file publication
last. The root can therefore be visible before it is ready. No existing destination or unowned
receipt is overwritten or adopted, even when its bytes match.

Handle the complete `EditorSourceToolInstallationOutcome`:

- `Published` retains the admitted artifact and live created-member provenance. Its `verify()`
  rechecks the selected destination, identities, digests and complete directory inventory.
- `Incomplete` retains a published but unready directory, original plan and acquired custody.
  Keep these recovery facts; the attempt has not committed its ready receipt.
- `PublishedUnverified` retains both completed publication receipts plus the actual admission
  refusal. It cannot be presented as an unpublished failure or silently selected for execution.
- `Failed` retains its original plan and actual pre-ready filesystem error/orphan facts.

`publication()` reports the empty-root naming operation; `receipt_publication()` reports the
ready-receipt commit. Member `sync_receipt()` values record actual file synchronization. These
facts neither combine into whole-tree atomicity nor prove crash durability. In particular,
Windows parent-directory synchronization is unsupported. Later writes remain possible through
ordinary read sharing and must be revalidated by an execution or content consumer.

After closing an artifact owner, reopen its directory through a strict capability and call
`EditorSourceToolInstallationInspection::read(&directory, expected_receipt_digest)`. Inspection
verifies the selected receipt and recaptures the full declared release; unknown, missing or changed
files reject. This returns data observations, not recovered creation provenance or deletion rights.
Unready custody has no automatic/public deletion operation until stable-content reclamation is
qualified; dropping it closes handles and preserves files for reconciliation.

This is an artifact preparation/publication API, not the complete Package Manager. It does not
select an installed graph, satisfy declared requirements, adopt project Schema, approve a launcher,
enable a tool or complete update/removal. Those operations remain in the local lifecycle plan.

## Retain Installed Content And Adopt Samples

Keep a successful `EditorSourceToolInstallation` in an `Arc` when transferring its live
created-member custody into a Project Session or Application. Observe `source_tool_content_view()`
and wait for its initial read and actual pending work to return. Then call
`prepare_source_tool_content_management(&basis, project_relative_directory, Arc::clone(&installation))`.
The returned candidate describes the finite member baselines and protection facts. Preparation
and dropping the candidate write nothing. `commit_source_tool_content(candidate)` confirms that
original successor and admits its known facts before IO; failed publication retains them.
Observe its eventual operation outcome separately from the returned attempt. A changed Session
basis or different actual project directory refuses the candidate.

`prepare_source_tool_content_adoption(&basis, &installation_id, &["samples/readme.txt"])` prepares
explicit project ownership of declared Content members. Confirm through the same commit API.
Adoption changes no source, history, graph or executable approval. It remains set even while
bytes equal the release baseline or its publication fails. Unknown members and tool/metadata
members cannot be adopted as samples. An imported project copy is outside this member set;
management never follows its origin to delete that copy.

`prepare_source_tool_content_observation(&basis, &installation_id)` observes declared member
presence and bounded default-stream digests without requiring the whole installation to remain
a valid executable release. Confirming that candidate retains observed modification. Restoring
baseline bytes or registering the same installation again cannot clear admitted modification
or adoption, including after a failed publication. Unknown siblings remain untouched. This cannot
discover an external change that was reverted entirely between observations.

For document edits, the Session captures the exact bound project-relative source and owner content
identity before five actual acceptance seams: Workspace commands (including Undo/Redo), prepared
edit commit, prepared construction commit, final Prefab-conversion publication and extension
actions. Accepted content changes immediately mark matching managed members `modified`, even
before Save. No-op, refusal, query, navigation, preview and reclassification do not mark.
Modification remains sticky through Undo, baseline restoration, Save file replacement, Reopen and
Discard. Existing persistence lanes privately retain accepted-touch facts through initial
asynchronous content reads and later management; there is no additional document store/history.
Path matching conservatively protects ASCII-case variants. It grants no exact-object deletion
proof and never follows an imported copy's origin to claim ownership.
If the management record is unavailable, an accepted touch cannot yet be classified. That
document's Save/Reopen/Close returns `RequiresReconcile`, and whole Close retains the Session.
Restore a valid record and explicitly reconcile before retrying; Undo to clean does not remove
the obligation. Opening an unchanged document does not create it.

The separate bounded `.nara/source-tools/content.json` preserves up to 32 installations,
66 members each and 2 MiB of data, including installations already withdrawn from the graph.
Capacity refusal never evicts protection records. `protection_publication_pending` reports known
facts still awaiting accepted disk publication; `physical_work_pending` separately includes actual
task return and publication cleanup. Additional protection invalidates an older content basis. Already
pending bytes stay exact; later facts form an automatic coalesced successor. Success settles only
facts covered by the accepted publication, so an older receipt cannot clear later debt. Automatic
metadata work updates protection/phase/error observations without overwriting the explicit
`content.operation` attempt or outcome.

Publication drift or uncertainty retains known facts. `reconcile_source_tool_content(&basis)`
explicitly reads the current record without writing; it cannot discard known entries, replace
immutable baselines or clear protection. Each read remembers the protection basis from its start.
Disk weaker than that basis yields `NeedsProtectionPublication`, with
`protection_publication_pending` and `RequiresReconcile`, requiring explicit reconcile/republication.
An older in-flight read missing only newer document facts can instead queue their automatic
successor when its start-basis protection is intact.

After the reconciliation read succeeds, including a `NeedsProtectionPublication` result, call
`prepare_source_tool_content_protection(&basis)` on Session or Application with the returned
basis, then `commit_source_tool_content(candidate)`. The repair API prepares the exact
already-known protection without IO or additional ownership;
accepted explicit repair publication reports `Protected`. Preparing or dropping the candidate
writes nothing. A failed or uncertain publication retains the facts and requires another fresh
read before repair. Read-only reconciliation itself never republishes them.

Save, Reopen and new document Close guard the owning source before writes, replacement or release.
Wait on `Busy`; use explicit reconciliation and repair for `RequiresReconcile`. Discard does not
bypass protection. Unknown pending classification may first admit the original owned Save/Reopen
request; persistence polling itself advances classification/protection before starting its IO.
Whole physical Close advances protection before irreversible closing or
Host/task admission shutdown; failure leaves the retained Session repairable. During Close or
project replacement waiting, Application still exposes content inspection, original-basis repair
preparation/commit and reconciliation on that retained Session, including while a replacement
opens. Other authoring remains gated. Already-entered Save IO keeps its actual publication/receipt
result; source and protection writes are not a cross-file transaction.

Accepted source changes protect memory only. Only an accepted file-publication receipt supplies
disk evidence for its covered facts; a crash between acceptance and publication may lose them.
Windows parent-directory synchronization remains unsupported, with no power-failure guarantee.

Fresh Open reads the ledger without recovering historical creation custody, approving code or
deleting any file. An unavailable/corrupt record preserves installed files. A pending recovery
fact is not permission to resume deletion. Whole-file reclamation remains unqualified: actual
Windows probes show that restrictive default-stream custody still allows named alternate-stream
creation/modification. Default-stream digest equality therefore cannot authorize destroying the
complete file. See the [protected-content plan](../plans/2026-10-28-protected-source-tool-content-plan.md)
for the source-edit successor's pending qualification and the separate all-stream safe reclaimer.
Generic exact-object unlink already exists; it supplies no unchanged-content policy. This
source-edit contract is being implemented, with no new executed qualification claimed here.
It does not add UI controls or complete reclamation or the full foundation Goal.

## Review And Adopt A Missing Schema Owner

An installed catalog does not change the metadata already admitted by an open Session. Use
`prepare_installed_schema_owner(project_relative_directory, expected_receipt_digest)` on the
Project Session or Application to begin code-free preparation. This first operation accepts one
previously absent generation-one owner, preserving every existing owner's complete declarations
and lineage. It neither approves nor enables the executable tool. An existing equal owner returns
`AlreadyAdmitted` only when the live project and actual disk bundle agree.

Drive ordinary frames and observe `schema_owner_adoption_view()`. Preparation first fences new
conflicting work and observes existing physical capture/index return. It then prepares the saved
startup and every current open Scene against all prepared working Prefabs, preserving their exact
source/history and saved checkpoints. Read-only queries and navigation remain available.
The returned Review exposes per-root diagnostics and typed composition failure through
`schema_owner_adoption_diagnostics(document)` and
`schema_owner_adoption_composition_failure(document)`; `None` names the actual saved startup.
A Degraded capture does not authorize Play or execution.

Confirm the exact operation with `adopt_schema_owner(operation)`. This starts finite validation
and publication; an `Ok` return means the request was admitted. Observe the eventual outcome
separately. `Adopted` means the complete Workspace classification and matching project were
installed together. Source revisions, source bytes, Undo/Redo, selection and saved states retain
their owners. Old Schema-bound queries and prepared edits reject. Cancel before publication
leaves the incumbent; cancel or Close after publication cannot erase the actual disk result.

`PublishedWithoutAdoption` and `Uncertain` require explicit reconciliation. The actual available
file evidence remains readable through `schema_owner_adoption_publication()`. Ordinary source
repair, Save and Close remain available. Call `reconcile_schema_owner_adoption(operation)` to
begin a new operation over the current dirty set, then review and confirm it again. Recovery
requires the original manifest and the exact permitted complete owner extension. An already
published successor attaches its observed leaf without another write or a synthetic publication
receipt. External declaration/owner changes refuse instead of merging behind an old review.
No automatic rollback or source/history discard follows. Terminal acknowledgement is available
only after the operation's task return and temporary-file cleanup have completed.

`schema_owner_adoption_failure()` borrows a terminal failure separately from the Review and
publication evidence. It identifies the original operation and phase, and retains the actual
Workspace source/history report or source diagnostics, document and safe project-relative source
when available. Task admission failures retain the original typed rejection. Cancellation alone
does not create a failure; a cancelled operation can still retain an actual failure returned by
its worker. The Application exposes the same observation while it still holds the project,
including pending physical Close. Acknowledging the terminal operation, starting a new one or
beginning explicit reconciliation clears this failure. Diagnostic limits and truncation remain
those of the owning reports; arbitrary native error strings are not presented.

## Approve Captured Managed Images

The concrete managed adapter uses the Host-owned
[Nara.SourceToolHost](../../tools/source-tool-host/Nara.SourceToolHost.csproj) bootstrap. Build it
with the .NET 10 SDK into the repository's existing target tree:

```powershell
dotnet build tools/source-tool-host/Nara.SourceToolHost.csproj --configuration Debug --artifacts-path target/source-tool-host --disable-build-servers -m:1 -p:UseSharedCompilation=false
```

The trusted Host explicitly selects a strict runtime root containing `dotnet.exe`, a strict
bootstrap distribution directory, the four expected launcher/bootstrap digests, an exact
`10.0.patch` framework version and a finite timeout. `EditorSourceToolManagedHost::new` prepares
that approval without executing package code. The machine runtime, servicing and OS dependencies
are explicitly trusted platform prerequisites; these observations do not freeze their transitive
closure. Bootstrap members and the launcher retain restrictive native leases. Startup clears
ambient environment and supplies only the selected Windows session/temp facts and fixed settings.

After code-free installation inspection, explicit Host approval creates the ordinary Session
definition:

```rust,ignore
let host = EditorSourceToolManagedHost::new(
    runtime_root,
    bootstrap_root,
    approved_digests,
    &approved_framework_version,
    timeout,
)?;
let inspected = EditorSourceToolInstallationInspection::read(
    &installation_directory,
    selected_receipt_digest,
)?;
let definition = host.approve_installation(inspected)?;
let attempt = session.enable_source_tool(definition)?;
```

The Session still probes the complete catalog and finite facet before replacing active routing.
Every describe/proposal streams the same captured assembly images followed by the existing JSON
request through one stdin writer. The actual child does not probe package files from the mutable
installation directory. Pending Tasks retain the approved bytes and native custody through late
spawn, cancellation, process reaping and all pipe-worker returns. Windows starts the direct child
suspended, verifies its image and then resumes; unsupported Hosts reject this route explicitly.
There is no development-command fallback. Existing source-built trusted development commands keep
their own explicitly configured path below and share the same Session/proposal owner.

The first managed subset is synchronous static `int Main(string[])` with no arguments, including
a private entry point. Declare the matching `net10.0` one-framework runtimeconfig, project-library
deps graph, catalog and managed Tool members in the release. The bootstrap admits all package
metadata/references before loading images, reserves actual framework identities and rejects
unknown dependencies without default-context fallback. Async/STA entry semantics, original argv/
assembly location, package native/mixed/ReadyToRun images, file-backed resources, unsupported
probing/configuration and unresolved installed requirements reject. Content members are not
implicitly imported or made available as a package filesystem. Fully trusted tool code retains
ambient process authority; this adapter is not a sandbox.

Approval and successful Session activation are independent of durable desired selection. Fresh
Open does not start a tool because its receipt exists. Schema metadata must already be admitted
in the project; missing-owner adoption uses the operation above. Existing-owner evolution and
safe content reclamation remain in the lifecycle plan.

## Select A Project-Local Installed Tool

The installed set and optional single execution slot share `.nara/source-tools/selection.json`.
This format-1 record is bounded to 64 KiB, 32 members, 128 aggregate required edges and 16 outgoing
requirements per member. Each member has package identity, a project-relative installation
directory, a nonzero installation ID, exact receipt/manifest digests and explicit package-to-
installation bindings. The desired slot names one member and enabled/disabled intent.
It contains no launcher, native file identity, active invocation ID or approval
flag. The installation receipt remains a separate artifact fact. No duplicate setting belongs
in `nara.toml`.

Supply a Host-issued strict capability for the actual project root, the selected installation
directory and its observed ready-receipt digest. Independently imported project capabilities may
have different capability-session identities; the filesystem owner compares their continuously
live native directory objects without combining authority or reopening a pathname.

```rust,ignore
let prepared = session.prepare_installed_source_tool_selection(
    approved_project_root,
    "tools/enemy-stats",
    selected_receipt_digest,
    &host,
)?;
let observed = session.source_tool_selection_view();
let attempt = session.enable_installed_source_tool(&observed.basis, prepared)?;
```

Preparation performs bounded code-free installation inspection and explicit managed Host approval;
call it during Host preparation, never while painting. Enablement consumes the original selection
basis. Drive the ordinary Session and correlate the durable outcome with the returned attempt.
Enable admission first observes completed work, then validates that original basis. A returned replacement
may therefore reject an older enable request as stale; callers must not silently refresh and replay it.
The selection view keeps Observe/Enable/Reconcile in `operation` and Disable in `disable`.
Each observation contains its original attempt and an optional outcome: absent means pending.
The two terminal observations survive until another operation is explicitly accepted in their
own lane, including when both finish during the same owner drive. A pending Disable refuses a
second Disable instead of accepting an operation whose result could be lost.
A matching exited probe first produces a ready candidate. Only then does background IO publish
desired selection. Route promotion rechecks the Session, original selection basis, predecessor
route and admitted Schema. Invalid probes preserve the existing route and selection bytes.

`desired` and `active_origin` in the selection view are separate observations. A published update
whose route can no longer be promoted reports `PublishedWithoutActivation`; it does not roll back
the saved record or pretend the candidate became active. `ProbeFailed` refers to the tool attempt
diagnostic, while `NotApproved` means matching Host approval was absent. A cancelled probe reports
`ProbeCancelled`, including cancellation by Disable or Close. Cancelling a ready publication
only revokes route promotion and retains the actual eventual IO outcome. Published selection does
not strengthen the shared file publisher's detect-only
replacement or prove crash durability.

`disable_installed_source_tool(&basis)` revokes execution and retained review immediately, then
persists Disabled separately. A failed write leaves execution stopped. Disabling retains the
installed identity and deletes no artifact. Session Close preserves desired state; late IO cannot
restore activation. Physical process, pipe, task and file-cleanup retirement remain observable
through their existing owners.

Fresh Open reads the record without executing a describe probe. An enabled record awaits fresh
explicit Host approval. Missing means unselected; corrupt, inaccessible, oversized and unsupported
records are failures that block blind overwrite. `reconcile_source_tool_selection(&basis)` admits
fresh disk data without approving or executing code. Project-external installations remain an
explicit session-only workflow. Complete dependency-safe removal and Package UI are separate
work; the installed graph grants no file deletion authority.

## Register Requirements And Review Graph Withdrawal

`prepare_source_tool_installations(&basis, project, requests)` prepares an additive installed-set
successor without executing code. Each `EditorSourceToolInstallationRequest` supplies a directory,
receipt digest and explicit `EditorSourceToolRequirementBinding` values naming exact installation
IDs. Binding a package to the first matching release is never implicit. Multiple identical release
installations remain distinct. Repeating an exact member is allowed; rebinding an existing member,
missing or extra declarations, mismatched release digests, cycles and overlapping directories
reject. Installation roots cannot contain the fixed selection parent; a dedicated descendant of
`.nara/source-tools` is allowed. `register_source_tool_installations(prepared)` confirms that
original basis, then rechecks actual artifacts in background before publishing the complete graph.
Observe its attempt until Registered or Failed; an admitted request alone is not publication.

Preparation and revalidation charge receipt, manifest and payload bytes against one shared
128 MiB work allowance before reading/allocating. This is a logical byte-work bound, not an RSS
or filesystem deadline. The ordinary requirement-free enable can add its selected member in the
same publication. Required tools must first have their complete graph admitted. Session preparation
then approves the selected captured images through the same managed Host; raw standalone approval
continues to reject unresolved requirements. Dependencies are installed relationships, not automatic
execution or cross-installation assembly search paths. Every release provides its own complete
managed image closure. Required installations are checked again in the worker before invocation.

Disable keeps all installed members and their relationships. To withdraw a member, call
`prepare_source_tool_withdrawal(&basis, &installation_id, mode)`. RejectDependents refuses if any
other member requires it, including a disabled or nonselected member. Cascade exposes the complete
affected set through `installations()`. Dropping this review changes nothing. Confirm it with
`withdraw_source_tool_installations(prepared)`; any intervening graph or route change makes it
stale. Affected routing and retained proposals stop before the graph publication. A failed durable
update cannot restart them. Withdrawn reports the graph result; a child or pipe can still be
physically Pending. Ordinary Close retains that obligation until actual return.

Executable installation provenance remains independent of `active_origin`: a session-only
activation that actually uses a registered installation is also subject to its withdrawal.
Read-only review compares the held approved directory with the named project object without
re-reading payloads. An implicated unreadable installation conservatively loses its route; an
unrelated installation or a proven different external object does not. Explicit reconciliation
does its association/read work in the existing background owner, retains positively proven
relations, and stops the original linked route on missing or unavailable graph data. New tool
execution is fenced during reconciliation; Disable, source editing and Close remain available.

Withdrawal removes no files and revokes no admitted Schema or source/history. Broken installations
can be withdrawn without passing complete artifact inspection. Modified/adopted content protection
and exact-object file reclamation remain the next distinct lifecycle operation.

## Configure A Trusted Development Command And Probe

Build an admitted `ComponentSchemaOwnerSnapshot` using the existing catalog API. Supply that
snapshot, the component ID and an `EditorSourceToolCommand` to `EditorSourceToolDefinition::new`.
The command requires absolute executable and working-directory paths, at most 16 literal
arguments and a positive timeout no greater than 60 seconds. It launches directly without a shell.

Call `EditorProjectSession::enable_source_tool`. This starts a finite `describe` invocation and
returns an attempt identity. Drive the Session and observe `source_tool_view` until probing ends.
The child's complete catalog must equal the selected owner snapshot's current catalog. An invalid
candidate leaves the previous activation usable. Successful replacement establishes a fresh
activation and invalidates predecessor requests and review even if the command path is identical.
Retain `source_tool_view().active`'s exact activation ID for every later request or disable.
The view retains `attempt_error` with its attempt identity and `request_error` with its newest
request. Probing a replacement and invoking the active predecessor cannot overwrite each other's
failures. Starting another operation clears only that operation's result slot.

Source metadata admission and execution enablement are independent. Disabling the tool does not
remove component definitions or stored data.

## Request, Review And Accept

1. Capture `document_query_basis(document)` for the intended open Scene or Prefab.
2. Call `request_source_tool(&activation, &basis, intent, parameters)` with that unchanged basis,
   the retained activation, a Host-selected `Attach`, `Edit` or `Spawn` intent and copied parameters.
   The source must select the exact component owner and declaration. A replaced activation rejects
   before it can invalidate its successor's request or review.
3. Drive the Session until the request returns. `source_tool_review` borrows the retained title,
   prepared patch, inverse and fixed created-entity ID. Reviewing does not edit source or history.
4. Accept that request with `accept_source_tool_review`. Acceptance rechecks the original source
   observation, current activation and newest request, then consumes the same prepared candidate.
   The returned commit contains the actual source-owner report and created ID, when applicable.
5. Use ordinary Undo/Redo, Save and Reopen. One accepted proposal is one source transaction.

The child cannot choose a document, entity target, construction operation or source observation.
It returns only a title and stable-property writes for the selected component. Spawn's name and
root placement are retained Host intent. A newer request invalidates an older proposal; callers
must not refresh an old proposal's basis to make it current. Failed preparation publishes no
partial writes and returns no successful entity mapping.

`EditorApplication` delegates the same operations. Its acceptance result includes both the exact
commit result and subsequent application update. Enable, request and disable also publish a
post-operation frame so native maintenance can observe pending work immediately. A frontend can use these operations without
owning another document store, process manager or Undo stack.

## Panel And Integer Property Facet

The built-in **Source Tool** panel (`nara.source-tool`, initially closed) observes the same Session
as the ordinary Inspector. Open it through the workspace panel list after the Host explicitly
enables a tool. Select an entity to Attach or Edit its component, or enter a name to Spawn a root.
Preparing a proposal leaves source, history and disk unchanged. The review shows the actual
document, operation and proposed values; **Accept proposal** publishes one ordinary transaction.
The panel can accept a proposal prepared from the Inspector, and the Inspector can observe a
matching proposal prepared from the panel.

A description reply may include one optional finite declaration beside `catalog`:

```json
"integer_property": {
  "field": "health",
  "parameter": "bonus",
  "label": "Health bonus",
  "initial": 0
}
```

The field must be an editable i64 root of the selected component. With the admitted facet active,
the Inspector shows its current value and an integer proposal parameter in place of that root's
generic control. Preparing sends the parameter under the declared key; it does not directly assign
the displayed property. Other fields retain ordinary editors. A missing or disabled facet restores
the default field editor while the component's source metadata remains admitted.

The pure observations, IDs, actions and receipts live in `nara::tooling`. `EditorFrameModel::source_tool`
publishes the full immutable view; `EditorSourceToolReview` borrows the Session's single prepared
candidate. Source-built panels submit `EditorSourceToolAction` with
`ImguiEditorPanelContext::submit_source_tool` and receive the matching
`EditorActionOutcome::SourceTool` through the existing submission ledger. Its Request action
retains the activation and original query basis; IntegerProperty retains the exact property target.
Accept retains the request ID. No panel stores or executes a second prepared patch or inverse.

Parameter input belongs to its actual control instance. Source, Schema, selection, activation or
control retirement cancels that parameter; enter a new one against the current source. Ordinary
dirty field input hidden by the facet keeps the default editor's explicit recovery path and is
never reused as the tool parameter or automatically published on fallback. Child labels are display
text, not widget identities. The finite facet carries no drawing toolkit objects and does not define
a recursive widget language. Nara's own custom-drawn Editor runtime remains separate work.

## Finite Protocol

Each invocation reads one JSON request from stdin until EOF and emits one JSON reply to stdout
before exiting successfully. Protocol version is 1. The reply must echo the request number.
Requests contain `operation`, `name`, `fields` and `parameters`; fields and parameters use the
existing tagged `ComponentValue` JSON representation.

Description replies return `result.kind = "description"`, the complete `catalog` and optionally
the integer property declaration above. Proposal
replies use this shape:

```json
{
  "protocol_version": 1,
  "request": 2,
  "result": {
    "kind": "proposal",
    "title": "Increase EnemyStats health",
    "writes": [
      {
        "property": { "root": "health", "steps": [] },
        "value": { "type": "i64", "value": 120 }
      }
    ]
  }
}
```

The Host accepts at most 64 KiB of request or stdout bytes and 16 KiB of stderr. Overflow is a
failure rather than silent truncation. A definition has at most 16 component fields; parameters
have at most 16 entries. Copied inputs share a 4,096-node / 32 KiB logical-value allowance, and
returned values have the same aggregate allowance. A proposal has 1-64 writes and a title of at
most 4,096 UTF-8 bytes. Source-owner validation additionally checks selectors, value shapes and
the complete transaction. These finite logical and wire ceilings are not measured RSS or an
allocation sandbox. Stderr is consumed within its bound and is not published as a trusted error.

## Cancellation And Physical Return

`disable_source_tool(&activation)` revokes that exact activation's routing and retained review
immediately. `cancel_source_tool_attempt(&attempt)` cancels only that pending probe, preserving an
unrelated active predecessor. Both remain available during a workspace Close decision; they cannot
retire a successor through an old event. Replacement or a newer request also
retires obsolete work. Cancellation does not prove that a spawn, process or pipe worker returned.
The existing Tasks activity keeps custody until the child is reaped and all acquired pipe workers
are joined. Close continues polling that same owner; a held physical obligation remains pending.
The UI owner performs no synchronous process wait or IO join.

Use `EditorSourceToolView::has_pending_work()` for continued maintenance. Its
`physical_work_pending` observation includes deferred selection work and publication cleanup
retained by current or superseded file Hosts, even when the logical selection phase is Idle.
Reading this observation never advances cleanup. `process_cleanup_failed` is a sticky
Session-local terminal failure; a failed probe/request reports `TaskCleanupFailed`. Failed
executor cleanup does not keep maintenance pending forever, and absence of pending work does
not prove successful cleanup or authorize file reclamation.

The first contract supervises one trusted direct child per invocation. It does not contain
descendants or revoke ambient OS privileges, and it provides no hard cleanup deadline. A new
request can be refused while obsolete work still occupies the finite two-job allowance.

The independently built consumer in
[`tests/fixtures/behavior-languages`](../../tests/fixtures/behavior-languages/README.md) exercises
the actual C# EnemyStats executable. Run `python tests/fixtures/behavior-languages/run.py` to build
the artifacts and execute the Session consumers; Windows additionally runs the selected ignored
Application consumer with those same artifacts. It is a concrete protocol example, not an official C# SDK.
Add `--native-editor` on Windows to select the actual DX12 inline property/panel, history,
Save and fresh Open trajectory with those artifacts. Ordinary test runs do not execute this
ignored native case. The [owner plan](../plans/2026-10-28-finite-source-tool-owner-plan.md) and
[inline facet plan](../plans/2026-10-28-inline-source-tool-facet-plan.md) identify their respective
qualification scope.
