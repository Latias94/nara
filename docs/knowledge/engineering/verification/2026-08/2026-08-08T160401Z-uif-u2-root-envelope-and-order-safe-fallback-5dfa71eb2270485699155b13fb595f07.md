---
type: "Verification Evidence"
title: "UIF-U2 root envelopes and order-safe fallback"
description: "Closes root-local failure isolation, relative visual fallback, modal barrier safety, and rejected-candidate retirement."
timestamp: 2026-08-08T16:04:01Z
record_id: "5dfa71eb2270485699155b13fb595f07"
status: "complete"
producer_id: "codex-root"
run_id: "uif-u2"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "2fde103"
verified_by: "codex-root"
---

# Verification

- `cargo nextest run --locked -p nara_ui_kernel -p nara_ui --test-threads=1`: 96 passed,
  0 failed on committed code; run ID `1cda1cae-3ab7-40b9-a1a4-f4fa5c1c97da`.
- `cargo nextest run --locked -p nara_ui_kernel --test compile_order --test failure_atomic
  --test validation --test-threads=1`: 23 passed, 0 failed.
- `cargo run --locked -p nara_ui_kernel --example layout_gallery -j 1`: passed with three mixed
  roots and scales `1.0`/`1.25`; assertions cover retained-modal inertness and current barrier.
- `cargo clippy --locked -p nara_ui_kernel -p nara_ui --all-features --all-targets --no-deps
  -- -D warnings`: passed.
- `cargo fmt --all -- --check`, scoped `git diff --check`, staged-file inspection and precise
  commit review passed.
- Independent adversarial review found and then rechecked the stale unpresented-candidate P1. The
  final review found no remaining UIF-U2 P0/P1 or testing gap.

# Result

UIF-U2 is complete at `2fde103`.

- Snapshot-global root identity, order and source-node capacity are preflighted before root bodies.
  Known roots then compile independently into ordered Published, RetainedVisual or Absent outcomes.
- `UiCompiler` remains the small public Interface while private `cache`, `order` and `root` modules
  own relative visual facts, frame binding and concrete layout compilation.
- Last-good cache entries contain layout/paint facts only. Every fallback rebinds the current layer
  and ordinal and cannot contribute hit, focus, semantics or behavior.
- Current modal membership comes from the root envelope. Retained modal visuals remain inert while
  blocking underlay UI/gameplay; a modal without fallback rejects the complete candidate.
- Compiler rejection does not advance frame revision, cache clock or LRU state. Runtime rejection
  retires every older unpresented candidate while preserving already committed presentation,
  interaction and action authority.
- Node, final shaped-glyph and logical-paint facts share candidate-wide counters. Repeated shaping
  work during text measurement remains explicitly owned by UIF-U3 rather than claimed complete.

# Evidence

- `crates/nara_ui_kernel/src/authoring.rs`
- `crates/nara_ui_kernel/src/compiler.rs`
- `crates/nara_ui_kernel/src/compiler/{cache,order,root}.rs`
- `crates/nara_ui_kernel/src/frame.rs`
- `crates/nara_ui/src/runtime/{projection,state,presentation}.rs`
- `crates/nara_ui_kernel/tests/{validation,compile_order,failure_atomic}.rs`
- `crates/nara_ui/src/adapter_tests.rs`
- `crates/nara_ui_kernel/examples/layout_gallery.rs`
- `docs/migrations/2026-07-engine-foundation.md#uif-u2-root-envelopes-relative-fallback-and-candidate-retirement`

# Deferred Owners

- UIF-U3 owns early candidate text-work admission, atomic target-plan/atlas state, complete packet
  resource budgets, uniform radii, backend retry generations and one-shot presentation commit.
- UIF-U4 owns Host Surface Slot lifecycle, coordinate transforms and receipt-stamped accessibility
  ingress.

# Citations
