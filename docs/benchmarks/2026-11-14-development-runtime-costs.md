# Desktop Development And Runtime Cost Attribution

Status: measured and qualified within the finite Windows scope below. The owning
[runtime reconstruction plan](../plans/2026-11-13-rust-state-reconstruction-plan.md#development-cost-attribution-and-measured-optimization)
retains the complete workflow and qualification limits. These observations extend the
continuous-game implementation at `a1490b9`/`ec57133`; they do not qualify complete R12.

The qualified implementation is `f1c2256a50a2e3b8e33b19e6db3b006ec346d5bd` for opt-in Host
timings and `f79c7da38178138dcb7dc3b68df36b887aa9d684` for the complete measurement consumers
and structural-fixture correction. Measurements were taken before those commits, from the same
implementation on base `9e42efa`; no production source changed after final qualification.

## Machine And Execution

The observed machine is Windows x64, OS build `10.0.26300`, Intel Core i9-13900KF with 32 logical
processors and NVIDIA GeForce RTX 4090. Adapter identity and actual surface extent are retained
per native run. The machine also exposes virtual-display adapters; it is not an isolated hardware
lab. No other workload, power or display configuration is controlled or changed.

Builds use Rust 1.95.0, locked/offline resolution, one Cargo build job and the existing root
`target`. Outer executables/tests use release; explicitly selected reconstructed children use dev.
The child's reviewed environment projection does not inherit parent graphics hints. Runtime
comparison uses hardware DX12 selected explicitly in the parent process; contained child backend
selection is reported separately. No Rust 1.100, cross-platform or clean-machine claim follows.

## Complete Rebuild Endpoint

An external source copy supplies actual no-edit, movement-body and private-Session-representation
cohorts. Each uses one excluded warmup and five retained samples in a separate Host, below the
existing eight-rebuild allowance. No-edit retained samples do not rewrite source. Body edits
alternate actual 160/80 movement implementations. Structural edits preserve speed 160 and
alternate the private held-intent representation, keeping public DTOs and the original movement
law unchanged.

The measured outer interval starts after source writes, before request/preparation. It includes
complete-source checks, public Cargo build and captured executable validation, installation,
probe startup, full-state restoration, three semantic probe steps with complete comparisons
and presentation checks, actual probe retirement, fresh-adopter restoration/presentation, final
source/dual-state/presentation revalidation, route adoption, a new-logic exact Step while predecessor
cleanup is Pending, native activation and Resume through two naturally completed ticks.
Predecessor retirement is timed separately. Setup, source writes, later re-isolation/reset and
final experiment shutdown are outside the outer interval; their original cleanup checks remain.

This endpoint includes deliberate qualification and input reconciliation. It is not compiler time
alone, arbitrary Rust hot patching, first photons, or a zero-interruption Editor Play loop.
Fixed per-Host stage aggregates are opt-in. Their counts and total elapsed durations are differenced
between attempt boundaries; lifetime maxima are labeled cumulative. Restore, probe and adoption
spans include child phases, so stage totals overlap and cannot be summed or percentile-subtracted
to infer an unmeasured cost. No generic trace callback or scheduler is added.

Nextest `9d957f4c-11f9-48e3-8d26-bfb83a2feddc` passes all 18 complete workflows in 469.399 seconds,
including warmups, nested builds, setup and final cleanup. The three excluded warmups and fifteen
retained samples remain in the raw record. Retained seconds:

| Workflow | Complete correct-running median | Range | Prepare median | Adopt median | Predecessor cleanup median |
| --- | ---: | ---: | ---: | ---: | ---: |
| No edit | 14.7706 | 14.4900–15.0020 | 12.1582 | 0.9770 | 0.3829 |
| Body edit | 23.7901 | 22.9331–24.4715 | 20.9952 | 1.1153 | 0.3595 |
| Private structure edit | 25.4273 | 25.0128–30.8045 | 22.6199 | 1.0651 | 0.3961 |

Prepare and Adopt are contained in the complete outer interval. Predecessor cleanup follows that
endpoint and is separate. The recorded original process receipts require normal game Close,
successful native exit, empty containment, stopped readers, drained output and no fallback kill.
This is independent of whether timing is enabled.

Selected inclusive per-attempt phase medians, in seconds:

| Host phase | Calls per attempt | No edit | Body | Structure |
| --- | ---: | ---: | ---: | ---: |
| Cargo adapter, including capture/validation | 1 | 3.7280 | 12.1573 | 14.0347 |
| Original child startup through READY | 2 | 3.1516 | 3.1923 | 3.1760 |
| Three complete probe steps | 3 | 2.2324 | 2.2383 | 2.2343 |
| Exact canonical presentation confirmations | 6 | 1.9871 | 2.0088 | 2.0481 |
| Complete state capture | 13 | 1.8132 | 1.9305 | 1.9573 |
| Fresh candidate state restoration | 2 | 1.1033 | 1.0794 | 1.0843 |
| Source fence capture/bind/revalidation | 6 | 0.6069 | 0.7043 | 0.5364 |
| Executable installation | 1 | 0.1083 | 0.0977 | 0.1135 |

Probe/restore/adoption contain some capture/presentation work: these rows cannot be added as a
partition of the outer interval. In particular, this table does not subtract medians to claim an
unmeasured residual or identify raw compiler execution time from the Cargo adapter's elapsed call.
Installation is consistently a small fraction of the complete workflow, so an installation cache
is not selected from this evidence. The meaningful next attribution is inside Cargo admission/
commands/capture and native child/control turnaround, retaining the complete qualification endpoint.

## Runtime Observer Comparison

`runtime_cost` alternates plain/witness, witness/plain, plain/witness using the same ordinary
game, backend, actual window size, native pacing, neutral input, telemetry and measurement observer.
Each original Runtime/window/device is physically retired before the original event loop is
recovered for the next run. Warmup excludes 30 successful presented frames per run; each run then
retains 120 exact successful frames. Both groups validate unchanged game semantics. Every witness
sample must additionally match its exact canonical Seal and original receipt.

Frame wall time is original `FrameExecutionStart.started_at` through post-Host observation time.
It includes App, driver/native Host and submission waits; measurement-observer work after that
endpoint is excluded. GPU-completion spans independently retain original App/render/device
identity and a complete telemetry seal. Neither metric is pure CPU execution, GPU execution or
scanout. The three run medians, rather than 360 correlated frames treated as independent trials,
are the comparison unit.

The initial comparison completes all six runs and native cleanups. Plain run medians are
33.2443, 33.2638 and 33.2776 ms; witness run medians are 33.2772, 33.2749 and 33.2447 ms. Their
median-of-run-medians is 33.2638 versus 33.2749 ms, with overlapping ranges. This paced wall metric
does not establish a CPU regression or justify replacing the complete state witness.

A measurement-only `cost-measurement` feature therefore adds read-only current-process CPU and
memory observations through the already locked Windows SDK package. Counters are sampled at the
first and last measured post-Host callbacks: 120 frames produce 119 observed intervals. Whole-
process CPU includes all threads, observer/poll/background/driver work; it can differ from elapsed
wall time and is not attributable to a single UI subsystem. Working-set/private values are
whole-process point observations. Sequential runs share loaded code and driver allocations, so
their memory values are not per-UI retained bytes or evidence of a leak.

The final six-run comparison additionally compares actual Schema, level source/content stamps
and initial time policy. The recipe configuration fingerprints are recorded separately because
adding the witness is the intended recipe difference. Its read-only process observations report:

| Run | Recipe | Process CPU | Process observation wall | Final private committed memory |
| --- | --- | ---: | ---: | ---: |
| 1 | Plain | 140.625 ms | 3.9667291 s | 489.30 MiB |
| 2 | Witness | 140.625 ms | 3.9671069 s | 484.11 MiB |
| 3 | Witness | 140.625 ms | 3.9665315 s | 484.63 MiB |
| 4 | Plain | 140.625 ms | 3.9665684 s | 496.82 MiB |
| 5 | Plain | 109.375 ms | 3.9665457 s | 499.57 MiB |
| 6 | Witness | 125.000 ms | 3.9666361 s | 486.42 MiB |

Plain CPU median/range is 140.625 / 109.375–140.625 ms. Witness is
140.625 / 125.000–140.625 ms. Values occur in 15.625 ms increments on this run; overlapping
variation and that quantization prevent claiming a stable additional witness cost. The observation
contains telemetry and the measuring consumer in both recipes; it is not a measurement of an
uninstrumented shipping process.

Each 300 ms paused-idle window records actual poll calls and App frames without requesting Step,
maintenance or manual delta. The initial comparison observes ten App frames, five poll calls
and zero simulation/logical ticks in every run. This establishes retained paused execution with
render/presentation work, not a CPU-free or no-redraw idle policy. Changing idle cadence needs
its own original-owner demand/resize/input/telemetry acceptance.

## Qualification, Integrity And Remaining Scope

The timing controls pass 4/4 in run `723c5d41-4e4f-423d-9116-0c88e0d4c1cd`. Final ordinary
package tests pass 107/107 in `f497ce8a-b3ee-49fb-b072-55240553b71f`, with the two actual-build
cases kept separate. Desktop-feature library/child/CLI CPU targets pass 80/80 in
`3d76ee66-b791-4c45-835b-1ff35b86dc65`. Existing default-disabled headless A/B/C and production
adoption pass 2/2 in `6947afa2-efe6-4762-a439-2dfa11ec540b`; complete continuous native
gameplay/adoption passes 1/1 in `d8d55286-601b-43ba-8c76-56447a931271`.

Correctness Clippy passes all default and all-feature sample targets, including the opt-in
measurement executable. Root/sample formatting, affected manifest/lock parsing, raw-record
validation and local document link checks pass. Existing style/dead-code warnings remain outside
that gate. These cohorts overlap and are not an additive independent test count. Independent
timing/measurement, native comparison and Windows counter reviews find no remaining P1/P2; all
five scoped threads are closed. Actual success paths and original cleanup are exercised, not
injected external focus theft, counter API failure or native cleanup timeout.

An initial rebuild run reaches the structural cohort and fails an inherited helper assertion
that required a prior 80-speed body edit. It is excluded from final cost evidence. The owning
helper now checks preservation of either original movement implementation, allowing structure-
only changes without mixing in a body edit. Independent review confirms the correction. The
consumer also records actual `debug_assertions` rather than mislabeling any arbitrary test build
as release.

The runtime executable includes ordinary game startup messages on stdout before its single
structured result. Raw stdout remains distinct from the normalized, uniquely selected JSON
record for the early exploration. The final runner supports an explicit absolute **new**
`NARA_RUNTIME_COST_OUTPUT` file and writes its structured record directly after all original
cleanup, without overwriting an existing file. Parsing failure of the earlier whole stdout
stream is not a native comparison failure or grounds to discard a successful receipt/cleanup
observation.

Raw observations are [workflow samples](data/2026-11-14-development-runtime/workflow-baseline.json),
[final runtime/process samples](data/2026-11-14-development-runtime/runtime-process-baseline.json)
and the clearly labeled [earlier wall-span exploration](data/2026-11-14-development-runtime/runtime-wall-exploratory.json).
[Measurement metadata](data/2026-11-14-development-runtime/measurement.json) retains commands,
feature/profile scope and data digests. Digests cover repository blob bytes with LF line endings,
following the existing root `.gitattributes`; this newline normalization changes no JSON values.
Final integrity checks compare the staged blobs as well as decoded observations, so Windows
working-copy newline conversion cannot silently invalidate the published digests.
Those initial baseline and observer records are not a before/after optimization comparison.
The subsequent paused-control change has its own matched observations below.

Representative larger content, uninstrumented production CPU costs, controlled memory retention,
other graphics backends/platforms, measured idle-policy improvements and reliable latency tails
remain open. Complete state/source/presentation checks, physical retirement and the separate
supervised Editor Play protocol are preserved. UI and mixed-DPI work remain paused.

## Paused Control Turnaround Successor

Implementation revisions are `d11a8c0b1e73c4f717d070f3b14c0a03b7468f4b` for the six Cargo
segments/public control consumer and `ebbc1e630df4fb2d77663b40bfacae4e65d45273` for the child
wait change. Runs used the same production source before those commits. Consumer wording was
later clarified to distinguish API-internal validation from caller-side checks, and a local
binding was renamed; neither changes the measured operations or retained observation values.

The original child called `poll_live` before every quiescence check, including when its original
healthy complete paused boundary was already eligible for capture. The successor checks that
same original authority first and polls only when it is incomplete. Startup still drives a
NotStarted Runtime before checking pause; each loop checks the deadline and paused state.
Maintenance still invalidates the previous boundary, native ingress is still polled before
request reception, and confirmation still requires a fresh original presentation. No state
cache, transport replacement, weaker validation or alternate native owner is introduced.

The new ordinary `development_turnaround` consumer builds and starts a real child, collects both
levels' pickups and markers, travels, releases movement and retains a complete canonical paused
basis. It rotates capture, execution status and fresh presentation within that one hidden,
input-isolated process. Each operation has one excluded warmup and ten retained requests.
Before and after use the same consumer source digest, actual tools, target, features and
complete canonical state/status. The only changed declared child input is
`src/bin/continuity/desktop.rs`. Both original processes close normally, without fallback, before
their source fixtures are removed and result files are written.

| Complete public API call | Before median | After median | After range |
| --- | ---: | ---: | ---: |
| Capture | 135.344 ms | 60.965 ms | 59.273–76.799 ms |
| Execution status | 70.822 ms | 66.817 ms | 39.238–78.546 ms |
| Fresh canonical presentation | 461.789 ms | 268.541 ms | 253.535–301.732 ms |

Before run `d2233cb8-6cf6-4d28-849f-086faff672d0` passes 1/1 in 40.114 seconds; after run
`6940ede9-029d-495c-9c95-3152af29e309` passes 1/1 in 32.721 seconds. Captured canonical bytes,
logical clock and complete execution status are identical between the two measured processes.
Actual returned receipts identify a discrete Vulkan adapter; the contained child's environment
does not inherit the parent's DX12 hint. These are complete API wall spans, including API-internal
validation, encoding, file transport and native waits. Only caller-side validation/report
assembly, startup, preparation and cleanup are excluded from the per-call timer.

The observed capture and fresh-presentation medians decrease by about 55% and 42% respectively.
Execution-status samples overlap and do not establish a separate status-query improvement.
Ten correlated requests in one process per variant do not establish independent-process
distributions, stable tails, CPU savings or scanout latency. Shared root dependencies are not
snapshotted by the child inventory; HEAD/status and the explicit outer build command describe
this local qualification envelope, rather than an isolated source or hardware experiment.

Raw [before control observations](data/2026-11-14-development-runtime/control-before.json) and
[after control observations](data/2026-11-14-development-runtime/control-after.json) retain all
samples, exact original responses, tool/input provenance and physical cleanup. The six new
Cargo checkpoint segments remain within the original `CargoBuild` total, including adjacent
validation work. Their complete rebuild observations and regression qualification follow the
same correct-running endpoint as the initial baseline.

### Complete Rebuild And Cargo Segments

Run `3aa881d4-719a-4692-baa1-d26040df8cfa` passes all 18 complete workflows in 419.557 seconds.
It retains the same three warmups/fifteen measured attempts, probes, fresh-adopter restoration,
final source/state/presentation validation, changed-logic Step, native activation, naturally
resumed ticks and normal original cleanup. No-edit still returns one identical executable digest
across its six builds; body and structural cohorts each produce six different executable digests.
Every recorded phase has zero returned failures.

| Workflow | Initial correct-running median | Successor median | Successor range | Successor Prepare | Successor Adopt |
| --- | ---: | ---: | ---: | ---: | ---: |
| No edit | 14.7706 s | 12.6964 s | 12.5336–16.6993 s | 10.9609 s | 0.6861 s |
| Body edit | 23.7901 s | 20.9315 s | 19.1596–24.8939 s | 19.1911 s | 0.6588 s |
| Private structure edit | 25.4273 s | 23.1441 s | 22.3167–24.3183 s | 21.4156 s | 0.6521 s |

Predecessor cleanup remains outside the correct-running interval, with successor medians
0.3245/0.3917/0.3691 seconds. The complete workflow's before record predates six-segment Cargo
instrumentation; the successor includes that additional optional observation. Unlike the matched
control pair, this is a sequential local comparison with varying compilation cost, caches and
uncontrolled background work. Overlapping ranges and five retained samples do not establish a
stable end-to-end speedup or a latency-tail budget.

Successor per-attempt Cargo segment medians, in seconds:

| Original adapter interval | No edit | Body | Structure |
| --- | ---: | ---: | ---: |
| Entry through BeforeMetadata: admission/tool observation | 0.2412 | 0.2049 | 0.1724 |
| BeforeMetadata through BeforeBuild: metadata and rechecks | 1.5414 | 1.2933 | 1.3158 |
| BeforeBuild through AfterBuild: compilation, checks, decoding and artifact hash | 1.9607 | 10.4497 | 12.2498 |
| AfterBuild through BeforeCapture: capture admission | 0.0542 | 0.0551 | 0.0791 |
| BeforeCapture through BeforeReturn: bytes/provenance | 0.0427 | 0.0458 | 0.0671 |
| BeforeReturn through return: final validation | 0.0976 | 0.0953 | 0.1198 |
| Complete CargoBuild total | 3.8238 | 12.0092 | 14.2737 |

These six intervals partition each individual adapter call; their separately computed medians
must not be added to reconstruct a median total, and the total must not be added to its segments.
The compile interval is not raw rustc time. Source/tool checks, Cargo itself, decoding and the
artifact hash remain included. No source, tool, lease or executable revalidation was removed.

Original two-child startup remains about 3.15–3.20 seconds. Thirteen complete captures now
take median totals 1.1086/1.0549/1.0867 seconds, six original presentation confirmations
1.2359/1.2267/1.2266 seconds, and three complete probe steps 1.4342/1.4361/1.4270 seconds.
Those parent/child phases overlap. The attribution supports retaining the small quiescence-wait
change and focusing subsequent work on metadata/rechecks and original child startup, rather than
adding executable-installation or witness caches.

The [successor workflow samples](data/2026-11-14-development-runtime/workflow-after.json) retain
all phase counts, elapsed values, source-edit classes and cleanup results. Complete platform,
Editor, hard compiler cancellation and representative production budgets remain outside this
bounded qualification.

### Successor Qualification

| Scope | Actual result |
| --- | --- |
| Initial desktop-feature library, child and CLI CPU targets | 83/83, one ignored native consumer, run `ea652361-9e7d-4981-a54c-95fef7694569`, 8.201 seconds. Includes all seven timing tests. |
| Matched before/after control requests | Each 1/1, runs `d2233cb8-6cf6-4d28-849f-086faff672d0` and `6940ede9-029d-495c-9c95-3152af29e309`. Each retains thirty measured requests and three warmups. |
| Complete successor native rebuild/adoption | 1/1, run `3aa881d4-719a-4692-baa1-d26040df8cfa`, 419.557 seconds; eighteen actual workflows. |
| Original continuous and paused native adoption | 2/2, run `9561f554-5dbd-4663-9a11-1b0fdf421b20`, 206.580 seconds. Includes actual input, stale/refused candidate custody, restored presentation and Running Close. |
| Final all-feature ordinary sample | 113/113, eight ignored hardware/actual-build consumers separate, run `fdc5fbf1-2350-43a6-82e9-4a721a62264e`, 11.255 seconds. |
| Final default-feature ordinary sample | 110/110, two ignored actual-build consumers separate, run `ba2c2509-4fbc-4077-89bf-c05ace5e905e`, 10.658 seconds. |

All-target/all-feature correctness Clippy and sample formatting pass. Existing style/dead-code
warnings remain. Two independent bounded reviews cover the actual wait change and Cargo/control
observation contract, with no remaining P1/P2. All four scoped agents are closed. The native
process/file owners retain their earlier qualification; unrelated Editor, UI and complete
framework matrices are not repeated. No shipping CPU saving, general cached-state mechanism,
foreign native execution or full R12 completion follows from these results.

## Declaration-Only Cargo Preparation Successor

The qualified implementation is `215ef548c7871b69baa871057bb6382ebed4cc00`. The independent
input-adapter integration correction is `eec5f0079009e35373d3d4cec500c9f1cd565b93`.
Measurements precede these commits; the recorded production source is unchanged by committing.

The synchronous finite build owner needs a selected workspace package/bin declaration before
execution, then exact compiler-artifact binding and immutable executable capture. It does not
consume the full dependency graph or metadata's workspace feature union. Full
`CargoMetadataFacts` remains a strict graph observation for its existing consumers; the new
`CargoBuildInvocation::from_metadata_declarations` instead requires explicit `resolve: null`
from `cargo metadata --no-deps`. Both use the same bounded package/target selectors and original
artifact decoder. Invocation metadata features are explicitly optional: unavailable differs from
an observed empty set. Cargo still owns dependency-qualified target eligibility and actual
compiled features. No graph cache, dependency resolver or process owner is added.

An alternating command-only experiment uses the installed Rust 1.95.0 Cargo, the same actual
Two-Level Game manifest, desktop features, native target, locked/offline environment, jobs one
and shared root target. Each mode has one excluded warmup and five retained commands. The
selected package ID, manifest, complete target declaration, workspace root and output directory
are identical across all twelve outputs.

| Cargo metadata mode | Retained median | Retained range | Output bytes |
| --- | ---: | ---: | ---: |
| Full dependency graph | 1063.828 ms | 988.259–1265.279 ms | 1,316,920 |
| Declarations with `--no-deps` | 72.328 ms | 70.445–76.059 ms | 7,044 |

This is command elapsed time and output size, not complete build/capture latency or compiler CPU.
It uses the current source workspace, without physically isolated dependencies or controlled
hardware/background load. The [raw command observations](data/2026-11-14-development-runtime/metadata-command-exploration.json)
retain exact arguments and selected output facts. The complete workflow, independent actual
Cargo consumer and affected build-facts regressions supply separate qualification below.

The finite adapter changes only metadata acquisition/preparation. Existing checkpoints,
source/tool/output leases, required-feature/profile/output artifact matching, compilation
diagnostics and incomplete-cleanup custody remain. The supervised Editor still obtains full
metadata. Its invocation digest now distinguishes unavailable from observed-empty workspace
features while preserving the previous full-observation encoding.

### Complete Declaration-Prepared Workflow

Run `c4051bab-a36c-4066-b4e2-6b5d2f5ab907` passes all eighteen workflows in 424.893 seconds.
The exact consumer, six Cargo timing intervals, native placement, source-edit classes and complete
correct-running endpoint are unchanged from `workflow-after.json`. Each class retains five
measurements after its warmup; every phase has zero returned failures. No-edit still yields one
executable digest across six builds; the body and private-structure classes each yield six.
Original probe retirement, fresh adopter, final source/state/presentation revalidation,
post-adoption Step, activation, naturally resumed execution and normal native cleanup all remain.

| Retained median | No edit | Body | Private structure |
| --- | ---: | ---: | ---: |
| Complete correct-running, previous full-metadata adapter | 12.6964 s | 20.9315 s | 23.1441 s |
| Complete correct-running, declaration adapter | 11.7132 s | 21.8066 s | 22.0341 s |
| Declaration-adapter complete range | 11.4283–15.9017 s | 20.3299–22.7349 s | 21.5621–22.1121 s |
| Prepare | 9.9907 s | 19.9302 s | 20.1119 s |
| Adopt | 0.6632 s | 0.7556 s | 0.7361 s |
| Separate predecessor retirement | 0.3481 s | 0.4130 s | 0.3880 s |
| Previous metadata/rechecks segment | 1.5414 s | 1.2933 s | 1.3158 s |
| Declaration metadata/rechecks segment | 0.1685 s | 0.1586 s | 0.1464 s |
| Previous compilation/checks/decoding/artifact segment | 1.9607 s | 10.4497 s | 12.2498 s |
| Declaration-adapter compilation/checks/decoding/artifact segment | 2.2332 s | 12.4603 s | 12.4625 s |
| Declaration-adapter complete CargoBuild | 2.9563 s | 13.0643 s | 13.0381 s |

The same metadata/rechecks interval drops by roughly 1.1–1.4 seconds per build. These intervals
include source/tool validation around the original command, so their complete-workflow values
are distinct from the command-only 72 ms observation. No source/tool/lease check was deleted.
Compilation variation offsets that reduction in the body cohort, whose complete median is
slower on this run. The combined test duration also includes initial compilation, warmups,
preparation and cleanup. Neither the total run nor the three complete medians establish a stable
overall speedup, a CPU budget or a latency tail; do not sum independent phase medians.

The [declaration workflow observations](data/2026-11-14-development-runtime/workflow-declarations.json)
retain all eighteen samples. Original two-child startup remains about 3.16–3.20 seconds.
Source inspection finds no justified startup deletion: the boundary includes namespace setup,
exact executable admission, native creation and waiting for initialized READY. Existing neutral
input installation and its maintenance have real original-owner preconditions. Further startup
work should first distinguish those intervals, without treating READY as exact restored
presentation or weakening launch/cleanup authority.

### Declaration Preparation Verification

The root Cargo facts/native owner selection passes 44/44 in
`16fc6ee6-f6a0-4236-b9c7-d610e03073c6`, 1.773 seconds, with 846 cases outside the focused
filter. The independent declaration/decoder API passes 3/3 in
`d836b769-8f37-4419-989c-b1518b8483f3`, 0.090 seconds. Actual native and foreign Cargo consumers
pass 2/2 in `963fd950-f547-4c68-bf3f-9f73ed8e3d17`, 157.590 seconds. They retain custom
package/bin/profile selection, expanded/dependency-qualified features, deliberately wider full
metadata, failed compilation diagnostics, stale-source rejection, revocation, immutable A after
failed B, source-removed native Player execution and build-only WASI bytes. No foreign Nara
Runtime execution follows from that target build.

The affected Editor CPU compilation initially fails because an existing UI timeline match omits
the runtime's `InputEventKind::KeyRestored`. Its documented logical hold admission has no
physical Press/Started edge. The adapter now explicitly passes it through without synthesizing
UI navigation or activation. This integration correction preserves the existing route and input
owners and does not resume UI feature development. The initial failure is not counted as
successful Editor qualification; its affected rerun has separate results.

After the logical-input match correction, the affected Editor build-facts cohort passes 11/11
in `7ca6cf6e-0a1c-4884-b18b-2c895aa6085e`, 1.244 seconds, with 214 cases outside the filter.
Full metadata still requires its selected resolve node, actual compiler features remain separate,
and the invocation digest distinguishes unobserved from resolved-empty features while preserving
the original encoding of full observations. No native Editor or UI product qualification is
claimed by these CPU build tests.

The affected UI library passes 124/124 in `1a0b57af-a851-44b8-869e-7fc848b33e69`, 4.299 seconds,
after the explicit restored-input integration correction. Root `project-tooling` library and
independent all-target correctness Clippy pass; affected root/Editor/UI and independent workspace
formatting pass. Existing style/dead-code warnings remain. No manifest/lock dependency versions
change. Final document checks cover added targets/fragments, raw observation consistency and
repository-byte SHA256 digests.

Independent architecture and actual-diff correctness reviews find no remaining P1/P2. Four
scoped agents are closed. The previous startup investigation remains a source-grounded next
attribution, not a measured native-spawn/GPU bottleneck or an authorization to delete checks.
All Cargo/native observations are terminal. Complete R12, representative latency tails, hard
compiler interruption, foreign Nara execution and paused Editor/UI work remain open.

## Original Child Startup Attribution

The qualified implementation is `ed4cd866c133dcedfdd8178e1ca15ab035c77657`.

The existing game process owner now observes namespace preparation, exact executable launch
authority, native spawn and initialized READY wait separately within `ChildStartup`. These
default-disabled elapsed observations preserve original operation order, errors, startup deadline
and post-spawn custody. A private prepared-launch value retains the original control directory,
namespace lease, command and identities until they enter the original process. No alternative
process, input, device or trace owner is introduced. Disabled observation adds no timing clock
reads; the process's pre-existing deadlines still use their original clock.

The public `development_startup` consumer builds explicit headless and desktop executables once,
then alternates six rounds of fresh independent Hosts/processes. The first process per placement
is an excluded warmup; five are retained. Each original process installs the exact captured
executable, reaches initialized READY, produces complete canonical state with zero progressed
ticks and neutral input, then retires normally. Canonical bytes are equal within each placement;
the two placements have deliberately distinct feature/recipe/configuration observations.
Desktop additionally confirms a fresh original non-CPU presentation with the exact state digest
and original frame/window/device identities after the startup timer ends.

Run `15323d4f-4bc7-4f8c-a9f7-a73e6cbf9f06` passes 1/1 in 81.953 seconds, including both actual
builds, twelve successful starts, later assertions and cleanup, plus a separate failure control.
Initial build work is excluded from per-start timing. Outer code uses release and captured
children use dev with the recorded actual Rust 1.95.0 tools, locked/offline resolution and jobs
one. The contained desktop reports a discrete Vulkan adapter, independently of the parent DX12
environment hint.

| Parent-observed elapsed boundary | Headless median | Headless range | Desktop median | Desktop range |
| --- | ---: | ---: | ---: | ---: |
| Complete public Host start | 242.078 ms | 238.991–272.646 ms | 1780.243 ms | 1653.903–1822.121 ms |
| Executable installation | 79.547 ms | 70.433–91.299 ms | 132.720 ms | 88.935–146.471 ms |
| ChildStartup | 145.984 ms | 142.926–156.933 ms | 1595.177 ms | 1456.503–1694.717 ms |
| Namespace/control preparation | 4.437 ms | 4.180–5.062 ms | 5.099 ms | 4.157–7.199 ms |
| Exact launch authority | 23.813 ms | 17.786–28.758 ms | 34.087 ms | 24.364–40.941 ms |
| Original native spawn | 30.522 ms | 27.412–34.248 ms | 34.682 ms | 33.279–39.715 ms |
| Initialized READY wait | 89.252 ms | 81.936–96.447 ms | 1518.218 ms | 1377.181–1631.502 ms |

The timing snapshot is copied immediately after `host.start` returns, before later capture,
status, presentation and cleanup. That complete API includes executable installation,
`ChildStartup` and an immediate post-READY observation. Four named child operations fit inside
the original startup total with small unmeasured construction gaps; they are not an exact
partition. Independently computed medians must not be summed to reconstruct a parent median.
READY wait covers child loader/startup, source/Runtime/native/GPU work, response transport and
parent polling after native admission. It is not GPU initialization alone or native spawn time.

An excluded thirteenth process uses the actual captured headless executable with malformed
`nara.toml`. Native spawn succeeds, the child exits before READY and both `ChildReadyWait` and
`ChildStartup` record failure while earlier phases remain successful. The original candidate
receipt records non-successful process exit, no successful game Close, empty containment,
stopped readers and drained output; it is not relabeled as a normal success. Cleanup completes
without fallback termination, the manifest is restored, and the source fixture is removed before
the new output file is created.

The [raw startup observations](data/2026-11-14-development-runtime/startup-attribution.json)
retain tool/input/executable provenance, all sample times/counts, complete per-placement canonical
bases, original presentation responses and successful/failing cleanup facts. Five independent
processes per placement improve on repeated requests in one child, but share a machine, loaded
driver/code caches and sequential source/build preparation. They are not controlled cold starts,
stable tails, foreign-platform qualification or a before/after optimization comparison.

The observation places most desktop startup inside the broad initialized READY wait, rather than
namespace preparation or native creation. No startup optimization is selected from this parent
attribution alone. The next useful measurement separates original child source/Runtime startup,
first native/GPU drive and neutral-input maintenance; it must preserve exact launch, full paused
frame eligibility, separate restored presentation and normal original retirement.

### Startup Attribution Qualification

| Scope | Actual result |
| --- | --- |
| Desktop-feature library, child and CLI CPU controls | 83/83, run `1a4a8738-2e4c-42bf-bd74-34958f1557b1`, 7.348 seconds; one ignored native consumer separate. |
| Independent original startup measurement/failure | 1/1, run `15323d4f-4bc7-4f8c-a9f7-a73e6cbf9f06`, 81.953 seconds, twelve successful original processes plus one excluded actual failure. |
| Original continuous/paused desktop rebuild adoption | 2/2, run `45251983-2d33-459f-a861-7f69ea75d224`, 202.622 seconds; original complete state/source/presentation/input, stale refusal and Running Close assertions retained. |
| Final default-disabled ordinary sample | 110/110, run `94a9563c-edda-40c9-b41a-c869aff01e7e`, 11.101 seconds; two actual-build consumers remain separate. |

All-target/all-feature correctness Clippy, sample formatting and the final startup-consumer
compile check pass. Existing warnings remain. Independent process-diff and measurement reviews
find no remaining P1/P2. The latter identifies a P3 in test-only panic cleanup: after successful
physical cleanup, a failed measurement assertion still retained its source fixture. The corrected
consumer now drops the Host/fixture before propagating that original panic; only cleanup failure
retains them. The invalid-manifest restoration stays inside the proven cleanup path.

The raw successful measurement retains its pre-correction consumer source digest and unchanged
operation/response/cleanup values. The latest test-only panic branch is source-reviewed and
compiled, not claimed to have been exercised by injected panic. No production process code
changes after its native measurements. All three scoped agents close, all owned executions reach
terminal observations, and no unrelated UI/native framework matrices are repeated. Full R12,
representative cold/tail/CPU budgets and paused Editor/UI work remain open.

## Original Desktop Startup Operations

The qualified implementation and ordinary consumer are
`11c1a6733bfc1b466de637da1ccde108c301c28d`. Final native measurements use that production source;
no production code changes after the final six-run observation.

The optional `DesktopRunOptions::with_startup_timings()` observes original content preparation,
Runtime startup completion, native session construction, first native poll and accumulated native
polling through the first actual presentation. `DesktopStartupTimings` returns completed elapsed
observations through the public desktop action. Ordinary game and reconstructed-child options
leave it disabled, with no startup-instrumentation clock reads. This API adds no game protocol,
process, input, native window or render owner.

The polling aggregate uses the original `WgpuPresentedFrameReceipt`. It sums complete native
poll calls through the first receipt, including the remainder of that poll and excluding caller
gaps. It overlaps the first-poll span. It is absent before presentation and remains unavailable
if blocking execution interrupts the observation; a whole blocking game lifetime is never
reported as startup. Completed observations do not extend during subsequent controls, frames
or cleanup. A returned source-preparation span can describe a failure, so timing values alone
do not grant readiness, successful startup or a complete interactive outcome.

An earlier four-span exploration passed the selected CPU/public tests 26/26 in
`22aed45a-1f11-4d37-90b5-9aa1355c0311` and all six complete native runtime comparisons. It
exposed the need for the aggregate: the first run spent 699.092 ms in its initial native poll,
while the five later runs returned from that poll in approximately 0.002–0.004 ms despite
initial settlement taking 666–916 ms. The original physical event loop is recovered between
runs, so a fast first return does not identify the amount or completion of initialization.
That exploratory record is not the five-span successor's qualification or a speedup comparison.

The existing `runtime_cost` consumer now copies the completed snapshot after neutral keyboard
restoration, original paused maintenance and canonical capture, before reveal, input activation,
Resume and warmup. Each already-completed observation must agree with its earlier initialized
paused snapshot. The same final snapshot must survive the original measured frames, telemetry
seal and physical event-loop recovery. Initial settlement and neutral-maintenance wall spans
remain separate public-call observations; they overlap internal spans and cannot be summed
with them. All original frame, canonical-witness, process-counter and retirement checks remain.

The final six-run release consumer exits successfully on the recorded hardware DX12 RTX 4090
at 960 by 540. Each run retains 30 excluded warmup frames and 120 exact accepted samples.
Every witness sample matches its canonical Seal and original receipt, all six completed startup
snapshots remain unchanged through cleanup, paused idle advances no simulation or logical ticks,
and all six original Runtime/native/telemetry owners retire before physical-loop recovery.

| Run | Recipe | Content | Runtime | Session construction | First native poll | Native polls through first presentation | Initial settle | Neutral maintenance |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | Plain | 10.069 ms | 4.310 ms | 101.053 ms | 708.712 ms | 708.712 ms | 824.173 ms | 28.088 ms |
| 2 | Witness | 39.273 ms | 4.969 ms | 0.004 ms | 0.003 ms | 1043.260 ms | 1087.528 ms | 32.729 ms |
| 3 | Witness | 9.210 ms | 2.963 ms | 0.004 ms | 0.003 ms | 621.888 ms | 634.077 ms | 28.413 ms |
| 4 | Plain | 10.659 ms | 3.164 ms | 0.005 ms | 0.003 ms | 768.033 ms | 781.877 ms | 22.878 ms |
| 5 | Plain | 9.763 ms | 3.118 ms | 0.005 ms | 0.004 ms | 741.466 ms | 754.371 ms | 21.413 ms |
| 6 | Witness | 15.942 ms | 5.152 ms | 0.006 ms | 0.004 ms | 765.473 ms | 786.588 ms | 31.194 ms |

The [complete raw record](data/2026-11-14-development-runtime/runtime-startup-operations.json)
retains all five durations, public-call spans, exact frame associations, workload, actual adapter,
process counters, completion samples and cleanup facts. The runs share a process, physical event
loop, loaded code and driver caches; they are not independent cold starts or representative tail
samples. The first run also constructs the physical loop. Later fast session construction and
first polls do not imply that the new window, device or first-frame work is absent.

Native polling remains the dominant observed direct-call span. Further attribution should
distinguish the original window/device/resource/first-frame operations before choosing a cache,
retention or pipeline change. The aggregate includes waits and the presenting poll's remainder;
it is not pure GPU initialization, GPU execution, scanout or an exact presentation timestamp.
The prior contained-child observation uses dev and Vulkan and includes loader/protocol/parent
work, whereas this direct consumer uses release and DX12. Neither set can be subtracted from
the other to infer an unmeasured residual. No startup optimization or performance budget is
selected from these observations alone.

### Desktop Startup Qualification

| Scope | Actual result |
| --- | --- |
| Final selected root desktop/presentation and public controls | 28/28, run `a085eacd-15bb-4d08-ba5d-7e2f527a2d9d`, 2.836 seconds; 654 cases outside the filter. Includes default-disabled and code-first construction, missing/failed preparation, pre-start Close, original observer veto/cleanup, interrupted accumulation and unchanged public snapshots while incomplete. |
| Actual five-span runtime consumer | Six complete release/DX12 runs, each with 30 excluded warmup frames and 120 exact accepted frames, both recipe variants, full canonical/telemetry and physical cleanup checks; normal exit. |
| Original default-disabled continuous desktop rebuild/adoption | 1/1, run `50d3eb09-54ef-4608-88e2-9bbd94ae521c`, 129.347 seconds including nested actual Cargo. Function-body and private-structure edits, full restoration, native input, stale refusal, Pending-time execution and Running Close retain their original assertions. |
| Minimal code-first desktop feature closure | Locked/offline release library check passes with only `desktop-winit,render-wgpu`, without runtime-2d or serde. |

The independent review first catches the existing presentation-test fixture's missing state and
tuple fields after the report extension; the fixture is corrected without weakening its original
assertions. A subsequent P2 identifies unpublished accumulation leaking through the public
snapshot's derived equality and debug output. The corrected private observer retains that
accumulation, while the public Copy/Eq/Debug value contains only completed phases. An additional
non-presenting poll now explicitly preserves snapshot equality before the first real receipt
publishes the full sum. The driver also latches the first receipt once rather than repeatedly
writing the same diagnostic on later frames. Final source rereview finds no remaining P1/P2.

Selected root-library correctness Clippy and all-target/all-feature sample correctness Clippy
pass, alongside affected root and sample formatting. Existing style/dead-code warnings remain.
All ten raw-data SHA256 values are checked against canonical LF bytes, including staged blobs.
Three scoped agents in this unit are closed, and all owned Cargo/native executions are terminal.
The selected Windows/Rust 1.95.0 evidence does not qualify foreign execution, independent cold
starts, numerical CPU/GPU/tail budgets, cross-domain trace-viewer capture, complete R12 or the
paused Editor/UI/mixed-DPI work.

## First Presentation And Original GPU Calls

Qualified consumer: `42b4eefb6050a304530d3fb0b3b2f312cf81e351`.
Qualified renderer: `44725e0a4affe527788e1a89abd50baeaed3b27b`.
Final native observations use these production sources; no production code changes after them.

The ordinary runtime consumer additionally retains its first actual presentation as a separate
record. The same original App/render/device transaction checks validate this receipt before it
is saved once; the 30-frame running warmup and 120 measured running frames remain independent.
Its frame-start-to-post-Host wall span includes App, Host, driver work and waits. It is neither
GPU execution nor a timestamp of scanout. The first six-run
[baseline](data/2026-11-14-development-runtime/first-presentation-baseline.json) completes all
original canonical, frame, telemetry and cleanup checks. All first receipts name App/render
frame 1, the original window and device epoch 1; original Runtime and backend identities differ
between runs as expected. First-frame spans are 650.534–1430.304 ms while corresponding native
poll totals are 657.198–1440.734 ms. These separate spans are not an exact additive partition.

Source inspection confirms that a recovered Winit loop must drain old events before a new
Session receives them. That first handoff poll can return without constructing the next window.
Window construction has its original one-window guard, and reveal uses the same target.
No evidence justifies deleting handoff isolation. A Windows pump can also reach multiple
`about_to_wait` service boundaries, so one poll is not necessarily one managed frame.

The WGPU stock resource constructor does not eagerly create all Pipelines: actual captured keys
select cache misses. The sample needs its stock quad draw, 320 by 180 Canvas resolve and final
960 by 540 Surface presentation. The latter two have different responsibilities. Skipping an
actual required pass would not preserve the measured workload.

An explicitly temporary source-local experiment measures four original calls in each of the
same six serial runs: Instance creation, Adapter request, Device request and full stock quad
Pipeline creation, including its shader/layout/error-scope work. There is one event of each
kind per run. Output is associated through the original serial run boundaries, not a new
Runtime, device or trace owner. The [instrumented raw record](data/2026-11-14-development-runtime/first-presentation-probes.json)
retains all ordinary frame/cleanup checks and the 24 operation observations. The
[exact source delta](data/2026-11-14-development-runtime/first-presentation-probe-source.json)
retains its base, complete owned probe/consumer patch and patch SHA256. Unrelated paused WIP is
preserved; this is not an isolated complete-checkout source snapshot.

| Run | Recipe | Instance | Adapter request | Device request | Quad Pipeline | First actual App-frame span |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 1 | Plain | 46.028 ms | 523.847 ms | 60.413 ms | 11.742 ms | 664.806 ms |
| 2 | Witness | 68.183 ms | 588.197 ms | 73.275 ms | 13.969 ms | 774.540 ms |
| 3 | Witness | 87.298 ms | 740.125 ms | 61.465 ms | 11.575 ms | 928.093 ms |
| 4 | Plain | 67.493 ms | 669.933 ms | 71.168 ms | 13.849 ms | 854.609 ms |
| 5 | Plain | 64.056 ms | 648.711 ms | 67.174 ms | 11.536 ms | 822.874 ms |
| 6 | Witness | 90.505 ms | 658.959 ms | 68.522 ms | 14.807 ms | 862.533 ms |

Adapter request is the largest measured original call. The operation spans include driver and
thread waits; they do not identify a hardware-execution, DXGI, virtual-adapter or compiler
subcomponent. Printing is outside each operation's bracket but inside the whole-frame span,
so the instrumented run is not an uninstrumented CPU benchmark. Surface creation/configuration,
Canvas/final presentation, uploads and other work are not individually attributed by these four
probes. All probes are removed after this experiment, with no permanent environment flag,
instrumentation clock or logging added to the production renderer.

Two source-proven redundancies have bounded owner-local corrections: material layout preparation
is required only for actual material-extension use; Surface configuration uses one capabilities
snapshot instead of querying it again inside WGPU's `get_default_config`. The latter preserves
the pinned WGPU defaults, current requested-mode fallback and sRGB view-format behavior. Neither
operation's individual cost is measured here, and these corrections do not establish a cure
for the dominant Adapter request. Original shader validation, allocation/error scopes, device/
epoch caches, target admission and physical retirement remain required. No new global device,
cross-process cache, renderer or ownership policy is selected from these measurements.

The source findings are grounded in the existing
[Winit Session/pump](../../crates/nara_winit/src/lib.rs),
[Device owner](../../crates/nara_render_wgpu/src/device.rs),
[stock resources](../../crates/nara_render_wgpu/src/stock_quad.rs),
[quad realization](../../crates/nara_render_wgpu/src/quad.rs) and
[Surface configuration](../../crates/nara_render_wgpu/src/surface.rs). WGPU's default-config
comparison uses the locally locked `cddb2f5d80dc0009b2ef6767635140cacf90a007` implementation,
not an assumption about current upstream defaults. Other source findings retain the experiment's
base revision in its exact source-delta artifact.

### First-Presentation Qualification

| Scope | Actual result |
| --- | --- |
| Affected stock/quad/material/Surface/UI paths | 24/24, run `02ad471f-9223-45ea-bf12-1e2e8083c1d2`, 15.156 seconds; 443 cases outside the filter. Includes actual device-backed image/material/UI readbacks and the two new finite Surface-capability controls. |
| Final uninstrumented ordinary consumer | Six complete release/DX12 runs with the original first receipt, 30 running warmup and 120 accepted running frames each, exact witness/neutral-state/telemetry and physical cleanup; normal exit. |
| Original continuous and paused desktop rebuild/adoption | 2/2, run `d593e489-f515-4cf5-87d8-cb75b67cce7d`, 227.734 seconds including actual nested builds. Continuous native input/body/structure adoption passes in 119.711 seconds; exact paused restore/presentation/adoption passes in 108.023 seconds. Original incumbent failures, stale source/state refusal, Pending-time execution, activation and physical cleanup assertions remain. |
| Feature closure and static checks | Locked/offline release renderer library check passes without stock features. Renderer and sample all-target/all-feature correctness Clippy and affected formatting pass; existing warnings remain. |

The [final record](data/2026-11-14-development-runtime/first-presentation-final.json) observes
first-frame spans of 701.214, 728.648, 801.142, 747.566, 720.508 and 656.714 ms. Corresponding
native poll totals are 731.708, 736.971, 810.167, 756.519, 729.574 and 665.413 ms. Every first
receipt retains the same Runtime/window/surface/backend/epoch as that run's later measured
frames, and the actual adapter/extent agree. These sequential ranges overlap earlier runs and
share driver/code caches; they do not establish a stable improvement from either small correction.

The material trajectory strengthens the original permanent-pipeline-rejection oracle on one
device/resource set. A real ordinary blue draw leaves the material-layout failure injection
unconsumed. The first custom material attempts that allocation and truthfully returns the
injected retryable failure, without losing the ordinary cache or retaining instance charges.
After retry, exact green/red/black pixel checks exercise auxiliary texture and parameter effects;
ordinary/custom mixed draws reuse the original layout and pipelines, and later frames avoid
reuploading the exact retained image versions. Original permanent shader refusal, clear, budget
release and replacement-device revalidation remain. Generic readback helpers still accept mixed
materials; the private argument migration does not reduce their capability.

Independent Surface/consumer and subsequent material-diff reviews find no P1/P2. Default Surface
configuration semantics are checked against the actual locked WGPU source. No performance gain
is inferred from the removed allocation/query counts. The platform evidence remains Windows,
Rust 1.95.0 and the recorded hardware adapter; complete R12, representative larger games,
stable cold/tail budgets, foreign native execution and paused Editor/UI/DPI work remain open.

All fourteen raw-file SHA256 values and the temporary patch linkage are checked against canonical
repository LF bytes, including staged blobs. Added local document targets/fragments and ordinary
whitespace checks pass. All four scoped investigation/implementation/review agents are closed;
all owned Cargo and native executions are terminal. No remote publication or branch/main movement
occurs. The next startup investigation should locate the original Adapter request's internal
cost and compare any proposed policy or reuse against exact target, failure, device and complete
rebuild/continued-gameplay behavior before selecting a production change.

### Adapter Request Source Follow-up

At Nara `8e646f2`, read-only inspection of the locked WGPU
`cddb2f5d80dc0009b2ef6767635140cacf90a007` finds no justified additional Nara optimization.
The native `wgpu/src/backend/wgpu_core.rs` request performs core adapter selection before
returning a ready future. Core `instance.rs` enumerates/exposes the backend candidates before
applying power-preference sorting. Moving only future polling or changing preference therefore
does not remove that earlier work.

The DX12 HAL's `instance.rs` ignores the Surface hint while enumerating. Its DXGI factory
enumerates candidates and `adapter.rs::expose` creates a native D3D12 device for each surviving
candidate, queries driver/PCI information and probes capabilities. The PCI helper separately
enumerates Windows display-device information. Later `Adapter::open` reuses that native device
and prepares queue/fence resources. Consequently, the separately measured WGPU Device request
is not a measurement of all native device initialization. These are source-proven operation
boundaries, not individually measured contributors to the 524–740 ms Adapter request.

Nara's healthy Ready path performs no repeated selection; ordinary window and image entry
points share the original device owner. The pinned public request/backend options offer no
equivalent early candidate filter or switch that simply omits PCI/capability work.
`WGPU_ADAPTER_NAME` belongs to a separate WGPU utility helper, is not selected by Nara's current
request path and itself starts with full enumeration. This investigation does not change
fallback, Surface compatibility, validation flags, device selection or native ownership, and
does not select a global/cross-process cache. Further internal cost attribution needs its own
evidence; unrelated runtime delivery should continue without a speculative adapter rewrite.

## Host-Composed Execution Capture

The independent [Editor panel consumer](../../tests/fixtures/module-consumer/README.md#scoped-execution-capture)
now composes an opt-in scoped tracing Layer around actual authoring work and the original
Two-Level Game desktop product. No Editor frontend opens. The ordinary source-import worker,
prepared-edit owner, `DesktopRun`, telemetry seal and native cleanup remain authoritative.
The [complete raw record](data/2026-11-14-development-runtime/host-capture-results.json)
contains eleven serial Windows/Rust 1.95.0 release runs: excluded disabled/enabled warmups,
three alternating disabled/enabled pairs, then overflow, collection failure and export refusal.
All runs use the same actual discrete DX12 adapter, vendor 4318/device 9860 and 960 by 540 target.

Each run imports a real text asset with a controlled 10 ms hold inside the original Io worker,
preserves source and metadata identity, commits a valid property value of 23, and rejects the
original stale basis without changing complete authoring state or Undo/Redo history. Actual
Undo/Redo and the original Session/OpenHost Close complete. The hold deliberately creates a
visible wait; it does not identify an ordinary production importer bottleneck. The desktop
half captures the full canonical paused game DTO, retains twelve exact original presentations
and completion samples, seals original telemetry and completes target destruction and physical
loop recovery. The authoring and game projects are separate consumers composed by this Host;
this does not implement Editor Play or an integrated profiler UI.

The [enabled trace](data/2026-11-14-development-runtime/host-capture-enabled.json), retained
from run 4, imports successfully into official Perfetto `trace_processor_shell` v59.1-1e9c61e98.
The [actual query record](data/2026-11-14-development-runtime/host-capture-viewer.json) retains
the tool hash, exact SQL and parser output: 115 intervals on 115 capture-local observation
tracks, one held import, one `stale_revision` refusal, twelve presented/completed frame pairs,
zero missing parent correlations and zero parser errors. The import lifetime is 17.8764 ms,
including its 11.7597 ms validation child and actual worker/publication retirement. App-start
to GPU-complete spans range from 2.8352 to 679.5499 ms. They retain distinct Runtime, App-frame,
render-frame, window, Surface generation, backend and device-epoch fields. These intervals
include CPU work, submission and waits; they are neither GPU execution nor scanout. A track
identifies a retained operation, not an invented OS thread. Explicit parent arguments preserve
the asynchronous relationships without forcing crossing lifetimes onto one synchronous stack.

### Capture Cost And Failure Controls

| Retained pair | Disabled elapsed | Enabled elapsed | Enabled minus disabled |
| --- | ---: | ---: | ---: |
| Runs 3 / 4 | 1025.061 ms | 992.902 ms | -32.158 ms |
| Runs 6 / 5 | 1010.910 ms | 1093.840 ms | +82.930 ms |
| Runs 7 / 8 | 1039.458 ms | 1022.333 ms | -17.125 ms |

Disabled and enabled means are 1025.143 and 1036.359 ms respectively, a difference of
11.216 ms (1.09%). This is a finite sequential whole-workflow comparison with shared process,
code/driver caches and recovered native loop, not isolated collector cost, a stable overhead
estimate, a speedup, a cold-start distribution or a latency-tail budget. Product comparisons
exclude these times and require identical complete state, authoring outcomes, adapter and extent.

The enabled collector reserves 1,698,304 bytes in two preallocated Vec payloads for at most
4096 retained events and 64 active spans. Observed occupied high-water payload is 45,304–49,384
bytes across normal enabled/export-refusal runs. Registry, Arc, allocator metadata and process
RSS are excluded. Normal enabled runs retain 111–121 events with zero drops, active spans,
rejected fields or faults after cleanup. Disabled capture reserves zero payload bytes and
retains zero events. The
[overflow control](data/2026-11-14-development-runtime/host-capture-overflow.json) reserves
832 bytes, retains one event, and reports 24 dropped events plus 90 dropped spans. The
[collection-failure control](data/2026-11-14-development-runtime/host-capture-failed.json)
reports one fault and an incomplete capture. Export refusal preserves the original existing
file, returns `AlreadyExists` only after product cleanup, and then marks capture incomplete.
All three controls preserve the same original product state, results and physical cleanup.

### Capture Corrections And Scope

The first consumer compile corrected private-API and observer-borrow mistakes. An initial
collector test assumed full Registry ID reuse within a fixed number of spans; its oracle now
checks our actual contract: unique capture IDs and correct retained parent lifetimes. The
native-loop handoff can return before a new paused full frame exists. The consumer now waits
for a successful original paused capture before requesting maintenance, preserving the native
handoff and public maintenance rules. No native-owner workaround is introduced.

An actual enabled run then reported one missing span despite ample capacity and valid fields;
its [incomplete trace](data/2026-11-14-development-runtime/host-capture-contention-before.json)
is retained. A focused four-thread regression reproduces that loss and the resulting failed
close bookkeeping with the original global `try_lock`. The private collector now serializes
only bounded primitive bookkeeping with a short mutex; visitation and parent lookup happen
outside it, and no product callback or I/O runs under it. The same 256 span lifetimes and 512
records now retain every update and parent with zero loss. Buffer exhaustion and capture faults
still produce explicit incomplete observations. Mutex wait has no hard bound: no lock-free,
nonblocking or real-time guarantee is claimed. Export remains synchronous after original
physical cleanup. Source review also corrects prefix-based metadata acceptance to an exact
whitelist and gives crossing operation lifetimes separate bounded viewer tracks.

Selected public/collector regressions pass 34/34, run
`66ecadf1-8e06-412a-be06-8e9099e768d3`, with one unrelated ignored test. All eleven native runs
complete with identical original product facts and actual physical cleanup. The additional
bounded concurrency review finds no P1/P2. The affected import-lifecycle and settings-transition
cohort passes 10/10 (`8b309ff4-a07b-40b8-8202-9e06873b5973`), with 1359 other cases outside the
filter. Its actual root feature selection is `runtime-2d,serde,tooling,editor-work-observation`;
the first narrower command selected no tests and is not accepted as test evidence. Feature-off
root checking, feature-on root and independent-consumer correctness Clippy, affected formatting,
new local targets, JSON and raw-record hashes pass. Existing warnings remain.
This unit does not establish foreign native execution, game-scale performance budgets,
per-pass GPU timings, Editor frontend integration or completion of full R12. Editor/UI,
Dialogue UI and physical mixed-DPI work remain paused.

## Explicit Paused Native Cadence

Qualified implementation revisions are `ef67473ed71786cf98e405b489f0ef1dbc808ec0` for the
original runtime interface and `8b8954cd93417caed64cf645a7206b7fedec3dc5` for the game/consumer
integration. Measurements ran the same production code before those local commits; the
structural fixture correction has its separate failed-attempt and final-rerun record below.

The successor starts from `27ea617` and compares the existing continuous Poll policy with
explicit Wait in the same release executable. This is a matched current-policy comparison,
not a CPU comparison against the historical v2 record. The original WinitSession and DesktopRun
now accept live policy changes. Wait retains pending frame demand, presentation maintenance
and the original elapsed-time baseline; selecting Poll from Wait requests original frame work.
Runtime Pause itself remains independent of cadence, and the default stays Poll.

The ordinary `runtime_cost` consumer retains its three alternating plain/witness pairs, thirty
excluded running frames and 120 sampled successful frames per run. Format v3 adds paired
300 ms paused Poll/Wait intervals in alternating order, each following 100 ms excluded settling.
Each interval compares complete `encode_resume` bytes, original App/fixed/logical counters,
unique actual presentation receipts, poll calls and whole-process CPU counters. The preselected
acceptance criterion is at least an 80% aggregate paused App-frame reduction with unchanged
canonical bytes and zero simulation/logical ticks. A rejected reduction gate retains the
complete report after original cleanup and exits unsuccessfully; it does not discard a result
that disagrees with the hypothesis. Cooperative overruns remain visible in actual wall spans.

Separate controls request original Wait maintenance, exactly one fixed Step plus a complete
paused frame, then Poll Resume/Pause and continued gameplay. They remain outside the idle
intervals and running-frame samples. Every original run must still complete telemetry sealing,
native target destruction, Runtime retirement and physical event-loop recovery.

The source-desktop continuity child selects Wait for its known paused inventory and Poll for
continuous play. Review identified that the game's internal reconstruction loop previously
only polled: under Wait, completed asynchronous Scene work could remain unintegrated. Its
original `restore_desktop_resume` helper now explicitly requests each required full paused
maintenance frame until reconstruction finishes. No new task wake, clock, scheduler or process
owner is added. The recipe does not install a source watcher or require animated paused
real-time shaders. Arbitrary added services still need explicit App work or Poll.

The original owner CPU cohort passes 87/87 (`d0bf3498-ff4b-4392-aee8-ae4b4f1ad125`), including
six new switch/refusal/deadline/demand/elapsed controls. Actual native cadence and existing
maintenance trajectories pass 2/2 (`4f19ba71-294e-49c2-a013-8d22d1326f11`). They prove original
window/pump demand and retirement, independently of GPU presentation. The native cadence test
uses a 10 ms caller polling cap and a 20 ms maintenance deadline; its deadline counter alone
does not prove native timer causation or a power saving.

### Matched GPU Observations

All six release runs complete on Windows 11, RTX 4090, driver `32.0.16.2114`, DX12 and a
960 × 540 surface. They share the original physical event loop and process/driver caches.
All twelve idle intervals retain identical complete canonical bytes and zero simulation/logical
ticks. Every Poll interval produces ten App frames and ten actual presentations; every Wait
interval produces zero of each. Aggregate App-frame and presentation counts change from 60
to zero, passing the preselected frame-reduction gate.

| Run | Recipe | Idle order | Poll wall / Wait wall (ms) | Poll / Wait poll calls | Poll / Wait process CPU (ms) |
| --- | --- | --- | ---: | ---: | ---: |
| 1 | Plain | Poll, Wait | 332.931 / 300.594 | 5 / 123 | 0 / 15.625 |
| 2 | Witness | Wait, Poll | 333.044 / 301.900 | 5 / 123 | 15.625 / 0 |
| 3 | Witness | Poll, Wait | 332.810 / 300.985 | 5 / 123 | 15.625 / 31.250 |
| 4 | Plain | Wait, Poll | 333.972 / 300.331 | 5 / 123 | 15.625 / 15.625 |
| 5 | Plain | Poll, Wait | 333.060 / 300.491 | 5 / 120 | 15.625 / 0 |
| 6 | Witness | Wait, Poll | 333.322 / 302.461 | 5 / 124 | 0 / 0 |

Each run separately presents after explicit Wait maintenance, advances exactly one canonical
fixed/logical tick through Step and complete maintenance, then resumes ordinary gameplay for
two additional simulation/logical ticks before Pause. All original telemetry seals, surface
retirements, provider releases, target destructions and physical event-loop recoveries complete.
The running workload still samples 120 actual presentations after thirty excluded warmup frames.

These observations select explicit Wait for this known paused child inventory. They demonstrate
suppressed demand-free App/presentation work, not a stable CPU or power improvement: CPU samples
overlap and are quantized; the observer still services 120–124 polls under Wait. Poll's cooperative
FIFO waits also extend its nominal 300 ms intervals to about 333 ms. No claim compares equal
whole-process CPU durations, GPU execution time, scanout or representative game tails.
The [raw record](data/2026-11-14-development-runtime/paused-cadence.json) and
[measurement metadata](data/2026-11-14-development-runtime/paused-cadence-metadata.json)
retain all current observations, conditions and the LF-normalized data digest.

The first complete desktop attempt (`4a8088d7-100d-4990-85e1-8f18b6b869e6`) reaches the
private-structure edit and fails both tests in their source fixture, before the C candidate
build. The previous shipping-Player work added `player_launch` before `held` in private Session;
the fixture incorrectly required `held` to be the first field. It now bounds the existing
private Session declaration, replaces only its exact `held` field and preserves every other
field, attribute and public DTO check. The finite textual edit remains a known fixture, not
a Rust parser. That failed attempt is excluded from final adoption acceptance and does not
invalidate the separate matched GPU observations.

The corrected full desktop rerun passes 2/2 (`565c699b-d665-4e25-8ebd-ece4df9dd413`): original
paused adoption and continuous native-input gameplay both complete real body and private-
structure rebuilds, fresh reconstruction/probes, exact adoption and original physical cleanup.
The continuous child reports Vulkan; the parent's DX12 environment hint is not inherited as
child GPU authority. The separate six-run in-process measurement above reports DX12 itself.
The known fixture correction is independently reviewed without a remaining P1/P2. These actual
builds use their existing dev-profile children inside the release nextest harness; neither
the recorded build wall times nor that setup qualify compiler, runtime or memory performance.

The final sample CPU selection is 149/149 (`65619666-42d7-4a01-b2f7-403ac6f29065`) under
`desktop,cost-measurement`, with nine actual-build/native cases kept separate. Runtime tests,
native tests and the ordinary GPU comparison cover different and partially overlapping paths;
their counts are not summed into an independent product total. The sole production compile
correction was an import from `nara::prelude` to the existing `nara::winit` re-export.
Arbitrary delayed-worker/focus-theft/driver-fault injections, foreign native execution,
uninstrumented CPU or power budgets, shipping Player pause policy and full R12 remain open.

Final release correctness Clippy passes Winit's all-feature/all-target owner, the root desktop
Runtime library closure and all sample desktop/cost targets. Root and independent sample
formatting, new relative document targets, JSON decoding and the LF-normalized raw-record
digest pass. Existing lint/style warnings remain; this is not a warning-free release claim.
All nine scoped threads close with no remaining reported P1/P2. No unrelated Editor/framework
or source-free Player delivery matrix is repeated for this explicit cadence change.

## Observer Deadlines And Submitted-Work Maintenance

Qualified revisions are `47490b1cf67de2b71b318077fb5a38bb7c670feb` for the original runtime/GPU
contract and independent-consumer migration, and `bf92633885eeff53cd0632efd7ef8d5b157a5ad9`
for the ordinary measurement consumer. Execution ran the same source before these local commits.

This successor starts from `6a665ab`, after the explicit cadence measurement above. The original
desktop driver still arranged a 2 ms wake for any installed observer, even when the observer
only recorded presentation facts. The previous idle caller independently capped every poll at
2 ms, so those results did not isolate owner-scheduled maintenance from caller polling.

The revised contract separates these causes. `Continue` requests no observer timer;
`WakeAt(absolute_deadline)` requests an observation without requesting an App frame. Native
events and other owner deadlines may deliver it earlier; the next callback supplies the current
request. Original first-present/READY and telemetry seal retain their deadlines. Original WGPU
submission-prefix custody arranges maintenance independently of any observer or telemetry
configuration, including completed prefixes awaiting owner bookkeeping. Telemetry sample and
resource counters do not replace that custody. Host cache cleanup runs again after completion
releases retained owners, before that pass can report no remaining submission work.

This query does not promise all caches are empty or arrange arbitrary future cache-lifetime,
World or Task wakes. The private root Host does not escape to observers and retains its
original producer through native retirement. Existing cache maintenance remains at original
admission/maintenance/retirement boundaries, and producer/device-loss interaction revocation
retains its synchronous owner. Source review found no Accepted requirement for an additional
idle cache-drop wake or a wall-clock device-loss detection deadline. No generic notifier,
background GPU polling thread or replacement scheduler is added.

Five finite source consumers now return their original absolute observer deadline while
waiting: stock Runtime composition, Heatmap, temporal History, source/processed content capture
and Material source watching. Their actual drawing, source, history, failure, seal and cleanup
assertions are preserved. Other observers whose callers own polling or which only record
facts retain Continue. This is a behavior/API migration; the obsolete implicit observer timer
is removed rather than retained as a second policy.

The ordinary comparison format advances to v4. Each of six runs retains the original running
plain/witness workload and compares three 300 ms paused conditions after 100 ms excluded
settling: Poll, Wait with explicit periodic 2 ms observer requests, and Wait with no observer
deadline. Order reverses on alternating runs. Callers allow each interval's remaining time as
the native wait cap. The periodic condition reproduces the former observer behavior through
the explicit interface. Complete canonical bytes, App/fixed/logical counters, actual presentation
receipts, actual observer callbacks, native poll calls, process CPU and owner telemetry facts
remain separate observations. Acceptance requires both Wait conditions to retain the prior
80% App-frame reduction, and deadline-free Wait to reduce callbacks and native polls by at
least 90% relative to periodic Wait. Rejected gates preserve the full report after cleanup.

An additional control arms an original absolute 100 ms observer request through a zero-wait
maintenance boundary, then allows a 1 s caller wait. Its first original callback at or after
the deadline must be observed within 250 ms without changing App/presentation/game state.
This is a cooperative finite-workload gate, not a guarantee that callbacks or native APIs can
be interrupted. Existing maintenance, exact Step, Resume/Pause and all original telemetry/native
retirement checks remain. Final execution observations and qualification follow below.

The production desktop library check passes. The selected root presentation/first-present/
control cohort passes 27/27 (`db654a36-d228-4515-aff8-6d447b15fce6`), with 955 other tests
outside the filter. Six new owner tests cover quiet Continue/no observer, absolute deadline
merging, withdrawal, already-expired requests, final veto and the original non-renewed seal.
They verify unchanged Runtime/frame/time facts without claiming native GPU execution.
Independent production and measurement source reviews report no remaining P1/P2; their
conclusions do not replace the GPU/native execution results below. The release test build
took 23m54s in this shared checkout and feature closure; that compilation duration is not
runtime cost evidence or a controlled compiler comparison.

The affected Host/submission/telemetry CPU cohort passes 45/45
(`89d0b2ea-8aef-4c7e-81d8-990f69bf623d`), with 459 other/native cases outside that selection.
The real DX12 adapter cohort passes 17/17 (`464b5c78-73f3-4927-bd11-cad554c3baee`), including
four new maintenance tests plus original submission-abandonment and publication controls.
Actual program resources settle with telemetry off and on; upload refusal, callback panic
and native command validation failure retain their error and settle through the original
prefix, followed by independent byte readback. A prefix retaining the final producer closes
it in its real completion callback, and the same maintenance pass reclaims the resulting
closed owner before reporting no submitted work. No extra queue submission proves completion.

The isolated GPU tests manually register telemetry on their actual submissions to inspect
its exact identity/sample behavior; they do not qualify full Runtime presentation by themselves.
Their small workloads may already complete before the first observation, so no GPU-busy-duration
or deterministic pending-state claim follows. The complete ordinary native comparison supplies
the separate Runtime/presentation/deadline evidence. Independent test source review finds no
remaining P1/P2. CI now selects the new original maintenance tests on software DX12; that local
configuration is not a claim of remote CI execution.

### Three-Condition Native Results

All six v4 release runs pass on the same reported Windows 11/RTX4090/DX12 adapter, driver
`32.0.16.2114`, and 960 × 540 surface as the earlier cadence comparison. Each run retains
120 successful running samples after thirty excluded warmup frames, then executes all three
idle conditions, the separate deadline control, maintenance/Step/Resume/Pause and physical
retirement. Every one of the eighteen idle intervals preserves complete canonical bytes and
zero simulation/logical ticks; both Wait conditions produce zero App frames/presentations.

| Aggregate across six runs | Poll | Periodic observer Wait | No observer deadline Wait |
| --- | ---: | ---: | ---: |
| App frames | 60 | 0 | 0 |
| Actual presentations | 60 | 0 | 0 |
| Observer callbacks | 60 | 1474 | 12 |
| Native poll calls | 30 | 737 | 6 |

Deadline-free Wait reduces both callback and native-poll counts by **99.1859%** versus explicit
periodic Wait, passing the preselected 90% gates. Each deadline-free 300 ms interval has one
native poll and two observer calls at the original Windows pump boundaries. Those two calls
are not evidence of a periodic timer. The absolute deadline controls observe their original
100 ms request at 100.0954, 100.1769, 100.6636, 100.3464, 100.2153 and 100.0873 ms, all within
the finite 250 ms gate despite a 1 s caller wait cap. They advance no App/game/presentation
state and retain unchanged canonical bytes. All original telemetry seals, surface/provider
retirements, native destructions and physical event-loop recoveries complete.

| Run | Recipe | Poll CPU (ms) | Periodic Wait CPU (ms) | No-deadline Wait CPU (ms) |
| --- | --- | ---: | ---: | ---: |
| 1 | Plain | 15.625 | 0 | 0 |
| 2 | Witness | 46.875 | 0 | 0 |
| 3 | Witness | 0 | 31.250 | 0 |
| 4 | Plain | 15.625 | 15.625 | 0 |
| 5 | Plain | 15.625 | 15.625 | 0 |
| 6 | Witness | 0 | 0 | 0 |

These whole-process samples remain quantized. Zero observed CPU increments are below the
counter's resolution and do not prove zero work. Callback/native-poll reduction is established;
a stable CPU or power budget is not. Poll intervals last about 332–333 ms and Wait intervals
about 300 ms; cooperative overruns remain in the raw record. This is a matched comparison of
three conditions in the maintained implementation, not an isolated historical-binary or
cold-process comparison. The [full v4 record](data/2026-11-14-development-runtime/observer-deadlines.json)
and [metadata](data/2026-11-14-development-runtime/observer-deadlines-metadata.json) retain
all conditions, identities, state, deadline timing and cleanup facts with an LF-normalized digest.

Two initial consumer builds hit the default JSON macro expansion limit, first in the deadline
report and then in the idle report. Both objects are now constructed in smaller literal maps
and merged without changing fields, limits or runtime behavior; no global recursion-limit
override is introduced. Neither failed build ran a native experiment. The final v4 execution
exits zero on its first native attempt. Software DX12 also passes the four new GPU maintenance
tests, independently from the hardware timing comparison. Final consumer/check qualification
and remaining scope follow below.

The software cohort passes 4/4 (`d743e53f-829b-4aa8-82a3-c75f36c2ebbc`). Correctness Clippy
passes affected WGPU all-targets, the root desktop library and the ordinary measurement binary.
All five migrated finite observers compile through their independent consumers: all policy
examples, stock Runtime composition, content example/native wrapper, and Material source-watch
with its desktop feature closure. Their existing continuous native pixel/history workflows
are not newly executed by those compile checks. Existing style/dead-code warnings remain.
The seven scoped research, implementation and independent review threads close with no
remaining reported P1/P2. Source-desktop protocol polling remains a separate 2 ms caller cost.
General task/World/cache-lifetime wakes, foreign execution, representative CPU/power or latency
budgets, and full Foundation/R12 remain open.

Root/sample and all four affected independent workspace formatting checks pass, as do new
relative document links, workflow YAML, JSON and metadata/raw-record integrity. No dependency
or lockfile change is selected. These final document checks do not repeat unchanged native
acceptance or expand the scoped result into a framework-wide release claim.

## Native Request Notifications And Reconciliation

The preceding observer comparison leaves a separate caller cost: the development child probes
its private request file every 2 ms. This successor retains the file-packet transport and
original process/game owners, adding a private nonrecursive watcher and a maintenance-only
wake on the original native session. Headless uses a condition variable. Notifications carry
bounded hints; the original capability-relative inbox and exact version/placement/session/
generation/request checks still admit packets. A 250 ms reconciliation ceiling handles lost
notifications within existing deadlines. Close freezes delivery and joins the original watcher
before Closed; actual native process cleanup remains separate.

The vendor Windows watcher still has a 100 ms internal wait and legacy backend reporting
limitations. This is not zero periodic work, reliable message delivery or a universal task
wake service. Parent response polling stays at 10 ms. Required protocol-v5 control-service
diagnostics report original inbox checks, waits, callback/hint counts and join/saturation flags;
they are outside canonical state and grant no authority. The Host commits them only after
envelope, reply-kind and operation-specific semantic validation. Active and retired children
retain their own observations rather than sharing a latest-response cache.

The ordinary `development_notifications` consumer builds periodic and notification variants
for both source/headless and source/desktop placement through the public Cargo adapter. The
only declared-source difference within each pair is the child's reconciliation constant,
2 versus 250 ms; both use the same native watcher implementation. Each placement runs three
alternating pairs with no warmup processes, twelve fresh children total. After real pickups,
travel and markers, each records 500 ms idle with exact canonical bytes, the original process
CPU handle, and diagnostic deltas between validated Capture responses. Endpoint request and
parent observation overhead remain in the counter window. Explicit Frame must preserve
canonical state; Step must advance one logical tick with the independent movement expectation.
Two consecutive public Frame calls must also advance the original `FrameExecutionStart`
sequence, exactly once in the quiescent headless case, while engine/logical ticks remain
unchanged. The process-local sequence is read together with the game observation through
the original capture/observe boundary and never enters canonical state.
Desktop additionally confirms actual hardware presentation before and after the idle/control
trajectory. Every original game/watcher/process must close normally before Fixture removal.

Acceptance is at least 90% fewer aggregate idle inbox checks per placement and every selected
ordinary request within 100 ms. Startup and gameplay preparation are excluded from the request
latency gate; their observations remain separately scoped. The finite cooperative gate cannot
interrupt synchronous callbacks, drivers or native APIs. Whole-process CPU counters are
quantized and do not stand in for a stable CPU/power budget. Rejected numerical gates preserve
the full raw report after original cleanup. Actual execution results follow below.

Independent review found two evidence gaps before the first comparison: a cleanup failure
could exit before saving obtained observations, and state equality alone could not distinguish
an explicit Frame from an accidental Observe. Failure now records the completed/current runs,
cleanup duration/error and retained directories in a rejected partial report while preserving
the original Host/Fixture owners. Original App-frame sequence checks supply the separate Frame
evidence. The earlier ordinary sample suite passes 170/170
(`1a6e1752-ee79-4b0d-8b2e-5685f1f808df`); final source qualification follows these corrections.

Winit all-feature CPU tests pass 95/95 (`66b3743a-54b3-4c24-be75-ef0608c81ebd`), including
coalescing/refusal/revocation and unchanged original session state. The actual native cadence
cohort passes 4/4 (`73eeb0cb-5b06-4048-94b7-4d24b4c1234a`): cross-thread maintenance wakes
preserve App time/capture, current hints survive pre-first-poll handoff, stale handles reject
successor sessions, and failed startup/Drop revoke handles. These are native owner controls,
not notification-comparison or full game-adoption results.

The affected original retained-pump/maintenance native cohort also passes 8/8
(`2171b9e7-1e83-431a-9d34-e61cb6ff09fe`), preserving sequential sessions, old queued/late
proxy isolation, keyboard continuation, hidden windows, unused-pump recovery and maintenance
without gameplay. Final ordinary sample execution passes 170/170
(`81663bbb-9279-4644-9ead-0ecc9d68a85f`) after both measurement corrections. The first sample
build required 8m22s and the corrected-source build 4m21s; these are verification build costs,
not runtime or controlled compiler-speed measurements.

### Rejected Initial Comparison

The first complete experiment (`5c606d8d-9d8a-4d79-b751-c57609c64adb`, 237.716 s including
four real builds) fails the headless check-count gate. All twelve children complete their
canonical state, explicit Frame/Step, selected request latency and original cleanup checks;
all four Fixture roots are removed. Across three headless pairs, the periodic control records
105 idle-window inbox checks and notifications record eleven, an **89.5238%** reduction below
the required 90%. Desktop records 601 and twelve, a **98.0033%** reduction. All 150 selected
ordinary requests stay within 100 ms; the maximum is 78.0432 ms for desktop presentation.
The [full rejected report](data/2026-11-14-development-runtime/request-notifications-rejected.json)
remains rejected rather than being relabeled as a passing implementation.

The 2 ms headless condition-variable timeout does not establish an actual 2 ms period on this
Windows desktop: each 500 ms interval contains only 34 completed waits and 35 checks. The
observation window also includes endpoint requests and their wake/check overhead. Source
inspection identifies an unnecessary hint: the child deletes a consumed request itself, and
that definite file-removal event cannot publish a successor command. The watcher now ignores
known non-root file removals while retaining empty/uncertain removal, root-directory removal,
rescan, creation/rename/modification and backend failures. Original fixed-inbox admission,
250 ms reconciliation and operation deadlines are unchanged. Independent review checks the
actual capability-relative publication and platform rename mappings and reports no P1/P2.
The same numerical gates and matched two-constant comparison are retained for the rerun.

### Qualified Notification Results

The corrected ordinary sample suite passes 170/170, including consumed-removal negative
controls and retained uncertain/root/rescan cases. The same twelve-child comparison then
passes (`077d5e17-a1ed-41f0-93b4-81a9de297c26`, 136.033 s including four actual builds).
Each placement retains three alternating pairs, 500 ms idle intervals and all original
canonical/Frame/Step/presentation/cleanup checks. No numerical gate is relaxed.

| Aggregate across three pairs | Periodic checks | Notification checks | Reduction |
| --- | ---: | ---: | ---: |
| Source/headless | 103 | 9 | 91.2621% |
| Source/desktop | 594 | 12 | 97.9798% |

All 150 selected ordinary requests return within 100 ms. The maximum is **74.629 ms** for
desktop presentation confirmation; startup, real builds and gameplay preparation remain
outside that finite latency gate. Two explicit Frame calls advance the original App sequence
for every run, exactly once for headless, while engine/logical ticks and complete canonical
bytes stay unchanged. Step then advances exactly one expected tick and preserves meaningful
pickups/travel/markers. All twelve game/watcher/native process retirements complete normally,
their exact active-generation receipts and joined diagnostics are retained, and all four
original Fixture roots are removed.

The outer test is release-optimized under Rust 1.95.0 on Windows. Actual captured game builds
use the public adapter's `dev` profile: headless has no features, desktop has only `desktop`,
and neither enables defaults. Every desktop receipt reports `vulkan` / `discrete-gpu`; parent
environment preferences cannot substitute a DX12 result for these sealed-child observations.
The child report does not identify GPU model/driver, so none is inferred from the earlier
independent cost consumer. Per-run idle CPU deltas are retained and quantized; the comparison
establishes fewer inbox checks, not a stable CPU, power or latency-tail improvement.

The [accepted full report](data/2026-11-14-development-runtime/request-notifications.json)
retains every result, state byte array, source/provenance digest, CPU observation, counter and
cleanup fact. The rejected report remains separately linked above. Raw source digests and
repository status describe the measured uncommitted source, not a clean historical binary.
Notification-loss fallback, unchanged 10 ms parent response polling, vendor internal wakes,
cooperative deadlines and foreign-platform gaps remain material limits.

The original complete production Host regressions also pass **3/3**
(`9fecaca5-bdff-4720-a4ef-dd3a5908cbbb`, 244.838 s including actual nested builds): continuous
desktop, paused desktop and headless. All retain actual Rust function-body/private-structure
edits, exact probe and fresh-adopter semantics, refused stale/foreign candidates, separate
postcommit predecessor cleanup and continued gameplay. Expected compile-error controls remain
recoverable, and original game/watcher/native owners close normally. The continuous child
reports Vulkan and ends Running before Close. This is a workflow regression, not a controlled
latency benchmark or completion of the full Foundation plan.

Qualified implementation revisions are `686a5d9` (original native-session maintenance wake)
and `021814c` (game-private watcher, protocol-v5 observations and public comparison). The
no-default-feature sample independently passes 166/166
(`65bd2042-31d2-4fc4-a235-5bca651487e6`). After a final headless unused-reference warning
cleanup, the affected child suite passes 16/16 (`e7ade70b-f888-48b5-af7d-5db7f1934f17`);
the [metadata](data/2026-11-14-development-runtime/request-notifications-metadata.json)
keeps that post-measurement source distinction. Locked all-feature/all-target checking,
Winit/game all-feature/all-target correctness Clippy and root/sample formatting pass. Existing
non-correctness warnings remain. Ten scoped threads close with no remaining reported P1/P2.
No remote CI run/publication or foreign native execution is claimed. The sample dependency
change reuses existing vendor notify and its required platform closure without upgrading
existing packages or editing the vendor. Metadata/raw integrity and local document checks
complete evidence closure; unchanged native acceptance is not repeated for documentation.

## Physical Task Return And Request Check Ordering

The November 16 [physical-return unit](../plans/2026-11-13-rust-state-reconstruction-plan.md#physical-task-return-and-game-capture)
replaces diagnostic task counts in game capture with original admitted physical-return
observation. A cancelled capture may still be undergoing executor-owned Drop after its
terminal is consumed and all queued/running diagnostics reach zero. The game now refuses
that capture and returned cleanup failure; its existing child waits settle original returns
without requesting an extra App frame solely for disposal. Task query and game correctness
are qualified separately from the notification measurements below.

### Rejected Return-State Regression Comparison

The first twelve-child comparison after that migration fails the unchanged headless 90%
check-count gate (`4a6b4e70-f2ed-4eb3-bf36-10ffcc941e1d`, part of the 506.983 s four-test
native cohort). Headless records 107 periodic checks and eleven notification checks,
**89.7196%** fewer. Desktop records 580 and twelve, **97.9310%** fewer. All 150 selected
requests stay within 100 ms; the maximum is 89.6024 ms. Every original canonical-state,
Frame/Step, presentation and normal game/watcher/native cleanup check passes, and the four
original Fixture roots are removed. The [full rejected report](data/2026-11-14-development-runtime/task-return-request-checks-rejected.json)
remains rejected.

The original loop performs an unnecessary immediate empty-inbox probe after each response
even when no new hint is pending. The correction checks immediately at startup, then waits
before the next probe unless a hint already requires one. A notification or the original
bounded reconciliation wait makes that probe due. The loop acknowledges hints before the
original receive and rechecks lifetime/request limits after waiting. It retains native
maintenance pumping, 250 ms reconciliation, the original transport and counter definitions;
no request limit, timing limit or numerical gate changes.

### Qualified Wait-First Comparison

The affected child suite passes 16/16 (`f81d441f-803e-4e0e-83c4-03acb43fb9f5`). The same
twelve-child comparison then passes (`9482c865-13d3-48c1-a091-d1957cbb7302`, 135.308 s
including four actual builds). Both variants use the corrected loop and actual native
watchers; their sole difference remains the 2 versus 250 ms reconciliation constant.
Each placement retains three alternating pairs and 500 ms requested idle intervals.

| Aggregate across three pairs | Periodic checks | Notification checks | Reduction |
| --- | ---: | ---: | ---: |
| Source/headless | 105 | 9 | 91.4286% |
| Source/desktop | 595 | 10 | 98.3193% |

All 150 selected requests return within 100 ms; the maximum is **80.7093 ms** for desktop
presentation confirmation. Original canonical bytes, explicit App-frame progression and
one-tick Step remain valid. All twelve original game/watcher/native process owners close
normally with joined diagnostics and exact native cleanup receipts, and all four Fixture
roots are removed. The [accepted full report](data/2026-11-14-development-runtime/task-return-request-checks.json)
retains individual requests, state bytes, CPU observations and build/source provenance.

Outer tests are release-optimized on Windows x86_64 under Rust 1.95.0. Captured children use
the public adapter's dev profile, with no default features: headless requests none and desktop
requests only `desktop`. Actual desktop receipts report Vulkan/discrete-gpu. These are matched
checks within the maintained implementation, not a controlled comparison against historical
binaries or proof of a CPU, power or tail-latency improvement. Parent response polling remains
10 ms, Windows waits remain quantized, vendor internal wakes and notification-loss fallback
remain, and synchronous/native callbacks retain cooperative deadlines. Foreign execution,
universal quiescence and full Foundation/R12 remain open.
