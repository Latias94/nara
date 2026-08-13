---
type: "Verification Evidence"
title: "PUEC-U2 receipt-bound presented observation"
description: "Proves that the presentation owner publishes one immutable observation from the exact accepted WGPU packet only after committing its matching receipt."
timestamp: 2026-08-13T11:05:13Z
record_id: "97aa78c6bbf74fada0b7d99219e271a6"
tags: ["runtime-ui", "puec-u2", "wgpu", "presented-observation", "receipt"]
status: "completed"
producer_id: "codex-root"
run_id: "presented-ui-evidence-correlation-correction"
related_plan: "docs/plans/2026-08-13-001-refactor-presented-ui-evidence-correlation-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "89aadf2"
verified_by: "cargo-nextest-cargo-check-clippy-static-review"
---

# Verification

Commit `89aadf2` makes the root presentation bridge the sole publisher of one latest-only,
read-only observation for the exact packet whose backend acceptance advanced a presentation
receipt. The WGPU backend contributes packet-local facts from the first lowering pass and records
them only after a native surface submission succeeds.

# Result

- `PresentedUiObservation` binds the exact receipt, backend submission identity, backend/device
  generation, immutable packet and its `Arc<UiFrame>`.
- WGPU observations additionally bind distinct App-frame and Render-transaction identities,
  packet payload counts, submitted quad/glyph counts and resident material bindings.
- The WGPU facts reuse the first packet lowering pass; no second packet lowering, texture scan or
  mutable telemetry join is needed after submission.
- A newer unpresented candidate, rejected successor or stale backend acceptance cannot replace the
  previous observation.
- Narrow builds without `render-wgpu` retain the same presentation contract and publish an
  observation without concrete WGPU facts.

# Evidence

All Cargo commands ran serially with `CARGO_BUILD_JOBS=1` and reused the workspace target.

- `cargo nextest run -p nara_render_wgpu --features ui-submitter` — **68/68 passed**.
- Focused root presentation nextest for exact acceptance, stale acceptance, unpresented successor,
  rejected successor and unconsumed backend frame — **5/5 passed**.
- Focused post-simplification WGPU nextest for exact packet facts, target selection and distinct
  App/Render identities — **3/3 passed**.
- `cargo check -p nara_render_wgpu --no-default-features` — **passed**.
- `cargo check --features presentation-ui` — **passed**.
- `cargo check --features 'presentation-ui,render-wgpu'` — **passed**.
- Scoped strict Clippy for `nara_render_wgpu` passed after allowing the crate's four pre-existing
  `too_many_arguments`/`derivable_impls` findings. Unscoped root `-D warnings` remains blocked by
  pre-existing `nara_app` large-error and collapsible-if findings outside this unit.
- `rustfmt` on the owned Rust files and `git diff --check` — **passed**.
- Independent final quality review found no P0/P1 issue in the PUEC-U2 scope.

# Follow-up

PUEC-U3 is active. The Reference Game candidate must consume this owner-produced observation,
discard every pre-readiness completion sample, warm up only on exact admitted product frames and
seal only when the matching Render transaction completes. RUP-U3 remains paused; its UI Foundation
and Product Composition baselines remain unchanged.

# Citations

- `docs/plans/2026-08-13-001-refactor-presented-ui-evidence-correlation-correction-plan.md`
- `src/ui_presentation.rs`
- `src/ui_presentation/observation.rs`
- `crates/nara_render_wgpu/src/backend.rs`
- `crates/nara_render_wgpu/src/ui.rs`
