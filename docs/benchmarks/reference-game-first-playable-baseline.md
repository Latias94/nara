# Reference-Game First-Playable Baseline

## Status

The surviving RGD-U9 Windows capture at source revision
`b2ddb5b0ea6ea9b98e213619dccf65a5326b1505` supports a historical **Redirect** observation: 10
recorded metrics passed their frozen targets, while 10 required metrics have no complete population.
No observed hard-stop metric failed, so the historical observation does not support `Stop`.

RGD-U9 is nevertheless **evidence-repair pending**. The executed one-run collector was not committed
and was modified after its first preflight failure, so the repository cannot independently replay
the capture from reviewed instructions. The samples remain useful context, and the missing metrics
still imply `Redirect`, but this document no longer claims a reproducible U9 completion or satisfies
RGD-U11's U9 dependency.

The local-preparation snapshot correctly stated, "No first-playable baseline or product decision has been recorded yet."
That sentence describes the pre-U8 state only. It is retained here to make the distinction between
the non-executing planner and this later collection explicit.

## Source And Prerequisites

- Measurement source: `b2ddb5b0ea6ea9b98e213619dccf65a5326b1505`.
- Hosted prerequisite: RGD-U8 GitHub Actions run
  [`30462379022`](https://github.com/Latias94/nara/actions/runs/30462379022), with all six
  root/reference-game/module-consumer jobs passing on Windows and Ubuntu at executable revision
  `ef8f300889086cfa1241c45c19bfc8d4edf8ffb3`.
- The only change from the hosted executable revision to the measured revision is the U8
  verification and engineering-memory documentation. No Rust, Cargo, policy-test, or workflow input
  changed.
- Protocol ID: `reference_game_first_playable_v1`; committed protocol BLAKE3
  `82986505c1074833e87c05feae3189e985165bc04adb7bfd9769724c4452cc03`.
- Measurement-plan JSON SHA-256:
  `a41c43a9873820de79f1ee1aef63e8393127a94abe70eb85f78fc3f1102f4121`.
- One-run collector SHA-256:
  `c70aea68f493ca3874e4beb25ad78b4183407d0e200c925b5951e286fe899cf4`.
- Raw JSONL SHA-256:
  `076b6057413375b74d49f0c5bed1abfea08cbddba6841e278ee56f21aca82515`.
- Run-manifest SHA-256:
  `7b8d587af8eb3a9f7cd0b4484a32a20edd3d441c62be4866d6b049b4b59ef527`.
- Historical archive:
  [`data/runs/v1/rgd-u9-b2ddb5b/import-receipt.json`](data/runs/v1/rgd-u9-b2ddb5b/import-receipt.json),
  with LF-normalized semantic copies and exact original transports encoded separately.

The planner and collector operated on a clean detached worktree. Cargo targets, HOME, and temporary
files were placed under the external collection root. Cargo used one build job and offline
dependency resolution against a copied, read-only dependency population in an isolated
`CARGO_HOME`. The user Cargo configuration, credentials, and global-cache tracker were not copied.
HOME, USERPROFILE, TEMP, TMP, APPDATA, and LOCALAPPDATA also resolved under the external root;
only the stable Cargo, rustc, and rustdoc executables were read from their installed toolchain.
The active source checkout and detached source bytes were unchanged after collection.

The plan, raw JSONL, run manifest, and preflight failure now have repository-addressable historical
copies. The original transport bytes are recoverable from their Base64 records and retain the
published SHA-256 values. Per-command logs remain local support and were not individually bound by
the original manifest, so they are not claimed as canonical evidence. The complete normalized
numeric populations are also reproduced below.

## Environment

| Field | Value |
|---|---|
| OS | Windows 11 x86_64, version `10.0.26200` |
| Runner class | `local-windows-11-workstation-build-26200` |
| CPU | 13th Gen Intel Core i9-13900KF, 24 physical / 32 logical cores |
| Rust | `rustc 1.97.1 (8bab26f4f 2026-07-14)`, `x86_64-pc-windows-msvc`, LLVM 22.1.6 |
| Cargo | `cargo 1.97.1 (c980f4866 2026-06-30)` |
| Profile | Debug, Cargo incremental state retained only for warm populations |
| Build concurrency | `CARGO_BUILD_JOBS=1`, command `--jobs 1` |
| Dependency cache | Copied crates.io registry under isolated `CARGO_HOME`, read with `CARGO_NET_OFFLINE=true` |
| Collection window | 2026-07-29 18:05:05Z to 18:23:17Z |
| Environment fingerprint | `23c62c56777a214c57686aa7ee4513e8c52be6912584a64537160485e660ce6e` |
| GPU / desktop stack | Not collected by the automatic slice |

This is one Windows population. The hosted Ubuntu job proves the committed public workflow builds
and tests there; it is not a Linux timing population and is not merged with these samples.

## Mechanisms

| Workflow | Public boundary and mechanism |
|---|---|
| Cold build | Five distinct empty target directories; `cargo build --locked --bins --jobs 1` from `reference-game`. |
| Clean headless journey | For the first three cold targets, time from clean target readiness through cold build and direct execution of the public `headless` binary with `--max-ticks 96`. |
| Incremental build | The first five body-only edits, each followed by `cargo build --locked --bins --jobs 1` in one retained target. |
| Body edit | Toggle `saturating_sub(1)` and the type-equivalent `saturating_sub(1_u64)`, rebuild, then start a fresh public headless runtime. Patch SHA-256: `828c7aef98d5981c989382f853fde4ef4e163de29adf637261044e593ae150ec`. |
| Data edit | Toggle the persistent player `hit-points` field between 20 and 21, then start a fresh public headless runtime without rebuilding. Patch SHA-256: `6729bf40be8c43314e55b3857c7eb72440709964139eedf8a834a3e5da331e19`. |
| Structural edit | Add/remove a private, zero-effect `_measurement_generation: u8` field and initializer, rebuild, then start a fresh public headless runtime. Patch SHA-256: `12338d675fcac9a7cd5ffb4cf078b361b47fff19efe3a9f2434e0acf84ea06b6`. |
| Public coverage attempt (not admitted) | `cargo test --locked --test public_surface --jobs 1` passed, but the test does not implement the frozen denominator/executed-call ratio or both-terminal-path boundary. |

Every base run produced one canonical terminal summary:

```json
{"schema":"nara-reference-game.wave-summary-v1","outcome":"completed","tick":49,"score":300,"player_hit_points":20,"enemies_remaining":0,"projectiles_remaining":4}
```

Its SHA-256 is
`77819d5cac39ab3e25d9e9537e09995480e96d0e9488515cb480c7d50dba5169`.
The data variant changed `player_hit_points` to 21 and reverting restored the base digest. Body and
structural variants retained the base digest. This proves that the data edit was observed and the
two Rust edits remained behavior-neutral for the authoritative terminal result.

## Observed Results

Aggregation uses the protocol's one-based `nearest_rank_v1` rule. Timing values are shown in
seconds here; the exact nanosecond populations follow.

| Metric | Samples | Aggregate | Frozen target | Result |
|---|---:|---:|---:|---|
| `build.cold_ns` | 5 | P95 204.677 s | <= 1200 s | Pass |
| `build.incremental_ns` | 5 | P95 17.024 s | <= 120 s | Pass |
| `journey.clean_to_headless_wave_ns` | 3 | P95 205.422 s | <= 1200 s | Pass |
| `gameplay.headless_wave_success` | 1 | Exact 1 | = 1 | Pass |
| `iteration.body.p50_ns` | 10 | P50 7.829 s | <= 15 s | Pass |
| `iteration.body.p95_ns` | 10 | P95 22.469 s | <= 30 s | Pass |
| `iteration.data.p50_ns` | 10 | P50 0.069 s | <= 2 s | Pass |
| `iteration.data.p95_ns` | 10 | P95 0.078 s | <= 5 s | Pass |
| `iteration.structural.p50_ns` | 10 | P50 7.713 s | <= 30 s | Pass |
| `iteration.structural.p95_ns` | 10 | P95 9.257 s | <= 60 s | Pass |

### Raw Numeric Populations

All timing values below are nanoseconds, sorted only for compact review. Sample identities in the
collector were contiguous from one before sorting.

- `build.cold_ns`: `164688980400`, `173258829400`, `174531722300`, `183132403800`,
  `204677231200`
- `build.incremental_ns`: `6846283700`, `7643491200`, `9590718600`, `12258288200`,
  `17023536700`
- `journey.clean_to_headless_wave_ns`: `164814822500`, `173389213900`, `205422314400`
- `iteration.body.*`: `5180075400`, `6807884800`, `6962794700`, `7428467700`,
  `7828814200`, `9724545600`, `12155078000`, `12405504500`, `17129375300`,
  `22468936500`
- `iteration.data.*`: `60057400`, `63842900`, `66610700`, `67535500`, `69055300`,
  `71330600`, `73059400`, `73356400`, `74712600`, `77573600`
- `iteration.structural.*`: `6871664500`, `6879980200`, `7145894200`, `7667626900`,
  `7712519500`, `8050960700`, `8206640000`, `8327614000`, `8544481200`,
  `9256838100`
- `gameplay.headless_wave_success`: `1`

The P50 and P95 metric pairs deliberately consume the same 10 observations for their edit class;
they do not represent two separately timed populations.

## Incomplete Required Metrics

| Metric | Required floor | Current evidence | Reason |
|---|---:|---:|---|
| `gameplay.desktop_playable_success` | 1 | 0 | Current-revision human play observation is not yet recorded. Process exit cannot substitute for movement, HUD, terminal, retry, and close confirmation. |
| `journey.clean_to_desktop_playable_ns` | 3 | 0 | No three-sample clean-checkout-to-human-terminal population was collected. |
| `frame.p99_ns` | 1000 | 0 | The public desktop product emits no bounded 1080p pressure-frame sample stream. |
| `runtime.memory_bytes` | 10 | 0 | No admitted Windows peak-working-set collector spans runtime publication through the terminal frame. |
| `runtime.gpu_resource_bytes` | 10 | 0 | Backend-owned resource peaks are not exposed by the public desktop product. |
| `module.add.time_ns` | 3 | 0 | No committed module-addition task definition or public completion command exists. |
| `module.add.success` | 3 | 0 | Same missing task contract. |
| `public.production.coverage_basis_points` | 1 | 0 | The collector emitted `10000` from a successful static `public_surface` test, but did not freeze an inventory denominator, record executed calls, or complete both terminal paths. |
| `slot.configure.time_ns` | 3 | 0 | No committed window-slot configuration task or public completion command exists. |
| `slot.configure.success` | 3 | 0 | Same missing task contract. |

The original RGF-U14 approach also requested render-packet batch, instance, retained-byte, and
clone/allocation costs. Those values are not version-1 decision metrics, and the current public
desktop path exports none of them. `RenderFramePacket` remains topology-only while sprite/UI
payload statistics and submit-time resource reads stay private. No pixel payload was copied merely
to invent a measurement.

These gaps are named bottlenecks, not fabricated metric failures.

## Collection Failures

Two current-refresh failures or rejected records were retained as local evidence:

1. The first exact data-edit preflight retained the prior Windows CRLF byte anchor while the
   canonical scene fixture is LF. It rejected before Cargo or source mutation with zero matches.
   The one-run collector changed only that exact external anchor; repository and detached-subject
   bytes remained unchanged. The failure record SHA-256 is
   `d298057804996303b5b50249ec2df431bd5e897ad3f419af1ec752147a714fa6`.
2. The admitted-environment collector translated a successful
   `cargo test --locked --test public_surface` exit into a
   `public.production.coverage_basis_points = 10000` record. Review showed that the test checks
   dependency/import boundaries and headless CLI source tokens; it neither freezes the public-call
   denominator nor records executed calls, and it does not complete both terminal paths. The record
   remains in the 75-line raw artifact and its digest, but it is not admitted into this baseline.

All 75 raw records have exit status zero, one environment fingerprint, contiguous per-metric
indices, and no failure-output reference. Those command-level facts do not override the semantic
rejection of the coverage record; the other 74 records remain admitted.

## Verdict And Bottleneck

The deterministic version-1 outcome is `Redirect`. Missing required evidence forces `Redirect`
even when every observed value passes; only a complete passing suite could produce `Continue`.
There is no failed complete hard-stop observation, so `Stop` would be false.

The observed Windows data does not justify adding a mandatory scripting language, a Subsecond-style
hot-patch dependency, a render graph, a GPU arena, or a packet-transfer abstraction. Body and
structural rebuild-plus-fresh-runtime P95 are 22.469 and 9.257 seconds in this workload; data
reconstruction P95 is 77.6 milliseconds. Those measurements describe this reference game only.

The next product bottleneck exposed by U9 is **desktop product observability and documented author
tasks**, not a measured runtime-budget violation:

1. define a frozen public-call denominator and record the executed-call inventory across both
   terminal product paths;
2. establish one public, bounded desktop pressure tracer for frame, process-memory, GPU-resource,
   and packet-transfer observations;
3. define one real external module-addition task and one real window-slot configuration task from
   committed user documentation;
4. recollect the missing populations before any `Continue` claim.

OQ-007's optional gameplay-language feasibility ladder explicitly requires a first-playable
`Continue` result, so this baseline does not admit that research. The delivery plan may continue to
standalone candidate work because U14 names bottlenecks rather than requiring a fabricated
performance pass.

## Evidence Repair

The committed helper can still generate a non-executing plan from a clean source revision:

```text
python reference-game/tools/measure_first_playable.py plan \
  --subject <clean-repository-root> \
  --output <new-directory-outside-the-repository>
```

That planner is not a collector. RGD-U9 remains open until a parameterized `collect`/`verify` path is
committed and tested, then used at the integrated source revision to create a fresh bounded raw
record set and manifest. The collector must own detached-worktree creation, empty and retained
targets, one-job Cargo execution, isolated dependency/home/temp paths, exact edit/revert checks,
bounded logs, raw-record validation, and manifest hashing. Manual reconstruction from the prose
above cannot close the gate.

## Non-Claims

- This historical capture is not a reproducible RGD-U9 completion and does not unblock RGD-U11.
- This is not a Linux timing baseline; hosted Ubuntu CI is compatibility evidence only.
- It does not yet prove current-revision manual desktop playability or clean desktop journey time.
- It does not measure frame P99, process memory, GPU memory, driver residency, or packet transfer
  cost.
- It does not prove public production-call coverage; the collector's static-test-derived value is
  explicitly rejected.
- It does not establish parity with Bevy, Godot, Unity, or Fyrox, or a general Nara performance SLA.
- It is not release-grade provenance, a reusable benchmark framework, or a runtime telemetry API.
- It does not approve C#, Luau, Subsecond, a Render Host redesign, or a scaling mechanism.
- Any source, reference-game, Cargo, protocol, collector, or environment-class invalidation requires
  recollection under the committed protocol.

## Related Authority

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u9-record-the-first-playable-product-baseline`
- `docs/benchmarks/reference-game-first-playable-protocol.md`
- `docs/benchmarks/data/protocol/v1/reference-game-first-playable.json`
- `tests/support/first_playable_evidence.rs`
- `docs/knowledge/engineering/verification/2026-07/2026-07-29T163302Z-rgd-u8-final-hosted-three-workspace-ci-refresh-b3883a881dda4296b19b5490153dc3fc.md`
- `docs/knowledge/engineering/verification/2026-07/2026-07-29T182729Z-rgd-u9-refreshed-first-playable-product-baseline-verification-406733392389447c879f6de86245151e.md`
