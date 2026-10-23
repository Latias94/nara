# Post-Boundary Contract Redesign

**Date:** 2026-10-23.

**Status:** Historical design adopted through the amended Accepted ADRs. This document explains
the alternatives and owner boundaries; it does not supersede those ADRs or independently qualify
implementation. Unit evidence and remaining platform limits belong in the
[implementation ledger](adr/implementation-status.md).

**Design baseline:** `efefb40`; the PB1-PB7 evidence baseline remains `10ccc36` in the
[source audit](../audits/2026-10-23-post-boundary-architecture-and-interface-review.md).

## Scope and Architectural Position

Resolve PB1-PB7 together with two interface debts: exclusive Scene access (E1) and the full ECS
dependency entry name (E2). These are related responsibilities, not one replacement framework.
Retain Bevy ECS, the selected Nara authoring surface, one managed Runtime owner, frozen component
codecs and concrete domain-owned execution. Keep document, structural, spatial and native lifecycle
ownership separate. No compatibility layer is required for these unreleased interfaces unless an
existing Accepted contract explicitly requires it.

The design has five rules:

1. Represent facts with different lifetimes separately; do not replace every check with one token.
2. Transfer custody explicitly, including the return value of an operation that subsequently faults.
3. Capture target facts after the last permitted mutation/flush, not merely before preparation starts.
4. Have an owner construct identity together with the immutable value consumers will reuse.
5. Make ordinary workflows complete without pretending full Bevy access is backend-independent.

Names and signatures below preserve the design sketches, not an exact current API reference.
The owning source, rustdoc and current guides define available APIs. No Cargo command, product test
or performance measurement was run during the original design supplement; later implementation
evidence is linked from the ledger. The source audit retains its earlier probe results and limits.

The subsequent [pinned Bevy/Godot comparison](../research/2026-10-23-post-boundary-bevy-godot-comparison.md)
checks every unit against actual local reference sources. It supports the owner corrections without
claiming equivalent guarantees. An independent challenge of R6's diagnostic-only alternative retains
bounded recovery continuity as the preferred design, with explicit Loading/supersession controls.
That comparison is design evidence, not a product test result.

## R1: Lossless Driver Execution Outcome (PB1)

**Owners:** `crates/nara_app/src/runtime/owner.rs`, `crates/nara_app/src/runtime.rs`, and concrete
Host consumers, especially `src/project_host/runtime/editor/game_view.rs` and
`src/project_host/runtime/editor/authoring_runtime.rs`.

Replace the lossy scope result with a result whose error means only **the operation was not
invoked**. A normal operation return has an execution envelope containing its output and any
post-operation canonical fault. The output itself can still be a domain `Result`, including an
owned rejected input. These are three separate questions: admission, operation outcome, and fault.

Conceptually:

```text
NotInvoked(scope_error)
Returned { output: R, post_operation_fault: Option<RuntimeFault> }
```

The envelope is not rollback or permission to use a faulted runtime. A Host first retains or
reconciles the returned completion, then follows fault policy. A successful native completion is
not retried because the Runtime later faulted; a rejected completion retains its cleanup owner.
An already-faulted retirement scope remains available under its existing limited policy. The
reported optional fault describes this operation's post-check outcome; it does not replace the
Runtime's sticky fault/state observation or prove no later asynchronous fault can arrive.

The current fault-route entry helper can reject before invocation and otherwise returns the
closure's value. Do not add a panic-catching/rollback promise to this refactor: an actual unwinding
operation is not a normal returned output. Preserve current unwind and retirement behavior.

For move-only Host authorities, provide one typed owned-port entry that retains the input until
scope admission succeeds and returns it with pre-invocation rejection. It delegates to the same
scope owner and existing registered port, not a second execution protocol. A generic `FnOnce`
capture is not an ownership-returning input: dropping an uncalled closure can drop its captures.
Do not claim that the execution envelope alone solves that problem. The typed entry owns any
private staging needed for transfer, so Hosts no longer infer input custody from an external
`Option` and a lossy error. Already-invoked port output retains the original input when the port
itself rejects.

Migrate every `with_driver_scope` caller and relevant internal capture caller, not only the panic
line. In particular, task admission may already have spawned work and Image publication may already
have committed when a post-check fails. Their outputs and Host bookkeeping must be handled before
returning a higher-level fault. Inspect input, publication, candidate acquisition/completion,
revocation and retirement paths. Do not offer a general lossy `into_result` convenience for owned
outputs; explicitly discard only values whose owner has established they are disposable.

**Delete:** Host-side external `Option` inference that outer error means the completion was never taken;
the old result-dropping scope path after all consumers migrate. Do not create a parallel universal
transaction or change arbitrary business callbacks into driver ports.

**Acceptance:** deterministic post-consumption fault barriers; successful and input-returning port
outcomes; entry rejection that never calls the closure; post-check bridge/authority failure; sticky
fault plus reachable Stop. Qualify the affected native Game View completion trajectory and Image
publication bookkeeping, without claiming recovery from arbitrary user panics.

## R2: Task Disposal Custody and Outcome (PB2/PB4)

**Owner:** `crates/nara_tasks/src/runtime.rs`, with the existing Runtime close participant protocol.

Keep the current queues, worker pools, caller-thread destruction and bounded close/reaper owners.
Add private disposal custody for previously admitted work when it leaves a queue for destruction.
The queue-locked transfer must establish that obligation before removal becomes visible. Disposal
returns outside the lock; only after user destruction returns may the obligation finish.

Completion requires the queue/worker/reaper predicates **and** no outstanding admitted disposal.
If workers have already joined but a submitting thread still disposes an old task, close remains
pending or incomplete and retains the same close owner for retry. It must not regress to Open or
rerun completed cleanup. Include coalescing and reclamation of cancelled pending nodes. A new
rejected submission after the close linearization point cannot reopen close or create an unbounded
new drain set; define that rejection's synchronous return separately from admitted-work retirement.

Keep two independent observations:

| Fact | Representation and lifetime |
| --- | --- |
| Disposal has not returned | Live custody obligation included in close completion |
| Disposal returned but successful cleanup cannot be proved | Sticky bounded failure evidence, no permanently live obligation |

Have the disposal helper return its classified cleanup outcome in addition to updating the
single-task return observer. Close-owned reaper receipts must carry outcome, not just a boolean.
Aggregate a bounded failure summary before removing completed receipts; publish it in the typed
shutdown report and the Runtime's existing `CompleteWithError` path. No destructor or business
failure text needs an unbounded log. Ordinary task business failure is not automatically teardown
failure; distinguish it from caught destruction failure owned by close.

Do not publish return from an early RAII field drop that precedes capture destruction. An internal
guard can protect accounting, but ordering and caught-panic outcome must be explicit. Do not put
user destructors under the queue mutex, create another global executor, or treat worker joining as
proof about destruction on a different thread.

**Delete:** local untracked ownership of discarded admitted jobs; outcome-free close reap receipts;
the successful-close interpretation after known close-owned disposal failure.

**Acceptance:** barrier-controlled coalescing/shutdown, pressure reclamation, worker and reaper
destruction, panic during close-owned capture disposal, and timeout/retry. Verify single-task
`Pending/Returned/CleanupFailed`, report completion, terminal Runtime evidence, failed Stop ticket
and Host diagnostic independently. Retain normal task-error and healthy-close controls.

## R3: Capability-Owned Write Cursor (PB3)

**Owner:** `crates/nara_fs/src/file.rs`; platform positional operations remain inside `nara_fs`.

Select one capability-owned write offset, initially zero for newly created writable files. A
borrowed writer continues that offset across writer reacquisition. Reads and reader seeks never
change it. Moving a capability into a lock and releasing the lock preserves it. This follows the
existing continuation consumer rather than turning each `writer()` call into an implicit overwrite.

Each write uses the same authorized handle with an explicit position: Unix `write_at`, Windows
`seek_write`, or an explicitly unsupported platform result. Advance the logical offset by the
reported successful byte count; preserve it on a reported error. Validate offset arithmetic before
the affected native operation. Partial writes and `write_all` continuation use the same cursor.
No correctness decision may depend on either native API's incidental handle pointer afterwards.

The mutable capability borrow already serializes writers; no global lock or new shared cursor is
needed. Readers keep their own positional offsets. This is not append mode, immutable content,
external-writer exclusion, or additional native lock authority. Do not add seek/truncate/append
features without a consumer merely to complete a generic file interface.

**Delete:** sequential native `File::write` as the capability writer's position authority. Do not
save/restore the native pointer, reopen paths, or clone descriptors to work around cursor effects.

**Acceptance:** fresh Windows/Linux tests for the partial-read counterexample, multiple writer
borrows, reader seeking, partial/error advancement, and lock/release continuation. Preserve bounded
read/digest, exclusive creation, Export/publication staging and application Save behavior. Platform
claims remain limited to actually executed evidence.

## R4: Post-Flush Persistent Admission (PB5)

**Owner:** `crates/nara_reflect/src/persistent_apply.rs`; actual Scene/Prefab callers share this
contract rather than implementing their own queue-draining workaround.

Use the following sequence:

1. Validate the request's frozen definition, targets and original captured asset baseline.
2. Register all potentially required component/support metadata, without publishing values.
3. Perform the permitted flush once for that preparation boundary.
4. Revalidate the original captured asset/World association and target topology. Determine actual
   support-resource existence and insertion requirements from this post-flush World.
5. Validate lifecycle effects, receipts and binding compatibility against that same cut.
6. Enter the exclusive no-flush commit tail; publish components, supporting resources and receipts.

Recomputing binding-resource existence must **not** recapture an asset baseline that blesses an
intervening asset write. Frozen codec identity and actual target admission remain distinct.
Inspect `validate_persistent_apply_support_topology` and other helpers that derive insertion flags
before flushing, so the correction is not another one-site reorder. Preserve the documented narrow
window for hidden prepared batches; do not introduce a public transaction framework or require game
callers to manually flush before `PreparedComponent::apply`.

**Delete:** pre-flush existence flags used as post-flush commit facts and duplicated caller repairs.

**Acceptance:** queued compatible first apply plus direct apply succeeds; queued incompatible
binding returns a typed error before outer value publication. Preserve asset-state interference,
target observers/hooks, receipts, frozen snapshot authority, Scene/Prefab and renamed-consumer tests.

## R5: Image-Owned Prepared Association (PB6)

**Owners:** `crates/nara_image/src/prepare.rs` and `content.rs`; consumers are Sprite, UI, WGPU and
Editor revocation. `nara_render` retains backend-neutral key/identity vocabulary, not Image policy.

Make `PreparedImageSnapshot` an opaque owner-produced association of immutable content and identity.
Keep `ImageContentSnapshot::prepared_snapshot` for admitted managed content. For code-first content,
capture the image and its slot identity together from the actual typed store and applicable existing
availability policy. Never require a managed source catalog solely to preserve the code-first path.
Compute descriptor identity inside the capture; callers cannot provide an independent identity for
arbitrary pixels. Packet readers can inspect or clone a snapshot, not replace half of it.

Fixing only `PreparedImageSnapshot::new` is insufficient: the generic prepared store currently also
accepts an unrelated snapshot and resource, and consumers reconstruct the pair. Prefer an
Image-owned prepared collection whose ready entry is the complete `PreparedImageSnapshot`. Its
public lookup returns that snapshot; mutation accepts coherent captures or explicit failed/removal
operations, never identity-plus-pixels. Preserve invalidation/revocation and source-allocation
retention, including snapshots already retained by packets.

Current production uses of `PreparedRenderResources<T>` are Image-specific; the remaining concrete
generic examples are mock tests. During implementation, complete the public-consumer inventory. If
that remains true, remove the now-unused generic store/record abstraction rather than retain a
second bypassing Image cache. Do not create a generic sealed preparation-trait framework to preserve
a hypothetical second domain. Keep neutral render identities that actual backend consumers need.

**Delete:** public arbitrary `PreparedImageSnapshot::new(identity, resource)`; reconstruction at UI,
Sprite and WGPU call sites; the writable generic Image route. Code-first fixtures capture real
stored images instead of minting inconsistent identities. Historical test-only invalid-kind cases
must exercise the correct remaining admission seam, not reopen the production constructor.

**Acceptance:** public and renamed consumers, code-first images, managed/last-good capture, wrong
association prevention, same-frame UI deduplication, warm-cache GPU readback, material/Sprite paths,
Editor revocation and retained allocation lifetime. Preserve cache reuse for genuinely equal captures;
do not add per-frame full-pixel hashing to compensate for a weak constructor.

## R6: Recovery Continuity Separate from Attempt Baseline (PB7)

**Owners:** Asset owns source observation, request admission and causal state transitions; Image owns
successful-removal recovery eligibility and per-attempt publication admission.

Keep generation supersession, Failed state, diagnostics and ReloadRejected outcomes. Separate the
long-lived pixel-free recovery record from ImagePublicationSnapshot, which remains an exact
one-attempt mutation baseline. A successful removal of usable Image content alone can issue the
recovery record; it never grants first-load authority or retains deleted pixels.

Retain exact catalog/type, value/state store association, empty-slot/removal continuity and a private
bounded causal witness. Asset's known capacity-rejection transition may preserve that witness while
advancing the ordinary state revision and reporting failure. Accepted recovery and Image's own
validated failure path explicitly carry it forward. Public/external state writes, replaced stores,
raw slot mutation, superseding successful publication and actual revocation invalidate it.
An allowed transition must start from the exact still-eligible owner state; it cannot bless earlier
external interference simply because the next operation is a queue rejection.

The witness is an owner-defined equivalence across a finite set of transitions, not a log of writers,
diagnostic-message parsing, numeric revision-delta inference or permission to ignore all state
mismatches. Before choosing its representation, enumerate those transitions and qualify their
positive/negative pairs. Reuse a current identity only if it actually proves this distinction;
the present AssetStateRevision alone does not. Keep at most bounded per-asset facts, not a general
provenance or queue-reservation protocol.

Every admitted attempt still captures a fresh exact publication baseline and checks the resolver's
post-resolution state revision. A post-resolution external write rejects before an attempted receipt
continuation. Actual metadata identity/settings are re-admitted, with wrong stable ID/path/kind and
the existing serde capability boundary preserved. Old attempts remain retired and cannot publish.

**Why not make capacity rejection diagnostic-only?** It looks smaller for an idle Removed baseline,
but routing advances generation before capacity checking. A recovering attempt A already in Loading
can be superseded by a rejected Modified B. Leaving state unchanged strands Loading; writing Failed
again invalidates the old exact receipt. Preserving A or moving supersession behind queue admission
changes stale-result policy and expands the admission protocol. That alternative is not established
as simpler by the Bevy/Godot sources, whose removal/queue contracts differ from Nara's.

It remains a possible separately justified state-contract redesign, not the selected PB7 repair.
The bounded continuity witness retains current observable state semantics while fixing the missing
recovery fact. Existing desired recipes can sometimes let a later retry load successfully even when
the receipt was lost; that masks, rather than proves repair of, continuity.

**Delete:** long-lived recovery storage of ImagePublicationSnapshot and blanket retention against
its diagnostic state revision. Do not relax the shared mutation-baseline guard or introduce a second
general asset executor/state machine.

**Acceptance:** successful Remove followed by repeated item/byte rejection and source-only retry;
Loading recovery A followed by rejected successor B; distinguish receipt eligibility from an already
retained recipe. Pair each eligible transition with same-version external state/slot-write rejection.
Include raw empty-slot ABA, metadata/removal order, replaced stores, wrong metadata, public import
supersession and stale tasks. No ownerless Loading or old pixel resurrection; preserve AF2/AF4/AF5
and post-resolution external-write controls.

## E1/E2: Honest ECS Entry and Complete Scene Workflows

### Full Dependency Entry (E2)

Select `nara_ecs::bevy_ecs`, also available through `nara::ecs::bevy_ecs`, for the complete selected
Bevy ECS crate. Remove the `upstream` public alias rather than maintain two canonical paths. The
word upstream remains valid descriptive prose; historical audit/qualification records retain the
names they observed. Do not rename the `crates/nara_ecs_derive/upstream/` provenance directory.

This is an ergonomics/contract-clarity decision, not PB8, memory unsafety, or additional decoupling.
The original runtime types and six adapted authoring derives are unchanged. The full crate name
identifies the dependency/version contract; it does not grant Nara-only dependency resolution to
every re-exported Bevy macro. Ordinary callers continue using `nara::ecs` or its prelude.

| Alternative | Disposition |
| --- | --- |
| Keep `upstream` | Valid relationship name, but less self-explanatory at a consumer call site |
| `bevy_ecs` | Selected: exact crate and scope, no claim of an interchangeable ECS |
| `bevy`, `raw`, `advanced` | Reject: misleading scope or difficulty/safety category rather than actual dependency |
| Require a direct Bevy dependency | Reject as the ordinary integration rule: the facade already selects the compatible version |

Update Accepted ADRs 0002/0044 when adopting the path. Migrate actual root/direct/renamed consumers,
source snippets and import assertions, including `crates/nara_ecs/tests/authoring_surface.rs`,
`tests/fixtures/derive-dependencies/facade_contract.rs`, the renamed-root fixture and module consumers.
Keep `__private` generated support separate. No root-level wholesale re-export or compatibility
alias is needed; do not add a new ADR just to rename this existing responsibility.

### Exclusive Scene Access (E1)

The naming change must not be presented as repairing repeated `SystemState` choreography in
`samples/two-level-game/src/gameplay/`. Provide a small domain access seam:

- A read-only borrowed current-Scene view obtainable from `&World`, with source and receipt from
  one private owner. Share its read operations with `CurrentScene`'s SystemParam interface. Do not
  return the underlying owner resource or add a strong source clone that outlives retirement.
- A scoped exclusive ProjectScenes entry that lets callers request, inspect, cancel or acknowledge
  through the existing domain interface. It may internally use safe SystemState access; it must
  not remove/reinsert resources or introduce unsafe World borrowing merely to hide imports.
- Both scheduled and exclusive access delegate to the same request predicates. A borrowed prepared
  view cannot escape its scope or coexist with a later mutable World commit. Callers construct owned
  game data inside the inspection scope, release it, then use the existing exclusive commit entry.

No accessor implicitly flushes, advances requests, commits a scene, acknowledges terminal state or
frees a cancelling request's slot. No generic `with_world` facade is introduced. Internal access
caching is optional measured work, not a performance claim attached to this refactor.

Do not expand the ordinary ECS exports with `SystemState` solely to make the sample import shorter:
that leaves the workflow burden intact. Keep manual parameter construction available through full
Bevy access for actual integration work. Reconsider selected exports only with separate ordinary
consumer pressure.

**Acceptance for E1/E2:** same original World/component types, six adapted derives with direct/root
and renamed dependencies, explicit full dependency access, no production `ecs::upstream` import,
and simpler two-level travel/restart/reconstruction with unchanged cancellation, private ownership,
receipt association and fresh-process Save/restore behavior. Historical prose is not a failing import.

## Adoption Order, Contract Reconciliation and Evidence

This order minimizes regression ambiguity; it is not a request to run concurrent Cargo commands.

| Step | Scope | Required deletion/qualification boundary |
| --- | --- | --- |
| 1 | R1 | Replace the lossy scope and migrate all concrete owned-result consumers |
| 2 | R2 | Account for admitted disposal and retain close-owned cleanup outcome through Host Stop |
| 3 | R3 | Preserve logical write continuation on fresh Windows/Linux owner tests |
| 4 | R4 | Derive support insertion facts after flush; preserve original asset interference guards |
| 5 | R5 | Migrate every Image prepared consumer and remove the writable pairing bypass |
| 6 | R6 | Qualify bounded recovery continuity, superseded Loading and genuine external-write rejection |
| 7 | E1/E2 | Finish the real Scene caller path, then migrate the complete dependency name together |

Naming is deliberately last: mechanical import churn should not obscure high-priority ownership
corrections. It can be its own precise commit after the ordinary workflow seam is settled. The units
need not share a generic abstraction or one giant commit.

Reconcile the affected clauses in ADRs 0002/0044 (ECS), 0052 (task retirement), 0070 (file cursors),
0081 (persistent admission), 0033/0037 (prepared identity/recovery), and 0076/0117 where Runtime
execution/close behavior is affected. Change the implementation ledger only with actual source and
verification evidence; this design does not mark anything complete. Preserve existing migration
entries, add no release migration guide solely for this unreleased refactor, and do not rewrite
unrelated historical plans.

Implementation starts with current-source regression tests for the recorded counterexamples, not
with trust in cached probes. Use serial Cargo, existing `target`, one build job, focused nextest,
renamed/public consumers, and `cargo fmt --all` after inspecting concurrent changes. Native Game
View and GPU readback evidence must be rerun when their contracts change; cross-compilation is not
Linux runtime qualification. Record unavailable evidence explicitly rather than broaden claims.

Before closure, perform independent adversarial review of custody transfer, concurrent close,
post-flush facts, image association and public imports. No new architecture kernel, synthetic Rust
resolver, global provenance log, speculative cache mode or ECS replacement is part of acceptance.
