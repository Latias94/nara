---
type: "Verification Evidence"
title: "NUI-U4 closed first-party render and UI internals"
description: "Verifies deletion of arbitrary render topology, closure of technical UI stages and mutable batch resources, deterministic first-party ordering, and facade/dependency cleanup."
timestamp: 2026-08-04T18:25:40Z
record_id: "af4e0733217a483cb73239914fd21ef1"
tags: ["nui-u4", "render", "ui-render", "api", "verification"]
producer_id: "codex-root"
run_id: "019fcacd-bf84-7ee1-85a5-a6b920b331e2"
source_session: "019fcacd-bf84-7ee1-85a5-a6b920b331e2"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "e1a45953c012eeb339b0b064ac83c3f44caa1eeb"
verified_by: "four-way WGPU feature checks, focused render nextest, structured public API compile fixtures, reference-game desktop proofs, strict changed-target Clippy, exact formatting/diff checks, manifest audit, and independent correctness/API/architecture review"
---

# Verification

NUI-U4 landed at `e1a45953c012eeb339b0b064ac83c3f44caa1eeb`.

- The unused constructible render phase, pass-plan, and dependency model is deleted rather than
  hidden or replaced by another graph vocabulary.
- `nara_render` owns one closed first-party assembly rule; sprite-owned explicit ranks establish
  `Tilemap -> Sprite`, and the renderer appends the read-only UI snapshot after content.
- UI extraction, queueing, comparison, batching, material-key construction, statistics, and
  mutable ECS batch resources are private. WGPU can only capture an unconstructible,
  non-`Resource` `UiBatchSnapshot` through the temporary hidden reader.
- Root and crate preludes expose product concepts only. Structured rustc diagnostics prove every
  removed symbol independently rather than matching source text echoed by the compiler.
- Redundant direct `bevy_ecs` dependencies are removed from all four affected render crates.
  The `SystemParam` derive resolves through a crate-local alias to the existing `nara_ecs`
  boundary, so no duplicate dependency returned.
- No `architecture_docs` parser or prose-validation replacement was introduced.

# Result

Passed for the complete NUI-U4 contract.

- WGPU compiled with no submitter, sprite-only, UI-only, and both submitters enabled.
- `nara_render`, `nara_sprite_render`, and all-feature `nara_render_wgpu` passed 94 tests; the
  subprocess-only child test remained intentionally skipped, while the parent-owned GPU scissor
  pixel proof passed.
- The independently locked public consumer passed its positive product fixture and every precise
  negative topology, obsolete-frame-signature, technical UI, prelude, and hidden-resource fixture.
- The reference-game desktop render suite passed 6 tests; both candidate-smoke unit tests passed.
- Strict library Clippy passed for the four changed render crates and the root facade with only the
  explicitly enumerated pre-existing workspace lint classes allowed.
- Final diff checks, stale-symbol searches, manifest review, and three independent review lenses
  found no unresolved P0, P1, or P2 issue after the structured-diagnostic and read-only snapshot
  corrections.

# Concurrent Worktree Gates

- The user-owned `crates/nara_ui/src/tests.rs` and `crates/nara_ui_render/src/tests.rs` remained
  unstaged and unchanged by this unit, at numstats `140/3` and `61/2` respectively.
- The complete `nara_ui_render` test target currently cannot compile because the concurrent test
  draft calls a nonexistent `UiNode::with_sibling_order`. Implementing that API would violate the
  accepted structural-order contract in KTD3, so U4 verified the library through all-feature
  checks, Clippy, WGPU integration tests, and independent compile consumers instead.
- Workspace rustfmt check reports formatting differences only in the concurrent
  `crates/nara_ui/src/tests.rs`; all U4 Rust files were formatted explicitly.
- Reference-game feature builds emit pre-existing root dead-code warnings; all selected behavior
  tests pass.

# Follow-up

NUI-U5 is the sole next unit. Split 2D/UI content features from presentation composition, keep WGPU
as a separate backend capability, and prove the same reference content recipe on headless and
desktop Hosts.

# Citations

- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md#nui-u4-close-render-and-ui-internals`
- `docs/architecture/adr/0105-closed-first-party-render-plan.md`
- `docs/migrations/2026-07-engine-foundation.md#nui-u4-closed-first-party-render-and-ui-internals`
- `crates/nara_render/src/lib.rs`
- `crates/nara_sprite_render/src/types.rs`
- `crates/nara_ui_render/src/types.rs`
- `crates/nara_render_wgpu/src/lib.rs`
- `tests/product_capabilities.rs`
