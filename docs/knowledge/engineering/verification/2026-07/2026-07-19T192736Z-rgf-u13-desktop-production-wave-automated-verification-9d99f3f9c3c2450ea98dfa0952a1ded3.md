---
type: "Verification Evidence"
title: "RGF-U13 desktop production wave automated verification"
description: "Commit 198a680 completes the supported desktop product path, ordered input, owned render packet, native surface lifecycle, HUD, and atomic Retry; manual Windows play remains pending."
timestamp: 2026-07-19T19:27:36Z
record_id: "9d99f3f9c3c2450ea98dfa0952a1ded3"
tags: ["rgf-u13", "verification", "reference-game", "desktop", "input", "render"]
status: "automated-verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "198a680"
verified_by: "focused nextest,workspace nextest,reference-game nextest,native winit/wgpu probes,workspace check,examples,clippy,fmt,independent review"
---

# Verification

RGF-U13 was reviewed at implementation commit `198a680` on
`refactor/engine-foundation-contracts`. The automated scope starts the independent reference game
through the root product Host, drives its managed runtime through Winit, lowers ordered physical
input into the same semantic command path as headless, renders one admitted window/target/view,
and joins surface retirement with runtime shutdown.

# Result

Automated verification passed. The implementation supplies the supported desktop product path,
but RGF-U13 remains active until a human completes the documented Windows play check.

- `DesktopRun` owns desktop-profile ingest, content/plan lineage, candidate publication, Winit
  driving, App exit propagation, bounded close, and privacy-safe process results without exposing
  ordinary callers to candidate/admission/retirement choreography.
- `ButtonInput` retains current button state while recording a bounded ordered transition stream.
  Press/release in the same frame is preserved, focus loss emits deterministic releases, and
  transition-capacity or sequence exhaustion rejects atomically.
- `RenderFramePacket` owns the admitted generation/frame/window/target/view topology. The wgpu
  path captures owned sprite/UI batches, rejects extra, stale, mismatched, or repeated topology
  before surface work, and records acquire/submit/present transaction counters.
- Transparent sprite order is preserved across material changes. UI clips survive extraction and
  batching into clamped backend scissor state, with non-finite geometry rejected.
- The desktop projection supplies player/enemy/projectile sprites, health/progress HUD geometry,
  distinct terminal feedback, WASD movement, Enter Retry, and Quit over the same authoritative
  fixed-tick game as headless.
- Retry prepares a complete candidate before mutation, validates lifecycle-free insertion and
  retirement, then atomically replaces scene identity and despawns the prior generation. Hooks or
  observers that could make the transaction non-atomic reject before either authority changes.

# Evidence

- Focused input/render/window/gameplay gate: 116 passed.
- Feature-enabled `nara_render_wgpu` gate: 28 passed, 1 declared conditional skip.
- Public schedule-extension gate: 7 passed.
- Root product Host/runtime/content gate with desktop features: 87 passed.
- Independent reference-game desktop gate: 36 passed.
- Full root workspace: `cargo nextest run --workspace --locked --test-threads=1
  --no-fail-fast` -> 904 passed, 3 declared conditional skips.
- Full independent reference game: `cargo nextest run --manifest-path
  reference-game/Cargo.toml --locked --test-threads=1 --no-fail-fast` -> 49 passed.
- Both native surface-retirement smoke modes passed, including backend-before-exit destruction.
  The desktop render test also reaped the production Winit/wgpu pixel probe under a hard deadline.
- `cargo check --workspace --locked`, reference-game desktop all-target checks, and the required
  `windowed_clear`, `windowed_sprites`, and `runtime_ui_panel` example checks passed.
- Strict Clippy passed for the changed ECS/identity/scene core and the independent desktop game
  with only the repository's documented pre-existing allowances.
- Root and independent-workspace formatting plus `git diff --check` passed.
- Independent correctness, specification, and test reviews found no remaining P0/P1. The final
  review specifically covered App-exit parity, ordered edge exhaustion, packet mismatch/viewport
  rejection, candidate insertion rollback, duplicate retirement, and observer-safe Retry.

# Follow-up

Run the Windows play check from the repository root:

```powershell
cargo run --manifest-path reference-game/Cargo.toml --locked --features desktop --bin desktop
```

Verify WASD movement, visible sprite/HUD updates, terminal geometry, Enter Retry without replacing
the window/runtime generation, and normal window-close exit. Only after this succeeds should the
RGF-U13 registration become `completed` and RGF-U14 become active.

The current packet still clones sprite/UI batches and reads live image/prepared-resource storage
during submission. RGF-U14 owns allocation/clone/batch/instance/retained-byte measurement before
any transfer budget or render-host redesign. RGF-U23 owns process-global runtime execution
authority and duplicate registry review; RGF-U8 owns watcher queue bounds and observable overflow.
Browser WebGPU, device epochs/recovery, multi-target rendering, and a RenderGraph remain outside
this unit.

# Citations

- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u13-complete-the-desktop-input-and-render-wave`
- `src/project_host/runtime/action.rs#DesktopRun`
- `crates/nara_app/src/runtime.rs#RuntimeDriverScope`
- `crates/nara_input/src/lib.rs#ButtonInput`
- `crates/nara_render/src/lib.rs#RenderFramePacket`
- `crates/nara_render_wgpu/src/backend.rs#WgpuFrameTransactionStats`
- `crates/nara_ecs/src/transaction.rs#LifecycleFreeInsertionPlan`
- `crates/nara_identity/src/domain.rs#WorldIdentityDomain::replace_scene_instance_and_despawn`
- `reference-game/src/bin/desktop.rs`
- `reference-game/tests/desktop_flow.rs`
- `reference-game/tests/desktop_parity.rs`
- `reference-game/tests/desktop_render.rs`
- `reference-game/tests/first_wave.rs`
- `tests/project_runtime_boot.rs`
- `tests/runtime_driver_boundary.rs`
- Commit `198a680`
