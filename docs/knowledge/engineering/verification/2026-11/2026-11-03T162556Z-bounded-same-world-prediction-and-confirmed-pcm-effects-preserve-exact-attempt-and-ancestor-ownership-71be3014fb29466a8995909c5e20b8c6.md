---
type: "Verification Evidence"
title: "Bounded same-World prediction and confirmed PCM effects preserve exact attempt and ancestor ownership"
description: "Complete persistent Rapier restoration, closed logical state and outcome admission, confirmed-prefix witnesses and sample-clock PCM effects qualify independently of ordinary FixedTime rollback."
timestamp: 2026-11-03T16:25:56Z
record_id: "71be3014fb29466a8995909c5e20b8c6"
producer_id: "codex-foundation-main"
related_plan: "docs/plans/2026-11-03-joint-motion-execution-plan.md"
git_branch: "refactor/foundation"
git_commit: "d81f3b2"
---

# Verification

Windows 11, Rust 1.95.0, `CARGO_BUILD_JOBS=1`, `RUSTUP_TOOLCHAIN=1.95.0`, root `target`,
one Cargo invocation at a time. `git_commit` identifies the integrated base; this record covers
the independent bounded prediction/PCM successor. No production Physics or Audio backend is added.

| Gate | Result |
| --- | --- |
| Final default motion consumer, including complete 32-tick continuation | 22/22, nextest `ece1bbba-cad0-41fa-8368-4b70ffff2063` |
| Final Bevy `multi_threaded` feature-unified motion consumer | 23/23, `23417f7e-fa92-4cf9-999e-6dde320e9f24` |
| Fixture workspace Clippy, all targets/features | `-D clippy::correctness` passes; the pre-existing Reflect dead-code warning remains |
| Existing independent native history consumer, after the optional solver-checkpoint addition | 7/7, `b80d6080-256e-4f88-9953-db7ff540cf03`; locked dependencies, actual Windows/DX12, fallback disabled |
| Independent review | Audio/checkpoint review and prediction-model review close with no remaining P1/P2; reviewers did not run Cargo |
| Formatting and documents | Fixture `cargo fmt --all --check`, `git diff --check` and 1,223 local Markdown link targets pass; memory render/validation passes with existing legacy warnings |

Reproduction commands, with the environment above:

```text
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/motion-execution/Cargo.toml --target-dir target -p motion-execution-consumer --test-threads 1
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/motion-execution/Cargo.toml --target-dir target -p motion-execution-consumer --features unified-multi-threaded --test-threads 1
cargo +1.95.0 clippy --manifest-path tests/fixtures/motion-execution/Cargo.toml --target-dir target --workspace --all-targets --all-features -- -D clippy::correctness
cargo +1.95.0 nextest run --locked --manifest-path tests/fixtures/render-policy-consumer/Cargo.toml --target-dir target --test history --run-ignored only --test-threads 1
```

The last command also requires `WGPU_BACKEND=dx12` and `NARA_WGPU_FORCE_FALLBACK=0`.
The native consumer's existing lock remains valid; no unrelated dependency update was needed.
Raw command logs stay in `target/motion-prediction-*.log` and are not tracked memory.

# Result

The selected restoration owner captures the complete persistent Rapier 0.33.0 state alongside
motion input/intent/actor locals, logical particle membership/values, explicit seeded RNG/phase,
pending deterministic commands, admitted ordered outcomes and pending semantic cue intents.
Rapier documents `PhysicsPipeline` as rebuildable scratch; restore reconstructs it and revokes
the prior attempt's join permission. A configured opaque `WorkerGate` refuses capture/restore.

One managed custom schedule executes each logical attempt in the same Runtime and World. These
logical ticks do not rewind App clocks or claim ordinary FixedUpdate rollback. The corrected run
matches a clean corrected run's complete authoritative checksum, including encoded solver bytes,
then continues matching through logical tick 32 for direct and actual borrowed-thread modes.
Entity spawn/despawn, RNG and pending commands survive restoration. Physical frame grouping and
real worker completion order vary without changing the classified state.

Attempts use the existing Runtime generation plus a non-rewound ordinal. Semantic audio identity
instead uses input event, occurrence, cue and sequence. Old same-tick results held until a
replacement exists refuse before state/effect application. Other Sessions' matching tick/ordinal
tokens refuse admission, simulation, abandonment and confirmation.

The confirmed frontier is a tick plus its semantic ancestor digest. A checkpoint retains at most
33 local ancestor digests; restore checks the confirmed witness before changing World state and
then replaces the speculative branch history. A later checkpoint from a discarded branch cannot
rewrite a confirmed prefix. The PCM ledger and sample cursor are outside that checkpoint, and
failed cue publication cannot advance confirmation.

The concrete offline PCM owner emits actual finite sine samples on an absolute sample clock.
No discarded attempt publishes its cue. Tests observe silence before confirmation, nonzero
scheduled output, identical-duplicate suppression, payload-conflict and late-cue refusal, atomic
batch validation, chunk-independent samples and closure through managed Runtime retirement.
The actual native file-writing and Local-state systems are rejected before registration/execution;
the refusal test verifies no file appears and no Local system executes.

The previous managed motion cohort remains in both final runs: exact Pause/Step and Transform
projection, held-worker join before pose publication, NonSend placement, narrow command Ack and
retained partial effects after a later sticky fault. The separate DX12 cohort retains its actual
captured discontinuity, two-View numeric histories, output custody and retirement coverage.

## Counterexamples and corrections

Two meaningful bugs were caught before qualification. Tick/ordinal alone let a foreign Session's
outcome and confirmation token equal the current attempt. Tick comparison alone admitted a future
checkpoint from an old branch after confirming a different ancestor. Each new regression first
failed on its corresponding `Ok` result: nextest `9af0d9d4-436d-477d-be01-118134da961b` and
`be5f795f-e781-40ae-9850-ed0aa5d9e274`. Windows then reported fail-fast teardown after the failed
assertions; neither run is GREEN evidence. The generation field and confirmed ancestor witness
fix the owners rather than weakening the assertions. Final runs include both regressions, a
valid same-prefix forward-restore control and failed-publication confirmation control.

# Follow-up

The current unit selects this finite prediction/effect boundary under ADR0024. It does not add
an engine prediction API, universal World snapshot, canonical seeded-RNG service, arbitrary
trusted-Rust purity enforcement, physical audio device, cross-build checkpoint loader,
cross-platform bit-exactness or production automatic parallel profile. The inventory refuses
uncovered contributions; it cannot discover hidden effects in arbitrary Rust. If a cue's sample
time has already passed, publication refuses rather than rewinding the external audio clock.

Next, complete the separately planned R10 authored collision-definition workflow: source-owned
generated body/collider/debug roles, edit/remove/duplicate membership, diagnostics and retirement
without persisting runtime handles. Reuse these motion authors and temporal owners. Conditional
affine authoring, broader repeatability/effects, R4c's remaining consumers and the full foundation
rebaseline remain open. Do not mark the overarching Goal complete for this fixture qualification.

# Sources

- [Owning contract](../../../../architecture/adr/0024-determinism-fixed-update-and-replay-policy.md)
- [Joint unit](../../../../plans/2026-11-03-joint-motion-execution-plan.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Independent fixture](../../../../../tests/fixtures/motion-execution/README.md)
