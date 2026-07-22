---
type: "Verification Evidence"
title: "RGD-U4 fresh runtime session reconstruction verification"
description: "Verifies fresh mutable runtime sessions across generations while retaining only named immutable or process-parent authorities."
timestamp: 2026-07-22T17:29:58Z
record_id: "0714cd8b40bb48d2a76a03ab75bde09d"
tags: ["rgd-u4", "runtime", "reconstruction", "isolation", "wgpu"]
status: "completed"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "549d5c25a4091585c8cca3dc51e1f7748fd2cd9d"
verified_by: "Focused nextest, strict Clippy with documented pre-existing allowances, formatting, source-boundary review, and staged-scope audit"
---

# Verification

RGD-U4 was verified against implementation commit `549d5c25a4091585c8cca3dc51e1f7748fd2cd9d`.
The unit is characterization-first: U2/U3 already supplied the construction and per-runtime fault
boundaries, and U4 adds the missing production-shaped evidence that those boundaries reconstruct
fresh mutable sessions while retaining only explicit immutable or process-parent authority.

# Result

- Sequential and concurrently published `ProjectHost` runtimes receive distinct `RuntimeGeneration`,
  `World`, gameplay queue, fixed time, task pools, test-owned service-session identity, and
  `WorldIdentityDomain` identity.
- Alternating and true overlapping Runtime drives preserve independent time and gameplay-command
  state. Existing U3 overlap coverage confirms that system, condition, command, and observer faults
  remain scoped to their own runtime while a healthy peer continues.
- Project content, the frozen `ComponentRegistrySnapshot`, and the compiled plan fingerprint remain
  explicitly shared immutable authority. No mutable runtime owner is reused to obtain that sharing.
- Editor Play startup publishes exactly one test-owned service session for each successful runtime;
  cancelled starts publish none, and Stop/Restart reconstructs a distinct session and generation.
- A `CloseIncomplete` task owner prevents a replacement Host start before construction. After the
  original owner completes truthful retirement, one replacement starts with a fresh generation and
  no double-close.
- `WgpuRenderBackend` documents `(instance_id, device_epoch)` as its process-local device namespace.
  A real adapter/device/cache test deliberately retains a test-owned `Device` and `Queue` parent
  while dropping the predecessor backend, then proves the replacement has a different namespace and
  an empty texture-cache state.

# Authority Matrix

| Authority | U4 treatment | Evidence |
|---|---|---|
| Expanded startup scene and retained content | Immutable shared parent | Both sequential and concurrent Host starts retain the same snapshot scene pointer. |
| Frozen component registry | Immutable shared parent | Both published Worlds retain `ComponentRegistrySnapshot::ptr_eq` with the prepared snapshot. |
| Compiled definitions and recipe | Immutable shared parent | Reconstructed runtimes retain the same `PluginPlanFingerprint`. |
| `World`, gameplay queue, fixed time, and task pools | Reconstructed per runtime | Concurrent owner addresses differ; alternating drive counts and accepted commands remain independent. |
| Service state | Reconstructed per runtime | Test-owned startup service IDs differ across Host and Editor restart generations; each successful Editor runtime publishes exactly once. |
| Runtime identity | Reconstructed per runtime | `RuntimeGeneration` and `WorldIdentityDomain` IDs differ across generations. |
| Wgpu backend/cache namespace | Reconstructed per runtime | Fresh App plugin installation gives a distinct backend instance; replacement cache begins empty even with a retained test-owned device/queue parent. |
| Error route and incomplete close owner | Retained until truthful terminal state | U3 route coverage plus U4 replacement rejection prove no early replacement or route-owner loss. |

# Evidence

- `cargo fmt --all`: passed.
- `cargo nextest run --locked -p nara --features runtime-2d,serde,tooling --lib --test
  runtime_instance --test workspace_play_runtime --test stable_runtime_identity --test-threads=1`:
  121 passed, run `dd94daa3-ad5b-4dec-a7ca-1c8ceeee1265`.
- `cargo nextest run --locked -p nara_tasks -p nara_identity --test-threads=1`: 79 passed, run
  `7f61b80a-63be-475d-b0cb-ca82a03f0077`.
- `cargo nextest run --locked -p nara_render_wgpu --features sprite-submitter,ui-submitter
  --test-threads=1`: 30 passed and 1 existing skip, run
  `6f75b781-4d6b-49d0-8611-f358e75efa3e`.
- `cargo clippy --locked -p nara --all-targets --features runtime-2d,serde,tooling -- -D
  warnings` passed after allowing only pre-existing unrelated lints: `dead_code`,
  `result_large_err`, `collapsible_if`, `needless_return`, `double_must_use`,
  `too_many_arguments`, `derivable_impls`, and `drop_non_drop`.
- `cargo clippy --locked -p nara_render_wgpu --all-targets --features sprite-submitter,ui-submitter
  -- -D warnings` passed with the same documented pre-existing dependency-tree allowances except
  `dead_code`; unallowed U4 lint findings would have failed the command.
- `git diff --check` passed for the five staged U4 files. The staged scope contained only the two
  Wgpu backend test/documentation files and three Runtime/Host/Editor test files.
- `architecture_docs` was intentionally not run: the active plan assigns that governance gate to
  U1 and U7, not U4.

# Follow-up

1. Activate RGD-U5 and run one bounded reference-game command stream through the real Headless,
   Desktop, and Editor product Hosts without inspecting a raw runtime `World` from the parent oracle.
2. Keep U4's explicit immutable-parent matrix and U3's fault-route retirement guarantees intact
   while building the parity probe.

# Citations

- `docs/plans/2026-07-21-001-refactor-runtime-authority-product-delivery-plan.md#u4-prove-fresh-runtime-session-reconstruction`
- `docs/architecture/adr/0039-main-loop-time-pause-and-runtime-state.md`
- `docs/architecture/adr/0042-runtime-service-and-backend-boundary.md`
- `docs/architecture/adr/0078-render-host-affinity-webgpu-initialization-and-device-recovery.md`
- Commit `549d5c25a4091585c8cca3dc51e1f7748fd2cd9d`
