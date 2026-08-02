---
type: "Verification Evidence"
title: "SRT-U2 exact image revision and invalidation correction"
description: "Verifies exact image slot identity, bounded immutable RGBA storage, transient prepare-to-submit races, and removal of duplicate invalidation authority."
timestamp: 2026-08-02T14:39:54Z
record_id: "a45da756131c43c7a3632928e8cd4d4a"
tags: ["srt-u2", "image", "render", "verification"]
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-08-02-001-refactor-startup-scene-activation-and-atomic-retry-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "0b5beba"
verified_by: "Codex correctness, adversarial, API, testing, and performance review plus serial Cargo gates"
---

# Verification

SRT-U2 binds backend-neutral image preparation to the exact value in the typed asset store, removes
the unconsumed invalidation event log, and validates immutable RGBA storage at every construction
boundary. A resource replacement or removal between Prepare and WGPU submission rejects only the
current frame and leaves the backend ready for the next snapshot.

# Result

Passed at `0b5beba`.

- `RenderResourceSnapshot` captures the existing opaque `AssetSlotRevision`; unchanged snapshots
  reuse one prepared/GPU record, while same-version same-descriptor pixel replacement prepares and
  uploads once.
- `ImageAsset::new` is fallible and normalizes admitted pixels to fixed-length shared storage.
  Direct construction, PNG finalization, and serde reject zero extents, unrepresentable sizes, and
  both short and long RGBA buffers.
- Prepare-to-submit replacement and deletion races become `ResourceChanged` frame skips. They do
  not clear WGPU resources, mark the backend unavailable, or report a required-service fault.
- `RenderPrepareInvalidations` and the unused version-only asynchronous result arbitration are
  deleted. The prepared snapshot map and WGPU frame-age cache are the only invalidation owners.
- Independent correctness, adversarial, API, testing, and performance re-reviews found no remaining
  P0-P2 after the replacement/deletion race corrections and migration update.

# Evidence

- `cargo nextest run --locked -p nara_render -p nara_image -p nara_render_wgpu --all-features -j 1
  --no-fail-fast`: 145 passed, 1 skipped before the final deletion-race case.
- `cargo nextest run --locked -p nara_render_wgpu --all-features -j 1 --no-fail-fast`: 50 passed,
  1 skipped after the final deletion-race correction.
- `cargo nextest run --locked -p nara_sprite_render -p nara_ui_render --all-features -j 1
  --no-fail-fast`: 25 passed.
- `cargo nextest run --locked -p nara --lib --features "runtime-2d,serde" -j 1 --no-fail-fast`: 35
  passed.
- `cargo check --workspace --locked -j 1` passed.
- Strict changed-target Clippy passed for render/image/WGPU/sprite/UI and the root library with only
  explicit pre-existing allowances. `cargo fmt --all -- --check` and `git diff --check` passed.
- No Cargo commands ran concurrently in this checkout. The documentation-only architecture test was
  intentionally not run, as required by the active plan and user direction.

# Follow-up

SRT-U3 owns the retained startup source plus successful spawn receipt, ordered Startup consumption,
and bounded static runtime fault detail. SRT-U4 owns the minimal scene replacement extension and the
hierarchy O(World) correction. A durable artifact store and second concrete asset domain remain a
later product-triggered slice; SRT-U2 does not introduce an Import Host.

# Citations

- `docs/plans/2026-08-02-001-refactor-startup-scene-activation-and-atomic-retry-plan.md#srt-u2-correct-image-revision-and-invalidation`
- `crates/nara_asset/src/storage.rs`
- `crates/nara_image/src/asset.rs`
- `crates/nara_image/src/prepare.rs`
- `crates/nara_render/src/prepare.rs`
- `crates/nara_render_wgpu/src/texture.rs`
- Git commit `0b5beba`
