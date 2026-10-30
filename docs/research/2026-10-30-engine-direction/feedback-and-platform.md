# Feedback, Product Evidence, And Platform Boundaries

Date: 2026-10-30. Baseline and reference revisions are recorded in [the review index](README.md).
This is a source review, not a fresh runtime benchmark or platform qualification.

## F1: Performance Evidence Needs A Repeatable Product Loop

Classification: architectural/process risk; not a demonstrated runtime performance defect.

**Evidence.** `STRATEGY.md:43-57` selects edit-to-result latency, production coverage, module
integration, programmable authoring, and frame P99 as product metrics. The current schedule owner
at `crates/nara_app/src/lib.rs:1934-1946` validates authority around schedule execution; this
function does not itself publish phase timing spans. The tracing cursor at
`crates/nara_diagnostic/src/runtime.rs:1014-1038` emits retained diagnostic entries, which is a
different responsibility from CPU/GPU profiling. OQ-025 explicitly retains the profiler/crash
artifact question in `docs/architecture/open-questions.md:436`.

The historical [production-cost report](../../benchmarks/2026-10-20-production-costs.md) and
[Rust/Play attribution](../../benchmarks/2026-10-20-rust-play.md) contain useful real measurements.
The latter's **final**, five-sample cohort at `2c26c92` records median seconds of 3.295 for unchanged
Play, 13.961 for body-edit Play, 10.700 for layout-edit Play, and 1.499 for restarting the admitted
executable. These are different operations. They are not current-baseline results, a stable P95,
or evidence that a native hot-reload mechanism will remove the full latency.

**Counterevidence.** Nara already has phase-specific experiments, pressure observations, dependency
feature analysis, and semantic oracles. It is incorrect to describe it as unmeasured. The missing
step is a continuously comparable view of complete current workflows and their costs.

**Mechanism comparison.** Pinned Bevy gates executor spans behind `trace` and `detailed_trace`
(`repo-ref/bevy/crates/bevy_ecs/Cargo.toml:41-45`,
`repo-ref/bevy/crates/bevy_ecs/src/schedule/executor/multi_threaded.rs:408-409`). Godot separates
profiler registration (`repo-ref/godot/core/debugger/engine_debugger.h:112-125`) from its performance
monitors (`repo-ref/godot/main/performance.h:140-149`). Bevy also reports automatic tracking of
runtime cost, compilation, and binary size in its
[August 2026 project report](https://bevy.org/news/bevys-sixth-birthday/), accessed 2026-10-30.
This is evidence for measuring costs separately, not a requirement to copy its infrastructure.

**Recommendation.** Add optional, low-overhead owner-phase tracing and a small scheduled measurement
job over three maintained workloads: an ordinary game, a large authored document, and the future
custom Editor vertical slice. Record source/toolchain/hardware, frame-time distribution, owner
occupancy, input-to-present latency, allocation/residency, and edit-to-result phases. Use a stable
machine and comparable populations before adding performance gates. Keep high-frequency profiling
separate from the bounded diagnostics stream; first establish a baseline, then agree budgets.

**Why now.** ECS replacement, a task kernel, incremental projection, and graphics graph optimizations
cannot be rationally prioritized if their end-to-end costs are only revisited after architectural
work. Instrumentation is cheaper before several execution domains proliferate.

## F2: Validation Cost Can Discourage The Refactors It Is Meant To Protect

Classification: engineering feedback risk with historical measured cost; current duration unmeasured.

**Evidence.** `.github/workflows/ci.yml:41-52` retains a 180-minute root-job envelope and records the
historical roughly 71-minute cold Windows path. Lines 108-127 run full default and all-feature
workspace suites followed by the Dear renderer cohort, with one test thread. `CARGO_INCREMENTAL=0`
and one build job are explicit at lines 16-19. The file contains no cross-job/cross-run cache step.
The [existing CI study](../2026-10-20-ci-feedback-and-bevy-comparison.md) attributes actual costs and
records already-landed positive/negative compiler-test separation. Its old failed run does not
establish that current CI fails or still takes exactly that long.

**Trigger.** A small shared-owner change needs a prompt correctness signal, while expensive native,
compiler-negative, external-product and platform journeys still need complete periodic/release
coverage. Increasing an all-inclusive timeout only addresses cancellation.

**Counterevidence.** One Cargo command per checkout, bounded native tests, negative API checks,
renamed external consumers, and all-feature coverage have real purposes. Arbitrarily deleting
those checks or parallelizing Cargo inside this checkout would remove protections.

**Recommendation.** Measure current job phases, maintain a fast deterministic owner/contract lane,
and schedule costly native/product/compiler suites with explicit ownership. Cache immutable build
artifacts by toolchain, OS, target, profile, and feature identity with trusted write policy. Evaluate
CI-only debug settings using controlled observations. Distinguish feedback ordering from reduced
coverage. No new Rust-source-analysis script or custom build scheduler is justified.

## F3: Desktop Portability Must Be Proven Below The Renderer

Classification: confirmed platform capability gap; future architecture risk if portability is
assumed rather than exercised. Not a defect in a claimed supported macOS release.

**Evidence.** The strategy names Windows/Linux/macOS as desktop design targets. The current Unix
adapter reports directory enumeration only on Linux
(`crates/nara_fs/src/platform/unix.rs:45-69`), and its non-Linux branch explicitly rejects enumeration
at lines 102-109. `crates/nara_fs/src/platform/mod.rs:101-109` rejects no-replace temporary-file
publication on non-Linux Unix, and lines 189-200 restrict exact stored-spelling verification to
Windows/Linux. Native Editor qualification explicitly does not establish Linux/macOS parity
(`docs/verification/native-editor-qualification.md:15-18`). The CI platform matrix at lines 49-52
contains Windows and Linux.

**Counterevidence.** These are explicit unsupported/proof tiers, not silent pathname fallbacks.
The filesystem layer already distinguishes source binding, publication atomicity, identity, and
durability. A working wgpu Metal backend would not prove the Browser, first Save, export, supervised
Play, input method, or window-retirement workflows.

**Recommendation.** Before freezing the next public filesystem/Host contracts, run a small native
macOS Open/new-Save/rename/export/Play/close spike and a Linux failed-candidate cleanup trajectory.
Keep the logical operation contract separate from the platform guarantee and configured trust
tier. Repair unavailable adapters or explicitly revise the selected platform policy; do not weaken
an existing guarantee by silently switching to paths. This does not require promising macOS now.

**Why now.** If mandatory operations embed one OS's proof mechanism, adding a second toolkit or
platform later can force changes across every owner rather than inside its adapter. The spike
tests that risk without prebuilding browser/mobile/console support.

## F4: The Next Consumer Should Exercise Cross-Domain Writes And Lifecycle

Classification: product-validation gap; not a claim that missing physics/audio/animation are bugs.

**Evidence.** `reference-game/README.md:1-7` explicitly describes a regression fixture, not a finished
commercial game. The independent Two-Level game exercises travel, checkpoint restoration, identities,
Images, and source-carrying export; its README carefully limits what this proves. Accepted
[ADR 0095](../../architecture/adr/0095-plugin-owned-specialized-domains-and-project-configuration.md)
deliberately assigns specialized domains to plugins, avoiding a universal solver/audio API.
OQ-005, OQ-028, and OQ-029 retain physics, animation write arbitration, and active/enabled semantics.

**Trigger.** A physics-controlled actor also has animated presentation, plays a sound, can be disabled,
and crosses a Scene boundary. This puts pressure on fixed/presented transforms, event ordering,
source versus runtime identity, asset lifetime, and physical shutdown together. No amount of
isolated admission tests establishes that combined authoring experience.

**Recommendation.** Extend a maintained small game with one opinionated external physics integration,
simple animation, sound, disable/re-enable, pause/resume, Scene travel, and save/restore. Use ordinary
public APIs and the real Editor. Qualify the ownership of writes and retirement at those actual
join points; do not standardize interchangeable physics engines or a general animation framework
first. In parallel, use one self-drawn Editor tool as the UI consumer.

**Decision rule.** A complete baseline game is valuable pressure, but it must not become a veto on
bounded experiments for foreseeable requirements. Conversely, the number of crates, ADRs, tests,
or passing refusal cases is not a measure of gameplay completeness. The appropriate next unit is
a working end-to-end capability with fewer caller responsibilities and measured cost.

## F5: Keep Current Navigation Aligned With Decisions Already Made

Classification: confirmed documentation drift; not a runtime defect or permission blocker.

`docs/architecture/open-questions.md:262-277` still frames OQ-010 around a complete existing
"egui panel" and a later decision about the primary Editor toolkit. The
[October 27 UI study](../2026-10-27-custom-drawn-editor-ui-boundaries.md) explicitly records the
user-selected Nara-drawn destination and the current Dear ImGui control implementation. The
[architecture map](../../architecture/README.md) already reflects that clarified direction.

The useful remaining question is which complete tool will qualify the new UI/Host boundary and
how it will migrate current behavior. An old toolkit name or a historical convergence gate should
not send future contributors back to a decision already made. Update current navigation during
the owning implementation unit, retain historical evidence in its original scope, and keep one
concise current status per decision. A missing registration or an obsolete plan cannot override
current task authorization. This review records the inconsistency without rewriting Accepted ADRs.

## Verification Limits

The main reviewer read the cited files and inspected Git status/reference revisions. No Cargo
command, native platform experiment, performance run, source change, or commit was performed for
these findings. Historical measurements retain their original revision, machine, and endpoint.
