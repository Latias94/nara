---
type: "Verification Evidence"
title: "PCC-U4 Product Composition Conformance Baseline"
description: "Publishes the independent product-composition baseline for explicit WGPU, Winit accessibility and ProductRecipe Host authority."
timestamp: 2026-08-13T05:24:31Z
record_id: "180d0dfe0a41417a9633c11cccb91eab"
tags: ["product-composition", "pcc-u4", "baseline", "product-recipe", "wgpu", "winit"]
status: "completed"
producer_id: "codex-root"
run_id: "product-composition-contribution-correction"
related_plan: "docs/plans/2026-08-12-002-refactor-product-composition-contribution-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "a1cac21"
verified_by: "cargo-nextest-cargo-check-clippy-memory-review"
---

# Verification

This record publishes the independent Product Composition Conformance Baseline for PCC-U1 through
PCC-U4. It proves the product-composition contract only: Cargo features define a compiled ceiling,
the normalized request plus `ProductRecipe` define the requested product, and the resolved plugin
plan installs concrete first-party contributions. It does not replace Runtime UI Foundation
baseline `a628828a8ff941239d79a0b8b3d60316` and does not claim Runtime UI product validation or
Editor completion.

The proving implementation lineage is:

- `742c1e8635b5572e550dabd5096377ceab264dd0` — feature-invariant WGPU base plus explicit Sprite/UI
  contributions;
- `18ec87808388f3aee9eec0ae25b91883ba52175c` — explicit Winit accessibility contribution;
- `864c151620aac9524d99a02ab01e301cf1d26165` — `ProductRecipe` as ordinary Host composition
  authority;
- `3ff567596014c4d46f0c301a78cd150c26b3b98a` — migration of the remaining internal Host probes;
- `a1cac21bc5e76c47a4cf87de187b1dde6b547822` — final changed-scope lint correction.

All Cargo commands ran serially with `CARGO_BUILD_JOBS=1`, reused the workspace `target` directory,
and used one nextest test thread.

# Result

- `WgpuRenderPlugin` has one feature-invariant declaration. Compiling Sprite or UI support cannot
  install a contribution, change its dependencies, or drain a domain queue.
- The requested `render-wgpu + presentation-2d`, `render-wgpu + presentation-ui`, and
  `desktop-winit + presentation-ui` intersections install the concrete Sprite, UI, and Winit
  accessibility contributions exactly once. Unrequested intersections install none under both
  narrow and all-features compiled ceilings.
- Native accessibility activation is captured once as disabled, enabled, or partial before window
  startup. Only the installed root contribution publishes exact receipt-stamped semantics.
- Ordinary file-backed Headless, Desktop, and Editor intents accept one `ProductRecipe` plus
  execution policy. Replayable schema owners use `SchemaContribution`; one non-replayable advanced
  probe cannot carry an independent schema provider; direct `App` composition remains separate.
- No submitter trait, contribution registry, RenderGraph, generic Host base intent, compatibility
  wrapper, or new public render phase was introduced.

# Evidence

- PCC-U1 full WGPU gates:
  - `cargo nextest run --locked -p nara_render_wgpu --features sprite-submitter --test-threads=1
    --no-tests fail` — **52/52 passed**;
  - `cargo nextest run --locked -p nara_render_wgpu --features ui-submitter --test-threads=1
    --no-tests fail` — **65/65 passed**;
  - `cargo nextest run --locked -p nara_render_wgpu --features
    sprite-submitter,ui-submitter --test-threads=1 --no-tests fail` — **70/70 passed**.
- PCC-U2 platform gate:
  - `cargo nextest run --locked -p nara_winit --features platform-accessibility --test-threads=1
    --no-tests fail` — **49/49 passed**.
- Requested-subset matrix:
  - the recorded all-features red reproduction became green — **1/1 passed**;
  - the seven named base/Sprite/UI/combined/accessibility closure tests passed under the narrow
    ceiling — **7/7 passed** — and under `--all-features` — **7/7 passed**;
  - the three direct-`App` contribution canaries passed — **3/3 passed**.
- PCC-U3 Host gates:
  - focused root Project Host/runtime composition suites — **61/61 passed**;
  - Reference Game recipe/runtime suites — **29/29 passed**;
  - Headless/Desktop/Editor Host parity — **2/2 passed**;
  - independently locked runtime-runner contract — **5/5 passed**;
  - final internal startup-failure probe migration — **1/1 passed**.
- `cargo check --locked --all-features --lib --tests --examples` passed.
- Strict changed-scope Clippy passed for `nara_render_wgpu`, `nara_winit`, the root production
  library, and the Reference Game. Allowances were limited to the repository's documented existing
  lint baseline. Exploratory root all-test Clippy additionally reached unchanged legacy test lints;
  those files are outside PCC and were not modified.
- `cargo fmt --all -- --check` and scoped `git diff --check` passed.
- Independent PCC-U1/U2 correctness reviews reported no blocking production finding. PCC-U3
  simplification reviews found no justified public abstraction or hot-path expansion; the applied
  changes reduced duplicate lowering and reused existing Host helpers.

# Follow-up

PCC is complete. The Runtime UI Product Slice keeps UI Foundation baseline
`a628828a8ff941239d79a0b8b3d60316`, pins this record separately as
`composition_baseline_record`, and resumes with RUP-U3 as the sole active product unit. RUP-U3 owns
final product evidence, cleanup, Standards/Spec review, and Product-Validated publication; it must
not reopen composition without a new owner-local reproduction.

# Citations

- `docs/plans/2026-08-12-002-refactor-product-composition-contribution-correction-plan.md`
- `docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md`
- `docs/architecture/adr/0040-render-resource-lifetime-and-submitter-ownership.md`
- `docs/architecture/adr/0046-plugin-metadata-and-default-plugin-groups.md`
- `docs/architecture/adr/0079-root-product-capabilities-and-placeholder-domain-retirement.md`
- `docs/architecture/adr/0106-content-and-presentation-product-composition.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-12T120000Z-pcc-u1-concrete-wgpu-contributions-55abac61d345418ca2dc60328bee6111.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-13T020518Z-pcc-u2-explicit-winit-accessibility-contribution-verification-c7031e35724f4c55928c2dd77dc0b63d.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-13T045032Z-pcc-u3-productrecipe-ordinary-host-authority-verification-02de9ad341db4dcbb1519e0f5e8843f1.md`
