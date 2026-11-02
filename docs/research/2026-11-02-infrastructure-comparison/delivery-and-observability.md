# Delivery, Iteration, Observability And Engineering Economics

Date: 2026-11-02. Nara: `a2640a10973b6d1724176a506237bc458aa5f3bc`.
This is source-based comparative research. No timing experiment or reference-engine execution
was performed for this audit. The [comparison index](README.md) owns revisions and overall scope.

## The Obligation

A production engine must make the author's edit reach the intended executable and content,
explain failure, and reproduce the resulting game outside the development checkout. It must
also let a developer attribute a slow or incorrect result to an owner without rebuilding the
mental model of the whole engine. Build correctness, iteration latency, resuming the same
playable situation, diagnosis and delivery coverage are separate obligations.

The economic quantity is the repeated cost of making a correct change: implementation effort,
waiting for feedback, locating failures, restoring context, and maintaining the target matrix.
A sophisticated publication protocol can reduce recovery cost while increasing integration
and diagnostic cost. Both effects belong in a CTO assessment. Neither raw file count nor test
count measures that tradeoff.

## What The Sources Actually Implement

| Concern | Nara | Bevy | Fyrox | Godot |
| --- | --- | --- | --- | --- |
| Executable selection | Explicit package/bin/profile/target facts, Cargo message decoding, captured bytes and provenance; no directory-name guess as final executable authority. | Ordinary Rust/Cargo application and feature graph; dynamic linking and limited system hotpatch facilities are present. | Project-manager build/run profiles and game/editor dynamic-library configurations; the inspected PC export helper scans a target directory for matching binary names. | Platform export presets and engine templates, with project resources assembled for the selected target. |
| Edit-to-result | A supervised fresh process, validated readiness and retained last-good generations. Rebuild/resume is still separate. | Native rebuild plus optional dynamic linking; the inspected hotpatch example changes function bodies through a specific toolchain. | Serializable plugin state and dynamic-library replacement form a concrete reload path, with object-lifetime constraints. | Script reload and export use an engine whose executable need not be rebuilt for an ordinary script edit; native extensions have different costs. |
| Diagnostics | Classified bounded reports, pressure snapshots and concrete GPU-completion/resource observations. | Frame/entity diagnostics and optional tracing exporters; scheduling and rendering have instrumentation hooks. | Engine/scene/render statistics and Editor statistics consumers. | Runtime debugger/profiler protocol and interactive CPU/script/visual profiler consumers. |
| Continuous qualification | Windows/Linux root and independent consumers, feature checks, portable wasm compilation, native evidence records. | OS-specific jobs, cache restoration, additional validation/example workflows and separate lint/test tools. | Windows/Linux/macOS test matrix, a dedicated CI profile and Rust cache; additional target jobs. | Static-check first stage, then separate platform workflows and selected tests. |

The table compares mechanisms in the pinned code, not equally mature product guarantees or
benchmark results. Nara's source-free cooked Player qualification is bounded to a Windows
headless consumer; it does not prove all desktop platforms. An engine having a CI job also
does not prove every hardware, input or Editor workflow on that platform.

Source anchors:

- Nara's [`CapturedCargoExecutable`](../../../src/project_tooling/cargo_build.rs#L238) retains
  bytes, digest, provenance, diagnostics and the original file capability.
  [`PreparedPlayerPackage::bind`](../../../src/project_tooling/player_package.rs#L293) checks
  selection and generated input; its comments explicitly deny proving executable semantics by
  scanning code. See the [Cargo guide](../../guides/cargo-builds.md) and
  [cooked Player plan](../../plans/2026-10-31-cooked-player-delivery-plan.md).
- Bevy's [feature declarations][bevy-features] distinguish dynamic linking, tracing and
  hotpatching. Its [hotpatch example][bevy-hotpatch] demonstrates function-body changes and
  names the required `dx` invocation. That is not arbitrary Rust layout migration.
- Fyrox's [build profiles][fyrox-profiles] select dynamic game/executor/editor builds and
  `prefer-dynamic`. Its [PC exporter][fyrox-pc] builds a package and searches the release
  directory for matching executable stems. That limited helper is useful counterevidence to
  the assumption that every mature engine has stronger artifact identity than Nara.
- Godot's [PC export implementation][godot-pc] selects an engine template and copies it to the
  target; [GDScript reload][godot-reload] has explicit keep-state behavior. Those paths do not
  make Rust compilation latency an apples-to-apples comparison with script reload.

## D1 — Keep Exact Build Authority; Complete Situation Restoration

**Assessment:** Nara should preserve its explicit artifact boundary. Cargo owns dependency
resolution, the engine owns captured build results, and the Runtime owns admission/execution.
This is a useful division. The existing bootstrap diagnostic regression in the
[preceding review](../../audits/2026-11-02-refactor-follow-up-review.md#f4--p2-failed-project-tool-bootstrap-discards-actionable-compiler-diagnostics)
is a reason to unify failure handling within that boundary, not discard it.

The missing product step is preserving the situation being debugged. In
[`begin_start`](../../../editor/src/supervised_play/owner.rs#L615), Restart and RunLastGood
activate an already admitted generation; the other branch starts a build. This does not capture
and restore game state through a rebuilt executable. The game already demonstrates the right
seam: [Two Level Game's checkpoint](../../../samples/two-level-game/src/save/mod.rs#L1)
excludes World entities, engine instances, tasks and native handles, and validates domain data
against prepared content before applying a restore.

**Decision:** Connect a game-owned, versioned checkpoint to the existing supervised rebuild
path. Define capture consistency, compatibility/refusal, target content identity and restore
failure behavior. Keep automatic capture of arbitrary ECS/native state out of the promise.

**Alternative:** Native hot reload may reduce a subset of iteration costs, but it requires
accounting for live callbacks, trait-object vtables, jobs and destructors. It also cannot remove
compilation itself. Fyrox provides a concrete example rather than a free guarantee; its official
[hot-reload documentation][fyrox-hot-doc] describes serialized plugin entities and dangling
object risks. Reuse its question set, not an assumption of universally safe unloading.

**Acceptance:** Reach a non-startup checkpoint, make a function-body edit, rebuild, restore and
observe the changed behavior in the same situation. A compile error preserves last-good;
incompatible checkpoint/content refuses explicitly. Repeat with a schema change whose migration
is deliberately unavailable. Report build, child startup and restoration separately.

## D2 — Make Feedback Latency A Current Product Metric

Nara has already done more useful measurement than a “no profiling exists” claim would admit.
The [October 20 Rust/Play experiment](../../benchmarks/2026-10-20-rust-play.md#final-public-consumer)
at `2c26c92` reports five retained samples per class after a Windows path correction:

| Historical public workflow | Median seconds | Observed range |
| --- | ---: | ---: |
| No-edit Play | 3.295 | 3.090–3.434 |
| Function-body edit then Play | 13.961 | 10.979–15.387 |
| Runtime layout edit then Play | 10.700 | 10.060–12.561 |
| Restart admitted executable | 1.499 | 1.314–1.698 |

These are historical Windows development observations with a small, non-isolated sample.
They are not current-HEAD numbers, stable tail estimates, or comparisons against other engines.
Their value is showing that “Restart” and “recompile” answer different questions and that
the edit-to-result path is important enough to attribute. No current speed regression follows
from the refactor's size or this audit's test-build duration.

**Decision:** Keep one small public game as the repeatable iteration benchmark. Track source
write -> Cargo result -> admitted generation -> Runtime ready -> restored situation. Select
thresholds from current measurements on named hardware, not from an invented competitor score.
Separate warm/no-edit, body edit, layout edit and persistent-schema edit.

Bevy's official [setup guide][bevy-setup] treats dependency optimization, dynamic linking and
linker selection as development choices. Nara can run bounded profile/linker/host-boundary
experiments with equivalent semantics before selecting one. Do not force a stable native ABI,
rebuild a package resolver, or serialize every runtime object merely to pursue faster feedback.

## D3 — Connect Existing Observations Into Causal Diagnosis

Nara's [`nara_diagnostic`](../../../crates/nara_diagnostic/src/lib.rs#L1) has bounded privacy-aware
reports and pressure observations. Its [GPU telemetry](../../../crates/nara_render_wgpu/src/telemetry.rs#L181)
measures App-frame-start to GPU completion for a submitted frame/device epoch; its resource
accounting [explicitly excludes](../../../crates/nara_render_wgpu/src/telemetry.rs#L274) several
GPU allocations and driver padding. Those limits are valuable and should survive any UI.

The gap in the inspected paths is a joined diagnostic workflow: which edit/request/frame spent
time in capture, validation, waiting, compilation, publication, simulation, rendering or retirement?
Existing safe counters and completion samples do not by themselves answer that question. A
search of Nara-owned production source found diagnostic tracing events but no pervasive
operation timing-span layer; this is scoped source evidence, not proof that external profilers
or inherited Bevy instrumentation cannot be used.

The references show different practical levels:

- Bevy has a [frame diagnostic consumer][bevy-diagnostics] plus optional Chrome/Tracy exports
  in its [features][bevy-features].
- Fyrox measures [UI/script/plugin durations][fyrox-engine-stats] and exposes them to consumers.
- Godot's [EngineDebugger][godot-debugger] manages named profilers/captures and feeds
  [Editor frame metrics][godot-profiler]. Its official [profiler guide][godot-profiler-doc]
  distinguishes inclusive and self time and notes language coverage limits.

**Decision:** Add correlation at existing owner boundaries and export to a standard trace
consumer before building a custom profiler UI. An observation identifier must not become a
new permission token. Trace work and waits separately; retain the same privacy classification,
finite retention and dropped-sample counters. Sampling must not own physical cleanup.

**Acceptance:** A deliberately slow import, rejected edit and delayed GPU completion can each
be explained by owner-stage durations and waits in one correlated capture. Existing completion
samples are not per-pass GPU timestamps and cannot attribute GPU pass cost by subtraction.
Disabling tracing leaves correctness and lifetime unchanged. Measure instrumentation overhead,
retained bytes and dropped observations.

## D4 — Optimize Assurance Coverage And Feedback Together

Nara's [CI](../../../.github/workflows/ci.yml#L16) deliberately uses one Cargo build job, disabled
incremental compilation and serial test execution. Its root job comment records a historical
cold Windows cost of about 71 minutes for checks plus the default suite, and the job has a
180-minute timeout. Those are configuration/history facts, not current hosted timings. The
same workflow has portable wasm compilation and real Windows/Linux runtime lanes.

The [preceding review](../../audits/2026-11-02-refactor-follow-up-review.md#verification-and-coverage-limits)
found two specific gaps: newly independent render consumer integration tests are not explicitly
enrolled, and one minimal feature test build imports `project_tooling` without enabling it.
That illustrates why adding tests without making the selected matrix executable is incomplete.

Bevy's [CI][bevy-ci] separates cached tests/lints and extra validation workflows. Fyrox's
[CI][fyrox-ci] uses a dedicated build profile and cache across three desktop systems. Godot's
[runner][godot-ci] runs inexpensive static checks before platform jobs. None of these files is
evidence that their total suites are faster than Nara's.

**Decision:** Map each supported public seam to a small required consumer lane, then keep
broader/native/hardware suites in their appropriate matrix. Measure feedback before splitting
jobs, selecting caches or changing debug information. Independent CI runners may parallelize;
that is different from starting concurrent Cargo commands in this shared local checkout.

Avoid a verification language that tries to infer Rust receiver types, callback ownership or
ABI safety from source text. Cargo/rustc should prove compilation; compile-fail tests should
prove forbidden API use; an executable consumer should prove the actual workflow. Static
string assertions can check a small repository convention but cannot replace those proofs.

**Acceptance:** Every supported consumer has a visible invocation; minimal supported features
compile their tests; a failure is attributable to one lane; the required feedback path gets a
measured baseline. Preserve original qualification records and their hardware/revision scope.

## D5 — Measure Public Choreography By Author Role

The [Collect Game recipe](../../../samples/collect-game/src/lib.rs#L10) is already small and
composes a contribution once. Its [gameplay plugin](../../../samples/collect-game/gameplay-plugin/src/lib.rs#L33)
also declares plugin, schema-owner/provider/binding identities and explicit schedule/projection
participation. Those concepts carry real persistence and runtime meaning; the sample is not
evidence that each gameplay function needs to repeat all of them.

The architecture risk is allowing Host admission/retirement details to escape into normal
author tasks as more contributions are added. Conversely, a convenient shortcut that discards
source/version/ownership meaning would increase future migration cost.

**Decision:** Separate ergonomics for game authors, contribution authors and Host implementers
through progressively exposed APIs backed by the same owners. For every new abstraction,
record its ordinary author-facing steps and the concrete invariant they protect. Derive or
assemble repeated facts at one ownership boundary where semantics allow it; keep real identity
choices explicit. Do not create a second engine API with weaker guarantees.

**Acceptance:** An independent author adds one persistent component, one importer, one command
and one effect. Record required engine edits, concepts chosen, repeated declarations, relevant
failure messages and time to completion. Use matched tasks before claiming Nara is more or
less ergonomic than the references. Do not use the largest qualification fixture as the
ordinary-author baseline.

## Scope Decisions

Define checkpoint compatibility, build artifact identity, trace correlation/retention and
supported feature lanes early because many consumers will depend on them. Delay a universal
native ABI, custom profiler frontend, remote build farm and broad platform promise until a
specific consumer justifies their implementation. Native desktop delivery and audio/physics
breadth should be assessed in the reference game; neither Headless nor wasm compile checks
qualify them indirectly.

[bevy-features]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/Cargo.toml#L287
[bevy-hotpatch]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/examples/ecs/hotpatching_systems.rs#L1
[bevy-diagnostics]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/crates/bevy_diagnostic/src/frame_time_diagnostics_plugin.rs#L33
[bevy-ci]: https://github.com/bevyengine/bevy/blob/ae6152152d6fb8308ad6038314b48ef295e80fbc/.github/workflows/ci.yml#L10
[bevy-setup]: https://bevy.org/learn/quick-start/getting-started/setup/
[fyrox-profiles]: https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-build-tools/src/lib.rs#L138
[fyrox-pc]: https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-build-tools/src/export/pc.rs#L26
[fyrox-engine-stats]: https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/fyrox-impl/src/engine/mod.rs#L194
[fyrox-ci]: https://github.com/FyroxEngine/Fyrox/blob/b5b151b603bba0549ac03db676568726f17e6807/.github/workflows/ci.yml#L18
[fyrox-hot-doc]: https://fyrox-book.github.io/beginning/hot_reloading.html
[godot-pc]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/editor/export/editor_export_platform_pc.cpp#L255
[godot-reload]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/modules/gdscript/gdscript.cpp#L745
[godot-debugger]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/core/debugger/engine_debugger.cpp#L41
[godot-profiler]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/editor/debugger/editor_profiler.cpp#L59
[godot-ci]: https://github.com/godotengine/godot/blob/95c0fa5fe8a019adc29d074c16b64469b04571f7/.github/workflows/runner.yml#L9
[godot-profiler-doc]: https://docs.godotengine.org/en/stable/tutorials/scripting/debug/the_profiler.html
