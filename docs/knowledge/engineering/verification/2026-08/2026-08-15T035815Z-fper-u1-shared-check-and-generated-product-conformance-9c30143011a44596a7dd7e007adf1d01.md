---
type: "Verification Evidence"
title: "FPER-U1 shared check and generated product conformance"
description: "Verifies the shared project check facts, presentation-free headless product, generated desktop product adapter, and camera-content/presentation scheduling split."
timestamp: 2026-08-15T03:58:15Z
record_id: "9c30143011a44596a7dd7e007adf1d01"
resource: "nara-first-party-project-workflow"
tags: ["project-workflow", "fresh-project", "fper-u1", "check", "headless", "desktop", "render-boundary"]
status: "completed"
producer_id: "codex-root"
run_id: "session-019fd057-fb48-7493-b35e-bfb034e732c7"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-14-002-feat-fresh-project-source-carrying-export-and-repair-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "3beb153"
verified_by: "focused-nextest-root-check-generated-project-headless-run-static-review"
---

# Verification

FPER-U1 is implemented at commits `f114d38`, `26e6fc6`, `8655a5d`, `bc393d7`,
`9455d77` and `3beb153`. The project-content check now returns one
`StartupContentClosureReceipt`, and the ordinary headless and desktop run adapters retain the
same manifest, schema and startup-content facts while keeping profile-bound settings lineages
separate.

The render boundary was corrected before adding the generated camera. `RenderComponentsPlugin`
owns `Camera2d` schema registration and runtime-2d content; `RenderPlugin` owns presentation
scheduling and remains unavailable to the presentation-free runtime-2d product. The generated
starter therefore has an authored camera without introducing a headless WGPU dependency.

# Result

- The generated `desktop-2d/v1` project is an independent Cargo workspace with library, headless
  and desktop targets. Its headless binary ran successfully and reported one fixed tick.
- The focused composition gate proves that runtime-2d selects render component content and schema
  without selecting render presentation scheduling.
- Exact first-present acceptance, timeout and unarmed behavior remain owned by the desktop
  verification module; no test-private presenter was added to the generated product.
- A real generated desktop first-build attempt was bounded at 180 seconds and did not complete in
  this workspace. It is not counted as a passing runtime observation; the product trajectory gate
  remains part of the later linked FPER evidence rather than being replaced with a fabricated pass.

# Evidence

- `CARGO_BUILD_JOBS=1 cargo check --locked --no-default-features --features runtime-2d,serde`
  - passed; no WGPU, Winit or presentation scheduler was selected.
- `CARGO_BUILD_JOBS=1 cargo check --locked --no-default-features --features runtime-2d,serde,presentation-2d,desktop-winit,render-wgpu`
  - passed with the explicit presentation composition.
- `CARGO_BUILD_JOBS=1 cargo nextest run --locked --no-default-features --features project-workflow --test project_workflow_cli -E 'test(generated_project_is_an_independent_cargo_product)' --test-threads=1 --no-fail-fast`
  - passed; the generated workspace completed offline all-target/all-feature Cargo checking.
- `CARGO_BUILD_JOBS=1 cargo nextest run --locked --no-default-features --features runtime-2d,serde --test plugin_composition -E 'test(runtime_2d_selects_camera_content_without_presentation_scheduling)' --test-threads=1 --no-fail-fast`
  - passed.
- Generated `target/fper-u1-real-20260815/fper-u1-run/target/debug/headless.exe`
  - ran from the repository workspace and printed `starter_headless: ok fixed_ticks=1`.
- `git diff --check`
  - passed for the implementation scope before the two focused commits.

# Follow-up

Activate FPER-U2 for the private Cargo JSON Adapter and source-carrying startup-closure export.
Keep the generated desktop live trajectory as a required linked-product observation for the later
FPER-U3/V5 evidence; do not add a platform-state matrix or another smoke implementation.

# Citations

- `src/project_workflow/check.rs`
- `src/project_content/closure.rs`
- `src/project_host/runtime/action.rs`
- `src/project_host/runtime/desktop_verification.rs`
- `crates/nara_render/src/lib.rs`
- `src/product.rs`
- `starters/desktop-2d/v1/`
- `tests/project_workflow_cli.rs`
- `tests/plugin_composition.rs`
- Commits `9455d77` and `3beb153`
