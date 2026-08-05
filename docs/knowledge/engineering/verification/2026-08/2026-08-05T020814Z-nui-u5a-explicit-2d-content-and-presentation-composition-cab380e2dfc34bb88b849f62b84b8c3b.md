---
type: "Verification Evidence"
title: "NUI-U5A explicit 2D content and presentation composition"
description: "Verifies the content-only runtime-2d boundary, explicit presentation-2d composition, stable project capability bits, exact headless and desktop plans, and independently locked consumers."
timestamp: 2026-08-05T02:08:14Z
record_id: "cab380e2dfc34bb88b849f62b84b8c3b"
tags: ["nui-u5a", "product-composition", "presentation-2d", "verification"]
producer_id: "codex-root"
run_id: "019fcacd-bf84-7ee1-85a5-a6b920b331e2"
source_session: "019fcacd-bf84-7ee1-85a5-a6b920b331e2"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "2f0bd69d4eb1c6bfd71992ca8f8feb8974ce5e74"
verified_by: "locked feature trees, independent consumer compile, focused and reference-game nextest, feature checks, strict changed-target Clippy, formatting and diff checks, and independent review"
---

# Verification

NUI-U5A landed at `2f0bd69d4eb1c6bfd71992ca8f8feb8974ce5e74` as the dependency-ordered
2D checkpoint of NUI-U5.

- Root `runtime-2d` now closes over content-only image, material, sprite, and tilemap domains.
  `Presentation2dPlugins` and the `presentation-2d` capability add backend-neutral render,
  image-prepare, and sprite packet owners explicitly; window and WGPU remain separate adapters.
- `nara_image` makes render preparation optional. `ImagePlugin` now owns
  `Assets<ImageAsset>` initialization, so content import/reload does not depend on a presentation
  plugin for basic asset storage.
- Project manifest ingest rejects `presentation-2d` without `runtime-2d` through the typed
  `InvalidCapabilityClosure` error and public diagnostic fields. Existing capability discriminants
  0 through 5 remain unchanged, and `Presentation2d` is appended at 6.
- The reference-game headless dependency and plan contain no render, image-prepare,
  sprite/UI-render, WGPU, window, or tooling owner. Its desktop profile requests content,
  presentation, window, and WGPU explicitly and installs every required group/plugin exactly once.
- A separate locked `presentation-2d` workspace compiles through a renamed root dependency. Its
  feature tree contains sprite presentation and image preparation but excludes UI, WGPU, and
  tooling.
- The active plan records the dependency correction: U5A closes 2D now; U5B follows NUI-U13 so
  runtime UI is not temporarily moved into a renderer-owned execution tree.

# Result

Passed for the NUI-U5A 2D checkpoint. NUI-U5 and ADR 0106 remain partial.

- Root checks passed for `runtime-2d,serde`, `presentation-2d,serde`, `runtime-ui,serde`, and all
  feature-gated examples.
- Root integration verification covered all 29 pre-fixture tests, followed by the new independent
  fixture, capability-closure, runtime-runner tree, and CI policy tests individually. The root
  `presentation-2d,serde` library passed 45 tests.
- `nara_project` passed 33 tests and `nara_image` passed 67 tests.
- The reference game passed 64 headless tests and 103 desktop tests. The final desktop exact-once
  composition test passed again after review simplification.
- The independent fixture passed locked `cargo check` and strict `cargo clippy --no-deps` while
  reusing the repository target for the final run.
- Strict changed-target Clippy passed for `nara_project`, the root library/tests, the changed domain
  libraries, and the reference game with only explicitly recorded pre-existing lint classes
  allowed where required. Selected formatting checks and `git diff --check` passed.
- Independent API/simplicity review found no blocking issue after preserving stable capability
  bits, adding the independent consumer, removing a vacuous App-fingerprint assertion, collapsing
  duplicate conditions, and keeping manifest versus resolved-plan validation responsibilities
  explicit.

# Evidence

- `runtime-2d` locked trees exclude `nara_render`, `nara_sprite_render`,
  `nara_render_wgpu`, `nara_ui_render`, `nara_tooling`, and `nara_image/render-prepare`.
- `presentation-2d` locked trees include `nara_render`, `nara_sprite_render`, and
  `nara_image/render-prepare`, while excluding UI, WGPU, and tooling owners.
- Root and reference lockfiles drop presentation-only packages from their content-only closures.
- CI compiles the coarse `presentation-2d` slice and the independent locked consumer on Linux and
  Windows.
- No `presentation-ui` compatibility placeholder was introduced, and the old implicit
  `runtime-2d` composition has no alias.

# Concurrent Worktree Gates

- The user-owned `crates/nara_ui/src/tests.rs` and `crates/nara_ui_render/src/tests.rs` remained
  unstaged and unchanged by this unit at numstats `140/3` and `61/2`.
- Those drafts still target a UI-local sibling-order API that NUI-U13 must not adopt as structural
  authority. U5A neither deletes nor legitimizes that concurrent work.
- The obsolete `architecture_docs` parser was not run or reintroduced. Documentation authority is
  reconciled through direct ADR/ledger/migration inspection and immutable-memory validation.

# Follow-up

Proceed to NUI-U6 and publish canonical ordered Scene/Prefab placement. Keep NUI-U5 open: after
NUI-U13 installs the renderer-neutral UI runtime adapter, NUI-U5B must split `runtime-ui` from UI
presentation and make Host-owned platform/backend selection semantic before NUI-U14.

# Citations

- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md#nui-u5-split-content-from-presentation-composition`
- `docs/architecture/adr/0106-content-and-presentation-product-composition.md`
- `docs/architecture/adr/0079-root-product-capabilities-and-placeholder-domain-retirement.md`
- `docs/migrations/2026-07-engine-foundation.md#nui-u5a-explicit-2d-content-and-presentation-composition`
- `Cargo.toml`
- `src/product.rs`
- `src/project_host/ingest.rs`
- `crates/nara_image/Cargo.toml`
- `crates/nara_project/src/capability.rs`
- `tests/product_capabilities.rs`
- `tests/fixtures/product-slices/presentation-2d/Cargo.toml`
- `tests/runtime_runner_contract.rs`
- `reference-game/tests/plugin_composition.rs`
