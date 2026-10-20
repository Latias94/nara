# Interface And Module Boundary Review

Date: 2026-10-19. Source baseline: `2713396`, before the U5/U6 migrations.
Two read-only research lanes examined public traits, asynchronous work boundaries and private
module responsibilities. These are source findings, not measured latency or throughput results.
The [correction plan](../plans/2026-10-19-1335-refactor-foundation-correction-plan.md) owns the work order.
The table and source locations below retain that historical baseline; the dated current disposition
at the end records subsequent measurement and implementation.

## Assessment

Keep the synchronous owner interfaces. A Future is appropriate for waiting on independent work;
it does not solve a transaction that must exclusively inspect or publish a World. Nara already
separates transportable definitions and results from thread-affine runtime owners. The useful
corrections are clearer task semantics, responsive preparation for measured slow operations,
removal of an unusable public extension trait, and smaller private modules around existing owners.

| Item | Evidence and consequence | Disposition |
| --- | --- | --- |
| I01: task execution semantics are easy to misread | `nara_tasks/src/runtime.rs:661-689,891-944,1396-1405` exposes cooperative cancellation and blocking closure jobs, including the name `AsyncCompute`. `Cancelled` may precede physical worker completion. Dropping `TaskHandle` does not cancel work. | G9: document these existing semantics before another task consumer adopts them. A true Future consumer must select explicit wait/waker, cancellation and drop/detach contracts. |
| I02: project-open stages can still block the UI owner | `runtime/editor/open.rs:107-153` calls bundle admission and startup closure preparation synchronously. `authoring/bundle.rs:60-75` reads/parses the bundle; `authoring/project.rs:686,1185-1214` traverses and parses the closure. Cancellation is processed after that call returns. | G9: measure per-phase native latency. Offload transportable reads and pure candidates if a phase exceeds the interaction budget; publish the session on its owner thread. This is a responsiveness gap without a measured current hitch. |
| I03: frame-driven persistence performs synchronous I/O | `runtime/editor/model.rs:23-66` advances pending Save/Reopen through synchronous encode/read/write/sync/replace in `editor/persistence.rs` and the persistence owner. Byte limits bound storage, not response time. | G9: measure large-scene and slow-storage Save/Reopen separately. Prepare owned results off-thread only when justified; preserve source identity, revision checks, uncertain publication and physical close ownership. |
| I04: public `ComponentCodec` has no installation route | The trait in `nara_reflect/src/codec.rs:288` is implemented only by private `FnComponentCodec`. Registry storage (`registry.rs:612`) wraps closure registration (`1271,1347`). Public exports exist in reflect root/prelude and the root facade, but no registry method accepts an external implementation. | U10: make the implementation trait private and remove those exports. Preserve the documented closure registration APIs, contexts and errors. Do not add a second registration route merely to retain the trait. |

The real manual codec consumer is
`tests/fixtures/module-consumer/tests/manual_component_codec.rs:50,69`; the
[persistent component guide](../guides/persistent-components.md) documents closures. I04 is public
interface clarity debt, not a data-safety failure or a demand for an extensible field-codec framework.

## Interfaces To Preserve

- `Plugin: Send + Sync` and repeatable factories carry definitions and configuration, while App
  and Runtime owners remain `!Send/!Sync`. Keep build/finish synchronous; transportable configuration
  does not authorize moving a World across threads.
- `RuntimeCloseParticipant` begin/poll callbacks run on the runtime owner thread. A participant can
  retain worker state between calls without borrowing a close context across an await.
- Component decode/encode and the owned runtime driver port are synchronous commit boundaries.
  Heavy I/O belongs before admission, with an owned result and retained rejection input.
- `PrefabSourceResolver` queries prepared snapshots and returns document borrows. Streaming should
  prepare the source closure upstream rather than turn that lookup into I/O.
- An Editor panel painter borrows one frame and its UI toolkit. An asynchronous panel job should
  live in the panel instance/Host state and expose a result projection to the painter.
- A general `RenderBackend` trait has no second implementation or complete consumer yet. Preserve
  immutable packets, resource snapshots and concrete WGPU ownership until a real replacement proves
  the common interface.

GPU initialization currently blocks on adapter/device creation. A cancellable loading screen or
another platform may justify a private pending-initialization state under ADR 0078; it does not
require making the whole Plugin lifecycle asynchronous. The ordinary ProductRecipe factory also
need not gain a generic fallible route just because its lower layer has one: U12 should first prove
typed invalid-configuration rejection and independent reconstructed state for its real defaults.

## Module Extraction Candidates

Line counts include inline tests and describe the reviewed baseline. Size alone does not justify
moving code; the suggested boundaries separate different ownership or validation responsibilities.

| File | Lines | Suggested private boundaries | Order |
| --- | ---: | --- | --- |
| `crates/nara_tasks/src/runtime.rs` | 3,479 | configuration/errors; task state/result; executor/queue; abandoned-owner reaper; plugin/close adapter | Before a second execution model or queue ownership change (G1/G9). |
| `crates/nara_app/src/runtime.rs` | 3,806 | fault bridge; close ledger; candidate admission; control; owner retirement | Extract when changing the relevant owner; avoid moving all state during U6/U7. |
| `crates/nara_reflect/src/registry.rs` | 2,969 | registry/owner merge; schema validation; value/reference coverage | Before a new reusable field-adapter API (G7); U10 may extract only the touched responsibility. |
| `crates/nara_tooling/src/workspace.rs` | 4,672 | document/source types; Scene/Prefab slots; persistence authority; selection/model projection | Before widening multi-document or panel authoring APIs. |
| `crates/nara_scene/src/patch.rs` | 3,055 | wire document/cost; validation; apply/inverse; entity placement helpers | Before Tile metadata/history extension (G8). |
| `src/project_host/runtime/editor.rs` | 3,219 | retain the existing submodule layout; reduce orchestration only with a concrete workflow | Lower priority; its persistence and image owners are already separated. |

The 7,338-line Runtime test module, 5,725-line Image tests, 5,395-line Scene tests and 4,054-line UI
adapter tests should split by contract domain when next edited substantially. Keep fixtures local,
preserve discovery and test names, and avoid a generic test framework. Pure moves need export/build
checks and the affected contract suites, rather than new tests that mirror the moves.

## Mature Engine Comparison And Gates

Pinned Bevy's `bevy_tasks/src/task_pool.rs:558-584` accepts Future jobs and distinguishes task
retention/detach semantics. Godot's `core/object/worker_thread_pool.h:343-405` exposes task identity
and completion; `core/io/resource_loader.h:249-251` separates threaded load request, status and
result. These show explicit waiting and result ownership, not a requirement to copy either kernel.
Nara's existing source-index attempt already advances by entry/directory budgets and publishes an
immutable Arc snapshot (`authoring/source_index/attempt.rs:278-322`, `source_index.rs:667-689`).

G9 should start with task API documentation and a chosen responsive-workflow measurement. A task
API change must preserve U2's logical supersession versus physical retirement: blindly changing
handle Drop to cancellation would alter existing Image behavior. A Future execution model requires
its own real consumer, not a speculative `async_trait` layer. G1 still owns measured admission and
capture-disposal work; G3 owns frame jobs; G6 owns residency/save semantics. Module extraction must
not create a second World owner, weaken candidate admission or expose new public seams.

## Current Correction Disposition — 20 October 2026

U10 made ComponentCodec crate-private and retained the documented closure registration methods,
contexts and errors in `b0648b6`. Independent manual-codec and data-only Vec2 consumers passed;
see [U10 evidence](../knowledge/engineering/verification/2026-10/2026-10-19-foundation-u10-closed-vec2.md)
and [later public-consumer results](../knowledge/engineering/verification/2026-10/2026-10-19-foundation-public-consumer-integration.md).
This closes I04's concrete interface correction. The baseline line counts above remain historical
extraction guidance, not current file-size promises.

- **I01:** task semantics now state blocking closure execution, cooperative cancellation and
  logical versus physical retirement. The standalone abandonment correction at `3b70e48` cancels
  the pending set before retained capture disposal. The [v1 investigation](../benchmarks/2026-10-20-editor-responsiveness.md)
  records the failing starting-source regression and passing correction. Rejected/coalesced capture
  disposal can still execute user destructors on the caller; measure that workload and queue
  pressure before promising bounded submission latency. No Future consumer or async trait is implied.
- **I02:** Project Open startup-closure preparation is now a measured trigger. At 4,096 empty
  entities, its owner-call P50 was 9.920 / 10.048 / 9.911 ms and P95 11.194 / 10.312 / 12.028 ms
  across the v1 processes. Owned bounded startup-closure preparation is the next selected slice
  after Host custody qualification; keep session/runtime construction and final publication on
  the owner thread. The 8 ms investigation trigger is not a native SLA. Browser document Open
  and source-receipt refresh remain separately unmeasured.
- **I03:** [Owned Save](../benchmarks/2026-10-20-owned-editor-save.md) and
  [Owned Reopen](../benchmarks/2026-10-20-owned-editor-reopen.md) are implemented and qualified
  within their linked scope. Workers own transportable preparation; original file authority,
  exact revision/source/schema/lane checks, native codecs/migrations and publication retain
  their appropriate owners. The lower owner-call measurements do not establish faster storage,
  lower total CPU, native UI latency or coverage of slow disks and complex native codecs.

Independent Reopen review found an adjacent pre-existing cleanup gap: after terminal failed Save,
operation-scoped custody no longer retained the returned Host if the session was dropped.
Full Scene/Prefab Host lifetime custody is implemented under the
[custody plan](../plans/2026-10-20-editor-host-lifetime-custody-plan.md), qualified at `fe3ad96`.
One retained lane owner now covers idle/failed operations, document and runtime replacement,
close and abandonment; first close rejects new file and Play admission before physical work retires.
It replaces operation-specific Save custody, without a public async framework or fake idle SaveWork.
Existing v1/v2/v3 timing data remains unchanged.

The [qualification record](../knowledge/engineering/verification/2026-10/2026-10-20T030524Z-editor-host-lifetime-cleanup-custody-qualification-12c6b025d6c74ed1bf6900c8b0413877.md)
records 125 selected root/public tests, 189 Editor/application tests and three selected actual
Windows/DX12 native workflows passing, together with the locked all-features/all-targets workspace
check. Counts overlap and do not establish full release or other-platform support.

The selected N/I reconciliation found no remaining unclosed confirmed correctness defect after
that custody follow-up. Task/GPU/aggregate-worker latency still needs
consumer measurement. Multiple documents, automatic code freshness, domain-field adapters and
Tile tools remain capability decisions, not evidence that synchronous owner traits are unsound.
Split private modules by their existing responsibility when those owners change; historical line
counts alone do not justify new crates or a test framework.
