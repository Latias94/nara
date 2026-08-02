---
type: "Verification Evidence"
title: "RGS-U3 completed 2D global transform projection"
description: "Verifies bounded global 2D propagation, completion freshness, parented render extraction, and parented Transform content admission."
timestamp: 2026-08-02T08:45:59Z
record_id: "1fa10428b5e240acbade0440cc25ae75"
tags: ["rgs-u3", "spatial-authority", "transform", "render", "verification"]
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-08-01-002-refactor-reference-game-2d-spatial-authority-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "fb4fdc7"
verified_by: "codex correctness/spec/standards review plus serial Cargo gates"
---

# Verification

RGS-U3 replaces the temporary parented-Transform rejection with one completed runtime 2D
projection. The transform Module publishes opaque immutable globals only after the current
hierarchy generation validates, and camera, sprite, and tilemap extraction publish only from that
completed projection. Startup, fixed-step, PostUpdate, and paused/late Extract paths use the same
freshness contract.

# Result

Passed at `fb4fdc7`.

- `GlobalTransform2d` is an immutable derived component with owner-only construction and read-only
  matrix/translation access.
- Dirty completion builds a Transform-only adjacency from participating entities' `Parent` facts,
  traverses each participant and Transform edge once, and commits the complete candidate before
  publishing its generation token. A hierarchy with 4,096 non-Transform children proves those
  edges are absent from transform propagation work.
- Unchanged completion points perform one allocation-free change-tick scan over Transform
  participants and perform no graph rebuild, parent/child traversal, or derived write.
- Camera, sprite, and tilemap extraction require explicit local and completed global transforms.
  Sprite and tilemap consume the full affine; the current camera contract accepts global
  translation only and rejects unsupported linear state.
- Scene, Prefab, Project Content, Direct App, Project Host, paused Editor Play, and the reference
  game now admit parented `Transform2d`. Inherited `Visibility` remains fail-closed.
- The final correctness/spec/standards re-review found no P0 or P1 after the sparse-hierarchy
  complexity correction.

# Evidence

- `cargo nextest run --locked -p nara_transform -p nara_render -p nara_sprite_render -j 1
  --no-fail-fast`: 62 passed.
- `cargo nextest run --locked -p nara --features "tooling,runtime-2d,serde" --test
  project_content_boundary --test workspace_play_runtime --test image_import_limits -j 1
  --no-fail-fast`: 34 passed.
- `cargo nextest run --locked --features desktop -j 1 --no-fail-fast` in `reference-game/`: 90
  passed.
- `cargo nextest run --release --locked -p nara_sprite_render
  parented_sprite_uses_the_completed_global_affine -j 1`: 1 passed.
- `cargo check --workspace --locked -j 1`, reference-game `cargo check --locked --all-targets
  --features desktop -j 1`, and the `runtime_ui_panel` example check passed.
- Strict Clippy passed for all affected engine packages, root all-targets, and reference-game
  all-targets. The commands explicitly allowed only pre-existing `result_large_err`,
  `collapsible_if`, `derivable_impls`, `double_must_use`, `too_many_arguments`, `drop_non_drop`,
  `needless_return`, and dependency-local `dead_code`; the RGS-U3 query-complexity warning was fixed
  rather than allowed.
- Root `cargo fmt --all -- --check`, targeted reference-game Rustfmt checks, staged and unstaged
  `git diff --check`, and the final staged-scope review passed.

# Follow-up

RGS-U4 owns the reference game's one-spatial-authority refactor, authored child product proof, and
topology-preserving Retry through the existing Scene replacement path. The current aggregate-to-
Transform projection is intentionally transitional. Inherited visibility, persistent sibling
order, runtime reparent, 3D, and physics remain outside RGS-U3.

# Citations

- `docs/plans/2026-08-01-002-refactor-reference-game-2d-spatial-authority-plan.md#rgs-u3-close-2d-global-transform-and-render-consumption`
- `docs/architecture/adr/0100-runtime-structural-hierarchy-and-completed-2d-transform-projection.md`
- `crates/nara_transform/src/propagation.rs`
- `crates/nara_render/src/lib.rs`
- `crates/nara_sprite_render/src/extract.rs`
- `tests/project_content_boundary.rs`
- `tests/workspace_play_runtime.rs`
- Git commit `fb4fdc7`
