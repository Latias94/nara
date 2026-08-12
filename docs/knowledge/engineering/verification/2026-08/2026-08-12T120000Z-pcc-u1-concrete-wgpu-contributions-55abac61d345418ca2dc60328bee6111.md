---
type: "Verification Evidence"
title: "PCC-U1 concrete WGPU contribution split"
description: "Verifies that the base WGPU backend is feature-invariant and Sprite/UI submission is installed only as an explicit requested contribution."
timestamp: 2026-08-12T12:00:00Z
record_id: "55abac61d345418ca2dc60328bee6111"
tags: ["product-composition", "wgpu", "presentation-2d", "presentation-ui", "pcc-u1"]
status: "completed"
producer_id: "codex-root"
run_id: "product-composition-contribution-correction"
related_plan: "docs/plans/2026-08-12-002-refactor-product-composition-contribution-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "742c1e8"
verified_by: "cargo-nextest-and-cargo-check"
---

# Verification

PCC-U1 only. This record covers the WGPU side of the Product Composition Contribution Correction;
it does not claim that Winit accessibility (PCC-U2), ordinary ProductRecipe authority (PCC-U3),
the composition baseline (PCC-U4), Runtime UI product validation, or Editor UI are complete.

## Commands

All Cargo commands were run serially with `CARGO_BUILD_JOBS=1` and reused the repository target
directory:

- `cargo check --locked -p nara_render_wgpu`
- `cargo check --locked -p nara_render_wgpu --features sprite-submitter`
- `cargo check --locked -p nara_render_wgpu --features ui-submitter`
- `cargo check --locked -p nara_render_wgpu --features sprite-submitter,ui-submitter`
- `cargo nextest run --locked -p nara_render_wgpu --features sprite-submitter,ui-submitter --test-threads=1 --no-tests fail --no-fail-fast`
- `cargo nextest run --locked --all-features --test plugin_composition --test-threads=1 --no-tests fail --no-fail-fast`
- `cargo check --locked --all-features --examples`
- `cargo fmt --all -- --check`
- scoped clippy for `nara_render_wgpu` with `--no-deps -D warnings` and only the repository's
  pre-existing `derivable_impls` and `too_many_arguments` allowances.

# Result

- `nara_render_wgpu` combined-feature nextest: **70/70 passed** (run
  `2289cc2a-28d7-4658-8ff0-2e69d6ca86db`).
- Requested-subset root canaries: narrow **6/6 passed**; all-features **7/7 passed**.
- All examples compiled under the all-features ceiling.
- Formatting and scoped clippy passed.
- A workspace-wide strict clippy run still reaches unrelated pre-existing lints in dependency
  crates; those files were not changed by PCC-U1 and were not suppressed globally.

# Evidence

- `WgpuRenderPlugin` now has one declaration requiring only `RenderPlugin` and `render-wgpu`.
- `WgpuSpriteSubmitterPlugin` and `WgpuUiSubmitterPlugin` are concrete, capability-gated product
  contributions and are not installed merely because their Cargo features are compiled.
- `frame.rs` owns the private ordered reset/collect/seal/submit/complete transaction.
- The base plugin does not drain Sprite batches or UI backend submissions. Isolation tests prove
  that compiled-but-uninstalled contributions leave their domain queues untouched.
- The product resolver installs Sprite only for `render-wgpu + presentation-2d` and UI only for
  `render-wgpu + presentation-ui`; direct Rust examples explicitly add the corresponding concrete
  contribution.
- UI backend attempts retain exact once-only completion through the private terminal handoff.

# Follow-up

PCC-U2 remains the next active implementation unit for explicit Winit accessibility contribution
activation. RUP-U3 remains paused until PCC-U4 publishes the independent Product Composition
Conformance Baseline. The working tree still contains concurrent user changes outside this record;
they were intentionally preserved and are not part of commit `742c1e8`.

# Citations

- `docs/plans/2026-08-12-002-refactor-product-composition-contribution-correction-plan.md`
- `docs/architecture/adr/0079-root-product-capabilities-and-placeholder-domain-retirement.md`
- `docs/architecture/adr/0106-content-and-presentation-product-composition.md`
- `742c1e8 refactor(render): separate WGPU presentation contributions`
