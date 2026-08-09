---
type: "Verification Evidence"
title: "UIF-U2C snapshot profile and attempted text-work correction"
description: "Verifies exact builder/compiler profile identity, failed-layout work admission, deterministic rejection reuse, and rejection-preserving cache order."
timestamp: 2026-08-09T03:58:23Z
record_id: "dda1efec999b40e98d6f38fac067503b"
status: "complete"
producer_id: "codex-root"
run_id: "uif-u2c"
source_session: "019fd057-fb48-7493-b35e-bfb034e732c7"
related_plan: "docs/plans/2026-08-08-001-refactor-ui-foundation-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "3dab804"
verified_by: "codex-root"
---

# Verification

- The authoritative V2 command, `cargo nextest run --locked -p nara_ui_kernel --test
  compile_order --test failure_atomic --test validation --test profile --test text_budget
  --test-threads=1`, passed 31 tests with run ID
  `fcd30105-a410-4831-85d9-e3f0b83b0bb0`.
- `cargo nextest run --locked -p nara_ui_kernel --test-threads=1` passed all 78 kernel tests with
  run ID `cbc9b93a-be6e-41ba-95f5-840213fa970f`.
- `cargo clippy --locked --no-deps -p nara_ui_kernel -p nara_ui -p nara_ui_widgets
  -p nara_ui_render --all-features --all-targets -- -D warnings` passed with
  `CARGO_BUILD_JOBS=1`.
- `cargo fmt --all -- --check` and scoped `git diff --check` passed.
- Independent Standards review reported no findings. Independent specification review found that
  V2 omitted the new profile/text targets and that cache-order atomicity lacked a regression; V2 now
  names both targets and `text_budget_rejection_preserves_root_cache_eviction_order` proves the
  bounded eviction consequence. The optional direct shape-call counter was not added because it
  would introduce test instrumentation into `nara_text`; the exact cache-hit branch returns before
  the text engine call, while the public regression proves one charge and one deterministic result.
- Three simplification reviews removed a result-shaped private enum, an unreachable failure
  predicate, a redundant post-layout glyph-limit branch, excess profile field visibility and
  duplicated test construction. A hash index was intentionally not introduced: the exact request
  cache is candidate-local and bounded by existing text/node admission, and no measured hot-path
  failure justifies a second fingerprint/index representation.

# Result

UIF-U2C is complete at commits `e6070bc` and `3dab804`.

- `UiAuthoringSnapshot` retains the exact private authoring profile used for root count,
  source-node count, depth and key admission. `UiCompiler` rejects either profile-mismatch direction
  before revision, last-good cache or LRU mutation; compile-only budgets remain independent.
- Candidate text input work is reserved before every unique shaping attempt. Deterministic layout
  failures are cached within the candidate, so an identical failed request neither reshapes nor
  recharges, while distinct failures eventually hit the candidate-wide work limit.
- Successful shaped-glyph output is counted from actual layout output. Unicode scalar count is not
  used as a glyph proxy, and glyph output remains separate from source-byte work.
- Whole-candidate rejection preserves revision, compatible last-good visuals and bounded root-cache
  eviction order. Existing modal fallback tests continue to prove the documented unsafe-modal
  escalation exception.

# Evidence

- `crates/nara_ui_kernel/src/limits.rs`
- `crates/nara_ui_kernel/src/authoring.rs`
- `crates/nara_ui_kernel/src/compiler.rs`
- `crates/nara_ui_kernel/src/compiler/text.rs`
- `crates/nara_ui_kernel/tests/profile.rs`
- `crates/nara_ui_kernel/tests/text_budget.rs`
- `crates/nara_ui_kernel/tests/failure_atomic.rs`
- `docs/architecture/adr/0109-root-local-ui-compilation-and-presentation-closure.md`
- `docs/migrations/2026-07-engine-foundation.md`

# Follow-up

- UIF-U3C is the sole next active unit. It owns prepared submission, all fallible route/history
  reservation before backend submit, backend-bound one-shot acceptance, infallible authority commit,
  supersession and target/device fault-domain separation.
- UIF-U4C remains inactive and owns Surface Slot suspension, endpoint migration/held-edge balancing,
  target uniqueness and exact generation-bound platform accessibility actions.
- No Reference Game matrix, product hash or new widget/product capability is claimed by UIF-U2C.

# Citations
