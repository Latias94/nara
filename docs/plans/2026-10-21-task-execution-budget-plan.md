# Task Execution Budget And Dependency Qualification

## Outcome

Reduce demonstrated unnecessary task execution infrastructure while preserving Nara's existing
bounded admission, cancellation, result custody and physical shutdown contracts. Use real Editor
and game consumers to select changes. The [first baseline](../research/2026-10-21-task-execution-baseline.md)
observes 50 idle worker threads plus three custody threads in the two-level Windows desktop.
The inspected Bevy ECS profiles do not enable multi_threaded; two dependency names are not proof
of duplicate execution. No executor replacement or public async trait is selected by this plan.

The earlier persistent two-level consumer is qualified in its recorded Windows scope. Its
Linux failed-candidate cleanup limitation remains a separate filesystem contract issue.

## Ordered Work

### T1: Attribute Owners And Work

Capture actual thread creation/retirement for ordinary Editor Open, Play, Stop, repeated sessions
and simultaneous authoring/game owners, plus the exported headless and desktop game. Record
runtime multiplicity, task kinds actually used and completed/cancelled work. Distinguish Nara
workers, custody/reaper threads, ECS execution and graphics/platform threads.

Measure startup, idle CPU/committed memory, queue wait/completion and frame P99 under the same
representative Open/Save/Scene workload. Preserve actual success/close observations. Single-run
thread counts alone do not establish a performance regression or a universal thread cap.
Root owns serial Cargo and the shared target. Controlled cold-build experiments may use scratch
build state only when necessary for attribution; never clean the normal shared target.

Acceptance: a reproducible owner/workload table, exact feature/toolchain/profile context, normal
and held-work cancellation/close controls, and explicit limits. Use those observations to choose
one narrowly justified policy correction rather than tuning several unrelated variables together.

### T2: Correct Worker Creation And Allocation

Compare bounded concrete alternatives against T1: product-specific worker allocation, demand-based
worker construction under the existing pool owner, or a shared execution facility with separately
proven runtime custody. Start with the least complex alternative that removes the observed cost
and keeps real workload latency acceptable. Do not choose arbitrary caps from one host's core count.

If worker creation moves out of plugin construction, define where thread-spawn failure is reported,
what admission can do on the owner thread, and how partially constructed workers remain retained.
Do not trade lower idle counts for hidden blocking on the frame path. If execution becomes shared,
runtime close must still observe its own physical work return without claiming global workers joined.

Keep one authoritative task terminal state and close ledger. Extract touched private responsibilities
from the large `nara_tasks::runtime` module along configuration, task state, queue/execution and
retirement boundaries where this implementation needs them. Do not expose internal state or add
an executor trait merely to make the split compile; line count alone is not the acceptance criterion.

Acceptance: repeat T1, report before/after attribution, and retain queue pressure/coalescing,
first-terminal results, monotonic/exhausted IDs, cancellation races, panic isolation, construction
failure, unfinished Drop custody and retryable uncooperative-worker close tests. Complete actual
Open/Save/travel/fresh-process restore and Editor Stop/restart controls. Reject a change that saves
idle threads but weakens physical retirement or fails the ordinary workflow.

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

## Interface And Scope Constraints

World mutation, simulation commit and render publication remain synchronous owner operations.
Finite blocking preparation can remain on Nara workers with explicit request/result/commit states.
`AsyncCompute` currently accepts owned closures rather than Futures; do not advertise a Future
executor or make all traits async as part of this correction. A future genuine async I/O consumer
must establish its own wake, cancellation, affinity and shutdown needs before a new public seam.

No new universal task kernel, process-wide scheduler replacement, arbitrary executor adapter,
physics/audio service design, native hot reload or browser execution is included. Bevy reuse remains
an evidence-backed alternative under the custody contract, rather than an assumed destination.
