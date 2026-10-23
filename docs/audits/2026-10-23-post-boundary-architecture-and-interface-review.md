# Post-Boundary Architecture and Interface Review

**Date:** 2026-10-23.

**Source baseline:** `10ccc362f1bd6045d593227af82f0143ccedaf65`.

**Disposition:** Findings and bounded redesign recommendations; no implementation changes.

## Scope and Evidence

This pass follows the completed
[ADR boundary and frozen-execution work](../plans/2026-10-23-adr-boundaries-and-frozen-execution-plan.md).
It does not reopen the repaired L01-L10, SF1-SF5 or AF1-AF5 counterexamples in the
[earlier follow-ups](2026-10-23-low-level-refactor-review-followups.md).

The subsequent [replacement design](../architecture/post-boundary-contract-redesign.md) analyzes
PB1-PB7 together with E1/E2 below, including the user's ECS naming follow-up at `efefb40`. It selects
bounded owner corrections and deletion targets; it does not change the evidence baseline or mark
these findings implemented.

Three independent Codex reviewers traced App/Runtime/Tasks, Asset/Image/Watcher/Render/Transform,
and Reflect/Scene/Hierarchy/Identity. The integrating reviewer traced filesystem capabilities and
real two-level-game consumers, challenged findings, and ran focused probes. Specific counterarguments
were exchanged; agreement is not execution evidence. The preexisting untracked `vendor/` tree was
not inspected or modified; `repo-ref/` was not changed.

Evidence classes:

- **Current-source probe:** a temporary external consumer built current `nara_fs` with Rust 1.99.0,
  offline, one Cargo build job, reusing `target` on Windows 11 x86-64/local NTFS.
- **Cached-artifact probes:** temporary Rust 1.95.0 executables linked existing App/Tasks and
  Reflect/ECS rlibs from the preceding qualification. Observations agree with the current-source
  traces. These are not fresh owner source rebuilds or complete product qualification.
- **Source-confirmed:** an exact trigger and its consumer/failure path were traced, including
  existing guards. No new execution result is claimed for that path.

Probe sources are local diagnostics under `target/architecture-review-20261023/`, not production
interfaces or committed fixtures. No full suite, native Editor trajectory, GPU readback, benchmark
or Linux runtime test was run. Earlier qualification retains its original scope.

## Findings

| ID | Priority | Owning contract | Consequence | Evidence |
| --- | --- | --- | --- | --- |
| PB1 | P1 | App driver scope / Host owned completion | Post-operation fault can trigger an impossible ownership assumption and panic Game View | Scope mechanism executed with cached artifacts; Game View consequence source-confirmed |
| PB2 | P1 | Tasks admission and physical close | Complete close can precede executor-owned capture disposal on a submitting thread | Cached-artifact concurrent probe |
| PB3 | P2 | Filesystem reader/writer cursor ownership | Partial read changes subsequent write position on Windows | Current-source public consumer probe |
| PB4 | P2 | Tasks close disposal outcome | Caught cleanup failure disappears from aggregate close evidence | Source-confirmed |
| PB5 | P2 | Reflect target-World admission | Pre-flush first-binding state can make ordinary public apply panic | Cached-artifact public consumer probe |
| PB6 | P2 | Image prepared-content identity | Publicly paired identity and pixels can be incorrectly deduplicated/reused | Source-confirmed |
| PB7 | P2 | Image removal recovery authority | Resolver queue rejection strands source-only recovery until metadata re-admission | Source-confirmed |

Priority reflects the supported trigger and consequence, not measured occurrence frequency. PB2 is
reachable through concurrent public standalone Tasks use; this pass does not claim that ordinary
current Hosts submit in that pattern. PB3 affects valid public mixed read/write use; the inspected
production staging callers perform one-shot writes.

### PB1: Game View Treats Post-Execution Failure as Unconsumed Input

**Locations:** [scope capture](../../crates/nara_app/src/runtime/owner.rs#L476),
[post-operation checks](../../crates/nara_app/src/runtime/owner.rs#L493),
[completion transfer](../../src/project_host/runtime/editor/game_view.rs#L1233),
[error branch](../../src/project_host/runtime/editor/game_view.rs#L1263).

A healthy Runtime enters `complete_candidate`. The closure takes the completion from an external
`Option` and invokes the owned port. If a canonical fault is reported after transfer but before the
scope's final checks, `capture_driver_scope` returns `Err` and drops the operation result. The Host's
outer error branch expects the external `Option` to retain the completion; it is already empty.
This can panic even when the port completed successfully. An input-returning port rejection inside
the discarded result is also affected.

The scope explicitly documents that a new canonical fault can produce `Faulted`. Returning that
error is not itself a contract violation. The defect is its composition with an owned caller that
equates outer error with non-execution. Exclusive Runtime borrowing does not exclude another thread
using a previously cloned [canonical reporter](../../crates/nara_app/src/runtime.rs#L496).
Preexisting-fault retirement and owned-port admission rejection handle different cases.

The cached-artifact probe consumes a `String`, reports a fault inside the scope, and returns that
value. It observes `driver_scope_rejected=true`, `driver_input_still_owned=false`, and
`driver_fault_visible=true`. This executes the result-loss mechanism, not the full Game View panic
or native completion path. The concurrent reporter interleaving is established by source.

**Recommended redesign:** At the App/Host owned-operation seam, distinguish pre-execution rejection
from executed output plus sticky fault. Preserve output custody through post-operation failure.
Keep fault isolation; do not promise rollback, reinterpret the fault as success, or merely delete
the Host's `expect`.

**Acceptance:** Control reporter publication after completion consumption. Cover successful port
execution and input-returning rejection: neither may panic or lose an owned return. Fault evidence
stays sticky, actual consumption remains truthful, and Stop remains reachable. Inspect other owned
scope consumers for the same assumption.

### PB2: Admission Disposal Is Missing from Physical Close Custody

**Locations:** [coalesced removal](../../crates/nara_tasks/src/runtime.rs#L2191),
[lock-free disposal](../../crates/nara_tasks/src/runtime.rs#L2235),
[drain predicate](../../crates/nara_tasks/src/runtime.rs#L2355),
[joined completion](../../crates/nara_tasks/src/runtime.rs#L2404).

Share standalone `Arc<TaskPools>` with one worker per kind. Occupy the I/O worker, queue A, then
submit same-domain/same-key replacement B from another thread. Admission removes A under the queue
lock, places B, releases the lock, and disposes A on the submitting thread. Block A's capture
destructor. Release the worker, allow B to execute, then call `shutdown_blocking` from the first
thread.

A is now in neither `pending` nor `running`, nor any close reaper receipt. The worker can exit and
be joined, so aggregate close reports complete while A's `TaskReturnObserver` is `Pending`.
The cached-artifact probe observes:

```text
pool_close_complete_before_drop_release=true
old_task_return_before_drop_release=Pending
old_task_return_after_drop_release=Returned
```

The individual return observation and lock-free user destruction are correct. Neither protects
aggregate retirement after custody leaves the queue. Public `spawn(&self)` and
`shutdown_blocking(&self)` permit this sequence; the close-owner mutex does not serialize submission.
[ADR 0052](../architecture/adr/0052-task-backpressure-cancellation-and-long-running-diagnostics.md)
requires admission/close linearization and physical retirement while permitting caller-thread
destruction. Its warning about blocking submission does not waive custody.

**Recommended redesign:** When accepted pending work leaves the queue for executor-owned disposal,
register a shared in-flight obligation under the same lock. Release it after disposal returns and
include it in close completion. Cover coalescing and pressure reclamation of cancelled nodes. Do
not move destructors back under the lock or introduce another executor.

**Acceptance:** The concurrent sequence remains incomplete until the destructor is released, then
completes on retry. Preserve truthful single-task return, bounded owner polling, worker joining and
healthy admission.

### PB3: Windows Positional Readers Move the Sequential Writer Cursor

**Locations:** [reader contract](../../crates/nara_fs/src/file.rs#L151),
[writer](../../crates/nara_fs/src/file.rs#L379),
[Windows read](../../crates/nara_fs/src/file.rs#L436),
[synchronous handle creation](../../crates/nara_fs/src/platform/windows.rs#L897).

Create a writable file through a directory capability. Write `abcdef`, borrow a reader and read
only `ab`, drop it, then borrow a writer and write `XY`. The current-source Windows probe observes
`abXYef`; an independent read cursor would leave write continuation at byte six, producing
`abcdefXY`.

The reader owns a logical offset, but Windows `FileExt::seek_read` also moves the native handle
cursor to the end of the read. The writer delegates to sequential `File::write`. Unix `read_at`
does not move that cursor. Installed Rust standard-library documentation explicitly describes the
Windows effect; the finding is not inferred from the method name.

Two readers remain independent because every read supplies its own offset. Thus the
[reader/reader test](../../crates/nara_fs/tests/filesystem_contract.rs#L1125) does not exclude this
failure. The owned-lock write continuation test reads the full file, ending at the previous write
position and concealing the difference. Borrowing prevents simultaneous readers and writers, not
this sequential interaction. This is not an append-mode promise or observed corruption in current
one-shot Export/publication staging.

**Recommended redesign:** `nara_fs` should own explicit logical write-position semantics as well as
read-position semantics. Prefer a platform-independent capability cursor with positional writes;
decide whether reacquired writers continue that cursor or start a new one, and preserve legitimate
consumers. Do not reopen ambient paths or duplicate native lock-release ownership. Saving/restoring
a shared native cursor is not a replacement for defining cursor ownership.

**Acceptance:** Current-source Windows and Linux public tests cover partial reads between writes,
independent readers, seek/read interaction, locked capability release/reacquisition, and write-error
cursor advancement. No broader handle or filesystem support claim follows.

### PB4: Close Loses Caught Task-Disposal Failure

**Locations:** [caught drop](../../crates/nara_tasks/src/runtime.rs#L2811),
[reap completion](../../crates/nara_tasks/src/runtime.rs#L3096),
[receipt removal](../../crates/nara_tasks/src/runtime.rs#L2459),
[participant outcome](../../crates/nara_tasks/src/runtime.rs#L3674),
[Host interpretation](../../src/project_host/runtime.rs#L1023).

During close, retain a blocked worker and a pending task whose capture destructor panics. After the
drain deadline, close cancels the pending task and sends disposal to the reaper. `safe_drop_pending`
catches the unwind and publishes individual `CleanupFailed`. The reap receipt records completion
only. Once the receipt is removed and workers are joined, the Tasks participant returns ordinary
`Complete`, without terminal cleanup failure evidence. With no other fault, a Host can eventually
report successful Stop.

This is not the old missing single-task return observer: that observer correctly retains the
failure. The loss crosses disposal, reaper receipt, pool close and Runtime evidence. No particular
native leak is proved. An ordinary task business error must not automatically become teardown
failure.

**Recommended redesign:** Retain the outcome of close-owned disposal in its receipt and aggregate
it through existing `CompleteWithError`. Physical completion can end the live obligation while
terminal failure remains visible. Keep PB2's custody accounting and this outcome accounting distinct
even if implemented together.

**Acceptance:** Managed close cancels pending work whose capture disposal panics. Runtime may reach
`Stopped`, but terminal evidence and failed Stop result remain; the Host reports cleanup failure.
Normal disposal and timeout-then-successful-retry controls still succeed.

### PB5: Reflect Caches First-Binding Admission Before Flushing Commands

**Locations:** [cached existence](../../crates/nara_reflect/src/persistent_apply.rs#L522),
[flush](../../crates/nara_reflect/src/persistent_apply.rs#L532),
[infallible commit assumption](../../crates/nara_reflect/src/persistent_apply.rs#L600),
[late resource check](../../crates/nara_reflect/src/persistent_apply.rs#L638).

Start with two empty target entities and no `PersistentWorldBindings`. Prepare two asset-free
components with the same native type and stable binding through one frozen registry snapshot.
Queue public `PreparedComponent::apply` for B in `world.commands()`, then directly apply the other
value to A. Outer preparation caches `needs_binding_resource=true`. Its flush executes B's legitimate
first apply and creates the binding resource. Outer preparation rechecks target and binding
compatibility but does not refresh the cached existence fact. Immediate commit rejects its own
prepared resource predicate and converts the error to an `expect` panic.

B's queued write already completed. This does not prove partial outer component publication. The
same snapshot/type/stable ID and asset-free setup exclude binding conflict, foreign World and asset
baseline mismatch as explanations. The existing
[prepare/commit success test](../../crates/nara_reflect/src/persistent_apply.rs#L1055) has no queued
first apply.

A cached-artifact public consumer registers an asset-free codec for the existing ECS `Name`
component, uses one frozen snapshot, and executes that sequence. It catches the panic with
`prepared persistent binding resource state changed`, observing `public_direct_apply_panicked=true`,
`queued_target_written=true`, and `direct_target_written=false`. This confirms the ordinary apply
failure on those qualified artifacts, not a fresh full-source Reflect regression run.

**Recommended redesign:** Establish resource-existence and support-publication facts on the actual
post-flush target baseline, after registration/flush and before the no-flush publication tail.
Reconcile the owning admission sequence, not callers' flush habits. No universal transaction
framework is needed.

**Acceptance:** Queue B's first apply, directly apply A, and require no panic and consistent
components/receipts/bindings. Incompatible queued binding remains a typed error before outer value
publication. The focused probe command and limits are recorded in the verification section.

### PB6: Prepared Image Identity Is Publicly Separable from Pixels

**Locations:** [unchecked pairing](../../crates/nara_image/src/prepare.rs#L120),
[public UI insertion](../../crates/nara_ui_render/src/planner/images.rs#L22),
[identity-only deduplication](../../crates/nara_ui_render/src/planner/lowering.rs#L174),
[backend reuse](../../crates/nara_render_wgpu/src/texture.rs#L355).

Capture legitimate red snapshot A. Public `PreparedImageSnapshot::new` can pair `A.identity().clone()`
with another same-sized green image's `PreparedImageResource`. Insert A and B under different UI
image IDs and reference both. UI lowering merges them by identity and uses one set of pixels. In a
warm GPU cache, B likewise reuses A's texture. Kind, extent and pixel-length checks do not detect
same-shaped conflicting content.

The constructor has no documented caller obligation to maintain the association. The snapshot
types describe exact immutable prepared content; a copied slot revision is not proof that arbitrary
replacement pixels came from that slot. This is a semantic public-interface gap, not Rust memory
unsafety. Automatic first-party capture correctly pairs one record's identity and resource; this
review does not claim that path spontaneously creates a mismatch.

**Recommended redesign:** The Image owner should construct identity and immutable content from one
capture. Preserve code-first image preparation with content-bound identity rather than requiring
public manual pairing. Generic prepared-store insertion and packet admission must not bypass the
association. A descriptor comparison alone does not prove pixel provenance. Any explicit advanced
unchecked assembly seam needs a truthful contract and conflict handling before cache reuse.

**Acceptance:** Public consumer tests reject or prevent mismatched construction; actual UI planner
tests cover conflicting same-identity values, and warm-cache GPU readback covers replacement. Do
not merely disable planner deduplication and pass the ambiguity to the backend.

### PB7: Queue Rejection Erases Successful-Removal Recovery Authority

**Locations:** [successful removal receipt](../../crates/nara_image/src/reload.rs#L911),
[resolver rejection](../../crates/nara_asset/src/reload/resolution.rs#L304),
[revision advance](../../crates/nara_asset/src/state.rs#L417),
[receipt retention](../../crates/nara_image/src/reload.rs#L788),
[source-only gate](../../crates/nara_image/src/reload.rs#L682).

Successfully remove a usable Image and retain its pixel-free recovery receipt. Restore valid source
and metadata with the same stable identity, path and kind; send only `Modified`. Make resolver queue
item/byte capacity reject that request before Image receives it. Rejection advances the state
revision. Image's spawn-stage retention compares the receipt's exact mutation baseline and deletes
it. Remove pressure and retry `Modified`: there is no recovery receipt, live recipe or post-removal
metadata observation, so it fails with `MissingImportSettings`. Further source-only retries cannot
recover without an extra metadata event or explicit import.

Successful admission transfers recovery into a new attempt, and failure handling can retain it.
Neither runs for resolver admission rejection. This differs from AF2's rejected Remove and
AF4/AF5's metadata/removal order: Remove succeeded and no new metadata observation is required by
the existing exact-receipt recovery path. Existing
[post-resolution state-write coverage](../../crates/nara_image/src/tests.rs#L3529) correctly rejects
an intervening external write and must remain intact.

**Recommended redesign:** Separate durable removal recovery authority from one mutation baseline.
The owner of rejection must explicitly preserve/continue eligible recovery while distinguishing
superseding external writes. State revision alone has no writer/reason/request provenance; do not
ignore mismatches or infer authority from an unchanged empty slot. Each actual publication attempt
still captures and validates its exact baseline and metadata identity.

**Acceptance:** Successful Remove, rejected source-only restoration, restored queue capacity and
source-only retry must recover. Preserve rejection after genuine intervening state/slot writes,
wrong metadata identity and stale task completion; never revive deleted pixels.

## Interface Design Debts

### E1: Exclusive Scene Access

The two-level game repeatedly constructs `SystemState<CurrentScene>` or `SystemState<ProjectScenes>`
inside exclusive `&mut World` workflows:
[travel](../../samples/two-level-game/src/gameplay/travel.rs#L136),
[request polling](../../samples/two-level-game/src/gameplay/travel.rs#L219),
[reconstruction](../../samples/two-level-game/src/gameplay/travel/reconstruction.rs#L91), and
[ordinary gameplay](../../samples/two-level-game/src/gameplay/mod.rs#L204). Current source/receipt
ownership remains private and correctly paired; the issue is access choreography, not a confirmed
correctness failure or measured allocation cost.

A small borrowed World-oriented view/request entrypoint is a justified design candidate because
the complete consumer already needs exclusive scene commit/restart boundaries. It should share
the SystemParam implementation and preserve private owners, request-slot custody and exact
source/receipt association. Compare it with caching SystemState in the existing exclusive system
before selecting an interface. Do not expose internal resources or create a generic World facade.
Acceptance is a simpler real game with unchanged travel, save, restart and cancellation behavior.

### E2: Full ECS Access Uses a Relational Name Instead of the Dependency Name

**Classification:** interface clarity, not an additional correctness finding or P1/P2 defect.

The [complete substrate entry](../../crates/nara_ecs/src/lib.rs#L39) is a direct Bevy ECS
re-export named `upstream`. ADRs 0002/0044 correctly separate selected Nara authoring, complete
version-coupled dependency access, and hidden generated/internal support. That split remains useful.
The alias itself is less explicit at call sites: it says the dependency is upstream but not which
dependency or what scope is exposed.

Select `nara_ecs::bevy_ecs` and `nara::ecs::bevy_ecs` as the full dependency entry when this design
is implemented. Keep ordinary authoring imports and the six adapted derives unchanged. This exposes
existing coupling honestly; it does not create backend independence or give unadapted Bevy macros
the Nara-only dependency guarantee. `bevy`, `raw` and `advanced` communicate scope less accurately;
requiring a direct dependency is unnecessary for access to Nara's selected Bevy version.

Remove the public `upstream` alias during adoption, with ADR 0002/0044 reconciliation and precise
root/direct/renamed-consumer migration. Do not maintain two canonical names in this unreleased
project. Preserve historical reports and the derive provenance directory, where upstream describes
actual lineage rather than a current public import.

E2 does not repair E1. Do not merely shorten `SystemState` imports or expand the ordinary prelude
to hide domain access choreography. The replacement design provides borrowed current-Scene access
and scoped exclusive request access while retaining the SystemParam path and private owners.

## Architectural Interpretation and Refactor Order

These findings do warrant mechanism-level refactors rather than seven isolated fallback checks:

1. **Execution and authority are different facts.** PB1 needs an executed-output protocol; PB5 needs
   admission facts from the correct temporal baseline. An outer `Result` is not proof of rollback
   or non-execution, and a prepared flag cannot span an unmodeled flush.
2. **Custody, physical return and successful cleanup are different facts.** PB2 loses custody from
   aggregate observation; PB4 loses an outcome during aggregation. One finished boolean cannot
   represent both. Keep the existing pool, close owner and Runtime evidence vocabulary.
3. **Identity must remain associated with what it identifies.** PB6 requires owner-issued coherent
   prepared content. PB7 requires a recovery fact with a longer lifetime than one attempt baseline.
   PB3 requires cursor state owned by the abstraction rather than an accidental platform side effect.

Retain Bevy ECS, the managed Runtime owner, concrete domain-owned execution, frozen codecs and the
separation of documents, structural hierarchy, spatial snapshots and lifecycle ownership. This pass
finds no new comparable-strength counterexample in repaired spatial publication or watcher custody.
It does not establish those modules are defect-free. No evidence here warrants replacing ECS with
a Godot-style object tree, adding a universal transaction/job/import kernel, or expanding future
3D/schema commitments. The existing
[pinned mechanism study](../research/2026-10-23-low-level-contract-mechanisms.md) remains background;
this pass makes no fresh comparative claim about current Bevy/Godot releases.

Recommended bounded order, if implementation is selected:

| Unit | Scope | Why | Required boundary evidence |
| --- | --- | --- | --- |
| R1 | PB1 owned scope and concrete completion consumers | Prevent the reachable Host panic without losing custody | Deterministic fault interleaving, owned-port controls, affected native Game View trajectory |
| R2 | PB2/PB4 Tasks disposal custody and outcome | Share the owner correction, not an overloaded completion boolean | Concurrent public pool probe, managed Stop result, close timeout/retry and return-observer controls |
| R3 | PB3 filesystem cursor ownership | Remove platform-dependent write location | Fresh Windows/Linux public capability and owned-lock tests; staging/save controls |
| R4 | PB5 post-flush Reflect admission | Restore typed ordinary-apply failure behavior | Queued first apply, binding-conflict controls, Scene/Prefab consumers |
| R5 | PB6 Image prepared association | Make caches consume owner-coherent data | Public/code-first consumers, UI planner, warm-cache GPU readback |
| R6 | PB7 durable recovery authority | Stop treating queue rejection as revocation | Complete removal/rejection/retry plus stale-write/metadata controls |
| E1 | Exclusive Scene access, after contract repairs | Reduce actual caller choreography | Two-level game and renamed public consumer; no performance claim without measurement |
| E2 | Explicit `bevy_ecs` entry, with E1 settled | Make the full dependency contract self-explanatory | Original type identity, adapted derives, full access and direct/root/renamed import coverage |

ADRs 0052, 0070, 0081 and 0033/0037 already own these responsibilities; implementation should clarify
their affected clauses where the chosen contract changes. Runtime lifecycle/Host scope decisions
must reflect R1/R2. Do not rewrite unrelated ADRs or mark these rows implemented before qualification.
Keep existing migration entries as requested, but add no release migration guide solely for this
unreleased review. Older O1-O6 follow-ups remain separate measurement/consumer triggers, not new defects.

## Verification Record

- Current-source FS probe: `cargo run --offline --manifest-path
  target/architecture-review-20261023/fs-cursor-probe/Cargo.toml --target-dir target`,
  `CARGO_BUILD_JOBS=1`, Rust 1.99.0. Exit 0 with the expected counterexample assertion:
  `reader_prefix=ab`, `actual=abXYef`, `write_continuation_changed=true`.
- App/Tasks protocol probe: `rustc +1.95.0 --edition=2024` with explicit cached App/Tasks rlibs and
  `-L dependency=target/debug/deps`, then execute `protocol-probe.exe`. Exit 0; outputs recorded in
  PB1/PB2. Initial linking with default Rust 1.99.0 failed with E0514, then selecting an unrelated
  Core feature artifact produced E0308. Matching Rust 1.95.0 and avoiding that explicit Core
  dependency succeeded; no cache cleaning or broad rebuild was performed.
- PB5 public consumer: `rustc +1.95.0 --edition=2024` with explicit cached Reflect/ECS rlibs and
  `-L dependency=target/debug/deps`, then execute `reflect-probe.exe`. Exit 0 after catching the
  expected public-apply panic; the queued target is written and the direct target is not. The final
  probe uses an existing ECS component and the matching dependency artifact, not an added product
  derive or modified registration interface.
- PB4/PB6/PB7 and the complete Game View failure remain source-confirmed, not newly executed.
- No implementation fix, full regression pass or new platform-support claim is included in this
  document. The recommendations are open work, not a completed refactor goal.
