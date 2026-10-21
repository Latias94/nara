# Task Execution Budget And Dependency Qualification

## Outcome

Reduce demonstrated unnecessary task execution infrastructure while preserving Nara's existing
bounded admission, cancellation, result custody and physical shutdown contracts. Use real Editor
and game consumers to select changes. The [baseline](../research/2026-10-21-task-execution-baseline.md)
observed 50 task workers plus three custody threads in one two-level Windows Editor process. The
fixed authoring runtime uses only its I/O queue, so this plan narrows that runtime's worker
allocation without changing the task API, game runtime, startup pool, renderer or ECS executor.
The inspected Bevy ECS profiles do not enable `multi_threaded`; two dependency names are not proof
of duplicate execution. No executor replacement or public async trait is selected by this plan.

The earlier persistent two-level consumer is qualified in its recorded Windows scope. Its
Linux failed-candidate cleanup limitation remains a separate filesystem contract issue.

## Ordered Work

### T1: Attribute Owners And Work

Trace the fixed Editor authoring consumers and their owning runtimes. Record actual task kinds,
completed/cancelled work, named worker and custody threads, idle process resource samples, and
normal versus held-work shutdown. Compare the same Open/Save/Reopen owner-workload probe before
and after changing the authoring pool. Distinguish Nara workers from custody/reaper, ECS,
graphics and platform threads.

The authoring I/O worker count remains unchanged and no production authoring consumer submits to
Compute or AsyncCompute. Therefore queue-wait P99 and GPU-completion frame P99 are not required to
justify this fixed-pool correction. If a later change alters queue scheduling, worker counts for a
used queue, or render cadence, add workload-specific wait/frame instrumentation before making a
latency claim. Do not infer these distributions from current queue age, owner-drive time or a
small nearest-rank sample. Root owns serial Cargo and the shared target; never clean the normal
shared target for this qualification.

Acceptance: a reproducible owner/workload comparison with exact feature/toolchain/profile context,
task completion and held-work cancellation/close controls, normal process exit, and explicit
measurement limits. Use those observations to choose one narrowly justified policy correction
rather than tuning several unrelated variables together.

### T2: Correct Worker Creation And Allocation

Compare bounded concrete alternatives against T1: product-specific worker allocation, demand-based
worker construction under the existing pool owner, or a shared execution facility with separately
proven runtime custody. The selected correction is a fixed 2/1/1 authoring pool because the fixed
authoring composition submits only I/O work and the before/after owner-workload probe shows no
material change at its measured resolution. Keep the I/O count at two; keep one worker in each
currently unused class to satisfy the task-pool contract without allocating machine-sized queues.
Do not generalize this allocation to game runtimes or arbitrary project plugins.

If worker creation moves out of plugin construction, define where thread-spawn failure is reported,
what admission can do on the owner thread, and how partially constructed workers remain retained.
Do not trade lower idle counts for hidden blocking on the frame path. If execution becomes shared,
runtime close must still observe its own physical work return without claiming global workers joined.

Keep one authoritative task terminal state and close ledger. Extract touched private responsibilities
from the large `nara_tasks::runtime` module along configuration, task state, queue/execution and
retirement boundaries where this implementation needs them. Do not expose internal state or add
an executor trait merely to make the split compile; line count alone is not the acceptance criterion.

Acceptance: repeat the scoped T1 comparison, report before/after attribution, and retain the
task-system regression suite covering queue pressure/coalescing, first-terminal results,
monotonic/exhausted IDs, cancellation races, panic isolation, construction failure, unfinished
Drop custody and retryable uncooperative-worker close. Exercise ordinary authoring Open/Save/Reopen,
held-work close and the existing supervised Play/last-good workflow. Reject a change that saves
idle threads but weakens physical retirement or fails these affected workflows. Broader travel,
fresh-process restore and repeated multi-owner thread matrices remain separate product workflow
qualification, not acceptance criteria for changing only the fixed authoring pool.

### T3: Trial Bevy Features Separately

Record enabled graphs for root, Editor, Reference Game, samples and independent consumers.
Trial one removable ECS feature at a time and retain only measured gains. ECS reflection bridge
removal changes prelude exports; Nara's direct reflection registry still needs bevy_reflect.
Disabling async_executor does not by itself remove bevy_tasks or Nara's workers. Backtrace removal
changes diagnostics. Do not enable multi_threaded as a dependency cleanup: scheduling parallelism
needs its own ordering, resource-budget and consumer qualification.

Acceptance: matched serial build/profile/cache conditions and executable-size attribution, current
MSRV/native/portable-wasm compilation, relevant reflection/derive/public API consumers and unchanged
managed fault behavior. Preserve explicit migration if exposed substrate types change. When a
feature has no material measured benefit or breaks a needed contract, retain it and record why.

## Execution Status

T1–T3 are complete for the fixed Editor authoring allocation and the inspected Bevy ECS feature
trial. The authoring pool is 2/1/1; Bevy defaults remain unchanged. The before/after Editor
owner-workload test, pool task counters, held-work cancellation/close test, task-system regression
cohort, focused native supervised Play test, feature-trial compile/test consumers and executable
size comparison are recorded in the [baseline and qualification report](../research/2026-10-21-task-execution-baseline.md)
and [verification evidence](../knowledge/engineering/verification/2026-10/2026-10-21T071613Z-editor-authoring-task-budget-and-bevy-feature-qualification-b9b5f36f37fd4db8858dfec9a08a3e33.md).

This scoped result does not establish admission-to-start queue-wait P99 or GPU-completion frame
P99. Those measurements need scheduler-boundary and native GPU-completion instrumentation. The
small 16-sample owner-workload probe is useful for a gross before/after regression check only.
Broader simultaneous runtime-owner and repeated project-session matrices remain separate
qualification work; they are not required to justify this authoring-only pool configuration.

## Interface And Scope Constraints

World mutation, simulation commit and render publication remain synchronous owner operations.
Finite blocking preparation can remain on Nara workers with explicit request/result/commit states.
`AsyncCompute` currently accepts owned closures rather than Futures; do not advertise a Future
executor or make all traits async as part of this correction. A future genuine async I/O consumer
must establish its own wake, cancellation, affinity and shutdown needs before a new public seam.

No new universal task kernel, process-wide scheduler replacement, arbitrary executor adapter,
physics/audio service design, native hot reload or browser execution is included. Bevy reuse remains
an evidence-backed alternative under the custody contract, rather than an assumed destination.
