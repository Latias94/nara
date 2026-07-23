# Reference-Game First-Playable Baseline

## Status

Prepared for collection. No first-playable baseline or product decision has been recorded yet.
RGD-U9 may prepare the local measurement plan, but execution and closure remain blocked until
RGD-U8 has one final hosted Windows/Linux CI matrix on the integrated revision.

## Collection Boundary

`reference-game/tools/measure_first_playable.py plan` is a small, standard-library-only planner
for the concrete reference game. It accepts only a clean repository root and writes one new output
directory outside that subject. It does not create a worktree, edit source, run Cargo, or create
or configure a home directory, evaluate protocol ranges, or emit `Continue`, `Redirect`, or
`Stop`.

The plan reads the committed U14 metric catalog and records the exact Git revision, required public
paths, per-metric minimum samples, isolation rules, automatic commands, manual confirmation work,
blocked workflows, unavailable collectors, and non-claims. It rejects a catalog/workflow mismatch
instead of silently choosing a global sample count. Its JSON output is local preparation data, not a
canonical evidence envelope and not a committed benchmark artifact.

```text
python reference-game/tools/measure_first_playable.py plan \
  --subject <clean-repository-root> \
  --output <new-directory-outside-the-repository>
```

The later collector must create a detached worktree beneath that output directory, place Cargo
targets there, preserve raw samples and failed samples, and delete only its own validated temporary
worktree. It must never use the active source checkout as its mutable measurement subject.

## Required Workflow

The eventual baseline must measure the public reference-game path rather than a private benchmark
hook:

| Step | Public entry point | Required observation |
|---|---|---|
| Cold and incremental builds | `reference-game/Cargo.toml` | Separate cold and warm `cargo build --locked --bins` command-boundary samples. |
| Clean headless wave | `reference-game/src/bin/headless.rs` | Terminal `wave-summary-v1` output and elapsed time. |
| Rust body edit/reload | `reference-game/src/systems.rs` | A reviewed compatible body-only edit, fresh headless runtime, and terminal wave. |
| Data edit/reload | `reference-game/scenes/startup.scene.json` | A reviewed data-only edit in the detached worktree, then a terminal headless wave. |
| Structural Rust edit | `reference-game/src/systems.rs` | A reviewed behavior-neutral source edit, rebuild, fresh runtime, and terminal headless wave. |
| Manual desktop playthrough | `reference-game/src/bin/desktop.rs` | Visible movement, HUD updates, terminal state, Enter retry, and normal close confirmed by a human. |
| Public production coverage | `reference-game/tests/public_surface.rs` | The documented public surface succeeds without a private measurement hook. |
| Module addition | No current committed public task | Blocked until a documented task and completion command exist. |
| Window-slot configuration | No current committed public task | Blocked until a documented task and completion command exist. |
| Desktop pressure telemetry | Public desktop product | Blocked until the frame, process, and GPU collectors have a bounded public product path. |

Raw samples must retain command or manual mechanism, start/end boundary, exit status, environment
fingerprint, sample index, and failure output reference. Different normalized environments are
separate populations and cannot be merged.

## Current Collector Gaps

The first plan intentionally names these unavailable measurements instead of inventing values:

| Measurement | Why it is unavailable today | Required next evidence |
|---|---|---|
| Frame P99 | The public desktop product emits no bounded frame-time sample stream. | A concrete product tracer with a defined start/end boundary. |
| Process memory | No cross-platform public process-memory collector is selected. | One platform-specific measured collector and environment contract. |
| GPU resource bytes | Backend cache statistics are not exported by the public desktop entry point. | A concrete desktop-output path for the existing backend-owned statistics. |
| Packet batch/instance/retained/clone cost | `RenderFramePacket` is currently topology-only; sprite/UI stats and clone/allocation counters are not exported by the public product. | A narrow, measured tracer before any transfer budget or Render Host redesign. |
| Module-addition task | The protocol names a task, but the reference-game documentation does not yet provide its public definition and completion command. | One documented task and public completion check. |
| Window-slot task | The protocol names a task, but the reference-game documentation does not yet provide its public definition and completion command. | One documented task and public completion check. |

These gaps are named bottlenecks, not failures hidden by a synthetic substitute. The product should
measure a new tracer only after its exact public boundary and disclosure cost are defined.

## Non-Claims

- This document contains no observed timing, memory, frame, GPU, or packet-cost value.
- It does not establish parity with another engine or a general Nara performance SLA.
- A successful `plan` command does not prove that headless or desktop gameplay succeeded.
- Desktop process exit is not evidence of manual playability.
- RGD-U9 preparation does not close RGD-U8, RGD-U9, RGD-U10, RGD-U11, or RGD-U12.

## Related Authority

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u9-record-the-first-playable-product-baseline`
- `docs/benchmarks/reference-game-first-playable-protocol.md`
- `docs/benchmarks/data/protocol/v1/reference-game-first-playable.json`
- `tests/support/first_playable_evidence.rs`
