# Reference-Game First-Playable Baseline

## Status

RGD-U9 collected the automatic Windows slice of the first-playable baseline at source revision
`c477f7de542ba171828d4b7d5f23f505c2c7c1cd`. The protocol verdict is **Redirect**: all 10 admitted
required metrics passed their frozen targets, but 10 other required metrics have no complete
admissible population. No observed hard-stop metric failed, so this is not `Stop`.

The local-preparation snapshot correctly stated, "No first-playable baseline or product decision has been recorded yet."
That sentence describes the pre-U8 state only. It is retained here to make the distinction between
the non-executing planner and this later collection explicit.

## Source And Prerequisites

- Measurement source: `c477f7de542ba171828d4b7d5f23f505c2c7c1cd`.
- Hosted prerequisite: RGD-U8 GitHub Actions run
  [`30154962046`](https://github.com/Latias94/nara/actions/runs/30154962046), with all six
  root/reference-game/module-consumer jobs passing on Windows and Ubuntu at executable revision
  `1e60291fd9ce890b0ddfd04cce427c02c4c9c4a5`.
- The only change from the hosted executable revision to the measured revision is the U8
  verification and engineering-memory documentation. No Rust, Cargo, policy-test, or workflow input
  changed.
- Protocol ID: `reference_game_first_playable_v1`; committed protocol BLAKE3
  `82986505c1074833e87c05feae3189e985165bc04adb7bfd9769724c4452cc03`.
- Measurement-plan JSON SHA-256:
  `e874993c85132c7bcc65b0102a5958c4f1e4323002e897de2b6bae4ee80915ac`.
- One-run collector SHA-256:
  `b6b3a3b5a8ce0f4f33170585cb838e7f828ed5fccbf1a684346a641a11f4a00b`.
- Raw JSONL SHA-256:
  `9a4175fda556672c3717757556252bd86e64d70851a6928ff2a2dcf49c59a50b`.
- Run-manifest SHA-256:
  `d2e39f23dca21858e5f52765baebda0adf63db7829e11ab3138bb84de1958ad4`.

The planner and collector operated on a clean detached worktree. Cargo targets, HOME, and temporary
files were placed under the external collection root. Cargo used one build job and offline
dependency resolution against a copied, read-only dependency population in an isolated
`CARGO_HOME`. The user Cargo configuration, credentials, and global-cache tracker were not copied.
HOME, USERPROFILE, TEMP, TMP, APPDATA, and LOCALAPPDATA also resolved under the external root;
only the stable Cargo, rustc, and rustdoc executables were read from their installed toolchain.
The active source checkout and detached source bytes were unchanged after collection.

The raw JSONL and per-command logs remain local audit support rather than a canonical evidence
envelope. The complete normalized numeric populations are reproduced below so this committed
baseline does not depend on the local collection directory.

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
| Collection window | 2026-07-25 15:45:21Z to 15:59:43Z |
| Environment fingerprint | `0952921828e09c731554a11538d7a2ba62191e935c718726af97082d1500c30f` |
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
| Data edit | Toggle the persistent player `hit-points` field between 20 and 21, then start a fresh public headless runtime without rebuilding. Patch SHA-256: `cf9fc98ce69f22f054d0d054c493f30b0e77e2fd2d94eccb45e1813dd22a6378`. |
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
| `build.cold_ns` | 5 | P95 154.578 s | <= 1200 s | Pass |
| `build.incremental_ns` | 5 | P95 6.801 s | <= 120 s | Pass |
| `journey.clean_to_headless_wave_ns` | 3 | P95 151.411 s | <= 1200 s | Pass |
| `gameplay.headless_wave_success` | 1 | Exact 1 | = 1 | Pass |
| `iteration.body.p50_ns` | 10 | P50 5.020 s | <= 15 s | Pass |
| `iteration.body.p95_ns` | 10 | P95 6.962 s | <= 30 s | Pass |
| `iteration.data.p50_ns` | 10 | P50 0.056 s | <= 2 s | Pass |
| `iteration.data.p95_ns` | 10 | P95 0.067 s | <= 5 s | Pass |
| `iteration.structural.p50_ns` | 10 | P50 5.616 s | <= 30 s | Pass |
| `iteration.structural.p95_ns` | 10 | P95 7.935 s | <= 60 s | Pass |

### Raw Numeric Populations

All timing values below are nanoseconds, sorted only for compact review. Sample identities in the
collector were contiguous from one before sorting.

- `build.cold_ns`: `144054990700`, `144771830100`, `148232009300`, `151311956400`,
  `154578005500`
- `build.incremental_ns`: `4501157200`, `4654676500`, `5775436200`, `6426947600`,
  `6800821900`
- `journey.clean_to_headless_wave_ns`: `144155632900`, `148998759400`, `151411080900`
- `iteration.body.*`: `4239782500`, `4601978100`, `4625741600`, `4749800900`,
  `5020209200`, `5395953000`, `5874899800`, `6537166800`, `6899859000`,
  `6962318900`
- `iteration.data.*`: `52456500`, `53883200`, `53889600`, `54891900`, `56360300`,
  `59001700`, `59459800`, `60002100`, `60365200`, `66575900`
- `iteration.structural.*`: `5068156300`, `5077273000`, `5291695600`, `5436338500`,
  `5616415600`, `5672217000`, `5811461300`, `6336315500`, `7417280700`,
  `7934850000`
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

Four failed attempts, gates, or rejected records were retained as local evidence:

1. A one-second terminal-tool timeout closed the collector output pipe before preflight completed.
   It started no Cargo sample and produced no manifest.
2. The first exact data-edit preflight used `hit_points` instead of the persistent field name
   `hit-points`. It rejected before Cargo or source mutation. The corrected exact byte anchor was
   reviewed before collection.
3. The first complete 75-record attempt exposed the user's existing Cargo home to its child
   processes. Cargo updated the shared global-cache tracker, so the attempt failed the unchanged
   user-home requirement even though all commands succeeded. Its raw SHA-256 is
   `dd8448f908377a2677989a875c3d8341f6deafac8dd8536c6640f6a32b2af8fe`; none of its values are used
   in this baseline. A later copy gate also rejected twice when another Cargo process started
   before cache isolation; neither rejection started a copy or a sample.
4. The admitted-environment collector translated a successful
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
structural rebuild-plus-fresh-runtime P95 are 6.962 and 7.935 seconds in this workload; data
reconstruction P95 is 66.6 milliseconds. Those measurements describe this reference game only.

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

## Reproduction

Generate the committed, non-executing collection plan from a clean source revision:

```text
python reference-game/tools/measure_first_playable.py plan \
  --subject <clean-repository-root> \
  --output <new-directory-outside-the-repository>
```

Then create one detached worktree under the output root, use empty target directories for cold
populations and one retained target for warm populations, set one Cargo build job, copy only the
required crates.io registry into an isolated Cargo home, redirect every writable home and temporary
environment path under the output root, and execute the mechanisms and exact edit identities above.
Preserve every raw record and failed attempt. Do not collect from the active source checkout or
combine another environment with this Windows population.

## Non-Claims

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
- `docs/knowledge/engineering/verification/2026-07/2026-07-25T115419Z-rgd-u8-final-hosted-three-workspace-ci-verification-0ee1fb9b871a4716ae3d0c533fbdd044.md`
