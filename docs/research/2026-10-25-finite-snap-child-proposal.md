---
title: Finite Snap Child Proposal Experiment
date: 2026-10-25
status: Researched experiment design; not implemented or qualified
---

# Finite Snap Child Proposal Experiment

## Question And Scope

Can the source-built Snap command cross a real process boundary without moving document,
validation, inverse, history or persistence authority into that process? This is the next bounded
R1 experiment in the [foundation plan](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md),
not R1b installation, a scripting SDK, a sandbox or qualification of arbitrary native tools.

Research used Nara `3a31343` plus the in-progress source-built Snap consumer. Its local upstream
sources are Bevy `85503941e488152711a4695bbe248e9ed9f453fc` and Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558`. Source inspection is not process-execution evidence.
The recommendation below is disposable experiment policy, not an Accepted engine contract.

## Ownership Before Transport

The parent retains the two bounded scalar observations and their complete original query basis.
The helper receives grid/X/Y only and invokes the existing pure `snap_local_position` calculation.
It cannot choose a document, entity, component, property selector or revision. A finite response
is a proposal, never an `EditorAction`, prepared candidate or inverse.
The existing `plan_snap_local_position` also computes on its caller: do not run it in the parent
to manufacture an expected child answer. Split observation extraction and result attachment
privately inside the consumer so the local and process placements share the same pure calculation.

```mermaid
sequenceDiagram
    participant Parent as Explicit experiment owner
    participant Source as Existing Session / Source owner
    participant Child as Selected local helper
    Parent->>Source: Read X/Y at exact basis
    Source-->>Parent: Two bounded scalar observations
    Parent->>Child: One finite request (grid, X, Y)
    Child-->>Parent: One finite scalar result
    Note over Parent,Child: Require successful exit, EOF and physical cleanup
    Parent->>Source: Validate original basis; prepare property edits
    Source-->>Parent: Exact candidate + forward/inverse preview
    Parent->>Source: Explicit Apply consumes same candidate
    Source-->>Parent: Owned exact result; one Undo entry
```

The relevant existing boundaries are
`tests/fixtures/module-consumer/editor-panel-consumer/src/snap_local_position.rs`,
`src/project_host/runtime/editor/prepared_edit.rs` and
`crates/nara_tooling/src/workspace/prepared_edit.rs`. Session already exposes basis validation;
the panel context performs it before preparation. No demonstrated authoring API gap requires a
new engine service. See [ADR 0015](../architecture/adr/0015-editor-tooling-and-dogfooding-boundary.md)
and the [panel guide](../guides/editor-panels.md#a-source-local-snap-tool).

## Alternatives

| Option | Benefit | Limitation | Disposition |
| --- | --- | --- | --- |
| Fixture-private `std::process` owner and selected no-descendant helper | Real process/pipe boundary; no engine API or async runtime; small disposable scope | Blocking pipe I/O and OS spawn do not have portable hard cancellation; no arbitrary process-tree guarantee | Preferred first experiment, subject to the custody limits below |
| Reuse or extract the private supervised Play process substrate | Existing Windows Job / Unix group, cancellable output and physical cleanup receipts | Private Play coupling; output queue permits loss; exposing the whole supervisor would freeze the wrong API | Reconsider for installed or arbitrary tools, not as this experiment's prerequisite |
| Generic public process/tool Host now | One apparent integration point | Premature trust, lifetime and transport promises before a real consumer selects them | Rejected for this unit |

The existing Play code at `editor/src/supervised_play/process/mod.rs` requires direct-child reap,
empty containment and stopped readers before its cleanup receipt. Its
`process/output.rs` queue can evict output and mark loss. That is useful log behavior, not a
reliable proposal channel: a partial response must never be accepted as success.

## Explicit Enablement And Trust

- Keep code in the independently compiled, unpublished panel consumer; use a default-off
  `snap-child-experiment` feature and an explicitly selected fixture executable.
- Resolve only Cargo's absolute helper path. Do not run project declarations, search PATH for
  user commands or invoke a shell. Use an owned temporary CWD and an empty environment with a
  minimal platform allowlist. On Windows launch without a visible console window.
- The selected native helper is trusted not to create descendants. The experiment does not
  enforce that promise and must not advertise isolation from filesystem/network access or bounds
  on child RSS/CPU. Opening project data never implicitly enables execution.
- Begin with a window-free Session driver. Do not put physical process custody in a panel that
  is only driven while painted. `ImguiEditorPanel` instances retire on hide/close/retarget/project
  replacement (`editor/src/frontend/imgui/workspace.rs`); arbitrary task cancellation is not
  native process cleanup (`crates/nara_tasks/src/runtime.rs`).

The last point is the important architecture pressure to carry into R1b: UI publication interest
and physical work lifetime need different owners. A future runner-owned tool lane must be proven
before integrating a long-running child into ordinary panels. This research does not select a
new public Host trait or declare the current source-built panel contract defective.

## Proposed Finite Exchange

Use one fixed tool-specific binary request/response, not a universal wire SDK. Proposed request:
40 bytes, containing an eight-byte magic/version/tag header, eight-byte correlation sequence and
three little-endian `f64` values. Proposed response: 32 bytes with an eight-byte header/result code,
the sequence and two values. The canonical experiment version starts at 1. Exact frame layout
and refusal-code assignment remain implementation details to verify before any reusable freeze.

Require one frame followed by EOF; reject short/extra frames, unknown tags/versions/reserved bits,
wrong correlation, nonfinite/out-of-range values and nonzero exit. A valid-looking frame followed
by a crash is failure. Limit retained stderr to 4096 bytes and treat overflow as failure rather
than continuing to accumulate. Raw helper stderr is not a public safe diagnostic. Fixed frame
limits bound parent retention, not child memory, kernel buffering or total bytes emitted.

Retain at most one physical child and one unpublished result. Busy admission may refuse; no
unbounded request/chunk queue is needed. If supersession is supported, revoke the old result
before cleanup and admit the successor only after physical return. Keep the parent's original
observations; never serialize opaque Schema/candidate authority or refresh the revision on return.

## Physical Custody And Freshness

Maintain publication interest separately from process/I/O custody. Timeout, cancel, supersession
and protocol failure revoke publication immediately but do not release the physical slot.
Success requires child exit success, both output EOFs, completed stdin work and joined I/O workers.
The selected helper must really compute, not echo a parent-computed answer.

Use a five-second experiment deadline from admission, including spawn; measure actual spawn,
response and cleanup durations separately. OS spawn cannot be interrupted portably: a late child
must still be received and terminated by its original owner. A one-second post-kill observation
budget and 500 ms post-exit drain budget may report `CleanupIncomplete`, never Returned. Retain
the owner and continue cleanup; do not detach blocked JoinHandles or admit a conflicting child.
Explicit close handles success, failure and test unwind; Drop is only a final custody fallback.

Portable blocking std readers cannot guarantee bounded join if descendants retain pipe writers.
That is outside the selected helper premise, not something a deadline fixes. If actual experiments
need that case, stop the std-only approach and evaluate the cancellable native substrate rather
than silently weakening cleanup evidence.

Request correlation and document freshness are separate fences. Revoked/foreign results cannot
create a preview. For a still-interested response, validate the original Session query basis before
preparing; edits, Undo/Redo, Reopen, Schema changes and project replacement must refuse stale work.
Commit then revalidates the same retained candidate. Neither a sequence number nor a successful
process exit grants document publication authority.

## Acceptance And Stop Conditions

1. An actual child computes `[23, -25] -> [16, -32]`; public inactive-document preparation is
   read-only until Apply, has the expected owner inverse, and yields one Undo/Redo and exact
   Save/Reopen attempt observations. NoChange and semantic refusal publish nothing.
2. Table-driven real helper failures cover malformed/oversized stdout, oversized stderr, invalid
   numbers, wrong request and success-frame/nonzero-exit disagreement. Each preserves source and
   history and ends with actual child reap and I/O return, not only cancellation requested.
3. Controlled barriers cover result-after-edit/Reopen, cancellation, supersession and delayed
   spawn observation. Old results never acquire a new basis or replace successor observations.
4. Record end-to-end and physical cleanup costs, parent frame/diagnostic retention and exact
   runtime/toolchain/platform. No latency improvement or non-Windows execution is presumed.
5. Stop and reassess if safe cleanup needs a general native Host, the parent computes the proposal
   first, or the helper needs document/candidate authority. Do not turn this finite comparison into
   an installed-tool platform merely to make the experiment look complete.

## Upstream Evidence And Limits

- Bevy `examples/async_tasks/async_compute.rs` separates background work, readiness polling and
  deferred application. Its returned in-process `CommandQueue` is executable mutation authority,
  not a safe cross-process source proposal. `bevy_tasks/src/task_pool.rs` owns worker joins;
  future cancellation/detach does not prove native child retirement.
- Godot `platform/windows/os_windows.cpp` uses explicit inherited pipe handles and
  `CREATE_NO_WINDOW` in `execute_with_pipe`; `drivers/unix/os_unix.cpp` supplies pipe/PID and wait
  bookkeeping. Those mechanisms do not supply Nara's source basis, semantic byte limits or a
  receipt proving all I/O and descendants have returned.
- Nara's [ADR 0116](../architecture/adr/0116-standalone-supervised-editor-play.md) and private Play
  implementation provide stronger process custody than the proposed trusted-helper premise.
  Their existing scope must not be weakened or advertised as new tool support by this study.
