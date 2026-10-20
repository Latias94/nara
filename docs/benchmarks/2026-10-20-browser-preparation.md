# Browser Preparation Locality

Status: Implementation `7f78c5923297862d0d4d4ff63ac3ed478360b96e` passes focused, aggregate and
selected native correctness checks and three public release measurement processes. Starting source is
`b1f9dfec1c51214a46937ac5adf879cdd785d150`.
The [L2 plan](../plans/2026-10-20-browser-preparation-locality-plan.md) owns this slice.

## Owning Changes

The source-index owner reserves generations without opening declared roots on the caller.
One existing I/O-pool task owns normalization, bounded enumeration, final lineage revalidation
and snapshot preparation. The owner observes small progress facts through nonblocking polling
and publishes only an actual completed matching result. Absolute budgets and last-good refusal
remain unchanged. Internal Image publication can reserve one successor; ordinary Refresh still
coalesces an already pending request. Cancellation and physical capture return are distinct.

Document Open captures a move-only request that shares the admitted schema, adapters, root and
exact index. Reading, decoding, complete authoring validation and persistence-Host binding run
on the existing I/O pool. The original second identity/digest observation remains in that binding.
There is no second validator, public async trait or new pool per request. An unpublished prepared
Host has a retirement guard and independent existing Host custody, including result disposal.

Dispatch returns Preparing with an identity bound to the admitting Host frame and local request
sequence. The public frame exposes preparation, cancellation, dirty-close waiting and an exact
terminal result; matching acknowledgement releases that result. An already bound document can
still activate synchronously. A second Open is rejected while the slot or terminal result exists.
Selection changes do not retarget preparation; direct public actions and the native panel have
the same cancellation and acknowledgement capabilities.

Completion checks the exact current index and predecessor DocumentId, revision, binding and
schema. Before irreversible predecessor retirement, it rechecks those owner facts after any dirty
decision or Save. Preparation refusal retains the prior working document and history. Once close
begins, its outcome is separate from eventual Open publication; no rollback is promised. The
prepared source is an immutable observed snapshot, not a claim of disk freshness at a later
dirty decision. Subsequent Save still detects external conflicts using its existing observation.

Native maintenance schedules pending scans and Open work; unchanged polls do not request repaint.
Project Close/Drop cancels publication interest and retains the existing task/runtime retirement
path until physical disposal. No unsafe compatibility Open route remains in production; older
owner-conformance tests use a test-only observation helper over the same asynchronous request.

## Verification And Measurement Protocol

Root runs Cargo serially with Rust 1.95.0, one build job and the existing target. New deterministic
gates cover held reads, preparation, revalidation and cancellation; there is no timing threshold
substituted for a correctness assertion. Existing public/native consumers are migrated to the
explicit preparation/result contract. Diagnostic failures remain excluded from acceptance.

The public interaction collector retains populations 64/1,024/4,096, two warm-ups and sixteen
retained iterations per group. Scalar/history/source and marker insertion/removal oracles remain.
For Browser Open, the request window now ends at truthful Preparing; completion still requires
the replacement to be active, the old document absent and its close settled, and additionally
checks the exact terminal Open result. Open and Close results are acknowledged outside timing.
The old synchronous dispatch and new Preparing dispatch represent different amounts of work;
total completion and maximum owner-call occupancy must be reported separately.

For scan phase-labelled owner intervals, the label is the last observed progress before the call.
It is not worker CPU attribution; the background scan may pass a phase between polls. Publication
includes source/safe snapshot adoption, the first query slice and frame publication. Query slicing
stays with the owner. Independent processes are not pooled or percentile-subtracted. With sixteen
samples, nearest-rank P95 is the observed maximum, not a stable tail estimate. Ordinary desktop
activity remains uncontrolled. No native input-to-display, release SLO or complete heap result is
implied by this experiment.

## Measured Results

Three independent optimized processes each retain 768 numeric rows and all semantic oracles.
The [summary](data/2026-10-20-browser-preparation/editor-summary.json) binds the implementation,
three nextest identities, raw gzip files, uncompressed digests and all 144 groups. Per-process
nearest-rank P50 / P95 milliseconds for the 4,096 population are:

| Workflow / interval | Run 1 | Run 2 | Run 3 |
| --- | --- | --- | --- |
| Refresh / request | 0.004 / 0.005 | 0.004 / 0.005 | 0.004 / 0.011 |
| Refresh / maximum owner drive | 0.201 / 0.257 | 0.151 / 0.376 | 0.135 / 0.280 |
| Refresh / publication owner drive | 0.190 / 0.257 | 0.151 / 0.376 | 0.130 / 0.280 |
| Refresh / total completion | 859.328 / 1201.276 | 822.882 / 1169.646 | 802.186 / 979.315 |
| Scene Open / request | 0.018 / 0.025 | 0.017 / 0.021 | 0.017 / 0.020 |
| Scene Open / maximum owner drive | 5.474 / 6.907 | 5.605 / 7.347 | 5.717 / 6.519 |
| Scene Open / total completion | 68.111 / 73.304 | 69.065 / 74.604 | 72.435 / 82.599 |

The nearest source baseline is the preceding scalar-preparation cohort. Its 4,096 population
per-process P50 milliseconds are retained below, including its substantial desktop variability:

| Preceding implementation / interval | Run 1 | Run 2 | Run 3 |
| --- | --- | --- | --- |
| Refresh / maximum owner drive | 65.743 | 167.195 | 109.088 |
| Refresh / total completion | 727.008 | 1482.050 | 1167.442 |
| Scene Open / request | 64.135 | 68.805 | 65.339 |
| Scene Open / maximum owner drive | 2.421 | 2.464 | 2.367 |
| Scene Open / total completion | 66.539 | 71.564 | 67.630 |

Those frozen groups remain in the [preceding summary](data/2026-10-20-scalar-preparation/editor-summary.json).
The still-earlier [indexed cohort](data/2026-10-20-production-costs/indexed-summary.json) has
Refresh maximum-drive P50 53.676-69.572 ms and completion P50 762.310-863.684 ms. Both cohorts
support investigating long owner work; neither is a controlled paired before/after experiment.

The evidence supports removing long scan/decoding calls from the Editor owner, not faster total
filesystem work. Refresh still takes approximately 0.8 seconds. Open shifts most preparation out
of dispatch, while owner adoption, old-document disposal and frame projection now appear in its
drive interval. Its total remains approximately 68-72 ms in these processes. Independent desktop
cohorts do not isolate a small total regression or improvement. Comparing old completed dispatch
to new Preparing dispatch alone would misrepresent the endpoint; inspect completion and both
request/drive occupancy. The measured new Browser owner intervals fit below the local 8 ms
investigation trigger in this fixture; this is not a capacity or native latency guarantee.

No temporary timing probes remain, and this slice does not claim allocation or retained-heap
improvements. File scans still perform the original bounded work; query, safe projection,
document publication and model construction stay with the owner.

## Qualification

The [verification record](../knowledge/engineering/verification/2026-10/2026-10-20T095710Z-browser-preparation-locality-qualification-554a0c2edbfb455f84d4b32304446a2e.md)
binds 328 aggregate owner tests, final 26 focused scan/admission/Browser tests, 172 Editor/public
recovery tests, two actual Windows/DX12 trajectories and the locked all-feature/all-target
workspace check. It retains diagnostic failures and their corrections, including the real
post-close TaskPools access failure. These overlapping counts are not summed into a new total.

Reproduce the unchanged population/window protocol through the migrated public result contract:

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo +1.95.0 nextest run --cargo-profile release --no-default-features --features project-workflow,tooling,runtime-ui,presentation-2d --test editor_persistence --locked --offline --run-ignored only -E 'test(measure_editor_interaction_costs)' --no-capture
```

## Remaining Units

Tile per-view locality and Rust/Play attribution remain separately ordered in the
[follow-up](../plans/2026-10-20-measured-work-locality-plan.md). The remaining scalar encoders,
document clone and O(N) state work also remain explicit; moving Browser preparation does not
remove those costs. Tile tools remain deferred.
