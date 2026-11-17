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

## Active Gameplay Workload Baseline

The selected November 16 successor measures the existing ordinary `Scenario::Travel` workflow
rather than repeating neutral continuous play. Three sequential native runs each create a fresh
Runtime, native window and renderer, recovering only the original physical event loop between
runs. Loaded code, driver state and filesystem caches are shared. The script issues normal
semantic movement, collects three pickups, creates markers, crosses level/instance boundaries,
restarts an instance and returns to Meadow. The game retains all gameplay and source owners.
No manual frame delta, exact Step, private source mutation or second input/render authority is
introduced by the measuring executable.

Before measurement, select hardware DX12 and `NARA_WGPU_FORCE_FALLBACK=0` on this Windows host.
Use Rust 1.95.0/release with `desktop,cost-measurement`, one Cargo build job and the root target.
The finite acceptance requires three complete verified trajectories, at least twenty unique
initialized presented frames per run, no more than 4096 retained frames, a sixty-second
cooperative gameplay deadline and complete telemetry plus original native retirement. Report
the actual observed sample count; do not stretch the script with idle frames to meet a quantile
claim. A failed sample/control gate remains failed rather than being relaxed after observation.

Frame wall spans run from the exact original `FrameExecutionStart` to its matching post-Host
observation, including native/driver/submission waits. Original queue-completion observations
are separate and are not GPU execution or scanout. Per-run nearest-rank P50/P95/P99/max values
describe these finite correlated samples, not a production tail estimate. Whole-process CPU
deltas and initial/final memory points include the measuring observer and all process threads.
The baseline makes no speedup, steady-state memory bound or leak claim and selects no frame-time
pass threshold. Its purpose is to expose actual gameplay and scene-transition work before
choosing an optimization. Exact source/toolchain/adapter, complete trace, method and cleanup facts
belong in the resulting report.

### Active Gameplay Results And Qualification

The measuring consumer, native controls and pure CI target are committed at
`6c9471ae36f1e0992d60919dd9ade1a5b792b4f8`.

The [original full report](data/2026-11-14-development-runtime/active-gameplay.json) retains all
447 unique initialized presentations, exact App/render/window/backend/device identities,
gameplay snapshots and original completion observations. The
[metadata](data/2026-11-14-development-runtime/active-gameplay-metadata.json) records the binary,
selected source and report SHA256 values, compiler, command and test identities. This is a
release executable measured from the uncommitted implementation on `fb07a22`, not a clean
historical binary. The actual adapter is NVIDIA GeForce RTX 4090, hardware DX12, driver
`32.0.16.2114`, with a 960 by 540 physical surface. The machine and uncontrolled background
conditions remain as described above.

| Run | Initialized presentations | Observed moving / transition frames | App-to-post-Host P50 | P95 | P99 | Maximum |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 147 | 38 / 21 | 6.9061 ms | 7.3097 ms | 7.6679 ms | 607.6306 ms |
| 2 | 151 | 38 / 21 | 6.8985 ms | 7.2816 ms | 14.3425 ms | 601.0571 ms |
| 3 | 149 | 38 / 21 | 6.9443 ms | 7.2664 ms | 13.7485 ms | 604.7734 ms |

Each maximum is the first initialized-baseline frame, retained as its own classification rather
than discarded as an outlier. Its broad original frame span includes first-presentation work;
this observation does not attribute the approximately 0.6 seconds to a particular GPU, shader,
driver or source stage. Moving-frame medians are 6.7749, 6.9658 and 6.7572 ms. The observed
scene-transition medians are 6.8652, 6.8763 and 6.9528 ms. Classification compares adjacent actual
presented snapshots; it does not measure internal load, simulation or render stages separately.
The script reaches its own verified final facts in every run: three pickups, score 30, four
travels, one restart, Meadow at `(-120, 0)`, live marker `[1]`, released movement and a current
six-member Scene. Each run observes five actual Scene-instance changes. Fixed/logical tick counts
vary with native pacing and are retained rather than forced equal across runs.

Whole-process CPU deltas are 31.25, 93.75 and 31.25 ms over separate counter windows of 1.003883,
1.032589 and 1.0165361 seconds. Those windows begin at the first initialized post-Host point,
after that frame's broad first-presentation span; they must not be divided by a different frame
wall interval to infer CPU utilization. Quantization and variation prevent a stable CPU-cost
claim. Initial/final private committed memory is 496.08/499.55, 657.99/661.22 and 533.46/536.37
MiB. These are shared-process point readings across scene changes, not per-Scene ownership,
steady-state residency or a leak trend. Original telemetry seals retain zero pending/lost
completion and resource-retirement evidence, with every initialized presentation matched to its
own completion. Logical tracked GPU payload sizes are not full physical GPU residency.

Native acceptance passes **2/2** in `39cc07bd-09ce-4f16-8f70-da81214c770a`: the three-run
gameplay measurement and a registered-observer panic after an actual presentation. The latter
requires a failed measurement report, original window destruction, physical event-loop recovery
and normal contained-process exit/empty containment/stopped readers/drained output. It cannot
pass on startup failure or a missing report. All three normal runs also complete their explicit
original telemetry seal, native/window/provider retirement and event-loop recovery before reuse.

Eight pure arithmetic, identity and public-snapshot controls pass in
`d114397c-ceb4-4285-8ce8-c9e21152942e`. The final gameplay library plus those controls passes
**102/102**, with no skips, in `fd687a6a-4300-4cc5-b9ec-86afaf0eb671`. Correctness Clippy for the
new executable and tests, root/sample formatting, evidence integrity, TOML and added local links
pass. CI adds the pure metric target; no remote CI execution or hardware-runner claim follows.
First compilation exposed an incorrect digest import, and the added failure-control
wrapper used the wrong process-command argument method; both were corrected before acceptance.
Independent review corrected the registered callback's panic boundary and post-poll deadline
checks. Three scoped agents close, with no remaining reported P1/P2.

This is a small game with six authored Scene members plus dynamic markers. Its native keyboard
input is isolated while the original semantic-command script drives gameplay. It does not prove
physical-input latency, a larger-world capacity, production tail bounds, GPU execution/scanout,
foreign platforms or shipping instrumentation overhead. No engine optimization follows merely
from collecting this baseline. The next relevant measurement should attribute the first
initialized frame through existing startup/GPU observations and extend a named content workload
before selecting a broader steady-state budget. Full Foundation/R12 remains open; paused
Editor/UI/Dialogue product work is unchanged.

## First Initialized Frame Attribution

The November 16 successor on base `3be7b0a` retains the complete ordinary Travel workload above
and explicitly enables existing Desktop startup observations plus five fixed operation wall spans
inside the original WGPU Runtime transaction. It adds no device, native-loop, resource or
presentation owner. Disabled phase collection performs no instrumentation clock reads. Returned
errors may retain reached spans; unwind and unreached calls remain absent. Each new transaction
clears its observations. Native work counters remain issued requests, not hardware completion.
The owner implementation is `46f798c`; the complete consumer and captured evidence are
`e88ec46`. Measurements use that implementation before those local commits; selected source and
executable hashes identify the measured worktree rather than a clean historical build.

The [complete report](data/2026-11-14-development-runtime/first-initialized-frame.json) retains
483 initialized presentations across three fresh Runtime/window/device lifetimes and all original
completion matches. The [metadata](data/2026-11-14-development-runtime/first-initialized-frame-metadata.json)
records actual source, compiler, executable/report hashes, command and executed test identities.
Execution uses Rust1.95.0/release, hardware DX12/RTX4090, one build job, locked/offline resolution
and the original target. The same process, loaded code/driver/filesystem state and recovered
physical event loop are shared; these are not cold-process or independent hardware trials.

The original first presentation and first initialized presentation are distinct report fields.
They happen to coincide in these runs; their App/render/window/backend/device identities are
checked against the original receipt. The observer retains actual phase spans even if an early
presentation lacks its matching App start; only the unavailable App residual remains null.
The entry endpoint starts before source-root/recipe construction and ends at the original first
presented observation. It includes caller/native waits and excludes executable/process loading.

| Run | Initialized frames | Entry to first presentation | First initialized App to post-Host | Namespace preparation | Program execution | Other App-frame wall |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 159 | 734.7560 ms | 619.0608 ms | 587.5639 ms | 29.8027 ms | 0.2714 ms |
| 2 | 165 | 626.6711 ms | 599.9730 ms | 571.3827 ms | 27.5906 ms | 0.2736 ms |
| 3 | 159 | 675.7288 ms | 633.0067 ms | 606.9740 ms | 25.0220 ms | 0.2021 ms |

The five disjoint operation spans are namespace preparation, Surface acquisition, candidate
preparation, program execution and Surface presentation. The omitted table columns are retained
exactly in each frame: first-frame acquisition is 0.0322/0.0208/0.0144 ms, candidate preparation
0.0549/0.0444/0.0311 ms and presentation 1.3357/0.6609/0.7631 ms. Their checked sum plus the
reported App residual equals that exact frame's outer wall duration. The residual is unmeasured
App work and gaps, not an inferred CPU or waiting duration. Queue-completion observations keep
their separate exact joins and may precede post-Host observation; no GPU execution/scanout
duration is derived by subtracting these timestamps.

Namespace preparation accounts for 94.91%, 95.23% and 95.89% of the first initialized frame's
outer wall span. This is the original namespace call, including producer maintenance and
instance/surface/Adapter/device/configuration preparation; it does not isolate Adapter selection,
driver work or a single native API. First-frame program execution includes its native preparation,
validation, encoding, submission and waits. These samples do not justify attributing all of that
span to Shader compilation or installing a disk pipeline cache. Later initialized frames have
namespace medians 0.0028/0.0029/0.0029 ms and Surface-acquisition medians
6.08865/6.08585/6.12095 ms, with original native pacing unchanged. Those operation wall medians
do not establish simulation CPU or GPU cost, and are not a steady-state production budget.

Existing startup observations remain separate. Content preparation is 13.3222/13.2450/22.7321 ms,
Runtime startup 4.1351/3.1648/5.0909 ms and session construction 69.1277/0.0035/0.0042 ms.
The first native poll takes 649.3214/0.0025/0.0032 ms, while polling through first presentation
takes 649.3214/611.4251/650.2024 ms. A short first poll can defer initialization to a later poll;
it is not proof of shorter startup. The polling aggregate overlaps the first poll and excludes
caller gaps. Completed startup snapshots freeze after the first actual presented poll and remain
unchanged through the full gameplay/seal/physical-retirement path.

The owning CPU selection passes 19/19 in `f92d4a71-d32c-4476-9cae-860ad5d19882`, covering seven
new timing controls and original completion/resource/work counters. The independent sample's full
game library plus metric controls passes 130/130 without skips in
`f3f78097-7221-411f-99d6-0beb5076b5a6`. Actual three-run Travel and registered-observer panic
cleanup pass 2/2 in `6a8ee287-031d-4282-87e4-33af37e884f3`. All original movement, pickup,
marker, Scene-instance, restart, end-state, completion/seal and physical cleanup assertions remain.
The affected original native Host cohort passes 8/8 in
`410c0970-da1b-4360-b5b2-61f85e90b54e`, including original-prefix settlement, failed upload,
no-telemetry execution and producer/device/window permission retirement.

Independent review corrects two consumer defects before the final native qualification: an early
frame's valid spans must survive a missing App envelope, and later attribution failure must retain
the already verified native cleanup report. Focused CPU controls cover both. This is executed
cost attribution, not a speedup or a whole Foundation/R12 completion. The next cost investigation
belongs to the original device/namespace acquisition operations and must retain the complete entry
endpoint, exact original lifetimes and failure/cleanup paths. Larger named workloads, reliable
tail/memory budgets, foreign native execution and paused product work remain separate.

Final minimal no-default WGPU production compilation, all-target/all-feature owning correctness
Clippy, consumer executable/test correctness Clippy, root/sample format checks and added document
links/fragments pass. Selected source, executable and report SHA256 values are rechecked after
qualification. Existing non-correctness warnings remain. Five scoped research, implementation and
review agents close with no remaining reported P1/P2. No Cargo commands overlap, no build cache is
cleaned and no remote publication/CI execution occurs.

## Original Namespace Acquisition Results

The successor on `4b4ed44` borrows five fixed nested observations through the original private
Host/backend/device calls. It preserves the existing optional transaction collection, one device
owner, original Surface authority, requirements/fallback checks, epochs and physical retirement.
It adds no adapter enumeration, eager device request, callback, logging or GPU cache. Timers cover
the native call expression as well as driving its returned Future; the locked native wgpu request
can perform synchronous work before constructing that Future. Returned errors retain reached
operation spans; missing/unwound calls do not invent completion. Nested observations publish only
when the original containing namespace call returns and clear with the original next transaction.
The owner implementation is `4a2584d`; the complete consumer and captured evidence are `e29e51c`.
Qualification uses that implementation before those local commits, with exact measured worktree
source/executable hashes retained rather than a clean historical-build claim.

The [full report](data/2026-11-14-development-runtime/namespace-acquisition.json) and
[metadata](data/2026-11-14-development-runtime/namespace-acquisition-metadata.json) retain 479
initialized frames, selected source/executable/report hashes and exact execution identities.
The same three-run ordinary Travel consumer uses Rust1.95.0/release, one job, locked/offline,
hardware DX12/RTX4090 and the existing target. Runtime/window/device owners are fresh per run;
loaded process/driver/filesystem state and the recovered physical loop are shared. The prior
batch's 483 frames and temporary probes are separate observations, not interchangeable samples.

| Run | Initialized frames | First-frame namespace | Instance preparation | Surface creation | Adapter request | Device request | Surface configuration |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 157 | 600.8823 ms | 40.9853 ms | 0.0279 ms | 502.3248 ms | 52.5445 ms | 4.9608 ms |
| 2 | 161 | 557.2569 ms | 39.5566 ms | 0.0149 ms | 451.8774 ms | 62.0042 ms | 3.7883 ms |
| 3 | 161 | 552.6635 ms | 37.3908 ms | 0.0144 ms | 450.7521 ms | 60.6964 ms | 3.7944 ms |

Other unmeasured work inside those namespace spans is 0.0390/0.0155/0.0154 ms. Each nested sum
plus its own residual equals that exact parent phase. The parent overlaps all children and must
not be added again. Whole first initialized frames are 630.0250/583.7951/580.0763 ms, and full
entry-to-first-presentation observations are 752.4937/608.5443/605.6519 ms. These sequential
ranges do not establish an improvement over the previous batch. All operation durations are
wall time including driver and native waits, not CPU usage, GPU execution or scanout.

Every run's actual Adapter and Device request pair belongs to its first original App/render frame,
device epoch one. Every later observed namespace, through all Scene transitions, has both request
spans absent. Returned Instance/Surface/configuration ensure calls can still report a measured
duration on reuse. Exact first-frame ownership is checked in addition to total request counts;
one misplaced later `Some(0)` cannot replace a missing initial request observation. All original
movement, pickups, markers, five Scene-instance changes, restart, final-state, completion/seal and
native retirement checks remain. The original owner independently exercises a real initial request,
Ready reuse and recovery into the next epoch, preserving late-predecessor-loss isolation.

### Official wgpu Mechanism And Nara Policy

Source inspection uses official wgpu `cddb2f5d80dc0009b2ef6767635140cacf90a007`:

- `wgpu-core/src/instance.rs::Instance::request_adapter` completes HAL enumeration before fallback
  and actual Surface filtering, then performs power-preference sorting. `HighPerformance` changes
  ranking after that work; it does not short-circuit candidate exposure.
- `wgpu-hal/src/dx12/instance.rs::enumerate_adapters` ignores its Surface hint and fully collects
  exposed DXGI candidates. `auxil/dxgi/factory.rs` contains the original platform blacklist and
  duplicate-WARP exclusions. Adding a generic software/virtual-device exclusion would change it.
- `wgpu-hal/src/dx12/adapter.rs::Adapter::expose` creates a real D3D12 capability-probe device,
  queries features and PCI information, and can scan Windows SetupAPI properties per candidate.
  `Adapter::open` reuses the probe device while creating the queue/fence/wrappers. These source
  facts explain why Adapter-request cost can exceed Device-request cost; the measured spans do
  not identify one candidate, SetupAPI or a particular driver call as the cause.
- `wgpu/src/backend/wgpu_core.rs::request_adapter` performs synchronous Core selection before
  returning a ready Future. Timing only a separately constructed Future's `block_on` would miss
  that work. `wgpu-types/src/backend.rs::Dx12BackendOptions` exposes no public pre-enumeration
  name/LUID/WARP/PCI-query filter.

Nara retains one original request with the actual compatible Surface and explicit fallback hint,
default power preference, followed by its original selected-Adapter and enabled-Device requirements
checks. It does not retry other candidates after selected-Adapter requirements refusal. Changing
that policy needs its own concrete consumer/selection contract; it is not an enumeration-speed fix.
The current experiment already selects DX12 only and observes the validation/GBV/compiler
environment overrides as absent. Release defaults preserve official indirect-validation flags.
No evidence selects a broader backend restriction, removed validation, manual HAL selection or
cross-Runtime Device/Queue reuse. A future upstream/native call-stack investigation can distinguish
SetupAPI, driver probing and capability-query work without changing Nara ownership. It is separate
from further ordinary engine-runtime implementation and is not a completed optimization here.

### Namespace Qualification

| Scope | Actual result |
| --- | --- |
| Owning Device/timing/telemetry/work CPU controls | 27/27, `79587e05-d997-42ee-97cc-9a04a320f019`; 499 cases outside the filter |
| Independent game library and metric controls | 132/132, `68fd14b0-d031-4302-894d-b55057d55719`; no skips |
| Complete hardware Travel and observer-panic cleanup | 2/2, `a8f94ae5-7880-4639-9b25-10e9bd665765`; no skips |
| Original native Device/recovery/Host consumers | 11/11, `26bc4c47-7582-4e7b-9314-b45ed2178cb3`; 515 cases outside the filter |

Minimal no-default WGPU library compilation, all-target/all-feature owning correctness Clippy,
consumer executable/test correctness Clippy and root/sample formatting pass. Existing warnings
remain. Review corrects an omitted private-call argument in an existing Windows ImGui test and
strengthens per-frame fresh/reused request attribution beyond aggregate counts; original assertions
are retained. Both independent reviews close without remaining P1/P2, and all four scoped agents
close. No remote CI, other-platform execution or new Foundation/R12 completion claim follows.
The bounded acquisition-cost investigation closes with the original official path retained;
larger named gameplay/resource workloads and reliable runtime tail/memory budgets remain open.

## Authored Tile Workloads Through Native Travel

The November 16 successor starts from `51486f2`. The original-owner correction is
`2b98a0c`; the complete measuring consumer and captured evidence are `9f0e5dc`.
It adds 64/4,096/16,384 source-authored Tilemap cells to the ordinary two-level game,
with three complete native Travel lifetimes per population. Every lifetime retains movement,
three pickups, score 30, markers, four travels, one restart, six observed Scene instances,
five instance changes, the exact final Meadow position and released input holds. The original
verifier and six authored Scene members are unchanged. The separate three-run no-Tile baseline
and both original/native-source observer-panic controls remain accepted.

Raw population reports are [64 cells](data/2026-11-14-development-runtime/authored-tiles-64.json),
[4,096 cells](data/2026-11-14-development-runtime/authored-tiles-4096.json) and
[16,384 cells](data/2026-11-14-development-runtime/authored-tiles-16384.json), alongside the
[baseline](data/2026-11-14-development-runtime/authored-tiles-baseline.json) and exact
[source/executable/report metadata](data/2026-11-14-development-runtime/authored-tiles-metadata.json).
These are qualified worktree builds with selected source hashes, not clean historical or cold
machine builds. Native execution uses Windows 11, Rust1.95.0/release, one build job,
locked/offline dependencies and hardware DX12/RTX4090. The fixed official wgpu revision remains
`cddb2f5d80dc0009b2ef6767635140cacf90a007`.

### Authored Representation And Admission Corrections

Each population covers world `[-120,136] x [-128,128]`, a 256-by-256 footprint. Cell sizes are
32, 4 and 2 world units. Original 16-by-16 PNGs supply the TileSets, with explicit white cell
colors, tile zero, layer -10 and `NEAREST_CLAMP`. Source copies are private, bounded and removed
after the original native lifetime and independent replay. The checked-out game content is not
edited. The 16K representation uses two 8,192-cell maps: one on the checkpoint, and one supplied
by a root-bound Prefab on the existing portal. It adds no Scene member or child. One inline 16K
value exceeds the original one-million wire-node file limit; splitting uses the existing source
representation rather than changing that limit, dropping color data or reducing the population.

| Cells | Source layout per level | Reported final source-file bytes across both levels and shared inputs | Initialized presentations per run | Source-copy/generation wall per run |
| ---: | --- | ---: | --- | --- |
| 64 | One direct map, no Prefab | 174,779 | 87 / 87 / 85 | 39.984 / 40.164 / 46.462 ms |
| 4,096 | One direct map, no Prefab | 2,909,047 | 133 / 135 / 133 | 248.563 / 270.628 / 257.513 ms |
| 16,384 | Two maps, one root-bound Prefab | 11,267,421 | 301 / 297 / 293 | 812.574 / 781.326 / 784.751 ms |

The final source-file byte column comes from the report's actual files/digests, not decoded heap
size. Copying, generation, serialization and default-limit preflight are timed before the native
entry endpoint. Those numbers are not engine Scene-loading latency. The 16K source layout differs
from smaller populations and is not a pure cell-count scaling experiment.

The new original content regression first failed before native gameplay. Staged compilation
reserved complete component-value bytes multiplied by configured maximum depth, even for an
empty entity-reference plan: two 4,096-cell maps requested 633,004,514 modeled working bytes
against 536,869,785 available. This diagnostic is not observed RSS. The correction uses Reflect's
existing admitted planner and separately admits every later plan visit through the original
cumulative ledger. Visit bounds include eager path-prefix copies, scratch growth and diagnostics;
Persistent references pay too. Source ownership, reference errors, exact quota refusal and
unchanged file/value/Host-memory policies remain covered. Property-claim preparation keeps its
separate conservative reservation and is not claimed optimized here.

After compilation succeeded, root asset discovery exposed an independent receiving mismatch:
its generic reference traversal default admitted only 16,384 nodes, although the compiled value
had already crossed the Host's much larger complete-value policy. Reflect now owns the existing
value-to-reference-limit conversion, reused by Scene and the receiving content Host. The original
allocation callback and caller-error identity remain authoritative. No source parser, reference
cache or second content/GPU owner is added. A 32,768-item no-asset control proves complete scanning
and exact refusal rather than skipping values without references. Synthetic long-key controls
retain default work-byte refusal, then isolate later copy/working guards using test-local options.

The native fixture also initially omitted its nearest sampler. The independent pixel oracle
refused that linear-filtered input; the copied TileSet now explicitly authors the intended
sampler, with a source decoding assertion. A separate fixture assertion wrongly counted an
unconsumed disk Schema bundle under its Native provider. It now verifies the seven actual closure
members without that unused artifact. These corrections retain the workload and pixel assertions.

### Presented Resource And Timing Observations

At each original presentation, the consumer joins Runtime/App/render/window/backend/device facts
and completion data before recording current CPU asset counts, extraction, queue, successful
drawing and logical GPU payloads. Running, non-restoring frames require the declared full cell
count, original RenderFrame identity, zero missing/invalid/unprepared items and matching actual
quad statistics. All recorded initialized Tilemap points have one current Image, TileSet, PreparedImage,
SoundBank and Scene instance. Those current-store points exclude other retained captures and do
not prove a resource plateau or complete retained-memory bound.

| Cells | Per-run stable-gameplay App-to-post-Host P50 range | P95 range | Original backend tracked-payload high water in each run | Final Meadow quads / draw calls |
| ---: | ---: | ---: | ---: | ---: |
| 64 | 16.528–16.557 ms | 16.734–17.094 ms | 2,311,652 bytes | 70 / 6 |
| 4,096 | 16.485–16.572 ms | 16.784–17.061 ms | 2,634,212 bytes | 4,102 / 6 |
| 16,384 | 16.239–16.466 ms | 16.722–17.582 ms | 3,617,252 bytes | 16,390 / 6 |

These native wall spans include waits. In run one, stable-frame Surface-acquisition medians are
15.635/14.635/10.608 ms for the three populations; program-execution medians are
0.475/0.473/0.566 ms. Component medians are separate distributions and must not be added into a
supposed median frame. Unmeasured App remainder includes extraction and other work; it does not
identify one CPU stage. Native pacing and warm state are not controlled across sequential batches,
so no before/after speedup or throughput inference follows from these ranges.

Observed Scene-transition frame counts are 21/21/21, 68/67/66 and 229/222/225. Complete initialized
measurement intervals are about 1.94–2.01, 2.76–2.82 and 5.51–5.65 seconds. These are finite
correlated observations, not independent samples, pure preparation times or a production P99
budget. Source generation, original startup, first initialized frames, transition frames and
stable gameplay remain distinct in the raw reports. Each original native lifetime retains one
Adapter/Device request pair and reuses it through all Scene changes. Independent replay later
creates its own Host outside the measured interval; it can affect subsequent warm driver state.

Whole-process private-byte points span roughly 499–774 MiB at the first initialized boundaries
and 501–752 MiB at final boundaries across the nine Tilemap runs. They include driver/code/allocator state,
background work and measurement retention. They are not attributable content bytes, allocation
peaks, physical GPU residency or leak evidence. Native GPU points cover only the existing tracked
payload model; they cannot be compared directly to process private bytes.

#### Existing Scene Work Span Attribution

The initial `a58c630` attribution below is rejected as complete cross-thread evidence. Its
thread-local subscriber did not reach existing IO workers: one expansion does not cover the
actual six-instance workflow. The 109-entry tables describe only a partial thread cut. Native
gameplay, pixel and cleanup assertions passed, but did not prove worker-span coverage. Retain
these raw reports as the incomplete observation attempt and do not rank complete preparation
costs from them. The correction requires actual worker entries, exact six original preparations/
publications and no unfinished spans after physical Runtime retirement, before replay.

The same native consumer was rerun with the opt-in `scene-work-observation` feature. It attaches
to the existing `nara::scene::work` spans; it does not add a profiler owner or alter the default
runtime. Raw reports are [64](data/2026-11-14-development-runtime/authored-tiles-scene-work-64.json),
[4,096](data/2026-11-14-development-runtime/authored-tiles-scene-work-4096.json),
[16,384](data/2026-11-14-development-runtime/authored-tiles-scene-work-16384.json) and
[baseline](data/2026-11-14-development-runtime/authored-tiles-scene-work-baseline.json), with
[source and executable metadata](data/2026-11-14-development-runtime/authored-tiles-scene-work-metadata.json).
The feature-qualified native cohort passes 4/4 with the same baseline, Tilemap and panic-cleanup
workflow. The existing metric controls with the feature pass 138/138.

Each historical run observes 109 Scene span entries. The table shows the range across three
runs per population; values are partial nested wall spans and can overlap. `scene_decode`
means compiled-component preflight/native codec validation, not source I/O or JSON/RON parsing.
The missing worker spans prevent a complete ranking of preparation costs. Nested values cannot
be summed into CPU time or an end-to-end preparation duration.

| Cells | All observed Scene spans | `scene_decode` | `scene_expand` | `scene_flat_admission` | `scene_prepare_unbound` | `scene_bind` | `scene_publish` |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 64 | 8.883–9.439 ms | 2.991–3.182 ms | 0.426–0.768 ms | 0.322–0.579 ms | 3.070–3.113 ms | 0.466–0.483 ms | 0.500–0.554 ms |
| 4,096 | 96.911–126.164 ms | 41.525–56.475 ms | 10.085–13.000 ms | 5.621–7.415 ms | 37.105–50.226 ms | 0.438–0.624 ms | 0.726–0.832 ms |
| 16,384 | 816.388–1,052.739 ms | 233.047–322.339 ms | 200.378–241.257 ms | 33.138–42.060 ms | 201.786–278.200 ms | 0.482–0.551 ms | 1.448–1.496 ms |

The partial observation suggests investigation targets but cannot rank complete worker and owner
work. Sprite extraction/queue CPU remains unmeasured. The current source layout is a confound for 16K because
it uses root-bound Prefab halves. Do not add a cross-frame cache or skip extraction from dirty
hints before measuring the owner-local path: existing tests require full observation after cleared
hints, current TileSet/Image authorization, source order, multi-View diagnostics and retained
captured inputs. The next candidate is a narrow per-extraction TileSet context/UV reuse or a
separate existing-owner extraction span, with the complete native consumer as the acceptance gate.

#### Corrected Original Worker Coverage And Per-Map Extraction

The corrected binary installs one optional process-wide subscriber before Runtime construction.
Each new span retains its original observation run, even if it exits after that run is detached.
Owner and worker calls are reported separately; unfinished, retained or same-ID reentrant spans
reject sealing. Physical Runtime retirement precedes sealing, and independent GPU replay follows
it. This is measurement-binary instrumentation, not a production scheduler or task-stop barrier.
Failure reports retain original observation/retirement facts; stderr carries a short diagnostic.

New reports are [64](data/2026-11-14-development-runtime/authored-tiles-scene-worker-64.json),
[4,096](data/2026-11-14-development-runtime/authored-tiles-scene-worker-4096.json),
[16,384](data/2026-11-14-development-runtime/authored-tiles-scene-worker-16384.json) and
[baseline](data/2026-11-14-development-runtime/authored-tiles-scene-worker-baseline.json), with
[exact source/executable/report metadata](data/2026-11-14-development-runtime/authored-tiles-scene-worker-metadata.json).
The complete hardware DX12 cohort passes 4/4 at `67547b48-173f-4906-a027-82289f09380e`.
Nine Tile lifetimes retain 1,655 initialized presentations and 27 accepted original-input GPU
replays, including exact later rereads and original Host/prefix cleanup. Three baseline lifetimes
retain 261 initialized presentations. Both observer-panic controls retain complete Scene
observation and original physical cleanup, plus copied-source removal where applicable.

Each complete run now records 149 entries. Expansion and flat admission each record one owner
and eight worker entries; compiled-component preflight records seven owner and eight worker
entries. Unbound preparation, bind and publish each record six owner and zero worker entries.
These are observed operation counts, not a one-to-one claim between source-preparation attempts
and the six committed Scene instances. Every report has zero unfinished entries and retained spans.

| Cells | Overlapping named-span wall sum | Compiled preflight `scene_decode` | `scene_expand` | `scene_flat_admission` | `scene_prepare_unbound` | `scene_bind` | `scene_publish` |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 64 | 18.582–21.246 ms | 6.321–7.342 ms | 3.957–4.252 ms | 2.941–3.182 ms | 3.014–3.960 ms | 0.467–0.531 ms | 0.561–0.620 ms |
| 4,096 | 270.742–335.801 ms | 85.402–102.067 ms | 83.601–106.055 ms | 59.031–72.404 ms | 37.301–48.749 ms | 0.457–0.518 ms | 0.771–0.940 ms |
| 16,384 | 4,239.803–4,776.749 ms | 558.751–660.309 ms | 1,844.166–2,038.457 ms | 370.201–504.269 ms | 203.776–303.773 ms | 0.491–0.638 ms | 1.493–2.190 ms |

Ranges cover three runs per population over the entire original Travel lifetime. They overlap,
include worker operations and are not additive CPU time, a single transition delay or a speedup
against the rejected thread-local report. Source I/O and JSON/RON parsing remain unmeasured
individually. The 16K root-bound Prefab representation is still a source-layout confound.

The same qualified source introduces a private per-map extraction context: current authorized
image/material and transformed tile axes are read once, and only the latest index/UV result is
reused within that map's current complete scan. Invalid entries still count per cell; per-cell
center, color and Y-sort calculations retain their original order and floating-point operations.
There is no cross-frame cache or dirty-only extraction. Full Sprite controls pass 56/56 at
`2dd0e831-f328-4e9f-9935-3312e0d8c7fa`, including an independent all-field multi-map oracle,
repeated/revisited valid and invalid indices, and next-frame atlas/size/sampler changes. Existing
authorization, ABA, cleared-dirty refresh and retained captured-image tests remain. This reduces
repeated operations by inspection; extraction time and a controlled elapsed improvement remain
unqualified.

Game/library/metric/original-thread controls pass 139/139 at
`bc6bdf20-73d9-47ac-a51b-81ad35982046`. The actual worker barrier control rejects sealing while
an old worker is active and proves its later exit cannot charge the next run. Independent reviews
close the missing worker coverage, incomplete failure reports and misleading decode semantics.
The first corrected native cohort was RED 1/4: the evidence variable incorrectly named a
directory, a repeated full-report error string overflowed bounded logs, and a 16K run ended with
the generic `Completed(Requested)` diagnostic. The invocation and diagnostic retention are
corrected before the complete passing run above. That pass does not establish the cause of the
earlier native stop or a statistical reliability bound.

The complete feature-off native cohort also passes 4/4 at
`57d61c33-2a4f-4781-a547-fde00c9599f9`, retaining disabled Scene records on the two panic paths
and the same original gameplay, GPU replay and physical cleanup assertions. This is a separate
feature/configuration control, not a controlled before/after performance comparison.

Observation correction is committed at `a91e68c`; the private extraction context is `f25312c`.
Scoped release correctness Clippy passes for the Sprite package's all-target/all-feature closure
and the game workload/metric/span targets. Root/sample formatting, workflow YAML, added document
links, selected-source hashes and raw-report hashes pass; existing warnings remain. All four
scoped implementation/review agents are closed. Cargo commands run serially against the original
target; no cache clean, remote CI execution or remote publication occurs. The first Clippy
invocation used the wrong hyphenated package name and is replaced by the successful
`nara_sprite_render` invocation; it did not change source.

Continue with owner-local Scene/source-read/expansion attribution under the same complete
consumer, retaining default admission, current authorization and original result/lifetime
contracts. Numeric runtime budgets, extraction timing, foreign execution and full Foundation/
R9/R12 remain open; Editor/UI/Dialogue product work remains paused.

#### Source Read And Candidate Decode Split

The `a7c7a27` successor separates original content reads and source decoding from compiled native
preflight, and divides staged expansion into program composition, component materialization and
reference closure. Its [intermediate 16K report](data/2026-11-14-development-runtime/authored-tiles-source-before-16384.json)
and [source/executable metadata](data/2026-11-14-development-runtime/authored-tiles-source-before-metadata.json)
retain three complete ordinary CLI Travel lifetimes, original physical retirement and nine
original-input GPU replays before value-cost reuse. This is an attribution baseline; final
wrapper/containment and changed compiler boundary qualification remain required below.

| Original envelope | Wall range across three complete 16K lifetimes | Owner / worker calls per lifetime |
| --- | ---: | ---: |
| `project_content_read` | 75.700–82.034 ms | 15 / 120 |
| `scene_source_decode` | 1,181.626–1,206.328 ms | 1 / 8 |
| `prefab_source_decode` | 1,148.630–1,160.499 ms | 1 / 8 |
| `scene_expand` | 1,725.355–1,778.274 ms | 1 / 8 |
| `scene_structure` | 1,188.633–1,201.951 ms | 1 / 8 |
| `scene_program_composition` | 0.063–0.083 ms | 1 / 8 |
| `scene_component_materialization` | 506.802–542.232 ms | 1 / 8 |
| `scene_reference_closure` | 0.017–0.027 ms | 1 / 8 |
| `scene_compile_output` | 29.428–33.639 ms | 1 / 8 |

The read envelope is `LoadContext::read_file_inner`: capability open, bounded reads, digesting
and budget bookkeeping, including catalogue metadata bytes read through it. Separate
SourceReadiness reads and catalogue discovery remain outside it. Candidate decode includes wire
checks, header/payload parsing, domain limits and canonicalization; later composition-source
admission and retained-size bookkeeping are outside. Nested Prefab acquisition/decoding belongs
inside `scene_structure`, so that phase is not pure layout work. The raw report's previous scope
sentence predates these hooks; its metadata and subsequent executable text correct that wording
without changing measured data. These wall envelopes overlap and are not additive CPU time.

The nine expansions are semantically required by the existing script: startup plus four Travels
each preparing another checkpoint level for progress validation and then the destination.
Publication is startup, four destination commits and one Retry. Retry uses resident compiled
source; cancelled validation preparations retain original custody and must not be skipped.

Source-confirmed repeated whole-value cost scans make existing component materialization/output
the selected bounded optimization. Reuse is private to one compiler candidate and must refresh
after migration, defaults and Properties changes, then disappear on recomposition. Full reference
planning and visit admission remain even for empty plans. Added metadata pays the original work
allowance. This cannot solve the larger source-decoding envelope; no improvement is claimed
until the modified compiler and complete consumer are qualified.

#### Validated Value Cost Qualification

The compiler now carries the complete cost returned by existing normalized-value validation in
its private reference program. Migration/default expansion precedes that fact. Properties
Set/Absent replaces value and cost together after final validation and invalidates the old plan.
Opaque records carry admitted raw cost; selected contextual defaults carry independent costs.
Reference closure leaves values unchanged; output applies its existing old/new reference delta
to that same cost. Recomposition drops the candidate and its costs together. Complete source and
depth checks, reference planning and plan visits remain; even empty plans pay original work.
Added metadata pays the existing component-entry allowance or `size_of::<ReferenceProgram>()`.
There is no public record, cross-compilation cache or increased ceiling.

Qualified positive reports are [64](data/2026-11-14-development-runtime/authored-tiles-source-cost-64.json),
[4,096](data/2026-11-14-development-runtime/authored-tiles-source-cost-4096.json),
[16,384](data/2026-11-14-development-runtime/authored-tiles-source-cost-16384.json) and
[baseline](data/2026-11-14-development-runtime/authored-tiles-source-cost-baseline.json), with
[selected source/executable/test/report hashes](data/2026-11-14-development-runtime/authored-tiles-source-cost-metadata.json).
The new source hooks record 311 Scene entries per 64/4K lifetime, 329 per 16K and 275 per baseline.
Those counts include content reads and staged subphases; they are not comparable to the earlier
149-entry envelope as an amount of work or a performance score.

| Qualified scope | Actual result |
| --- | --- |
| Original root content/residency and public receiving-limit controls | 81/81; 867 outside filter; `5f7edfe7-3856-4320-bf0e-0ddcfc230414` |
| Complete Scene/Reflect all-feature library controls | 981/981, no skips; `919378ba-391d-4097-8fad-507cba2ace0a` |
| Public finite reference-entry/conversion, including value-bearing recomposition | 5/5; `9ca1d716-a883-4906-8d98-89ffbb30aba3` |
| Release game/library/metric/span controls and all four native wrappers | 143/143, no skips; `5b9cc68a-ac8f-4742-8865-cf9a622e65de` |

Nine Tile lifetimes retain 2,379 initialized presentations and all previous gameplay/retirement
facts; three baseline lifetimes retain 257. Twenty-seven original inputs pass GPU replay and
exact retained-output rereads, with original Host/prefix cleanup. Both panic controls retain
complete observation and physical cleanup, including copied-source removal for Tilemaps.
Positive source observations require actual owner/worker reads and Scene candidate decodes,
plus worker Prefab candidate decode at 16K, and zero unfinished or retained spans. These are
physical Windows/hardware DX12 results, not foreign platform, GPU duration or scanout evidence.

The new public conversion removes the separate watcher query, so only the selected component
can demand both P occurrences. S/1, P/1 and P/2 reads witness its two reference-driven
recompositions. An independently known one-component/two-node/fourteen-byte effective output
fits, while node/byte one-short limits preserve exact diagnostics. Source/provenance, unchanged
prepare-time document/history, commit and Undo/Redo are checked together. Unit controls cover
layered growth/shrink/Absent, migration/default replacement and opaque raw/hidden-default
admission. Independent review reports no P1/P2. Observed exact/one-short work tests alone do
not independently prove every metadata debit; the unchanged full plan visits and explicit
metadata charge are also source-reviewed. A broad opaque Addition/Set/root-binding matrix and
combined contextual-default/recomposition cost matrix are not newly qualified here.

For a finite comparison, four sequential processes run before/after/after/before on the same
host, compiler/profile and phase hooks. Each completes three original 16K Travel lifetimes and
nine GPU replays. All twelve lifetimes and 36 replays pass with the same actual RTX4090/DX12
adapter. Raw reports are [1 before](data/2026-11-14-development-runtime/authored-tiles-source-cost-paired-1-before.json),
[2 after](data/2026-11-14-development-runtime/authored-tiles-source-cost-paired-2-after.json),
[3 after](data/2026-11-14-development-runtime/authored-tiles-source-cost-paired-3-after.json) and
[4 before](data/2026-11-14-development-runtime/authored-tiles-source-cost-paired-4-before.json).
[Paired metadata](data/2026-11-14-development-runtime/authored-tiles-source-cost-paired-metadata.json)
retains exact old/new executable and source hashes, report hashes, order and limits. The old
executable is frozen outside the repository; no source checkout is rewritten to compare versions.

| Named wall envelope, median over six full lifetimes per version | Before | After |
| --- | ---: | ---: |
| `scene_source_decode` | 1,236.780 ms | 1,323.715 ms |
| `prefab_source_decode` | 1,225.452 ms | 1,325.417 ms |
| `scene_component_materialization` | 591.236 ms | 592.519 ms |
| `scene_compile_output` | 38.455 ms | 0.145 ms |
| `scene_expand`, including nested Prefab loading | 1,891.559 ms | 1,969.560 ms |

Compile-output samples range over 32.271–48.981 ms before and 0.133–0.166 ms after. That local
decrease is consistent with removing its whole-value cost traversal. Materialization medians
are nearly equal; complete expansion and whole-process wall do not demonstrate a consistent
improvement. Process walls in order are 24.814, 27.878, 25.873 and 26.834 seconds, including source
generation, native startup/waits and independent replay. Named spans overlap, and driver/cache/
clock state and unrelated host load are uncontrolled. There is no statistical, CPU, frame-rate
or end-to-end speedup claim. Subsequent work should target source/candidate decoding through its
existing owners while retaining complete wire limits, errors, source freshness and cleanup.

Source attribution is `a7c7a27`, and private cost reuse plus owner/public regressions are
`4aaa3d8`. No-default-feature Scene and feature-off workload targets compile. Scoped root
content, Scene all-target/all-feature and workload/metric/span release correctness Clippy pass,
as do root/sample formatting and exact source/report/test hashes, added document links and
whitespace checks. Existing warnings remain. All four scoped agents are closed. Cargo and native
workloads run serially against the original target; no cache clean, remote CI execution or
remote publication occurs. Full Foundation/R9/R12 and paused Editor/UI/Dialogue work remain open.

#### Component Container Payload Decode Successor

The `e40f2a5` observation extends the same source workload with four original format phases:
wire-shape preflight, header decode, payload decode and domain/canonical finish. Byte, wire,
header contract and payload gates retain their original order. The
[pre-streaming 16K report](data/2026-11-14-development-runtime/authored-tiles-payload-before-16384.json)
and [selected source/executable hashes](data/2026-11-14-development-runtime/authored-tiles-payload-before-metadata.json)
retain three complete original Travel lifetimes, physical retirement and nine GPU replays.
Across eighteen Scene/Prefab decodes per lifetime, payload wall sums are 3,436.010–3,814.791 ms,
wire preflight 592.815–746.551 ms, header decode 68.950–79.720 ms and finish 74.173–82.646 ms.
These are nested lifetime totals rather than one transition delay, CPU time or scanout.

Source review identifies unconditional untagged buffering in Reflect's component-value adapter,
including canonical type-before-value containers. The selected reduction uses ordinary serde
MapAccess/SeqAccess and seeds for already known List/Map payloads. Scalars, unknown tags and
value-before-type preserve the previous payload classification; serialization stays unchanged.
The top-level struct entry remains, while nested values accept the original buffered Map/Seq
forms. Strict wire-field checks, business-map insertion behavior, RON struct/quoted keys and
parent-buffered values are retained. No custom JSON/RON parser or cross-load cache is introduced.

The characterization first passes 28/28 on the original decoder, including the existing public
authored-reference control. A first streaming implementation also passes that matrix but misses
direct RON child tuples and brace maps. Independent review supplies the actual counterexamples;
the nested-value entry is corrected and the exact inputs are added in both outer field orders
and parent-buffered contexts. The strengthened matrix passes 29/29 at
`898eb88b-c488-459c-8305-68d94f96459d`. Numeric integer/float coercions, finite/signed zero,
Null presence, duplicates/unknown fields, EntityRefs and JSON/RON ordering remain covered.
Semantic child errors can point to an earlier real parser location; no synthetic legacy cursor
is generated. Payload stage, syntax/data classification and actual parser locations remain.

Complete Reflect/Scene library controls pass 1,010/1,010 at
`80570866-d780-40ba-bdf8-45f79a405323`, public format/reference conversion 36/36 at
`52a4c6d3-daa4-49f3-b434-4b1367a51093`, and original content/residency/public limits 81/81 at
`4383d866-1d2a-4b51-b555-f82b7203ec80` with 867 outside the filter. The original receiving
predecode reservation, exact quota refusals, failed-load refunds and retained source generations
remain. Container construction is owned by the current visitor and released on failure; the
logical retained-byte model is not a physical peak-memory or Vec-capacity guarantee. Final native
game/pixel/failure controls and measured before/after results remain required below.

#### Streamed Container Qualification And Comparison

The final release game/library/metric/span/native cohort passes 143/143 at
`9e5dfac7-e5d6-43d6-9755-96c4819ca17f`, without skips. Nine original Tile lifetimes retain 1,951
initialized presentations; three baseline lifetimes retain 273. Both panic-cleanup controls,
twenty-seven original-input GPU replays and exact retained-image rereads pass. Copied sources
are removed, original native/event-loop retirement completes, and original Host/prefix cleanup
remains. Every successful source decode contributes one wire, header, payload and finish call,
with actual owner and worker entries; no registered spans remain live or unfinished at sealing.
There are eighteen format decodes per 16K lifetime, nine for Scene and nine for Prefab.

Raw qualified reports are [64](data/2026-11-14-development-runtime/authored-tiles-payload-stream-64.json),
[4,096](data/2026-11-14-development-runtime/authored-tiles-payload-stream-4096.json),
[16,384](data/2026-11-14-development-runtime/authored-tiles-payload-stream-16384.json) and
[baseline](data/2026-11-14-development-runtime/authored-tiles-payload-stream-baseline.json), with
[source/executable/report metadata](data/2026-11-14-development-runtime/authored-tiles-payload-stream-metadata.json).
The four added format subphases change the registered totals to 347 entries per 64/4K lifetime,
401 per 16K and 311 per baseline. These are named-operation coverage, not a cost score or a
comparison against the previous 329-entry 16K report.

For a finite comparison, the frozen original executable and new executable run in before/
after/after/before order on the same actual RTX4090/DX12 host and locked toolchain/profile. Each
process completes three original 16K Travel lifetimes and nine GPU replays. All twelve lifetimes
and 36 replays pass. Raw reports are [1 before](data/2026-11-14-development-runtime/authored-tiles-payload-paired-1-before.json),
[2 after](data/2026-11-14-development-runtime/authored-tiles-payload-paired-2-after.json),
[3 after](data/2026-11-14-development-runtime/authored-tiles-payload-paired-3-after.json) and
[4 before](data/2026-11-14-development-runtime/authored-tiles-payload-paired-4-before.json).
[Paired metadata](data/2026-11-14-development-runtime/authored-tiles-payload-paired-metadata.json)
retains source/executable/report hashes and order. The qualified native executable and paired
after executable have the same hash. No source checkout is rewritten to compare versions.

| Named wall envelope, median over six complete lifetimes per version | Before | After |
| --- | ---: | ---: |
| Wire-shape preflight | 629.486 ms | 633.750 ms |
| Header decode | 78.489 ms | 77.933 ms |
| Payload decode | 3,463.926 ms | 1,197.454 ms |
| Domain/canonical finish | 77.124 ms | 85.186 ms |
| Scene candidate decode | 2,144.028 ms | 960.831 ms |
| Prefab candidate decode | 2,114.618 ms | 1,008.948 ms |
| Component materialization | 1,275.166 ms | 1,397.676 ms |
| Expansion, including nested Prefab source loading | 3,477.677 ms | 2,504.648 ms |

Payload-decode samples range over 3,362.480–3,863.958 ms before and 1,050.214–1,347.691 ms after.
Its median decreases by 65.43% in this finite observation; complete expansion's median decreases
by 27.98%, while wire/header medians remain nearly unchanged. Component materialization is
higher, so the data does not imply every stage improves. Nested phases overlap and cannot be
summed into total CPU or transition delay.

Complete process walls in order are 43.919, 34.897, 34.488 and 42.650 seconds: both new groups
finish sooner than both original groups. This endpoint includes private source generation,
native startup/pacing/waits, three complete games and independent replay. Original initialized
presentation counts total 3,300 before and 2,550 after; waiting-for-source frames are part of the
ordinary workflow, so those counts are not rendering throughput evidence. Driver/filesystem/
clock state, allocator state and unrelated host load remain uncontrolled. This is observed
wall-time improvement for the declared workload, not CPU/GPU duration, scanout, statistical
confidence, a production tail budget or a physical-memory bound. Value-before-type and scalar
paths retain their original classification and are not claimed to have the same improvement.

The observation code is `e40f2a5`; the streaming adapter and owner regressions are `920cd9a`.
Additional public Reflect catalog/bundle/authored/durable-reference controls pass 20/20 at
`958f73cd-de1a-42b2-978b-378031da424c`. Feature-free Reflect/Scene and feature-off workload targets
compile. Owning all-target/all-feature and game workload/metric/span release correctness Clippy,
root/sample formatting and source/executable/report integrity checks pass; existing warnings
remain. Independent wire-correctness and resource/error reviews close without P1/P2 after the
RON child-form correction. The semantic-location control requires typed stage, data classification
and a real positive parser coordinate, not an exact old buffering cursor. All four scoped agents
are closed. No Cargo/native workloads overlap, no cache is cleaned, and no remote CI execution
or publication occurs. Continue existing-owner investigation of remaining wire/normalization/
resource costs; full Foundation/R9/R12, numeric budgets and foreign execution stay open, with
Editor/UI/Dialogue work paused.

#### Original Materialization Cost Return

The [original instrumented 16K report](data/2026-11-14-development-runtime/authored-tiles-value-before-16384.json)
and [exact selected source/executable metadata](data/2026-11-14-development-runtime/authored-tiles-value-before-metadata.json)
retain three complete Travel lifetimes, physical retirement and nine original-input GPU replays
before bounded-cost return. Seven optional subphases split the existing component materializer.
The original full component value, source admission, hidden defaults, Schema semantics and
reference planning remain; the spans do not add another profiler or value owner.

| Original materialization envelope, wall range across three complete lifetimes | Original calls per lifetime | Wall range |
| --- | ---: | ---: |
| Component materialization | 9 | 1,086.821–1,394.656 ms |
| Original-value admission | 180 | 156.483–201.889 ms |
| Value clone | 180 | 290.633–341.635 ms |
| Migration, including post-migration admission | 180 | 160.732–203.455 ms |
| Default materialization | 180 | 131.073–188.435 ms |
| Semantic validation | 180 | 57.301–80.043 ms |
| Reference planning | 180 | 288.431–381.335 ms |
| Reference-program validation | 180 | 0.019–0.023 ms |

These nested wall sums overlap and cover the whole original Runtime lifetime. They are not
instruction-level CPU attribution, one transition delay or allocation measurements. Source
preparation checks and mandatory clones remain separate from the selected reduction.
The value-admission subphase starts after the independent optional source-value policy gate and
includes bounded clone-cost/hidden-default checks and metadata sizing. It is not an envelope
for all source or file admission. Migration includes its original post-migration bounded check;
clone covers the complete candidate record, including retained contextual declarations.

The original Reflect raw materialization walker already counts complete nodes/logical bytes
while enforcing depth and source-reference policy. It now returns those counters on success;
the Scene caller does not rescan the same value only to recover cost. Native/admitted wrappers
preserve their previous unit success and exact failure identity. The owned default materializer
returns checked cost only when insertion already used complete preflight; without insertion it
returns no new cost and preserves the prior-admission no-scan move path. Scene carries raw cost,
replaces it after migration and takes either inserted-default cost or the unchanged value's cost.
Full semantic validation, reference planning/visits and final reference rewrite delta remain.
Properties Set/Absent still obtains cost from the final validated candidate. Opaque records and
hidden defaults retain their separate existing admission. There is no reusable cross-value
witness, cross-compilation cache, new budget owner or higher limit.

Production Scene/Material all-feature library checks pass. Focused new/existing cost/reference/
default/property/quota controls pass 33/33 at `94ef707d-93d7-4849-8462-6236b9ccb177`, with 982
outside the filter. Full Reflect/Scene/Material library controls pass 1,094/1,094 at
`591b3a6f-6a3d-4bde-b991-c23012233d6c`, public Reflect defaults/encoding/property-reference
controls 26/26 at `02863b1e-1122-4ff6-a19b-cc0167171cff`, and public Scene format/reference
conversion 36/36 at `df30cdc4-db4a-4c4d-bac6-ab8a03f8215a`. The latter retains value-bearing
reference-driven recomposition, exact output quotas, source/history and commit/Undo/Redo.
Five new owner controls compare returned costs with independently known complete values,
including authored routes, zero-byte values, original error order and no-insertion allocations.
Independent correctness review checks all hidden callers and original reference/error accounting
without P1/P2. Receiving-limit and complete native comparison results remain required below.

#### Bounded Cost Return Qualification And Comparison

The original root content/residency and public receiving-limit cohort passes 81/81 at
`c089c7f4-6b7e-4dad-8527-148b4cfdadf0`, with 867 outside the filter. The release game/library/
metric/span/native cohort passes 143/143 at `761c1fcb-c2f7-4c0c-911e-3ded640f3c58`, without skips.
Nine Tile lifetimes retain 845 initialized presentations and 27 original-input GPU replays;
three baseline lifetimes retain 193. Both observer-panic controls preserve physical cleanup and
complete span closure, plus copied-source removal for Tilemaps. All original gameplay,
source/receipt retirement, retained-image rereads and original Host/prefix assertions remain.

Raw qualified reports are [64](data/2026-11-14-development-runtime/authored-tiles-value-cost-64.json),
[4,096](data/2026-11-14-development-runtime/authored-tiles-value-cost-4096.json),
[16,384](data/2026-11-14-development-runtime/authored-tiles-value-cost-16384.json) and
[baseline](data/2026-11-14-development-runtime/authored-tiles-value-cost-baseline.json), with
[exact selected source/executable/report metadata](data/2026-11-14-development-runtime/authored-tiles-value-cost-metadata.json).
Each of the seven new value phases has 171 operations per 64/4K lifetime, 180 per 16K and 162
per baseline, including both owner and worker execution. Original raw-cost/clone/default/semantic/
reference phases retain corresponding call counts. No spans remain live or unfinished after
physical Runtime retirement. Registered totals are 1,544/1,544/1,661/1,445 respectively; adding
named envelopes does not mean more semantic work or constitute a performance score.

The frozen original and changed executables also run before/after/after/before on the same
actual RTX4090/DX12 adapter, compiler/profile and 16K source workload, with identical phase hooks.
All twelve complete lifetimes and 36 GPU replays pass. Raw reports are
[1 before](data/2026-11-14-development-runtime/authored-tiles-value-paired-1-before.json),
[2 after](data/2026-11-14-development-runtime/authored-tiles-value-paired-2-after.json),
[3 after](data/2026-11-14-development-runtime/authored-tiles-value-paired-3-after.json) and
[4 before](data/2026-11-14-development-runtime/authored-tiles-value-paired-4-before.json), with
[source/executable/report/order metadata](data/2026-11-14-development-runtime/authored-tiles-value-paired-metadata.json).
The qualified native and paired-after executable hashes match; the frozen original stays outside
the repository, without rewriting a source checkout to compare versions.

| Named wall envelope, median over six complete 16K lifetimes per version | Before | After |
| --- | ---: | ---: |
| Original-value cost/clone admission | 82.823 ms | 46.477 ms |
| Value clone | 181.842 ms | 198.354 ms |
| Migration, including post-migration admission | 81.628 ms | 47.319 ms |
| Default materialization | 52.881 ms | 45.535 ms |
| Semantic validation | 19.397 ms | 1.306 ms |
| Reference planning | 90.226 ms | 115.722 ms |
| Reference-program validation | 0.013 ms | 0.013 ms |
| Complete component materialization | 514.843 ms | 460.009 ms |
| Expansion, including nested Prefab acquisition | 1,092.808 ms | 1,045.651 ms |
| Source payload decode | 658.323 ms | 663.890 ms |

Admission, post-migration admission and semantic validation decrease where the same value's
pure cost walk was removed. Complete component-materialization median is 10.65% lower in this
finite observation. Clone and reference planning are higher; the result does not imply every
stage improves, and the measurements do not establish the cause of those differences. Nested
envelopes overlap and cannot be added into a CPU total or a single transition delay.

Complete process walls in order are 20.277, 20.809, 20.414 and 20.199 seconds. Both changed
groups are slower than both original groups, despite fewer pure cost walks and lower local
materialization. There is no demonstrated end-to-end speedup. Those endpoints include source
generation, native startup/pacing/waits, complete gameplay and independent replay. Initialized
presentation counts total 794 before and 786 after, not rendering throughput. Driver/filesystem/
clock state, allocator state and host load are uncontrolled; these finite wall observations
are not CPU/GPU duration, scanout, statistical confidence, production-tail or physical-memory
bounds. Continue owner-local runtime work without relaxing complete source, semantic, reference
or physical-retirement contracts.

Implementation and owner regressions are `e9c6f5d`. Feature-free Reflect cost/reference controls
pass 17/17 at `766afa03-a7e1-4a2d-9e4b-3e59ac5f6848`, with 178 outside the filter. Feature-free
Reflect/Scene/Material libraries and the observation-off workload compile. Owning all-target/
all-feature and game workload/metric/span release correctness Clippy, root/sample formatting
and selected source/executable/report integrity pass; existing warnings remain. Independent
correctness review and hidden-caller/reference/error-accounting follow-up close without P1/P2.
Both scoped agents are closed. No Cargo/native workloads overlap, no cache is cleaned and no
remote CI execution or publication occurs. Full Foundation/R9/R12, production budgets and
foreign execution remain open, with Editor/UI/Dialogue work paused.

#### Reference Planning Without Duplicate Payload Scans

The [owning successor](../plans/2026-11-13-rust-state-reconstruction-plan.md#reference-planning-without-duplicate-payload-scans)
reuses the original first raw reference scan's complete node/byte counts within one collector
call. Schema interpretation still follows that full bounded preflight. If no EntityRef occurred
and checked addition of actual Schema reference-result counts fits both receiving limits, the
final repeated payload scan cannot discover an undeclared reference or a new combined-limit
failure. EntityRef-bearing, overflowing and over-limit cases retain the original final walker,
including its error order. Non-null TileSet AssetRefs contribute result bytes and are included in
the proof; an earlier zero-output-only condition would not cover this ordinary Tilemap workload.

No source, Schema, depth, capability, shape, reference-visit or rewrite admission is removed.
Actual cumulative traversal and scratch charges decrease only with the work no longer performed;
all configured ceilings and the immutable-source candidate clone remain. The change introduces
no cross-value witness, source cache or new authority. The same optional phase hooks and complete
gameplay/GPU-replay/physical-retirement endpoints remain the comparison surface. Passing
receiving-limit tests alone is not a performance claim.

The frozen original and `d5acd0d` executables run before/after/after/before with the same original
16K Travel workload, phase hooks, compiler/profile and actual RTX 4090/DX12 adapter. All twelve
complete lifetimes and 36 original-input GPU replays pass. Original gameplay, source removal,
physical native retirement and complete owner/worker span closure remain required. Raw reports
are [1 before](data/2026-11-14-development-runtime/authored-tiles-reference-paired-1-before.json),
[2 after](data/2026-11-14-development-runtime/authored-tiles-reference-paired-2-after.json),
[3 after](data/2026-11-14-development-runtime/authored-tiles-reference-paired-3-after.json) and
[4 before](data/2026-11-14-development-runtime/authored-tiles-reference-paired-4-before.json), with
[exact source/executable/report/order metadata](data/2026-11-14-development-runtime/authored-tiles-reference-paired-metadata.json).

| Named wall envelope, median over six complete 16K lifetimes per version | Before | After |
| --- | ---: | ---: |
| Original-value cost/clone admission | 45.009 ms | 45.004 ms |
| Value clone | 179.751 ms | 176.689 ms |
| Migration, including post-migration admission | 44.567 ms | 43.693 ms |
| Default materialization | 43.377 ms | 42.518 ms |
| Semantic validation | 1.279 ms | 1.233 ms |
| Reference planning | 105.020 ms | 49.845 ms |
| Reference-program validation | 0.014 ms | 0.015 ms |
| Complete component materialization | 420.116 ms | 361.724 ms |
| Expansion, including nested Prefab acquisition | 996.156 ms | 945.196 ms |
| Source payload decode | 653.219 ms | 660.926 ms |

Reference-planning wall median is 52.54% lower, complete component materialization 13.90% lower
and expansion 5.12% lower in this finite comparison. Smaller differences in other envelopes are
not attributed to this optimization. Complete process walls in order are
19.190, 18.975, 18.742 and 19.728 seconds. Both changed groups are faster than both originals in
this observation, unlike the preceding bounded-cost-return comparison. These endpoints also
include source-copy generation, native startup/pacing/waits, complete gameplay and independent
replay. Initialized presentation counts are 764 before and 730 after; they are not rendering
throughput. Six correlated lifetimes per version and uncontrolled host/driver/cache/clock state
do not establish statistical confidence, a general speedup, CPU/GPU duration, physical-memory or
production-tail budgets. Nested wall envelopes overlap and must not be added into a CPU total
or one transition latency.

The original root content/residency and public receiving-limit controls pass 81/81 at
`2f4f749f-9dbb-42e0-85a8-2a8e1d09affc`, with 1,547 outside the filter and all 81 original test
names retained. Complete release game/library/metric/span/native controls pass 143/143 without
skips at `8cfca769-639e-4c18-ac9c-8f937602b92f`. Nine 64/4K/16K native Tile lifetimes retain
771 initialized presentations and 27 original-input GPU replays; three baseline lifetimes
retain 195. Both observer-panic controls and all original gameplay, copied-source removal,
receipt retirement, retained-image rereads and original Host/submission-prefix cleanup pass.
Raw reports are [64](data/2026-11-14-development-runtime/authored-tiles-reference-scan-64.json),
[4,096](data/2026-11-14-development-runtime/authored-tiles-reference-scan-4096.json),
[16,384](data/2026-11-14-development-runtime/authored-tiles-reference-scan-16384.json) and
[baseline](data/2026-11-14-development-runtime/authored-tiles-reference-scan-baseline.json), with
[source/executable/report and qualification metadata](data/2026-11-14-development-runtime/authored-tiles-reference-scan-metadata.json).
The qualified executable hash matches the frozen paired-after executable. Complete all-feature
Reflect/Scene/Material libraries pass 1,102/1,102; public Reflect/Scene controls pass 26/26 and
36/36, and new feature-free controls pass 8/8 with 195 outside their filter. The owning plan
records their exact run IDs. Independent final-condition and test review finds no P1/P2.
Owning all-target/all-feature correctness Clippy, feature-free Reflect/Scene/Material library
checks, root/sample formatting and source/executable/report integrity pass; existing warnings
remain. All four scoped agents are closed. One superseded intermediate release build was stopped
before execution; final-source qualification passes. No Cargo/native workloads overlap, no cache
is cleaned, and no remote CI or publication occurs. These results close this named optimization;
full Foundation/R9/R12, numeric budgets and foreign execution remain open, with UI/Dialogue paused.

#### Current-Version Migration Cost Reuse

The [owning successor](../plans/2026-11-13-rust-state-reconstruction-plan.md#current-version-migration-cost-reuse)
continues from the qualified `9ed1b1f` executable, frozen before editing. The same seven existing
value-phase hooks remain. Scene previously refreshed the complete raw cost after every successful
owned source migration, including current-version values that the original concrete authority
moves unchanged. Native and Mixed-native invoke callbacks only for strictly advancing migration
chains; Mixed-data and Authoring accept only the current version and return the same owned value.

Scene now reuses its already bounded raw cost when the returned version equals the input version.
Real migration output still receives complete node/byte/depth admission before defaults. The
source-upgrade flag, hidden contextual defaults, actual default insertion, semantic/reference
validation, final effective costs, cumulative working charges and configured limits remain. No
new callback, cache, witness or public interface is needed. The immutable-source candidate still
owns its complete cloned value. The existing grow/shrink/default cost matrix adds a 256-item
unchanged-version case with exact and one-short final limits; it does not introduce another
test harness or change the ordinary native workload.

The frozen original and `81a9d59` executables complete before/after/after/before with the same
original 16K workload, compiler/profile, phase hooks and RTX 4090/DX12 adapter. All twelve complete
lifetimes and 36 original-input GPU replays pass. Raw reports are
[1 before](data/2026-11-14-development-runtime/authored-tiles-current-migration-paired-1-before.json),
[2 after](data/2026-11-14-development-runtime/authored-tiles-current-migration-paired-2-after.json),
[3 after](data/2026-11-14-development-runtime/authored-tiles-current-migration-paired-3-after.json) and
[4 before](data/2026-11-14-development-runtime/authored-tiles-current-migration-paired-4-before.json),
with [exact source/executable/report/order metadata](data/2026-11-14-development-runtime/authored-tiles-current-migration-paired-metadata.json).

| Named wall envelope, median over six complete 16K lifetimes per version | Before | After |
| --- | ---: | ---: |
| Original-value cost/clone admission | 45.029 ms | 44.899 ms |
| Value clone | 179.303 ms | 182.473 ms |
| Migration, including post-migration admission | 43.325 ms | 0.051 ms |
| Default materialization | 42.405 ms | 44.944 ms |
| Semantic validation | 1.206 ms | 1.260 ms |
| Reference planning | 48.204 ms | 63.768 ms |
| Reference-program validation | 0.014 ms | 0.014 ms |
| Complete component materialization | 359.971 ms | 338.568 ms |
| Expansion, including nested Prefab acquisition | 945.356 ms | 937.516 ms |
| Source payload decode | 661.586 ms | 670.802 ms |

The current-version migration envelope is almost eliminated; complete materialization is 5.95%
lower and expansion 0.83% lower in this finite observation. Clone, defaults and reference planning
are higher; their causes are not established. Complete process walls in order are
18.620, 18.620, 18.626 and 18.712 seconds. No meaningful end-to-end speedup is established.
Both versions retain 726 initialized presentations, which is not a throughput measurement.
These endpoints include source-copy generation, startup/pacing/waits, complete gameplay and
independent replay. Correlated lifetimes and uncontrolled host/driver/cache/clock state do not
qualify statistical confidence, general performance, CPU/GPU duration, physical-memory or
production-tail budgets. Nested wall envelopes overlap and are not one transition delay.
Complete release game/library/metric/span/native controls pass 143/143 without skips at
`a2c95a6c-d55d-485f-b0c1-3480b59a2f5f`. Nine 64/4K/16K native Tile lifetimes retain 773 initialized
presentations and 27 original-input GPU replays; three baseline lifetimes retain 191. Both panic
controls and original gameplay, receipts/source retirement, copied-source removal, retained-image
rereads and original Host/submission-prefix cleanup pass. Raw reports are
[64](data/2026-11-14-development-runtime/authored-tiles-current-migration-64.json),
[4,096](data/2026-11-14-development-runtime/authored-tiles-current-migration-4096.json),
[16,384](data/2026-11-14-development-runtime/authored-tiles-current-migration-16384.json) and
[baseline](data/2026-11-14-development-runtime/authored-tiles-current-migration-baseline.json), with
[source/executable/report and qualification metadata](data/2026-11-14-development-runtime/authored-tiles-current-migration-metadata.json).
The qualified executable matches the paired-after hash. Complete Scene library and owned-storage
controls pass 783/783, with 240 unchanged Reflect cases outside the filter; public format, effective
references, hidden defaults and selected-reference controls pass 45/45. Exact IDs and independent review
are retained in the owning plan. This unit does not change cumulative working charges or receiving
limits; it does not claim another run of the preceding root receiving cohort.
Owning Scene all-target/all-feature correctness Clippy, feature-free Scene library checks, root
formatting and source/executable/report integrity pass; existing warnings remain. Two scoped
review/research agents are closed. Work remains serial against the original target without cache
cleaning, remote CI or publication. Full Foundation/R9/R12, numeric budgets, foreign execution and
the separate shared-payload storage design remain open; UI/Dialogue stays paused.

### Component-Local Payload Sharing

Date: 2026-11-17. This comparison starts from the qualified `0db96f5` executable, frozen before
the component storage change implemented at `bcc481f`. Source records privately retain one immutable component-local
allocation; private compilation can share that value into a draft and sealed output. Public
record/document Clone remains deep. Actual default insertion, migration, properties and changed
references isolate storage. Every retained owner still charges the complete logical payload and
backing overhead. The source wire format, receiving ceilings and original lifecycle owners remain.

The unchanged `scene_value_clone` hook now measures admitted record capture: a deep value and
metadata copy before, shared main payload and deep metadata copy afterward. It must not be
described as the same deep-copy operation becoming faster. Complete source/Schema/default/
reference interpretation and original work admission remain. These measurements precede the
separate custom-producer exact-Step correction; the measured Gameplay production sources are
unchanged. Exact source hashes preserve that distinction from later qualification.

The frozen programs complete before/after/after/before with the same original 16K workload,
Rust 1.95 release profile and hardware DX12 adapter. All twelve complete lifetimes and 36
original-input GPU replays pass, including gameplay, source removal, retained-image rereads and
original native/World/Host/submission-prefix cleanup. Original groups retain 726 initialized
presentations and changed groups 720; those paced workload counts are not a throughput result.
Raw reports are
[1 before](data/2026-11-14-development-runtime/authored-tiles-shared-payload-paired-1-before.json),
[2 after](data/2026-11-14-development-runtime/authored-tiles-shared-payload-paired-2-after.json),
[3 after](data/2026-11-14-development-runtime/authored-tiles-shared-payload-paired-3-after.json) and
[4 before](data/2026-11-14-development-runtime/authored-tiles-shared-payload-paired-4-before.json),
with [exact source/program/report/order metadata](data/2026-11-14-development-runtime/authored-tiles-shared-payload-paired-metadata.json).

| Named wall envelope, median over six complete 16K lifetimes per version | Before | After |
| --- | ---: | ---: |
| Original-value cost/capture admission | 44.874 ms | 44.640 ms |
| Record capture | 178.443 ms | 0.026 ms |
| Migration, including post-migration admission | 0.051 ms | 0.036 ms |
| Default materialization | 44.249 ms | 212.121 ms |
| Semantic validation | 1.239 ms | 1.258 ms |
| Reference planning | 64.416 ms | 65.017 ms |
| Reference-program validation | 0.014 ms | 0.018 ms |
| Complete component materialization | 335.002 ms | 323.956 ms |
| Expansion, including nested Prefab acquisition | 938.288 ms | 910.035 ms |
| Source payload decode | 524.793 ms | 526.745 ms |

The component materialization median is 3.30% lower and expansion 3.01% lower in this finite
observation. Source inspection explains why record sharing does not remove the dominant copy
from this workload: its Tilemap source omits `sort_key` and `y_sort_anchor`. The ordinary planner
inserts those defaults, so copy-on-write still copies the full `cells` payload before mutation.
That copy moves into default materialization. Do not rewrite the workload's source to hide this
limitation or claim that sharing eliminated copying for records with missing defaults.

Complete process walls in order are 19.158, 18.514, 19.097 and 18.609 seconds. The changed groups
do not consistently beat their corresponding original groups; no stable end-to-end speedup is
established. Correlated lifetimes, host/driver/cache/clock state, nested overlapping wall envelopes
and the unchanged pacing/control/replay endpoint do not establish CPU/GPU duration, physical-memory
savings, statistical confidence or production-tail budgets. Fully explicit, unchanged payloads
have actual compiler/preparation sharing controls, but those controls are not a second native
performance workload. Whole-unit consumer/native qualification remains recorded in the
[owning plan](../plans/2026-11-13-rust-state-reconstruction-plan.md#component-payload-sharing-through-existing-retained-owners).

Final behavioral qualification also includes the separate App-owned exact-Step correction at `c8f7c7d`.
The release game/library/metric/span/native cohort passes 143/143 without skips at
`a1230ba7-fe18-4fe3-b831-d6703b6c9763`. Nine Tile lifetimes retain 773 initialized presentations
and 27 original-input GPU replays; baseline lifetimes retain 193. Both observer-panic controls
and original gameplay, receipts/source retirement, copied-source removal, retained-image rereads
and original Host/submission-prefix/native cleanup pass. Reports are
[64](data/2026-11-14-development-runtime/authored-tiles-shared-payload-64.json),
[4,096](data/2026-11-14-development-runtime/authored-tiles-shared-payload-4096.json),
[16,384](data/2026-11-14-development-runtime/authored-tiles-shared-payload-16384.json) and
[baseline](data/2026-11-14-development-runtime/authored-tiles-shared-payload-baseline.json), with
[source/program/report and qualification metadata](data/2026-11-14-development-runtime/authored-tiles-shared-payload-metadata.json).
This executable is distinct from the frozen sharing-only comparison, so its results do not
replace the paired program hashes or support attributing the later scheduling change to sharing.
Complete independent Scene/Material/Asset-domain consumers pass 262/262 with one existing timing
case skipped; original App/Input/Gameplay and public scheduling controls pass 250/250. Exact
source/scope/run IDs and review findings belong to the owning plan. Owning/root/consumer correctness
Clippy and six additional independently locked release all-target compile checks pass; warnings
remain. Additional prepared-authoring controls pass 20/20 with three manual measurements ignored,
and feature-free storage/default controls pass 10/10. These are not scalar allocation or UI
performance observations. Paused UI/Dialogue changes remain outside this qualification.

### Frozen Effective Reads

Date: 2026-11-17. The before program is the already qualified `2dd0083` executable, frozen
outside target before build-cache cleanup. The implementation lands at `6ceb534`; the changed
program retains the same source record,
omitted defaults, native Tilemap and original measurement hooks. It freezes selected default
insertions over privately retained payloads and passes borrowed effective reads through actual
validation and codecs. Independently owned cook and mutation still admit and copy complete values.
The after program also includes the separately qualified isolated-migration correction; this
current-version Tilemap workload does not exercise that path.

All four serial before/after/after/before processes pass their original game, GPU and cleanup
checks: twelve complete 16K lifetimes and 36 retained-input replays, six lifetimes per version.
Rust 1.95 release, manifests, locks, population, source generator, phase hooks and actual hardware
DX12 adapter match. Reports are
[1 before](data/2026-11-14-development-runtime/authored-tiles-frozen-reads-paired-1-before.json),
[2 after](data/2026-11-14-development-runtime/authored-tiles-frozen-reads-paired-2-after.json),
[3 after](data/2026-11-14-development-runtime/authored-tiles-frozen-reads-paired-3-after.json) and
[4 before](data/2026-11-14-development-runtime/authored-tiles-frozen-reads-paired-4-before.json), with
[source/program/report hashes and order](data/2026-11-14-development-runtime/authored-tiles-frozen-reads-paired-metadata.json).

| Named wall envelope, median over six complete 16K lifetimes per version | Before | After |
| --- | ---: | ---: |
| Original-value cost/capture admission | 51.859 ms | 76.999 ms |
| Shared record capture | 0.030 ms | 0.032 ms |
| Migration, including post-migration admission | 0.039 ms | 0.041 ms |
| Default materialization | 242.509 ms | 94.931 ms |
| Semantic validation | 1.377 ms | 76.514 ms |
| Reference planning | 75.050 ms | 116.146 ms |
| Reference-program validation | 0.015 ms | 0.015 ms |
| Complete component materialization | 371.721 ms | 367.683 ms |
| Expansion, including nested Prefab acquisition | 1,002.514 ms | 1,009.882 ms |
| Source payload decode | 688.202 ms | 708.473 ms |

The default envelope is 60.85% lower, but more expensive validation and complete traversal offset
most of that difference. Complete component materialization is 1.09% lower and expansion 0.73%
higher in this finite observation. Source inspection confirms that the new frozen-value validation
branch performs a complete materialization-budget walk before shape coverage, while canonical
borrowed reads still traverse all raw descendants. The observations do not isolate each function's
exclusive cost or promise that removing one walk recovers its whole enclosing wall envelope.
The next bounded investigation should compare duplicate complete-budget work at an already
admitted compiler boundary and raw-subtree read overhead, preserving full receiving checks,
per-occurrence accounting and the same complete workload. No new cache owner or packed format
is selected by these results.

Complete process walls in order are 19.375, 19.767, 19.428 and 17.608 seconds. They establish no
stable end-to-end speedup. Correlated lifetimes, uncontrolled host/driver/cache/clock state,
overlapping wall envelopes and unchanged pacing prevent a CPU/GPU duration, physical-memory,
frame-tail or numerical production-budget claim. Allocation identity proves that unchanged cells
retain their actual original backing; it does not by itself measure physical-memory savings.

The changed program's separate qualification completes nine authored 64/4K/16K lifetimes with
2,923 initialized presentations and 27 original-input replays, plus three baseline lifetimes with
485 initialized presentations and both observer-panic cleanup controls. These counts describe
the paced scripts, not throughput. Reports are
[64](data/2026-11-14-development-runtime/authored-tiles-frozen-reads-64.json),
[4,096](data/2026-11-14-development-runtime/authored-tiles-frozen-reads-4096.json),
[16,384](data/2026-11-14-development-runtime/authored-tiles-frozen-reads-16384.json) and
[baseline](data/2026-11-14-development-runtime/authored-tiles-frozen-reads-baseline.json), with
[qualification metadata](data/2026-11-14-development-runtime/authored-tiles-frozen-reads-metadata.json).
The initial combined run passes all 143 game/library/metric/span/native cases and four of five
CPU record-probe cases. Its remaining pointer control compares against the wrong side of a
public deep Prefab clone. The corrected probe resolves the actual original allocation, also
checks that the public clone is independent, and passes 5/5 without changing production code or
the original native trajectory. The owning plan retains exact run IDs and review evidence.
The later public reference-copy input migration has its own owner/reference tests and external
language-consumer compilation; it is outside the earlier frozen measurement program's exact
source hashes. The compiler's private reference plan/in-place route is unchanged by that follow-up.
Full Foundation/R9/R10/R12, numerical budgets and foreign execution remain open.

### Compiler Validation Locality

Date: 2026-11-17. The before program is the qualified frozen-read executable with SHA256
`e341e03ac11fef074ac14a77659ac64e2ca1146b705c9df6464f27d32ddd3017`. Implementation `9fc9322` removes
the repeated complete materialization-budget walk between default materialization and field
semantics inside the original compiler. Standalone dynamic-value validation and receiving
World/definition/resource checks retain their original paths. Both programs use frozen defaults,
the same authored 16K Tilemap, source omissions, native codecs and observation hooks. The
[owning unit](../plans/2026-11-13-rust-state-reconstruction-plan.md#validation-locality-after-the-frozen-read-comparison)
and [pinned source investigation](../research/2026-11-17-large-tilemap-payload-ownership.md#5-validation-at-owning-boundaries)
distinguish this Nara correction from Godot/Bevy's construction and validation policies.

All four serial before/after/after/before processes pass the unchanged original game, copied-source
removal, physical cleanup and retained-input GPU replay checks: twelve complete 16K lifetimes and
36 replays, six lifetimes per version. Rust 1.95 release, manifests, locks, generated source,
measurement hooks and the hardware DX12 RTX4090 adapter match. Summary reports retain all
phase/call counters, acceptance, source, cleanup and replay facts:
[1 before](data/2026-11-14-development-runtime/authored-tiles-validation-locality-paired-1-before.json),
[2 after](data/2026-11-14-development-runtime/authored-tiles-validation-locality-paired-2-after.json),
[3 after](data/2026-11-14-development-runtime/authored-tiles-validation-locality-paired-3-after.json) and
[4 before](data/2026-11-14-development-runtime/authored-tiles-validation-locality-paired-4-before.json),
with [source/program/report hashes and order](data/2026-11-14-development-runtime/authored-tiles-validation-locality-paired-metadata.json).
The exports omit only four raw per-frame arrays; unchanged complete reports remain in the local
frozen observation directory and their original digests are recorded. Summary distributions and
complete source-workload descriptions remain. This avoids retaining another 17.7 MB of repeated
frame traces in the repository; it does not remove any check from the actual executed workflow.

| Named wall envelope, median over six complete 16K lifetimes per version | Before | After |
| --- | ---: | ---: |
| Original-value cost/capture admission | 83.902 ms | 81.219 ms |
| Shared record capture | 0.035 ms | 0.037 ms |
| Migration, including post-migration admission | 0.042 ms | 0.040 ms |
| Default materialization | 100.147 ms | 98.871 ms |
| Semantic validation | 83.977 ms | 0.815 ms |
| Reference planning | 125.855 ms | 123.961 ms |
| Reference-program validation | 0.015 ms | 0.015 ms |
| Complete component materialization | 394.190 ms | 310.312 ms |
| Expansion, including nested Prefab acquisition | 1,056.126 ms | 940.849 ms |
| Source payload decode | 722.578 ms | 716.564 ms |

The semantic-validation envelope is 99.03% lower, complete component materialization 21.28%
lower and expansion 10.92% lower in this finite observation. Before/after semantic-validation
ranges are 78.421-101.279 ms and 0.774-0.853 ms. These are cumulative wall envelopes over the
original compiler work, not a single call's exclusive CPU cost. The original 180 calls per named
value phase, eight worker expansion entries and six owner publications per lifetime remain.
No workload population, source format, schema, receiving ceiling or retained allowance changes.

Complete process walls in order are 21.673, 19.663, 21.925 and 22.214 seconds. Their variation,
correlated lifetimes, overlapping envelopes, unchanged pacing and uncontrolled host/driver/cache/
clock state do not establish a stable whole-process improvement, CPU/GPU duration, physical-memory
saving, frame-tail result or numeric production budget. The old frozen program predates the later
public reference-copy input migration; its exact source receipt is retained. That migration does
not change the compiler's private plan/in-place path exercised here. After-program SHA256 is
`1d80843f10ce3455804d28c5f5d3a6c3921ae2af5da5d6930ff46316d353403a`.

Separate release game/library/metric/span/probe/native qualification passes 148/148 without skips
at `44005346-b3e6-43f8-b7da-9e86dea54b48`, 62.398 seconds after a 9m31s serial build. It includes
baseline plus authored 64/4K/16K travel, 27 original-input replays, both observer-panic cleanup
controls and the complete 16K CPU record probe. The executable used for that qualification and
the paired after runs is identical. Remaining source-value admission, defaults and reference
planning still traverse data; source-compiled receiving reuse requires its own exact-definition
analysis. Full Foundation/R9/R10/R12 and foreign execution remain open.

### Receiving Field Validation Locality

Date: 2026-11-17. The before executable is the qualified compiler-locality program with SHA256
`1d80843f10ce3455804d28c5f5d3a6c3921ae2af5da5d6930ff46316d353403a`. Both versions already freeze
selected defaults and avoid the compiler's immediate repeated cost walk. Implementation `f5c34ff` removes
default planning and repeated frozen-value cost admission from the shared receiving kernel's
field-semantics step. That step follows the same operation's explicit-default check, aggregate
cost admission and Native depth/reference check. Actual schema-index, shape and diagnostic
scratch admission remains; standalone dynamic-value validation still owns default/cost handling.
The [owning unit](../plans/2026-11-13-rust-state-reconstruction-plan.md#receiving-field-validation-locality)
records the common expanded/compiled/processed/receiving paths, unchanged requirements and
reviewed changes to obsolete internal-work oracles.

All four serial before/after/after/before 16K processes complete the original three-lifetime
game/GPU/cleanup trajectory: twelve lifetimes and 36 retained-input replays, six lifetimes per
version. Compiler/profile, manifests/locks, authored population, source generator, observations
and hardware DX12 RTX4090 adapter match. The original 180 calls per named value phase, eight
worker expansion entries and six owner publications per lifetime remain. Reports are
[1 before](data/2026-11-14-development-runtime/authored-tiles-receiving-locality-paired-1-before.json),
[2 after](data/2026-11-14-development-runtime/authored-tiles-receiving-locality-paired-2-after.json),
[3 after](data/2026-11-14-development-runtime/authored-tiles-receiving-locality-paired-3-after.json) and
[4 before](data/2026-11-14-development-runtime/authored-tiles-receiving-locality-paired-4-before.json),
with [source/program/report hashes and order](data/2026-11-14-development-runtime/authored-tiles-receiving-locality-paired-metadata.json).
The finite exports preserve complete phase, source-workload, acceptance, cleanup and replay
facts plus runtime/frame distributions. As in the preceding comparison, only four raw per-frame
arrays are omitted; original raw-report hashes are recorded and complete reports/programs remain
outside target in the local frozen observation directory.

| Named wall envelope, median over six complete 16K lifetimes per version | Before | After |
| --- | ---: | ---: |
| Original-value cost/capture admission | 78.016 ms | 76.809 ms |
| Shared record capture | 0.038 ms | 0.027 ms |
| Migration, including post-migration admission | 0.040 ms | 0.040 ms |
| Default materialization | 98.596 ms | 94.808 ms |
| Compiler semantic validation | 0.812 ms | 0.792 ms |
| Reference planning | 125.538 ms | 115.800 ms |
| Reference-program validation | 0.016 ms | 0.015 ms |
| Complete component materialization | 308.555 ms | 292.253 ms |
| Flat claim admission | 490.235 ms | 383.589 ms |
| Native decoding and preflight | 932.016 ms | 776.535 ms |
| Unbound preparation | 358.099 ms | 299.969 ms |
| Expansion, including nested Prefab acquisition | 936.599 ms | 915.656 ms |
| Source payload decode | 706.983 ms | 703.655 ms |

Flat admission is 21.75% lower, native decoding/preflight 16.68% lower and unbound preparation
16.23% lower in this observation. They are cumulative named wall envelopes and can overlap;
their savings must not be added or presented as exclusive CPU time. Compiler field semantics are
unchanged by this successor; variation in the unaffected phases is not credited to this change.
Both versions retain the same source wire, schemas, value limits, default semantics, asset and
World checks, retained allowances and original frame/publication/retirement owners.

Process walls are 19.897, 18.794, 18.705 and 20.162 seconds in order. Each after observation is
lower than the before observations in this finite sample, but correlated lifetimes and
uncontrolled host/driver/cache/clock state do not establish statistical confidence or a general
whole-process gain. Unchanged pacing and wall instrumentation do not measure exclusive CPU/GPU
duration, physical memory, frame P99 or a numeric production budget. After-program SHA256 is
`7265e9516d72ca6b479e90d7b5810eff752f23c63b674e6a45230c65edfd4694`.

The identical after executable separately passes the complete release game/library/metric/span/
probe/native selection, 148/148 without skips at `de56ec26-1245-45b8-a35f-23cc43605bc9`, 63.357
seconds after a 9m39s serial build. It retains baseline and authored 64/4K/16K travel, 27 original
GPU replays and both observer-panic cleanup controls. The [plan](../plans/2026-11-13-rust-state-reconstruction-plan.md#receiving-field-validation-locality)
records owner/public/independent checks and exact scope. Further native decoding, reference and
raw-subtree traversals remain; this does not implement broader exact-definition reuse, change
the source format or finish Foundation/R9/R10/R12.

### Single Native Value Admission

Date: 2026-11-17. Before is the qualified receiving-field-locality program with SHA256
`7265e9516d72ca6b479e90d7b5810eff752f23c63b674e6a45230c65edfd4694`. Both versions already freeze
defaults and separate field semantics from repeated cost/default admission. This successor
at `ab19dc9` returns the exact cost already computed by the existing admitted Native walker. Scene consumes
that cost against its remaining aggregate allowance, removing the preceding cost-only scan for
accepted values. Native failures still classify the complete record through the original
cost-only operation and retain its full charge when the aggregate cost fits; caller admission
refusal stops immediately. Source/default semantics,
receiving limits, current-state and publication checks remain. The
[owning unit](../plans/2026-11-13-rust-state-reconstruction-plan.md#single-native-value-admission)
records source/Native cost controls and the rejected-value policy.

Four serial before/after/after/before processes pass the unchanged original 16K travel workload,
copied-source removal, GPU replays and physical cleanup: twelve complete lifetimes and 36 replays,
six lifetimes per version. Rust 1.95 release, manifests/locks, source generator, observation hooks,
value-phase call counts and hardware DX12 RTX4090 adapter match. Finite summary exports are
[1 before](data/2026-11-14-development-runtime/authored-tiles-native-value-cost-paired-1-before.json),
[2 after](data/2026-11-14-development-runtime/authored-tiles-native-value-cost-paired-2-after.json),
[3 after](data/2026-11-14-development-runtime/authored-tiles-native-value-cost-paired-3-after.json) and
[4 before](data/2026-11-14-development-runtime/authored-tiles-native-value-cost-paired-4-before.json),
with [source/program/report hashes and order](data/2026-11-14-development-runtime/authored-tiles-native-value-cost-paired-metadata.json).
They retain complete phase/source/acceptance/cleanup/replay facts and runtime/frame distributions,
omitting only the same four raw per-frame arrays as prior comparisons. Complete raw reports and
programs remain in the local frozen directory, with original raw-report SHA256 digests recorded.

| Named wall envelope, median over six complete 16K lifetimes per version | Before | After |
| --- | ---: | ---: |
| Original-value cost/capture admission | 78.173 ms | 78.682 ms |
| Shared record capture | 0.027 ms | 0.035 ms |
| Migration, including post-migration admission | 0.038 ms | 0.039 ms |
| Default materialization | 94.667 ms | 96.518 ms |
| Compiler semantic validation | 0.795 ms | 0.826 ms |
| Reference planning | 119.418 ms | 118.221 ms |
| Reference-program validation | 0.016 ms | 0.015 ms |
| Complete component materialization | 296.435 ms | 296.777 ms |
| Flat claim admission | 384.536 ms | 334.978 ms |
| Native decoding and preflight | 788.297 ms | 711.898 ms |
| Unbound preparation | 303.411 ms | 280.802 ms |
| Expansion, including nested Prefab acquisition | 924.321 ms | 927.946 ms |
| Source payload decode | 701.901 ms | 706.479 ms |

Observed flat admission is 12.89% lower, native decoding/preflight 9.69% lower and unbound
preparation 7.45% lower. These cumulative envelopes overlap and their savings are not additive
exclusive CPU costs. Source decoding, compiler defaults/semantics and expansion are unchanged by
this operation; variation there is not credited to the change. Original 180 calls per named value
phase, eight worker expansion entries and six owner publications per lifetime remain.

Process walls in order are 18.715, 18.648, 18.477 and 19.139 seconds. This finite observation,
with correlated lifetimes and uncontrolled host/driver/cache/clock state, does not establish
statistical confidence or a general whole-process speedup. Unchanged pacing and wall instrumentation
do not measure exclusive CPU/GPU duration, physical memory, frame P99 or production tail budgets.
After-program SHA256 is `0f0ff0b3ee057c6ca1c13ac2953e3e2abd2f531909ef62fcc8746f0f014fdd1b`.

The identical after executable separately passes the original complete release game/library/
metrics/span/probe/native selection, 148/148 without skips at
`becaacb3-a0e5-488e-b930-e756009d455e`, 64.198 seconds after a 9m31s serial build. Baseline and
authored 64/4K/16K travel, 27 retained-input GPU replays and both observer-panic cleanup controls
remain. Failed-value cost classification has CPU controls but is not isolated by this accepted-value
performance workload. Further immutable cost/depth/reference reuse and source decoding work are
separate operations; no broader exact-definition reuse, source-format change or full
Foundation/R9/R10/R12 completion is claimed.

### Immutable Compiled Value Measurements

Date: 2026-11-17. Before is the qualified single-Native-admission program with SHA256
`0f0ff0b3ee057c6ca1c13ac2953e3e2abd2f531909ef62fcc8746f0f014fdd1b`. Both programs already
retain frozen effective reads, separate compiler/receiving field semantics from cost/default
admission, and obtain complete cost from the original Native walker. This successor at `9583ad7` retains
that cost and measured maximum root-at-one depth in the existing immutable compiled component.
Receiving aggregate admission reuses those facts when they fit its actual remaining allowance;
stricter or exhausted policies retain the original failure path. Stored data groups and root
retained fees consume the same measurements. Changed data and independent cooked claims obtain
new measurements through their original owners. Schema/default, numeric, undeclared-reference,
resource, namespace and mutable World/publication checks remain. The
[owning unit](../plans/2026-11-13-rust-state-reconstruction-plan.md#immutable-compiled-value-measurements)
records the implementation and acceptance evidence.

Four serial before/after/after/before processes pass the original 16K travel, copied-source
removal, original-input GPU replay and physical cleanup: twelve complete lifetimes and 36 replays,
six lifetimes per program. Rust1.95 release, manifests/locks, source generator, observation hooks,
value-phase call counts and hardware DX12 RTX4090 adapter match. Finite summary exports are
[1 before](data/2026-11-14-development-runtime/authored-tiles-compiled-metrics-paired-1-before.json),
[2 after](data/2026-11-14-development-runtime/authored-tiles-compiled-metrics-paired-2-after.json),
[3 after](data/2026-11-14-development-runtime/authored-tiles-compiled-metrics-paired-3-after.json) and
[4 before](data/2026-11-14-development-runtime/authored-tiles-compiled-metrics-paired-4-before.json),
with [source/program/report hashes and order](data/2026-11-14-development-runtime/authored-tiles-compiled-metrics-paired-metadata.json).
They preserve complete phase/source/acceptance/cleanup/replay facts and runtime/frame
distributions, omitting only the same four raw per-frame arrays as the preceding comparisons.
Complete raw reports and frozen programs remain outside target; summaries retain original
complete-report SHA256 digests.

| Named wall envelope, median over six complete 16K lifetimes per version | Before | After |
| --- | ---: | ---: |
| Original-value cost/capture admission | 79.938 ms | 76.021 ms |
| Shared record capture | 0.032 ms | 0.030 ms |
| Migration, including post-migration admission | 0.040 ms | 0.039 ms |
| Default materialization | 95.205 ms | 101.509 ms |
| Compiler semantic validation | 0.826 ms | 0.836 ms |
| Reference planning | 116.410 ms | 118.249 ms |
| Reference-program validation | 0.017 ms | 0.016 ms |
| Complete component materialization | 293.752 ms | 296.975 ms |
| Flat claim admission | 344.346 ms | 331.005 ms |
| Native decoding and preflight | 746.259 ms | 590.254 ms |
| Unbound preparation | 295.399 ms | 230.450 ms |
| Expansion, including nested Prefab acquisition | 913.579 ms | 931.081 ms |
| Source payload decode | 709.722 ms | 707.587 ms |

Observed Native decoding/preflight is 20.90% lower and unbound preparation is 21.99% lower.
Their cumulative envelopes overlap and savings are not additive exclusive CPU costs. First flat
admission still performs its original Native traversal while also measuring depth and retaining
the facts; its observed variation is not credited as removed work. Source decode, defaults,
compiler semantics and expansion are not optimized by this unit. The comparison does not isolate
the separate cost of receiving reuse from retained-fee reuse. It establishes the complete selected
workflow rather than an independent microbenchmark of either operation.

Process walls in order are 18.472, 17.849, 17.970 and 20.470 seconds. Per-process median Native
decode/preflight is 704.006, 586.392, 593.499 and 752.628 ms; corresponding preparation medians
are 275.907, 230.297, 232.334 and 295.791 ms. Each after observation is below both before
observations in this sample, but the before drift, correlated lifetimes and uncontrolled
host/driver/cache/clock state do not establish statistical confidence or a general process gain.
Wall instrumentation and existing pacing measure neither exclusive CPU/GPU duration nor physical
memory, scanout, frame P99 or a numerical production budget. After-program SHA256 is
`65f1fabdd8729ee220a48fb6ae4d94a5622ec4e175b11706980c13bf99845cd4`.

The identical after executable separately passes the complete release game/library/metric/span/
probe/native selection, 148/148 without skips at `17831171-2490-434c-aa49-e6871884fc38`, 68.194
seconds after a 10m07s serial build. It retains baseline and authored 64/4K/16K travel, 27 GPU
replays and both observer-panic cleanup controls. Strict receiving/schema/default and independent
cook/replacement behavior have separate public CPU controls. Stored facts do not admit a changed
value or a different receiving definition. Broader definition-wide reuse, source decoding, larger
named workloads and full Foundation/R9/R10/R12 remain open.

### Same-Operation Native Codec Admission

Date: 2026-11-17. Before is the qualified immutable-measurement executable with SHA256
`65f1fabdd8729ee220a48fb6ae4d94a5622ec4e175b11706980c13bf99845cd4`. Both programs already
retain frozen effective reads and measured cost/depth, reuse receiving aggregate facts and
retained fees, and separate field semantics from default/cost admission. This successor at `6128594` removes
the generic native preflight's repeated default, Native-reference and field checks immediately
after Scene's complete all-record receiving admission. Internal dispatch still selects the
original binding and invokes its codec with the original context. An earlier codec's changed
materialization limits trigger ordinary preflight for later values. Standalone public preflight
keeps complete dynamic-input checks. No new definition witness, caller-selected skip mode, cache
or lifecycle owner is introduced. The
[owning unit](../plans/2026-11-13-rust-state-reconstruction-plan.md#same-operation-native-codec-admission)
records actual source/Runtime paths and mutable-policy controls.

Four serial before/after/after/before processes pass the unchanged original 16K travel, copied
source removal, original-input GPU replay and physical cleanup: twelve complete lifetimes and
36 replays, six lifetimes per program. Rust1.95 release, manifests/locks, source generator,
observation hooks, phase-call counts and hardware DX12 RTX4090 adapter match. Finite summaries
are [1 before](data/2026-11-14-development-runtime/authored-tiles-single-native-codec-paired-1-before.json),
[2 after](data/2026-11-14-development-runtime/authored-tiles-single-native-codec-paired-2-after.json),
[3 after](data/2026-11-14-development-runtime/authored-tiles-single-native-codec-paired-3-after.json) and
[4 before](data/2026-11-14-development-runtime/authored-tiles-single-native-codec-paired-4-before.json),
with [source/program/report hashes and order](data/2026-11-14-development-runtime/authored-tiles-single-native-codec-paired-metadata.json).
They retain complete phase/source/acceptance/cleanup/replay facts and runtime/frame
distributions, omitting only the same four raw per-frame arrays as earlier comparisons. Original
complete-report hashes remain; raw reports and programs stay outside target.

| Named wall envelope, median over six complete 16K lifetimes per version | Before | After |
| --- | ---: | ---: |
| Original-value cost/capture admission | 77.225 ms | 79.579 ms |
| Shared record capture | 0.032 ms | 0.032 ms |
| Migration, including post-migration admission | 0.039 ms | 0.042 ms |
| Default materialization | 104.818 ms | 101.964 ms |
| Compiler semantic validation | 0.809 ms | 0.823 ms |
| Reference planning | 118.642 ms | 120.833 ms |
| Reference-program validation | 0.019 ms | 0.016 ms |
| Complete component materialization | 303.354 ms | 304.266 ms |
| Flat claim admission | 342.284 ms | 337.891 ms |
| Native decoding and preflight | 600.139 ms | 474.344 ms |
| Unbound preparation | 240.167 ms | 189.013 ms |
| Expansion, including nested Prefab acquisition | 943.263 ms | 939.738 ms |
| Source payload decode | 700.519 ms | 715.540 ms |

Observed Native decoding/preflight is 20.96% lower and unbound preparation is 21.30% lower.
Their cumulative wall envelopes overlap; savings are not additive exclusive CPU costs. Source
decode, compiler/default materialization, flat sealing and expansion are not changed by this
operation, so their variation is not credited as removed work. Actual codec/domain construction,
asset preparation, strict receiving limits/definitions and World/publication/retirement checks
remain. This accepted-value workload does not isolate the changed-policy fallback cost; the
public codec-mutation control separately qualifies its refusal behavior.

Process walls in order are 18.584, 18.102, 18.319 and 18.461 seconds. Per-process Native
decode/preflight medians are 600.144, 467.618, 477.742 and 587.600 ms; corresponding preparation
medians are 240.605, 188.226, 189.799 and 234.332 ms. Both after observations are below both
before observations in this finite sample. Correlated lifetimes and uncontrolled host, driver,
cache and clock state do not establish statistical confidence or a general whole-process gain.
Unchanged wall instrumentation and pacing measure neither exclusive CPU/GPU duration, physical
memory, scanout nor numerical frame-P99 or production-tail budgets. After-program SHA256 is
`6d842833a451972e5c6484cbb3e9e2997ae702ebfd6f8a17b5dd10d383cdf60e`.

The identical after executable separately passes the complete release game/library/metric/span/
probe/native selection, 148/148 without skips at `fc15dda2-6df9-4ff1-bc62-048c82ef638e`, 62.612
seconds after a 9m51s serial build. Baseline and authored 64/4K/16K travel, 27 original-input
GPU replays and both observer-panic cleanup controls remain. Final owner/public controls pass
1,353/1,353, including raw/frozen and independently cooked/readmitted native values, concrete
codec-domain refusal/retry and actual shared-limit changes. Broader cross-preparation definition
reuse, remaining source/reference costs, larger named workloads and full Foundation/R9/R10/R12
stay open.

### Known-Tag Scalar Payload Decoding

Date: 2026-11-17. Before is the qualified same-operation-codec executable with SHA256
`6d842833a451972e5c6484cbb3e9e2997ae702ebfd6f8a17b5dd10d383cdf60e`. Both programs already
retain frozen effective reads and measured cost/depth, reuse receiving facts and dispatch original
codecs after complete receiving checks with changed-limit fallback. List/Map payloads already
stream when their tag precedes the payload. This successor at `3ecb373` also consumes scalar callbacks
directly for known Bool/I64/U64/F64/String tags, avoiding the untagged Content buffer and trials
of unrelated kinds. The original final tag/payload conversion remains. Value-before-type,
unknown tags, Null and EntityRef retain their previous fallback. Structural preflight, mandatory
header gates, source wire, serialization, receiving limits and native/publication owners are
unchanged. The [owning unit](../plans/2026-11-13-rust-state-reconstruction-plan.md#known-tag-scalar-payload-decoding)
records exact numeric/string compatibility and the independently found character correction.

Four serial before/after/after/before processes pass the original unchanged 16K travel, copied
source removal, retained-input GPU replay and physical cleanup: twelve complete lifetimes and
36 replays, six lifetimes per program. Rust1.95 release, manifests/locks, source generator,
observation hooks, original phase-call counts and hardware DX12 RTX4090 adapter match.
Finite summaries are
[1 before](data/2026-11-14-development-runtime/authored-tiles-scalar-wire-paired-1-before.json),
[2 after](data/2026-11-14-development-runtime/authored-tiles-scalar-wire-paired-2-after.json),
[3 after](data/2026-11-14-development-runtime/authored-tiles-scalar-wire-paired-3-after.json) and
[4 before](data/2026-11-14-development-runtime/authored-tiles-scalar-wire-paired-4-before.json),
with [source/program/report hashes and order](data/2026-11-14-development-runtime/authored-tiles-scalar-wire-paired-metadata.json).
They preserve complete phase/source/acceptance/cleanup/replay facts and runtime/frame
distributions, omitting only the same four raw per-frame arrays as preceding comparisons.
Original raw-report digests remain; complete raw reports and frozen programs stay outside target.

| Named wall envelope, median over six complete 16K lifetimes per version | Before | After |
| --- | ---: | ---: |
| Wire structural preflight | 332.650 ms | 383.826 ms |
| Header decode | 40.019 ms | 37.937 ms |
| Source payload decode | 713.247 ms | 297.126 ms |
| Source completion/domain shape | 51.892 ms | 51.304 ms |
| Original-value cost/capture admission | 80.190 ms | 93.659 ms |
| Shared record capture | 0.032 ms | 0.027 ms |
| Migration, including post-migration admission | 0.041 ms | 0.043 ms |
| Default materialization | 103.013 ms | 111.823 ms |
| Compiler semantic validation | 0.816 ms | 0.844 ms |
| Reference planning | 122.712 ms | 119.982 ms |
| Reference-program validation | 0.018 ms | 0.017 ms |
| Complete component materialization | 307.916 ms | 326.543 ms |
| Flat claim admission | 351.534 ms | 360.421 ms |
| Native decoding and preflight | 481.987 ms | 469.268 ms |
| Unbound preparation | 189.825 ms | 186.720 ms |
| Structure, including nested Prefab work | 644.274 ms | 445.419 ms |
| Expansion, including nested Prefab acquisition | 960.003 ms | 773.691 ms |

Observed source-payload wall time is 58.34% lower. Structure and expansion include nested Prefab
decoding, so their lower values overlap that work and are not additive exclusive CPU savings.
Actual scalar/container populations, source omissions, compiler phases and original acceptance
remain. Wire preflight is higher in this observation despite no implementation change there;
header, source completion, compiler/default, reference and native-preparation variation are not
credited to scalar dispatch. No wire budget or semantic check is removed to obtain this result.

Process walls in order are 19.531, 18.116, 16.568 and 18.059 seconds. Per-process payload
medians are 734.734, 300.390, 292.295 and 707.928 ms; expansion medians are 967.782, 804.015,
771.671 and 922.824 ms. Both after groups have lower payload and expansion medians, but the
first after process is slightly slower than the final before process. Thus there is no consistent
whole-process ordering across all observations. Correlated lifetimes and uncontrolled host,
driver, cache and clock state preclude statistical confidence or a general process gain. Existing
wall instrumentation and pacing measure neither exclusive CPU/GPU duration, physical memory,
scanout nor numerical frame-P99 or production-tail budgets. After-program SHA256 is
`054573517ce2a7d6dfd716869ec54827afebe669fdd66be30fe322c1ae7a00f0`.

The identical after executable separately passes the complete release game/library/metric/span/
probe/native selection, 148/148 without skips at `4a5726ff-79d7-4cd4-a0d2-48ab080999a1`, 64.939
seconds after a 10m08s serial build. Baseline and authored 64/4K/16K travel, 27 GPU replays and
both observer-panic cleanup controls remain. Owner/public controls pass 1,195/1,195, including
all 32 wire controls under the actual locked RON0.8.1/Serde1.0.228 dependencies. Wrong-kind and
malformed data retain rejection, without an exact parser-error-text or position promise. General
wire/header consolidation, remaining reference costs, larger named workloads and full
Foundation/R9/R10/R12 stay open.

### Borrowed Keys In Structural Preflight

Date: 2026-11-17. Before is the qualified known-scalar executable with SHA256
`054573517ce2a7d6dfd716869ec54827afebe669fdd66be30fe322c1ae7a00f0`. Both versions retain
frozen effective reads, measured cost/depth, same-operation codec reuse and known-tag scalar
decoding. This successor at `9bd3a54` changes only temporary key storage in the original generic structural
preflight. Its existing per-map duplicate set borrows serde input-lifetime keys, copies transient
keys after the original budgets and moves owned keys. Equality uses decoded string contents
across all forms. Original nodes, depth, strings, aggregate bytes, map items, duplicate rejection,
header gates, payload semantics, source wire and publication owners remain. The
[owning unit](../plans/2026-11-13-rust-state-reconstruction-plan.md#borrowed-keys-in-structural-preflight)
separates this change from any proposed merger of structural preflight and header decoding.

Four serial before/after/after/before processes pass original unchanged 16K travel, copied-source
removal, retained-input GPU replay and physical cleanup: twelve complete lifetimes and 36 replays,
six lifetimes per program. Rust1.95 release, manifests/locks, source generator, observation hooks,
phase-call counts and hardware DX12 RTX4090 adapter match. Finite summaries are
[1 before](data/2026-11-14-development-runtime/authored-tiles-borrowed-shape-keys-paired-1-before.json),
[2 after](data/2026-11-14-development-runtime/authored-tiles-borrowed-shape-keys-paired-2-after.json),
[3 after](data/2026-11-14-development-runtime/authored-tiles-borrowed-shape-keys-paired-3-after.json) and
[4 before](data/2026-11-14-development-runtime/authored-tiles-borrowed-shape-keys-paired-4-before.json),
with [source/program/report hashes and order](data/2026-11-14-development-runtime/authored-tiles-borrowed-shape-keys-paired-metadata.json).
Complete phase/source/acceptance/cleanup/replay facts and distributions remain, omitting only
the same four raw per-frame arrays as preceding comparisons. Complete raw-report digests remain;
raw reports and frozen programs stay outside target. Core paths first changed here were absent
from the preceding selected-source receipt. Metadata separately records their exact before Git
blob hashes, checked equal between the before implementation and its frozen build base, rather
than treating them as retroactively captured working-file hashes.

| Named wall envelope, median over six complete 16K lifetimes per version | Before | After |
| --- | ---: | ---: |
| Wire structural preflight | 338.446 ms | 206.105 ms |
| Header decode | 37.149 ms | 40.865 ms |
| Source payload decode | 289.616 ms | 288.053 ms |
| Source completion/domain shape | 48.277 ms | 48.821 ms |
| Original-value cost/capture admission | 90.010 ms | 89.709 ms |
| Shared record capture | 0.025 ms | 0.027 ms |
| Migration, including post-migration admission | 0.040 ms | 0.040 ms |
| Default materialization | 110.317 ms | 113.714 ms |
| Compiler semantic validation | 0.853 ms | 0.795 ms |
| Reference planning | 118.687 ms | 116.648 ms |
| Reference-program validation | 0.018 ms | 0.015 ms |
| Complete component materialization | 321.283 ms | 323.020 ms |
| Flat claim admission | 349.025 ms | 348.845 ms |
| Native decoding and preflight | 464.342 ms | 467.891 ms |
| Unbound preparation | 184.239 ms | 187.527 ms |
| Structure, including nested Prefab work | 416.508 ms | 358.178 ms |
| Expansion, including nested Prefab acquisition | 741.037 ms | 689.141 ms |

Observed structural-preflight wall time is 39.10% lower. Structure and expansion include nested
Prefab decoding/preflight and overlap that work; reductions are not additive exclusive CPU
savings. The semantic payload is unchanged, and source payload decode is nearly equal. Header,
compiler/default/reference and Native-preparation implementations are unchanged; their variation
receives no optimization credit. The original per-map set still allocates nodes. Parser scratch
for escaped strings and owned key allocations made before callbacks remain. The separate Core
allocation control proves no key-sized copy for its finite borrowed-key cases, not zero parsing
allocation or whole-process physical-memory savings.

Process walls are 16.217, 15.776, 16.037 and 16.014 seconds in order. Per-process preflight
medians are 343.909, 202.912, 209.298 and 326.002 ms; expansion medians are 759.568, 690.013,
688.269 and 728.634 ms. Both after groups have lower preflight medians, but the second after
process is slightly slower than the final before process. Whole-process ordering is therefore
inconsistent across all observations. Correlated lifetimes and uncontrolled host, driver, cache
and clock state preclude statistical confidence or a general process gain. Wall instrumentation
and original pacing measure neither exclusive CPU/GPU duration, physical memory, scanout nor
numerical frame-P99 or production-tail budgets. After-program SHA256 is
`bbad6d08dada3833ba64a134b4f142e4f52c8d24b1d9b9bf62ac2558ee9e9a0b`.

The identical after executable separately passes complete release game/library/metric/span/
probe/native qualification, 148/148 without skips at `51cedbd5-91ef-40cb-9bf1-c0598c6b9553`,
57.311 seconds after a 10m41s serial build. Baseline and authored 64/4K/16K travel, 27 GPU
replays and both observer-panic cleanup controls remain. Core/Reflect/Scene owner/public controls
pass 1,148/1,148, including mixed callbacks, transient scratch reuse, escaped Unicode equality,
nested duplicate scopes and exact budget/error precedence. No validation pass or capacity is
removed to obtain the result. Header fusion, remaining reference/source work, larger named
workloads and full Foundation/R9/R10/R12 remain open.

### Joint Effective Reference Interpretation

The subsequent [local module comparison](2026-11-17-runtime-module-comparison.md) identifies
post-decode compilation and native preparation as stronger attribution targets than another
envelope parser. The [owning successor](../plans/2026-11-13-rust-state-reconstruction-plan.md#joint-effective-reference-interpretation)
replaces one concrete duplicate: effective flat/receiving admission collects assets and then
plans entity references from the same unchanged value/Schema. Both enter the same complete raw,
declared-shape and hidden-reference interpreter. One shared interpretation now supplies assets
and operation-local pending entity leaves. Actual product/resolver checks remain before entity
output allocation and namespace closure. Independent visitors remain complete; no retained cache,
new proof owner or skipped mutable receiving fact is introduced.

An instrumented original 16K compact-JSON collection executes 12 warmup/sample iterations with
complete data checks. Before modification, effective asset/entity interpretation each runs 24
times and accumulates 86.787/86.823 ms. Flat admission is 133.341 ms across 12 calls and native
unbound preparation is 100.880 ms across 12 calls. The separate staged value-reference planner
is 43.062 ms over 12 calls. These nested wall spans locate the duplicated work; they cannot be
added to their enclosing envelopes or reused as a whole-game result.

Four frozen complete-game processes then execute before/after/after/before serially on Windows
11, Rust 1.95.0 and hardware DX12/NVIDIA GeForce RTX 4090. Each process completes three authored
16K lifetimes, nine retained-input GPU replays and original physical/copy cleanup. The first
before runs earlier while implementation is developed; the following three run consecutively
after qualification. No owning Cargo overlaps these processes. The original source generator,
Scene/Prefab encoding, gameplay, receiving/native callbacks and acceptance assertions remain.
The after program adds three feature-gated attribution spans; their overhead is included, and
the earlier before program has no corresponding child-span breakdown.

The
[source/program/report receipt](data/2026-11-14-development-runtime/authored-tiles-joint-references-paired-metadata.json)
retains the exact source hashes and executable digests. Four finite summaries retain complete
phase/source/acceptance/cleanup/replay facts and distributions; only the same four named raw
per-frame arrays are omitted. Original raw reports/programs remain in the local frozen directory.
The before executable is the previously qualified borrowed-key implementation `9bd3a54`; the
tested after source is captured as uncommitted relative to `70140a3` and subsequently committed
at `6295ccb1cfd8bee7be963d34d9b11211d572cba2`. No compiler/dependency update belongs to this
production optimization. The optional small consumer observer reuses the game's existing
tracing dependency versions in its own lock; root/sample production locks are unchanged.

Each number below is a median cumulative wall envelope across six complete lifetimes/version,
not one load, one frame, exclusive CPU time or additive work. Flat admission has nine calls and
unbound preparation six per lifetime; their enclosing and value stages can overlap.

| Envelope | Before | After | Interpretation |
| --- | ---: | ---: | --- |
| Structural wire preflight | 237.734 ms | 217.136 ms | Unchanged parser; variation receives no optimization credit. |
| Header decode | 41.172 ms | 39.040 ms | Unchanged. |
| Payload decode | 301.548 ms | 299.968 ms | Unchanged. |
| Source finish | 53.139 ms | 52.360 ms | Unchanged. |
| Original value admission | 94.719 ms | 94.210 ms | Unchanged. |
| Defaults | 121.459 ms | 119.168 ms | Unchanged. |
| Staged value-reference planner | 120.600 ms | 122.827 ms | Outside the joint receiving/flat replacement. |
| Complete component materialization | 338.825 ms | 339.603 ms | Nearly equal; no gain claimed. |
| Flat admission | 365.118 ms | 242.532 ms | 33.57% lower in this finite observation. |
| Native unbound preparation | 195.573 ms | 120.923 ms | 38.17% lower in this finite observation. |
| Expansion, including nested Prefab work | 720.986 ms | 709.156 ms | Overlapping, mostly unchanged work; no separate speedup attribution. |
| Publication | 1.543 ms | 1.489 ms | Unchanged; no pure-commit improvement claim. |

Per-process flat medians are 362.312/241.975/243.089/373.193 ms; unbound medians are
198.901/121.823/117.564/186.521 ms. Both after groups are lower than both before groups in these
changed envelopes. Process walls are 18.227/15.989/15.415/17.042 seconds. Whole-process ordering
also favors the after groups here, but uncontrolled host/cache/driver/clock state and correlated
lifetimes prevent a general speedup, statistical confidence, frame-P99, physical-memory or
exclusive CPU/GPU claim. The new joint asset-interpretation span includes both kinds' shared
work; its entity span now measures final output only. Do not compare that child span as if it
were the previous full entity planner. Pending paths/leaf scratch can coexist with asset output;
fewer walks are not proof of lower peak heap.

Qualification includes complete Reflect/Scene libraries and selected public compiled/native/data/
reference/cook paths, 1,148/1,148 without skips (`fd2e6d8f-23f9-417d-9430-e4008de98645`), and the
complete original release game selection, 148/148 without skips
(`ea33b63a-7f3d-4366-8f33-dc37a44c94d9`). It preserves 64/4K/16K travel, 27 original-input GPU
replays, both observer-panic cleanup controls and the complete 16K record probe. The first owner
run exposes one obsolete double-walk fee oracle and new fixture assumptions about Resource
entities, source reference closure and a narrowing Schema capability; these are corrected at
the actual owning boundaries. Exact/one-short limits, zero early native callbacks, complete
contents, source isolation and no premature Scene publication remain checked. Production and
bounded test-correction and numeric-report reviews find no P1/P2. The independently locked Scene
consumer passes 157/157 with one original manual timing skip
(`bd7a95f1-4231-46f6-a4a5-10edd4f2a5b2`). Owning all-target/all-feature correctness Clippy,
feature-free Reflect/Scene compilation, root/sample/scoped-consumer formatting, final observed
small consumer and document/source/hash checks pass. The owning plan records failures,
corrections and platform limits. Full Foundation/R9/R10/R12 remains open.

### Reference Paths From Active Container Cursors

The [private-walker successor](../plans/2026-11-13-rust-state-reconstruction-plan.md#reference-paths-from-active-container-cursors)
compares the remaining raw-reference traversal after joint interpretation. The control is the
qualified `6295ccb` program. Both versions already share effective asset/entity interpretation,
use the same source/receiving checks and expose identical feature-gated phase names. The private
candidate replaces per-node Visit/Exit events and mutable full-path maintenance with active
container iterators. It materializes an exact owned path from the root prefix and selected
ancestor positions when a reference needs declaration lookup or a diagnostic. No values, tail
bytes, depths or hidden-reference checks are skipped.

The [pinned source comparison](../research/2026-11-17-large-tilemap-payload-ownership.md#current-ownership-and-escape-contracts)
separates cursor position from owned paths without attributing Nara's budget contract to another
engine. Six focused cases compare raw scans with an independent recursive owned-value oracle,
including frozen insertions, root prefixes, UTF-8 fields, sibling recovery, UUID/authored-route
work, exact limits, depth 64/65 and every callback refusal. Allocation assertions observe modeled
debits; scalar/empty cases and flat-list scratch results are not process heap measurements.

The first focused run passes 22/23. The remaining test encodes the old unconditional initial
stack allocation. It now refuses the first actual nonempty-container allocation after the root
visit, still before children or Schema errors. A preliminary syntax error is corrected before
production compilation. The complete focused cohort then passes 29/29
(`f9f6cdc0-5173-446b-b11a-f4a5810c9c0d`), and full Reflect/Scene libraries plus selected public
consumers pass 1,154/1,154 without skips (`33e9a2fc-0c87-45ff-a5a8-545eccdb6ae4`). Independent
production and test review finds no P1/P2. Four exploratory small-consumer processes pass all
64/4K/16K encoding/data checks and justify continuing to complete game qualification; those
observations do not substitute for a final frozen game comparison.

The complete original release game/library/metrics/span/native/16K-record selection passes
148/148 without skips (`540dd04b-3ce7-4795-94a8-4ae0a09ebe67`), 66.510 seconds after a serial
10m21s build. Original 64/4K/16K travel, 27 original-input GPU replays and both observer-panic
physical cleanup cases retain their full assertions. Four additional frozen processes then
complete twelve 16K lifetimes and 36 GPU replays in before/after/after/before order. The first
before runs earlier while the implementation is developed; the final three run consecutively
after native qualification. No owning Cargo overlaps those measurement processes.

The [source/program/report receipt](data/2026-11-14-development-runtime/authored-tiles-reference-cursors-paired-metadata.json)
retains 159 after source/control hashes, the exact before/after executables and four finite
summaries. Complete phase/source/acceptance/cleanup/replay facts and distributions remain; only
the same four named raw per-frame arrays are omitted. Raw reports/programs remain in the local
frozen directory. Before is the qualified joint-reference implementation `6295ccb`, executable
SHA256 `ea00e61fd17250d75e0715382c56bc8c462d6db60714b0c229b078becd28f015`; after is captured
relative to `34c3272` and subsequently committed at `26c12dc8c991d671e8206f56d96de500b4bab07e`.
Both programs use Rust 1.95.0, hardware
DX12/NVIDIA GeForce RTX 4090, identical source/Prefab data and the same observer configuration.
The only production source difference is the private raw path walker; no dependency or file
format update belongs to this experiment.

Each value below is a median cumulative wall envelope across six complete lifetimes/version,
not one file, one frame or exclusive CPU work. The reference planner runs 180 times, joint
effective interpretation 480 times, flat admission nine times and unbound preparation six times
per lifetime. The broader envelopes contain some of these child spans and are not additive.

| Envelope | Before | After | Interpretation |
| --- | ---: | ---: | --- |
| Wire preflight | 209.897 ms | 210.490 ms | Unchanged. |
| Header decode | 38.501 ms | 37.989 ms | Unchanged. |
| Payload decode | 296.504 ms | 290.665 ms | Unchanged; no parser optimization credit. |
| Source finish | 50.900 ms | 49.673 ms | Unchanged. |
| Original value admission | 91.696 ms | 89.935 ms | Unchanged. |
| Defaults | 116.259 ms | 112.945 ms | Unchanged. |
| Staged value-reference planner | 119.518 ms | 70.918 ms | 40.66% lower in this observation. |
| Component materialization | 327.985 ms | 276.382 ms | Includes the changed staged planner. |
| Effective value admission | 113.806 ms | 115.626 ms | Unchanged Native cost/depth traversal. |
| Joint effective reference interpretation | 303.569 ms | 164.408 ms | 45.84% lower; one common child shared by flat/receiving paths. |
| Flat admission | 237.061 ms | 188.517 ms | 20.48% lower. |
| Native unbound preparation | 127.484 ms | 83.392 ms | 34.59% lower. |
| Expansion, including nested Prefab work | 693.165 ms | 635.735 ms | Overlaps changed and unchanged work. |
| Publication | 1.534 ms | 1.474 ms | Unchanged; no pure-commit gain claimed. |

Per-process reference-plan medians are 118.667/68.919/73.224/128.265 ms. Flat medians are
232.602/187.900/194.416/243.911 ms; unbound medians are 118.119/83.011/83.772/134.247 ms.
Both after groups are lower than both before groups in these changed envelopes. Process walls
are 15.041/14.216/14.306/16.074 seconds; both after groups are lower here too. Uncontrolled host,
cache, driver and clock state plus correlated lifetimes prevent general whole-process,
statistical-confidence, frame-P99, exclusive CPU/GPU or physical-memory claims. Fewer events and
modeled scratch requests do not establish process heap/VRAM reduction. No extra representation,
retained cache, validation certificate or mutable receiving owner is introduced.

Independent Scene consumers pass 157/157 with one original manual timing skip
(`626af222-a93f-41b9-b5bb-b9dde14dec58`). Owning all-target/all-feature correctness Clippy,
feature-free Reflect/Scene compilation, root/sample formatting and document/source/hash checks
pass; existing warnings remain. Independent numeric report review finds no P1/P2. The owning
plan records actual failures/corrections and platform limits. This closes neither full
Foundation/R9/R10/R12 nor numerical runtime or foreign-platform qualification.

### Value Admission Through Active Container Cursors

The [value-walker successor](../plans/2026-11-13-rust-state-reconstruction-plan.md#value-admission-through-active-container-cursors)
compares the remaining complete cost/depth traversal against the qualified `26c12dc` program.
The private Reflect budget now holds only active Map/List iterators and advances them in place.
It processes the current value directly, removing the additional Value event and parent-iterator
pop/reinsert for every child. Scalar and empty-container roots need no traversal allocation.
Nonempty container scratch still needs admission before growth; removed work retains no fee.

Complete logical nodes/bytes, Map key order, maximum depth, frozen effective insertions and
retained-default storage remain unchanged. Native authored-reference rejection and SourceContext's
complete tail scan remain with the same walker. Existing source/native reference units are not
replaced by the reference-discovery path's different text/path accounting. No new cache, source
format, Schema policy, receiving owner, dependency or runtime observer is introduced.

Both variants already expose the same phase names and call counts. The small public Scene
consumer supports continuing to full qualification: four before/after/after/before processes pass
36 population/encoding cases with complete value checks. Its exploratory frozen control predates
only a runtime-inert test-module registration; it is not presented as a new exact-source release
comparison. The original complete game supplies the qualified program and native acceptance below.
Nested wall envelopes include repeated operations and cannot be added to parent phases or divided
by another engine's single-file operation. Actual allocator/RSS, GPU cost and frame-tail improvements
require separate measurements.

The original focused cohort passes 42/42. The first complete owner/public selection passes
1,162/1,167 (`e9af5326-4e13-439c-b63d-4d3fc10b91d8`); all five failures reach one property-test
helper's obsolete unconditional initial-stack assertion. After that test-only correction, the
complete Reflect library passes 283/283 (`f1d5e7fe-c3bb-4528-9e8d-7df3e262fd4b`), including the
five cases and eight new independent recursive-oracle controls. The other 884 owner/public
cases passed with unchanged production. The original release game/library/metrics/span/native/
16K-record selection passes 148/148 without skips (`3ae4945d-079b-4ade-a077-105e72b70a6f`),
68.382 seconds after a serial 9m10s build. Original travel, 27 retained-input GPU replays and
both observer-panic physical cleanup cases retain their complete assertions. Production and test
reviews find no P1/P2.

The [paired receipt](data/2026-11-14-development-runtime/authored-tiles-value-cursors-paired-metadata.json)
retains 161 after source/control hashes, exact before/after executable hashes and four finite
summaries. Complete phase/source/acceptance/cleanup/replay facts and distributions remain; only
the same four named raw per-frame arrays are omitted. Raw reports/programs remain in
`%TEMP%/nara-value-cursors-paired-20261117-1903`. The tested changes are captured relative to
`8018603` and subsequently committed at `240a45352e5b745923af3f33b1d6ed19367bc7e1`.
Both programs use Rust 1.95.0, hardware DX12/NVIDIA
GeForce RTX 4090 and the same original input/observer configuration. Four serial processes in
before/after/after/before order add twelve 16K lifetimes and 36 GPU replays, all accepted.

Each row below is a median cumulative wall envelope across six lifetimes/version. A lifetime
contains 18 mixed file decodes, nine expansions/flat admissions, six unbound preparations,
180 original-value admissions, 180 default operations and 480 Native admissions. These are not
single-file, single-load, exclusive CPU or frame times; parent/child envelopes overlap.

| Envelope | Before | After | Interpretation |
| --- | ---: | ---: | --- |
| Wire preflight | 222.548 ms | 210.057 ms | Unchanged; no parser credit. |
| Header decode | 37.789 ms | 37.678 ms | Unchanged. |
| Payload decode | 291.135 ms | 286.925 ms | Unchanged. |
| Source finish | 47.489 ms | 46.681 ms | Unchanged. |
| Original value admission | 85.248 ms | 54.006 ms | 36.65% lower in this observation. |
| Defaults | 108.043 ms | 73.455 ms | Includes the changed complete preflight walker. |
| Staged reference planner | 61.639 ms | 53.127 ms | Unchanged; no independent planner credit. |
| Component materialization | 256.724 ms | 182.860 ms | Contains changed admission/default work. |
| Effective Native value admission | 114.366 ms | 75.490 ms | 33.99% lower in this observation. |
| Joint effective reference interpretation | 154.068 ms | 155.874 ms | Unchanged. |
| Flat admission | 183.112 ms | 145.077 ms | Contains changed Native admission. |
| Native unbound preparation | 82.313 ms | 82.281 ms | No demonstrated improvement. |
| Expansion, including nested Prefab work | 617.271 ms | 528.508 ms | Contains changed and unchanged work. |
| Publication | 1.434 ms | 1.389 ms | Unchanged; no commit gain claimed. |

Per-process original-admission medians are 87.123/53.658/54.726/83.909 ms and Native admission
116.491/74.642/76.021/110.007 ms. Both after groups are lower than both before groups for those
changed envelopes. Process walls are 18.169/15.758/16.391/16.706 seconds. The first control runs
earlier during development with PowerShell native-output redirection; the final three use Python
binary-file redirection. The value walkers emit no log lines, but process walls remain descriptive,
without an isolated whole-process improvement claim. Host/cache/clock/driver state is uncontrolled
and lifetimes within each process are correlated. No confidence interval, frame-P99, GPU-time,
physical-memory or universal speedup result follows. Final independent consumption and static
checks are recorded in the owning plan. The independently locked Scene consumer passes 157/157
with one original manual timing skip (`32df911f-edf0-48eb-a193-1e426f13586a`). Owning correctness
Clippy, feature-free Reflect/Scene compilation, root/sample formatting and document/source/hash
checks pass. Three scoped implementation/review agents are closed with no remaining P1/P2.
Full Foundation/R9/R10/R12 and numerical runtime-tail qualification remain open.

### Unbound Preparation Cost Attribution

The [unbound attribution unit](../plans/2026-11-13-rust-state-reconstruction-plan.md#unbound-preparation-cost-attribution)
adds five feature-gated scopes at the original Scene boundaries, using the existing binary-local
collector. Receiving validation and component preflight also serve loading diagnostics, so their
actual tracing ancestor chain selects two separate unbound buckets. The original `scene_decode`
has fifteen calls: nine diagnostic preflights and six actual unbound preparations. Bucket
selection follows the new span's registered parent chain, including explicit parents, rather
than the ambient current span. The synthetic collector control retains worker/reentry/live-span
failure checks and now proves both implicit and conflicting explicit-parent attribution.

Original focused Scene prepare/public admission controls pass 64/64, with 772 outside the filter,
at `0898b259-84c0-4bd3-a599-9bd0c5d41b48`. The release game/library/metrics/span/native/record
selection passes 148/148 without skips at `57a983b9-c093-4fe6-bf81-696c3a1e8852`, 63.608 seconds
after a 7m23s serial build. The subsequently extended explicit-parent test passes separately
1/1 at `b940fde0-e164-4d77-a094-6be61d3a00ae`, without rerunning unrelated game behavior.
Production and observer reviews find no P1/P2.

One further frozen hardware DX12/RTX4090 process completes three original 16K lifetimes, nine
retained-input GPU replays and complete source/native cleanup. The
[source/program receipt](data/2026-11-14-development-runtime/authored-tiles-unbound-attribution-metadata.json)
retains 162 exact source/control hashes, compiler and executable identity, counts, original raw
report hash and one finite summary. Only the same four listed raw per-frame arrays are omitted.
The frozen program and raw report remain in `%TEMP%/nara-unbound-attribution-20261117`.
Tested source is captured relative to `6b5dd4d` and subsequently committed at
`aeeb79f2792635b08f71a3aa849c2b926d3bba1e`. No owning Cargo runs during this process.

These are median cumulative wall envelopes across three lifetimes in one process, not per-frame,
single-file or exclusive CPU times. Each lifetime has two 8,192-cell maps per active level and six
unbound preparations. Child spans overlap the named parent; separately computed medians are not
an additive decomposition.

| Scope | Calls per lifetime | Median cumulative wall | Boundary |
| --- | ---: | ---: | --- |
| Original unbound preparation | 6 | 81.791 ms | Original complete advanced preparation, including its preflight and World work. |
| Unbound preflight | 6 | 81.248 ms | Decode-context setup and complete compiled Scene preflight. |
| Unbound receiving validation | 6 | 35.120 ms | Current compiled definition/value/reference admission before codec dispatch. |
| Unbound component preflight | 6 | 46.094 ms | All native codecs and data-component preparation, not a Tilemap-only timer. |
| Unbound World preparation | 6 | 0.501 ms | Product/baseline/topology/predecessor/identity work and component-location staging before candidate materialization. |
| Unbound candidate materialization | 6 | 0.028 ms | Original `ComponentApplyBatch::prepare_unbound` closure execution and result handoff, excluding later error formatting. |
| Non-unbound diagnostic receiving validation | 9 | 54.711 ms | Separate loader/diagnostic preflights; not part of the six unbound calls. |
| Non-unbound diagnostic component preflight | 9 | 59.384 ms | Separate codec preflight with diagnostic candidates subsequently discarded. |

Individual unbound cumulative walls are 80.223/81.791/83.752 ms, with receiving
34.420/35.120/35.963 ms and component preflight 45.052/46.094/47.179 ms. The process takes
15.831 seconds including all repeated game/verification/replay work. The new scopes and
ancestor selection change observation overhead, so neither those walls nor the previous
smaller-phase-set program establish an improvement. Correlated lifetimes, uncontrolled host/
cache/clock/driver state and nested walls establish no confidence interval, GPU, physical-memory
or frame-tail result.

The next bounded implementation should first compare native Tilemap construction inside its
existing owner. Source inspection identifies a separate coordinate BTreeSet plus temporary typed
Vec before final BTreeMap construction, and per-cell dirty-chunk updates; the component scope
above makes that a useful hypothesis, not proof of an individual sub-operation's cost. Preserve
duplicate input-index errors, numeric/color conversion, dirty revision and asset failure order.
World/identity work is not the large envelope here, and later candidate closures usually just
move/box already prepared stock values. Generic-reference reuse remains a separate compiled-value/
Schema ownership question; this attribution grants no exemption from hidden-reference checks.

Owning Scene and sample program/observer correctness Clippy, feature-free Scene compilation,
root/sample formatting and source/document/hash checks pass. Source tracing, production/observer
reviews and final numeric review retain their bounded scope in the owning plan. This adds no
dependency, public interface, runtime policy or new observation owner, and qualifies neither
foreign execution nor full Foundation/R9/R10/R12.

### Direct Native Tilemap Construction Experiment

The [direct-construction experiment](../plans/2026-11-13-rust-state-reconstruction-plan.md#direct-native-tilemap-construction)
tests one concrete hypothesis from the attributed component envelope. Instead of a coordinate
BTreeSet plus typed Vec followed by final cell BTreeMap construction, the candidate parses each
complete cell directly into the final tree with `Entry` duplicate rejection. Dirty-chunk revisions
advance in original input order. Deferred asset candidates own the complete private Tilemap before
handle resolution. Configuration, numeric/color parsing, duplicate input-index errors and asset
failure order remain; there is no new API, format, dependency or validation owner.

**Disposition: rejected; production restored.** Direct construction makes the actual World
preflight cheaper but makes the diagnostic candidate path more expensive. The final production
algorithm remains unchanged; five useful public codec controls are retained at `8e8a1f6`.
A private loop reduction is not a sufficient reason to keep a mixed whole-workflow outcome.

The candidate's original Tilemap/related rendering selection passes 113/113 with 48 outside
the filter (`3b460e2d-fb3f-4b97-8206-053a4c2aa0fa`), five new codec controls pass
(`0a350f1b-4b81-48b8-82ca-6175a9c46e46`), and independently locked actual Runtime TileSet
delivery passes 14/14 without skips (`d0547e7a-7c41-4880-a4a8-12add2858f10`). The complete
original release game/library/metrics/span/native/record selection passes 148/148 without
skips (`66fadd4c-d835-4e56-aeb8-4750b8377fb6`), 64.871 seconds after a serial 5m26s build.
Original travel, 27 retained-input GPU replays, both observer-panic cleanup cases and the complete
record/cook probe remain. Production/test reviews find no P1/P2; the early native-tree storage
tradeoff is explicitly reported rather than treated as a memory improvement.

The [candidate receipt](data/2026-11-14-development-runtime/authored-tiles-direct-native-rejected-paired-metadata.json)
retains 163 candidate source/control hashes, both executable hashes and four finite summaries.
The [archived patch](data/2026-11-14-development-runtime/direct-tilemap-candidate.patch) reconstructs
the candidate `lib.rs` relative to `936ac51` using `git apply --unidiff-zero`; the independent
new test module is retained in the final test commit. Candidate source snapshots, raw reports and frozen programs remain in
`%TEMP%/nara-direct-tilemap-paired-20261117`. The after hashes describe an uncommitted rejected
experiment, not the final runtime. Summaries preserve complete source/phase/acceptance/cleanup/
replay facts and distributions; only the same four named raw per-frame arrays are omitted.

All programs use Rust 1.95.0, hardware DX12/NVIDIA GeForce RTX4090, unchanged authored input and
the same 46-phase observation. Four serial before/after/after/before processes complete twelve
16K lifetimes and 36 GPU replays with exact expected phase counts and original physical cleanup.
Each row is a median cumulative wall envelope across six lifetimes/version, not one file, load
or frame. Parent/child phases overlap and must not be added.

| Scope | Calls per lifetime | Before | Candidate | Interpretation |
| --- | ---: | ---: | ---: | --- |
| Combined diagnostic/receiving decode | 15 | 196.600 ms | 196.310 ms | No meaningful demonstrated improvement; candidate destruction is outside this scope. |
| Diagnostic component preflight | 9 | 58.688 ms | 64.546 ms | 9.98% higher in this observation. |
| Actual unbound component preflight | 6 | 46.122 ms | 42.550 ms | 7.74% lower, but not a complete workflow improvement. |
| Unbound receiving validation | 6 | 36.754 ms | 35.491 ms | Unchanged implementation; no credit. |
| Complete unbound preparation | 6 | 83.473 ms | 78.674 ms | Contains actual changed construction and unchanged work. |
| Unbound candidate materialization | 6 | 0.029 ms | 0.029 ms | No demonstrated improvement. |
| Expansion including nested Prefabs | 9 | 537.035 ms | 530.629 ms | Contains changed/unchanged work; no isolated improvement claim. |
| Publication | 6 | 1.436 ms | 1.479 ms | Unchanged implementation; no credit. |

Per-process diagnostic component medians are 58.449/64.391/64.700/59.347 ms and actual unbound
component medians 46.014/42.060/42.739/46.229 ms. Both candidate groups have the same split
outcome. Process walls are 15.769/15.799/15.962/15.701 seconds, with no consistent improvement.
All logs use the same Python binary-file redirection. Host/cache/clock/driver state remains
uncontrolled and lifetimes within each process are correlated; no confidence interval,
exclusive CPU/GPU, frame-P99 or physical-memory claim follows.

The diagnostic preflight scope ends before returned candidates are destroyed. The candidate
moves final-map destruction outside that bucket while the old coordinate-set destruction occurs
inside `read_cells`, yet the measured diagnostic construction is still more expensive. Sparse
inputs can also retain more dirty-chunk tree entries before asset resolution. This is enough
reason to reject the selected candidate, not proof that every possible direct construction is
slower. Do not solve the result by deleting domain checks or extending a generic cache.

After restoring the original production algorithm, the complete Tilemap library passes 110/110
without skips (`0c30b04b-53e0-404e-b705-6e57aa777e1b`), including the five added controls.
The rebuilt original program completes another three 16K lifetimes/nine GPU replays and full
cleanup; its final receipt records correctness and exact program/source/raw identity separately
from the rejected candidate. Owning correctness Clippy, feature-free Tilemap compilation,
root/sample formatting and source/document/hash checks pass. Four scoped implementation/review
agents are closed with no remaining P1/P2. Future work must preserve the distinction between
compact admitted typed data for diagnostics and mutable native structures for actual World construction.

### Retained Original Inputs And Qualification

Nine original native Tilemap lifetimes retain three selected inputs each: first Meadow, first
Cavern and final returned Meadow. A four-entry original Seal ledger and exact accepted native
receipts select them. After the original World, raw Scene owner and native window retire, an independent public
Host replays those 27 inputs into equal-sized 960-by-540 `Rgba8Unorm` linear/premultiplied images.
It neither reconstructs a newer source snapshot nor claims to capture the displayed swapchain.

The independent oracle checks the complete captured terrain geometry and original texture
identity, then fixed known-source ground/background pixel witnesses in all four quadrants. Per
image, the 64-cell variant checks 144 ground and 36 clear physical pixels; the 4K/16K variants
check 72 ground and 36 clear pixels. The 16K witness samples the independently inspected center
texel, not an invented white fallback. These finite witnesses are not a full rasterization oracle.
Every complete output image is reread byte-for-byte after later publication and all producer
retirements. Actual render/readback/retirement prefixes complete; final Host tracked resources,
candidate metadata and cleanup faults are zero, without abandoned callbacks. Three retained
images account for 6,220,800 CPU baseline bytes and 6,223,876 observed Host payload bytes before
release, plus separately bounded readback and original captured-input storage.

The focused original content regression retains actual raw/compiled Scene allocations through
escaped materializers and releases raw Prefab input with its last original snapshot. It checks
the captured generation's complete conservative budget through successive source drops and the
final refund. Actual Weak reachability and reservation lifetime are distinct; neither is a process
heap measurement. Every Tilemap lifetime separately rejects all five old Scene receipts, observes
six retired source views and removes its exact copied source tree.

| Scope | Actual final result |
| --- | --- |
| Reflect/Scene plus original content/residency, debug test profile | 1,049/1,049; 867 outside the selected filter; `ad17f549-d801-443e-97f1-d2bbd76a28d5` |
| Game library/source/oracle/metric controls, release | 138/138, no skips; `283fb232-f419-431f-997d-535bb0ab08c5` |
| Native population/baseline workflows and two observer-panic cleanup controls, release hardware DX12 | 4/4, no skips; `6f494450-d654-4ff7-b087-39f1ea8ee691` |

Scoped shared all-target/all-feature, root library and consumer correctness Clippy pass, as do
root/sample formatting and source/executable/report integrity checks. Warnings remain. Independent
review closes the previously missing plan-visit admission and strengthens copied-source panic
cleanup and positive replay-result checks; all six scoped agents are closed. CI already selects
the metric/source/oracle target; hardware native execution remains explicitly qualified here,
not claimed as a remote CI run.

The continuation should attribute larger Scene preparation/replacement and extraction work in
this same complete workflow before selecting reuse or another optimization. Controlled pacing,
numeric runtime tail/resource budgets, larger named game workloads and foreign execution remain
open. This unit does not complete Foundation/R9/R12 or resume paused Editor/UI/Dialogue work.
