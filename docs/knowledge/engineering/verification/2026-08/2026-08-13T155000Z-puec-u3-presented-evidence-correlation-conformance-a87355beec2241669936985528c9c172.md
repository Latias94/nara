---
type: "Verification Evidence"
title: "PUEC-U3 Presented UI Evidence Correlation Conformance"
description: "Closes the focused correction with exact backend-to-receipt correlation and bounded post-readiness product measurement."
timestamp: 2026-08-13T15:50:00Z
record_id: "a87355beec2241669936985528c9c172"
tags: ["runtime-ui", "puec-u3", "wgpu", "presented-evidence", "conformance"]
status: "completed"
producer_id: "codex-root"
run_id: "presented-ui-evidence-correlation-correction"
related_plan: "docs/plans/2026-08-13-001-refactor-presented-ui-evidence-correlation-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "eff8620"
verified_by: "cargo-nextest-real-wgpu-clippy-static-review"
---

# Verification

Commit `d028e4c` binds candidate product evidence to exact WGPU submissions and the presentation
receipt that accepted them. Commit `eff8620` closes final review by keeping Retry identification
inside the Reference Game's frozen Facade surface rather than adding a product-specific shared
Kernel accessor.

# Result

- WGPU completion facts retain distinct App-frame and Render-transaction identities from capture
  through device completion.
- The presentation owner publishes one immutable observation only after exact backend acceptance
  commits the matching receipt; a newer candidate cannot replace the observed frame.
- Submitted UI facts carry the exact packet, target, generation, lowering, glyph, texture binding,
  instance and resource-byte facts used by that WGPU transaction.
- The desktop candidate follows `AwaitPresentedReady -> Warmup(4) -> Measure(16) -> Seal`, discards
  pre-readiness completions, accepts only exact same-generation product transactions and bounds the
  final device wait by the remaining candidate deadline.
- Structured schema v2 names App and Render identities separately and rejects type-confused or
  mismatched observations, including Python booleans masquerading as integer identities.
- Retry matching remains product-local through the unique typed `REFERENCE_RETRY_ACTION`; duplicate
  or malformed semantics fail closed. No new shared UI Kernel public surface remains.

# Evidence

All Cargo commands ran serially with `CARGO_BUILD_JOBS=1` and reused the workspace target.

- `cargo nextest run -p nara_render_wgpu --all-features` — **78/78 passed**.
- `cargo nextest run -p nara --all-features --lib` — **77/77 passed**.
- `cargo nextest run -p nara_ui_kernel` after the public-surface correction — **84/84 passed**.
- `cargo nextest run --manifest-path reference-game/Cargo.toml --features desktop --bin desktop` —
  **28/28 passed**.
- Focused artifact package policy nextest — **11/11 passed**.
- Scoped strict `nara_render_wgpu` Clippy passed with only the crate's documented pre-existing
  `too_many_arguments` and `derivable_impls` allowances.
- `cargo fmt --all -- --check`, scoped `git diff --check` and the engineering-memory checks passed.
- A real fallback-WGPU desktop candidate produced one schema-v2 observation whose packet,
  receipt, backend/device generation, App frame, Render transaction, glyph/resource closure and
  completion sample matched. It admitted 4 warmup plus 16 measured product frames with zero lost
  samples.
- Independent correctness, reliability, maintainability and standards reviews found no unresolved
  P0/P1. The reliability deadline issue, Python boolean type confusion and shared Kernel accessor
  were corrected before this record was published.

# Authority

This record is a focused Presented UI Evidence Correlation conformance record. It does not replace
UI Foundation record `a628828a8ff941239d79a0b8b3d60316` or Product Composition record
`56264181873249c18d5477ce2170c8b1`. PUEC is inactive and RUP-U3 resumes against all three records.

# Citations

- `docs/plans/2026-08-13-001-refactor-presented-ui-evidence-correlation-correction-plan.md`
- `crates/nara_render_wgpu/src/backend.rs`
- `crates/nara_render_wgpu/src/telemetry.rs`
- `src/ui_presentation.rs`
- `src/ui_presentation/observation.rs`
- `reference-game/src/bin/desktop_candidate_smoke`
- `reference-game/src/ui_observation.rs`
- `reference-game/tools/smoke_artifact.py`
