# Exact Editor Persistence Attempts

Status: implementation recommendation for foundation R1/F2, not an Accepted contract or
implementation qualification. Reconcile ADRs 0047/0118 before changing the owning interface.
Baseline: `1c7956908831742542261617354030cce9021c2e`, 2026-10-25.

## Problem And First Principles

A document, its saved revision and a persistence attempt are different identities. Two failed
attempts can have identical request/document/reason values. Two successful attempts may capture
the same revision. Neither content equality nor the current frame identifies which result an
observer has actually seen. Acknowledgment consumes an observation; it is not a new Save and must
name the observation it consumes.

Current `EditorPersistenceAction::AcknowledgeResult` and `EditorPersistenceCommand::AcknowledgeResult`
carry no identity. The concrete owner calls `persistence_result.take()` when no worker is pending.
An observer holding result A can therefore clear later result B, including an identical-looking
failure. This is terminal-observation/interface loss, not evidence that saved bytes are lost.
R1a's owned direct dispatch observations already solve a different problem and remain unchanged.

The private owner separately stores `pending_persistence`, `persistence_operation` and
`persistence_result`. They describe one attempt lifecycle. Introducing an ID field alongside all
three would leave association dependent on keeping several independently writable slots aligned.
Prefer one concrete lifecycle state rather than a general request registry or result bus.

Source evidence:

- `crates/nara_tooling/src/persistence.rs`: request/result/view definitions.
- `crates/nara_tooling/src/frame.rs`: public persistence actions.
- `src/project_host/runtime/editor/persistence.rs`: admission, background transitions,
  acknowledgment, workspace-intent progression and terminal publication.
- `src/project_host/runtime/editor.rs`: pending worker and workspace-intent custody.
- `src/project_host/runtime/editor/document_open.rs`: an existing exact-ID acknowledgment pattern.

## Required Outcomes

1. One session-provenanced, non-reused attempt ID connects admitted dispatch, pending state and
   terminal result. It is process-local correlation, not a durable document ID or security token.
2. A terminal acknowledgment names that exact attempt. Wrong, old, foreign and repeated IDs cannot
   clear a different result. An older frame may acknowledge the still-current exact result;
   unrelated redraws must not invalidate that observation.
3. A request that publishes an immediate terminal rejection returns its new attempt identity too.
   An admission refusal that publishes nothing returns no new attempt and changes nothing.
   Classify by publication, not reason code: Busy can also be a published capture/worker outcome.
4. Existing explicit retry/reconcile policy remains authoritative. A permitted retry gets a new
   identity and explicitly supersedes the previous terminal; ordinary requests cannot silently
   overwrite an unacknowledged result. Read-only normalize the optional target first, apply the
   retained-result/retry policy with that target, and publish NoActiveDocument only when that
   policy permits a new terminal. Target normalization itself must not disable a valid retry.
5. Close/Exit's Save step retains its admitted attempt and advances only on its exact result.
   Matching Close success retains the terminal while entering retirement; matching Exit success
   consumes only that attempt before advancing to its next document. Failure/uncertainty retains
   the result and refreshes the decision. A nonpublishing refusal installs no waiting Save attempt.
   Internal consumption or retry cannot let an old external acknowledgment consume a successor.
6. Existing document-local file custody, immutable Save capture, physical background retirement,
   uncertain-write reconciliation, content refresh and saved-source identity remain unchanged.

## Alternatives

| Design | Benefit | Cost or failure | Disposition |
| --- | --- | --- | --- |
| Reject every stale frame's acknowledgment | Small adapter check | Unrelated frames invalidate a genuine observation; direct Session callers still have no exact correlation | Reject |
| Compare the observed result's document/revision/content | Reuses existing fields | Identical failures/retries alias; byte equality is not operation identity | Reject |
| Add an independent ID beside the three private lifecycle slots | Smallest mechanical patch | More synchronization invariants and invalid combinations; association still spread across writes | Reject as final owner design |
| One attempt ID and one concrete lifecycle state | Exact correlation; state transitions own association | Breaking caller migration and internal state refactor | Recommended |
| Per-observer subscriptions or a retained global result log | Independent delivery history | Adds retention/expiry and observer lifecycle not required by this workflow | Defer unless a real consumer needs it |

## Selected Shape

Names are provisional until implementation; the semantic states are the recommendation.

- `EditorPersistenceAttemptId` is an opaque Copy/Eq value containing existing session provenance
  and a checked, monotonic local sequence. The existing frame source gives Host provenance;
  acknowledgment compares against the stored attempt, never against the newest frame generation.
  Checked sequence exhaustion refuses a new attempt without overwriting any retained state.
- Replace the private parallel lifecycle fields with one concrete `PersistenceState`: Idle,
  Pending with attempt, captured operation metadata and owned worker stage, or Terminal with
  attempt and terminal payload. The sequence allocator is separate monotonic allocation state,
  not another independently assigned current-attempt slot.
- Public projection mirrors Idle/Pending/Terminal as an immutable value. Pending operation metadata
  has only Saving/Opening, not an Idle variant nested inside Pending. Terminal payloads remain
  existing domain-specific Saved/Opened/Rejected/Failed/PersistenceUncertain facts.
- Dispatch distinguishes an accepted attempt, an immediate rejection with its published attempt,
  and a nonpublishing refusal. An exact acknowledgment has its own outcome and does not allocate
  an attempt. Prefer explicit sum variants over a boolean plus unrelated optional identity fields.
- Both public action and concrete command require the attempt in `AcknowledgeResult`. Do not keep
  the old payload-free variant, convenience wrapper or alias. Renderer and tool callers use the
  identity from the observed terminal, not a freshly sampled current result at click/dispatch time.
- Background work carries the enclosing attempt across every stage and publishes only to that
  owned attempt. Pending becomes Terminal only through the existing physical-return-aware poll
  and publication path, not `TaskHandle::is_finished()` alone. Preserve the work-mutex fence:
  Save retains an actual irreversible write outcome even after logical cancellation; Reopen
  requires successful task completion before publishing prepared reads. Acknowledgment is legal
  only for Terminal and clears its observation only, not file uncertainty or retirement custody.
  Host cleanup/task shutdown remain independently owned after that transition. Workspace-intent
  progression uses the same exact association and the distinct Close/Exit policies above.

```mermaid
sequenceDiagram
    participant A as Observer A
    participant H as Concrete Session owner
    participant W as Owned persistence worker
    participant B as Observer B
    A->>H: Save document
    H-->>A: Accepted attempt 1
    H->>W: Captured operation for attempt 1
    W-->>H: Completion for attempt 1
    H-->>A: Terminal attempt 1
    H-->>B: Terminal attempt 1
    B->>H: Acknowledge attempt 1
    B->>H: Next legal request
    H-->>B: Attempt 2
    H-->>B: Terminal attempt 2
    A->>H: Late acknowledge attempt 1
    H-->>A: Mismatch; attempt 2 unchanged
```

The diagram abstracts worker timing; the implementation still admits at most one persistence
worker. It does not promise concurrent saves, per-document asynchronous queues, broadcast delivery,
durable request IDs, multi-file atomic Save or arbitrary worker cancellation.

## Execution And Proof

1. Before production edits, add behavior-level proof of the current correlation gap using existing
   Session/persistence fixtures. Distinguish a real failing assertion from an absent-new-type
   compile error. Inspect current retry policy and existing Save/Close/Reopen evidence first.
2. Reconcile the owning ADRs for exact attempt/acknowledgment and explicit supersession. Refactor
   the concrete lifecycle and its public projection together; do not let a fixture-only matcher
   pretend the production acknowledgment became exact.
3. Migrate direct Session, Application, workspace-intent, frontend, ordinary/native and independent
   consumer call sites. Saved bytes and content refresh stay with their existing owners. Preserve
   the current default-first-write consumer's dirty Save and exact clean NotDirty proof.
4. Prove two observers, identical terminal failures, a permitted retry, duplicate/foreign IDs,
   delayed acknowledgment, intervening frame publication and no-active-document admission. An
   admission Busy refusal preserves pending custody; published Busy outcomes have their own
   attempt. ResultPending preserves the existing terminal exactly. Keep targetless rejection
   in an empty slot, refusal over retained success and a valid later active-target retry separate.
5. Hold a real Save and Reopen at existing test gates. Correlate dispatch/pending/completion and
   preserve Close-retains/Exit-consumes ordering, failure recovery and old physical custody.
   The original captured Save revision remains independent of later editing. Assert the ordinary
   drive's completion -> workspace-intent progression -> retirement -> frame publication cut,
   rather than allowing external acknowledgment to steal a result needed by a waiting intent.
6. Run focused serial nextest, independent/public workflows, root/Editor coverage and the relevant
   Windows/DX12 persistence/navigation trajectories, then workspace compilation/correctness lint
   and formatting. Qualify this owner replacement only, not complete R1 or the foundation Goal.

| Criterion | Current evidence | Required implementation proof |
| --- | --- | --- |
| Wrong-result acknowledgment changes a successor result | Possible through payload-free take | Zero state changes for old/foreign/duplicate IDs |
| Published attempts correlate across all lifecycle stages | No public attempt ID | Exact equality from dispatch through terminal, including immediate rejection |
| Retained terminal survives nonpublishing refusal | NoActiveDocument precedes retained-result check | Exact retained attempt/payload equality after every such refusal |
| Physical persistence owner cardinality | One existing in-flight owner | Still at most one; existing hold/retirement tests pass |
| Tool-side histories, registries and correlation buses | None required | Zero added; all observers use the same owner-issued ID |

No latency improvement is claimed; this is a correctness and Interface-depth correction.

## Risks And Limits

- Broad caller churn can accidentally sample the latest ID instead of the ID actually displayed.
  Review UI construction and deferred callbacks, not just enum compile errors.
- Workspace intents currently inspect result/document facts. They must retain the expected Save
  attempt rather than rely on a same-document result or clear a global result by convention.
- Immediate rejections are observable attempts only when published. Preserve explicit retry and
  uncertain-write rules; do not treat every refusal as a new terminal, classify by Busy alone,
  weaken ResultPending or clear file uncertainty by acknowledging its observation.
- A retained immutable frame can outlive the live Session. Session provenance plus exact stored
  equality fences foreign reuse; no persistent-format version changes follow from this identity.
- The single terminal slot intentionally has explicit consumption/supersession, not independent
  per-observer delivery guarantees. A consumer needing a history requires a separate measured use
  case, not an unbounded log hidden inside this correction.

## R1 Continuation

The separate useful-tool tracer is a concrete source-local Transform2d snap-to-grid proposal,
shared by a source-built panel and a window-free Session consumer. Existing bounded observations,
property intents and move-only prepared candidates are sufficient; do not add a universal proposal
SDK. Its grid/tie/range policy still needs a concrete consumer decision and is not adopted here.
Compiled-tool structured domain diagnostics are the separate F3 owner change; a string wire does
not become structured because this persistence result now has an ID.

The pinned Bevy consuming-command and Godot editor-Undo mechanisms examined for R1 motivate small
concrete commands and reuse of existing owners, not this exact correlation contract. Their
commands/history alone do not prove Nara's Source preview, persistence or physical custody.
This recommendation follows the actual Nara observer failure, not parity with another engine.

Related sources: [foundation plan](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md#r1-minimal-semantic-tool-and-exact-operation-outcomes),
[ADR 0047](adr/0047-editor-workspace-and-scene-document-state.md),
[ADR 0118](adr/0118-editor-workspaces-frontends-and-creation-templates.md).
