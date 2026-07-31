# Reference-Game First-Playable Baseline

## Status

RGD-U9 is complete at source revision
`f09887600d2161144b920b6e2618fc8151dad4fa` with a reproducible **Redirect** result.
Of the 20 required first-playable metrics, 9 complete metrics pass, 1 complete non-hard-stop metric
fails, and 10 metrics remain missing. The only hard-stop metric,
`gameplay.headless_wave_success`, passes, so this evidence does not support `Stop`.

This is a product-baseline decision, not a release gate. It names the current bottlenecks without
turning the reference game into a benchmark framework.

## Source And Prerequisite

- Measurement source: `f09887600d2161144b920b6e2618fc8151dad4fa` on `main`.
- Hosted prerequisite: GitHub Actions push run
  [`30629508555`](https://github.com/Latias94/nara/actions/runs/30629508555), whose six root,
  reference-game, and module-consumer jobs passed on Windows and Ubuntu at that exact revision.
- Protocol: `reference_game_first_playable_v1`.
- Protocol JSON SHA-256:
  `e55fe4205584b801e3e3301e391b23e86b2f9ded1e04e4936c92436da1c988c2`.
- Collector SHA-256:
  `51f1b0b7bb5738548540d0f391e0bc052317d3b1242d80bd9a0bb9f0f7a956b7`.
- Raw JSONL SHA-256:
  `040a9779de7c4f4d915f101626a832618e9e9d98b679e57bd7ba651782de3f80`.
- Run-manifest SHA-256:
  `ca321c8b6ed037215d30e2bbaa615e037e770700f6e12e79dcd8cf5dc1f590f5`.
- Committed transport:
  [`data/runs/v1/rgd-u9-f098876/run-manifest.json`](data/runs/v1/rgd-u9-f098876/run-manifest.json)
  and
  [`data/runs/v1/rgd-u9-f098876/raw-samples.jsonl`](data/runs/v1/rgd-u9-f098876/raw-samples.jsonl).

The committed helper created a clean detached worktree and isolated Cargo, target, home, and
temporary directories outside the repository. Cargo used one build job and switched to offline
resolution after the initial fetch. The helper bytes executed by the collector matched the helper
blob at the measured `HEAD`; source bytes and the active checkout were unchanged after cleanup.

The first local invocation used a normal PowerShell environment whose `PATH` selected Scoop's GNU
`link.exe` instead of the installed MSVC linker. It failed before producing a product sample. The
successful collection was invoked from the installed Visual Studio x64 development environment.
That invocation correction is diagnostic context, not part of the canonical run transport.

## Environment

| Field | Value |
|---|---|
| OS | Windows 11 x86_64 |
| Runner | Local workstation |
| CPU | Intel64 Family 6 Model 183 Stepping 1 |
| Rust | `rustc 1.97.1 (8bab26f4f 2026-07-14)`, `x86_64-pc-windows-msvc`, LLVM 22.1.6 |
| Cargo | `cargo 1.97.1 (c980f4866 2026-06-30)` |
| Profile | Debug |
| Build concurrency | `CARGO_BUILD_JOBS=1`; build commands also used `--jobs 1` |
| Dependency resolution | Online fetch followed by isolated offline builds |
| Collection window | 2026-07-31 13:18:27Z to 13:40:19Z |
| GPU / desktop timing profile | Not collected |

This is one Windows timing population. The hosted Ubuntu run proves compatibility, not timing
equivalence, and its observations are not merged into this population.

## Mechanisms

| Workflow | Mechanism |
|---|---|
| Cold build | Five empty-target `cargo build --locked --bins --jobs 1` runs. |
| Clean headless journey | Three cold builds followed by the public `headless --max-ticks 96` product path. |
| Body edit | Toggle a type-equivalent Rust system expression, rebuild, then run a fresh headless product. |
| Data edit | Toggle the persisted player hit-points value, then run a fresh headless product without an explicit build step. |
| Structural edit | Add or remove one private zero-effect Rust field, rebuild, then run a fresh headless product. |
| Public coverage check | The static `public_surface` test passed but was not admitted as production-call coverage. |

Every admitted headless base run produced the same terminal summary:

```json
{"schema":"nara-reference-game.wave-summary-v1","outcome":"completed","tick":49,"score":300,"player_hit_points":20,"enemies_remaining":0,"projectiles_remaining":4}
```

Its SHA-256 is
`77819d5cac39ab3e25d9e9537e09995480e96d0e9488515cb480c7d50dba5169`.
The data variant changed only `player_hit_points` to 21. Reverting it restored the base digest;
body and structural variants retained the base digest.

## Observed Results

The values below were independently recomputed from the committed JSONL using the protocol's
one-based nearest-rank rule. The Python helper only collected and verified transport shape and
digests; it did not aggregate metrics or decide the verdict.

| Metric | Samples | Aggregate | Target | Result |
|---|---:|---:|---:|---|
| `build.cold_ns` | 5 | P95 207.760 s | <= 1200 s | Pass |
| `build.incremental_ns` | 5 | P95 10.682 s | <= 120 s | Pass |
| `gameplay.headless_wave_success` | 1 | Exact 1 | = 1 | Pass |
| `iteration.body.p50_ns` | 10 | P50 9.281 s | <= 15 s | Pass |
| `iteration.body.p95_ns` | 10 | P95 12.345 s | <= 30 s | Pass |
| `iteration.data.p50_ns` | 10 | P50 1.814 s | <= 2 s | Pass |
| `iteration.data.p95_ns` | 10 | P95 8.323 s | <= 5 s | **Fail** |
| `iteration.structural.p50_ns` | 10 | P50 9.747 s | <= 30 s | Pass |
| `iteration.structural.p95_ns` | 10 | P95 12.342 s | <= 60 s | Pass |
| `journey.clean_to_headless_wave_ns` | 3 | P95 210.534 s | <= 1200 s | Pass |

### Raw Numeric Populations

Values are nanoseconds and sorted for review. The JSONL retains sample identity and command
outcomes. P50/P95 pairs deliberately consume the same population.

- `build.cold_ns`: `162274290700`, `173774486400`, `175096200900`, `179661110000`, `207760257200`
- `build.incremental_ns`: `7072167900`, `7176807100`, `8011261800`, `8486542800`, `10681867900`
- `gameplay.headless_wave_success`: `1`
- `iteration.body.*`: `8720158800`, `8742277300`, `8837569600`, `9059125900`, `9281180000`, `9327383300`, `9582185800`, `9844143200`, `9848554700`, `12344743000`
- `iteration.data.*`: `1498248700`, `1573196500`, `1706796300`, `1734806500`, `1814203100`, `1841314700`, `1941230800`, `1969827400`, `2297136000`, `8323292300`
- `iteration.structural.*`: `8799310300`, `9017699900`, `9356437000`, `9544779600`, `9747073400`, `10618139900`, `10874802700`, `11315804400`, `12064154600`, `12341665400`
- `journey.clean_to_headless_wave_ns`: `175388478700`, `181578500800`, `210534382400`

## Missing Required Metrics

| Metric | Required samples | Reason |
|---|---:|---|
| `gameplay.desktop_playable_success` | 1 | No current-revision human desktop observation. |
| `journey.clean_to_desktop_playable_ns` | 3 | No bounded human terminal population. |
| `frame.p99_ns` | 1000 | No public bounded pressure-frame stream. |
| `runtime.memory_bytes` | 10 | No admitted product-lifetime process-memory population. |
| `runtime.gpu_resource_bytes` | 10 | No backend-owned resource peak observation. |
| `module.add.time_ns` | 3 | No committed ordinary-author module-add task. |
| `module.add.success` | 3 | Same missing task contract. |
| `public.production.coverage_basis_points` | 1 | A static test is not an executed-call denominator. |
| `slot.configure.time_ns` | 3 | No committed ordinary-author slot-configuration task. |
| `slot.configure.success` | 3 | Same missing task contract. |

Missing metrics remain missing. Process exit, static source inspection, and private hooks are not
substituted for product observations.
These gaps are named bottlenecks, not fabricated product successes.

## Verdict And Bottleneck

The committed Rust policy maps a missing required metric or a failed complete non-hard-stop metric
to `Redirect`; only a failed complete hard-stop metric maps to `Stop`. This population therefore
produces `Redirect` for two independent reasons:

1. `iteration.data.p95_ns` exceeds its five-second target because one of ten data-edit-to-result
   samples took 8.323 seconds.
2. Ten required desktop, telemetry, and ordinary-author task metrics are absent.

The immediate product bottleneck is not cold Rust compilation. It is the combination of a real
data-iteration tail and missing desktop/author-workflow observability. This result does not justify
a scripting language, hot-patching dependency, render graph, or generalized telemetry framework.
Those choices need direct product evidence.

## Reproduction

The recorded verdict is reproduced from the committed manifest and JSONL population by
`tests/measurement_policy.rs`. The original collector and its exact process-isolation behavior are
available at the evidence-bound Git revision recorded above; they were removed from the active tree
after the baseline became historical. Current product-readiness work must collect decision-local
observations at their owning product seams instead of extending that collector.

## Non-Claims

- This local baseline grants no release or publication authority.
- It is not a Linux timing baseline or a cross-machine performance SLA.
- It does not prove desktop playability, frame P99, CPU/GPU memory, render-packet cost, module-add
  usability, slot-configuration usability, or production-call coverage.
- It does not establish parity with Bevy, Godot, Unity, or Fyrox.
- It is not a reusable benchmark framework or runtime telemetry API.
- It does not approve C#, Luau, Subsecond, a Render Host redesign, or a scaling mechanism.

## Related Authority

- `docs/plans/2026-08-01-001-refactor-product-readiness-delivery-reset-plan.md#u5-re-evaluate-product-readiness`
- `docs/benchmarks/reference-game-first-playable-protocol.md`
- `docs/benchmarks/data/protocol/v1/reference-game-first-playable.json`
- `tests/measurement_policy.rs`
- `docs/architecture/adr/0099-decision-local-product-evidence-and-publication-admission.md`
