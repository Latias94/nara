---
type: "Verification Evidence"
title: "SRT-U3 bounded startup activation and runtime fault detail"
description: "Verifies exact retained startup source and receipt ownership, ordered unpublished Startup activation, bounded runtime fault metadata, and finite lease retirement."
timestamp: 2026-08-02T19:42:44Z
record_id: "3b4d5b25813f420d8f0e62f478576077"
tags: ["srt-u3", "startup-activation", "runtime-fault", "verification"]
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-08-02-001-refactor-startup-scene-activation-and-atomic-retry-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "05c67b6"
verified_by: "Codex correctness, data-integrity, API, and standards review plus serial Cargo gates"
---

# Verification

SRT-U3 carries the exact retained startup document and its matching successful scene-instance
receipt through one unpublished managed-runtime candidate. A private root resource owns that pair
and its lease; product Startup receives only a read-only advanced system parameter after ordered
promotion and before finalization permanently closes the materialization window.

# Result

Passed at `05c67b6`.

- Bundled startup shares the exact snapshot `Arc` and lease. Editor Play expands its current
  document into a separately charged retained source, including failure and incomplete-retirement
  paths. Direct managed-App use is move-only and requires an explicit logical retained-byte limit.
- Materialization validates and derives the executable component registry from the candidate
  `World`, spawns the selected source, and installs the private input only after success. Product
  Startup observes one matching source/receipt pair; late materialization rejects without mutation.
- The active owner is private and cannot be removed through the public ECS resource API. The
  advanced `StartupSceneActivation` view is read-only and cannot be constructed, cloned, or treated
  as a resource by downstream code.
- Engine-classified fallible execution retains only validated static diagnostic code, safe summary,
  and producer origin. Unknown third-party error and system-context text is discarded and maps to
  the generic fault identity.
- Review findings closed private-owner removal, public error-carrier severity bypass, late
  materialization, foreign-registry substitution, ambiguous retained-byte exposure, and the final
  recoverable branch after scene authority mutation. No P0 or P1 remains. The provisional
  `runtime-2d + serde` feature placement remains explicitly assigned to SRT-U6 rather than becoming
  a compatibility promise in this Trial.

# Evidence

- `cargo nextest run -p nara --locked --features runtime-2d,serde,tooling --lib --test
  project_runtime_boot --test workspace_play_runtime --test project_content_limits --test
  startup_scene_public_surface --test-threads=1`: 76 passed.
- `cargo nextest run -p nara_app --locked --test-threads=1`: 87 passed.
- `cargo nextest run -p nara_scene --locked --test-threads=1`: 93 passed.
- `cargo nextest run -p nara_gameplay -p nara_reflect -p nara_winit --locked --test-threads=1`:
  162 passed.
- `cargo check --workspace --locked --all-features --all-targets -j 1` passed.
- Strict changed-target Clippy passed for `nara_app` and the root all-target feature surface with
  explicit pre-existing allowances. `cargo fmt --all -- --check` and `git diff --check` passed.
- No Cargo commands ran concurrently in this checkout. The documentation-only architecture test was
  intentionally not run, as required by the active plan and user direction.

# Follow-up

SRT-U4 owns the shared hierarchy-aware scene prepare/commit kernel, bounded typed product overlay,
exact additional retirement, and the O(World) hierarchy-validation correction. It must preserve the
startup source/receipt authority added here and may not widen this Trial into a scene manager,
provider registry, or general travel API.

# Citations

- `docs/plans/2026-08-02-001-refactor-startup-scene-activation-and-atomic-retry-plan.md#srt-u3-deliver-bounded-startup-activation-input`
- `src/startup_scene.rs`
- `src/project_content.rs`
- `src/project_host/runtime.rs`
- `crates/nara_app/src/runtime.rs`
- `crates/nara_app/src/runtime/fault_route.rs`
- Git commit `05c67b6`
