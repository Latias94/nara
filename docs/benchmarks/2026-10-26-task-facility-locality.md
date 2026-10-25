# Lazy Per-Runtime And Shared Task Facility Comparison

Status: the bounded release comparison passed. Source baseline `c62ff32` plus the qualified
demand-activation source; the [verification record](../knowledge/engineering/verification/2026-10/2026-10-25T164306Z-demand-activation-custody-and-matched-lazy-shared-image-comparison-c5d3a638a3884807b489a85f0b7b0b1a.md)
records its source and integration scope. The
[activity design](../architecture/task-activity-scope-design.md) owns the
candidate contracts; the [master plan](../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
owns R4b and the remaining foundation work. No default product deployment is selected here.

## Matched Consumer And Attribution

The maintained ignored Image test `measure_lazy_runtime_vs_shared_image_facilities` constructs
two managed Runtime owners with identical AssetId/domain/coalesce keys, distinct source
directories and independently checked Image pixels, versions and reload counters. Both candidates
use the same per-activity custody and first-use activation implementation:

| Candidate | Physical facilities | I/O workers after both owners are active | Aggregate pending capacity |
|---|---:|---:|---:|
| Lazy per-Runtime | 2 | 1 + 1 | 4 + 4 |
| Lazy shared | 1 | 2 | 8 |

Each facility also configures one Compute and one AsyncCompute worker, but both kinds must remain
dormant with zero admitted work throughout. Those configured maxima are not observed worker
counts. The shared implementation starts both I/O workers on its first use; the private candidate
starts only the used Runtime's I/O worker until its sibling first submits. Both remain zero before
first use. Facility counts and `live_workers` are internal execution observations, not whole-process
OS thread, memory or CPU attribution. Existing process-owned custody/reaper threads remain outside
this worker count; no saving is attributed to removing that custody.

Six ordinary reloads per trial comprise one first-use reload per owner and four warm reloads.
Every reload writes a real 1x1 RGBA PNG before the measured interval, submits SourceChange, drives
the owner until application, and checks exact output. Each has a separate no-op TaskHandle probe
on the same I/O queue after application. That probe's accepted-admission-to-start is not the Image
job's queue latency and cannot stand in for its first-use OS activation cost.

The interference control submits two held blocking closures from owner 0 in both candidates.
Private facilities therefore have one running and one queued job in owner 0, while owner 1's
worker remains independently available. Shared facilities run both jobs and saturate both workers.
An owner-1 queue probe plus actual PNG reload observes progress until release at a nominal 30ms.
Actual release/exit and request/completion offsets are recorded. Both candidates submit the same
two jobs and have the same total two-worker capacity; no extra private worker is supplied.

Only a sample whose `image_admission_observed_offset_us < actual_release_us` is confirmed to
contain an admitted PNG job while the held work still occupied its workers. The admission
observation is taken after the owner drive actually reports its successful Image spawn, so it is
a conservative upper bound, not an invented task timestamp. Later/equal observations are reported
as exclusions from held-PNG timing, even when the separately observed queue probe experienced
pressure. Microsecond rounding can conservatively exclude a near-boundary sample.

This is the intended topology comparison: sharing spare capacity also shares arbitrary blocking
occupancy. Scope custody does not reserve execution slots, guarantee fairness or preempt closures.
The fixture does not establish a production admission policy or recommend a priority algorithm.

## Execution

Use Windows, Rust `+1.95.0`, one Cargo build job and the ordinary checkout `target`. Execute Cargo
serially. Only one ignored test is selected, so `--no-capture` does not introduce concurrent tests.

```text
cargo nextest run -p nara_image --lib --release --locked --run-ignored only -E 'test(tests::activity_scopes::comparison::measure_lazy_runtime_vs_shared_image_facilities)' --no-capture
```

The fixture prints a method record and 16 trial records per candidate, alternating candidate order.
Per candidate this gives 32 owner-first-use reloads (not 32 facility activations for shared), 64 warm
reloads, 96 ordinary post-reload queue probes and 16 interference trials with additional probes.
One trial submits seven Image jobs, seven queue probes and two held jobs, then retires both
activities before physically joining all facilities. No performance threshold controls pass/fail.

## Observations

Windows release execution passed one selected test at nextest
`acf39ada-7c6d-4f5b-9b50-08a087c1c7e0`, with 32 complete trial records. The initial release build
took 8m30s after the user's earlier clean; measured test execution took 4.26s. Compiler time is not
included in any task timing. Raw JSON lines remain in ignored
`target/r4b-lazy-shared-release-measurement.log`; the direct nearest-rank reduction is in
`target/r4b-lazy-shared-release-summary.json`. Times below are microseconds, P50 / P95.

| Observation | Samples per candidate | Lazy per-Runtime | Lazy shared |
|---|---:|---:|---:|
| Facility construction | 16 | 3 / 70 | 3 / 5 |
| Owner 0 first-use PNG reload | 16 | 3,294 / 8,191 | 4,473 / 6,437 |
| Owner 1 first-use PNG reload | 16 | 3,344 / 7,713 | 4,181 / 7,806 |
| Warm PNG reload | 64 | 3,116 / 6,930 | 4,172 / 6,655 |
| Warm operation maximum owner drive | 64 | 1,541 / 4,033 | 1,812 / 3,467 |
| Ordinary separate probe admission-to-start | 96 | 7 / 44 | 7 / 16 |
| Confirmed held-PNG reload | 16 | 3,109 / 6,797 | 30,625 / 32,659 |
| Held operation maximum owner drive | 16 | 1,559 / 3,982 | 1,737 / 4,418 |
| Separate probe under held-work pressure | 16 | 5 / 52 | 30,238 / 30,678 |
| Actual held-work release offset | 16 | 30,385 / 30,515 | 30,266 / 30,702 |

Both candidates have zero I/O workers at creation and after the explicit idle owner drives before
first use. Activated workers remain allocated until physical close; this is not idle shrinking.
Their active multiplicity is exactly `[1, 1]` versus `[2]`; Compute and AsyncCompute stay at zero.
Every trial checks physical close after independent scope retirement, with no admission, task or
cleanup errors. Shared facilities have one fewer facility object, not fewer active I/O workers
in this matched two-active-owner workload. Initial construction and steady timing differences
are not treated as statistically significant general gains.

All **16 / 16** interference samples per candidate have observed successful Image admission before
actual release: **zero exclusions** under the conservative rule. Private owner-1 PNG completion
precedes release in **16 / 16**; shared completion precedes release in **0 / 16**. The shared queue
probe independently observes the same approximately 30ms occupancy barrier. Owner-drive maxima do
not show that entire stall: the main owner remains able to poll while accepted background work
waits. Responsive polling alone is therefore insufficient evidence of interactive job progress.

## Architecture Consequence

Keep physical sharing explicit. Independent activity custody solves admission/result/close
ownership; it cannot guarantee execution progress against arbitrary blocking closures. Under this
matched bounded workload, the lazy private alternative removes idle worker allocation without
giving up its sibling's independent capacity. It is a stronger default candidate for interactive
authoring alongside independently authored gameplay work than unconditional sharing based solely
on fewer facility objects.

This changes the design preference, not the product implementation: qualify the actual Editor
construction, refresh, startup-failure and shutdown owners before changing default allocation.
Retain shared facilities for concrete Hosts that deliberately accept shared occupancy or provide
a separately proven isolation policy. Do not respond by implementing a speculative global
priority/fairness scheduler: priority cannot preempt a blocking closure, and the measured pressure
does not justify merging frame borrowing, Future I/O or real-time callbacks into this pool.

## Interpretation Limits

Owner polling includes a 1ms sleep when work is incomplete. Reported wall time is owner-observed
completion, not raw worker service time or a GUI input-to-photon latency. Tiny PNGs isolate this
bounded execution consumer; they do not represent large imports, scene builds, native Editor
frames or arbitrary project plugins. Filesystem cache and Windows scheduling are not controlled.
The first process trial can include shared reaper initialization, and other local applications
can perturb elapsed time. Alternating order reduces order bias but is not machine isolation.

Nearest-rank medians/P95 from these bounded samples can characterize gross mechanisms only. There
is no P99, throughput, memory, native frame or general performance-improvement claim. Product
selection still needs its actual authoring/Play, failure and retirement workflows. Existing eager
defaults are not silently replaced by these candidate constructors.
