# Finite Source Tools

A trusted Host can explicitly enable one finite source tool in a Project Session. The tool runs
as a direct child process, receives copied component values and returns property proposals.
The Project Session owns activation, requests, review and acceptance. Scene/Prefab preparation,
generated entity IDs, inverse patches, history and persistence retain their existing owners.
No drawing toolkit or gameplay VM is involved.

This is explicit command configuration for a concrete tool. Ordinary project Open never executes
the command. Automatic discovery, persistent installation, executable byte pinning, dependency
resolution and a public language SDK remain separate work.

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
The view retains `attempt_error` with its attempt identity and `request_error` with its newest
request. Probing a replacement and invoking the active predecessor cannot overwrite each other's
failures. Starting another operation clears only that operation's result slot.

Source metadata admission and execution enablement are independent. Disabling the tool does not
remove component definitions or stored data.

## Request, Review And Accept

1. Capture `document_query_basis(document)` for the intended open Scene or Prefab.
2. Call `request_source_tool` with that unchanged basis, a Host-selected `Attach`, `Edit` or `Spawn`
   intent and copied parameters. The source must select the exact component owner and declaration.
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

## Finite Protocol

Each invocation reads one JSON request from stdin until EOF and emits one JSON reply to stdout
before exiting successfully. Protocol version is 1. The reply must echo the request number.
Requests contain `operation`, `name`, `fields` and `parameters`; fields and parameters use the
existing tagged `ComponentValue` JSON representation.

Description replies return `result.kind = "description"` and the complete `catalog`. Proposal
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

Disable revokes routing and retained review immediately. Replacement or a newer request also
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
The [plan](../plans/2026-10-28-finite-source-tool-owner-plan.md) identifies qualification scope.
