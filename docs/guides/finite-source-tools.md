# Finite Source Tools

A trusted Host can explicitly enable one finite source tool in a Project Session. The tool runs
as a direct child process, receives copied component values and returns property proposals.
The Project Session owns activation, requests, review and acceptance. Scene/Prefab preparation,
generated entity IDs, inverse patches, history and persistence retain their existing owners.
No drawing toolkit or gameplay VM is involved.

This is explicit command configuration for a concrete tool. Ordinary project Open never executes
the command. A separate code-free local release capture API prepares declared artifact bytes;
persistent installation, approved installed execution, dependency resolution and a public language
SDK remain separate work. Raw command paths are trusted local configuration, not immutable
installed release identity.

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

## Configure And Probe

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
