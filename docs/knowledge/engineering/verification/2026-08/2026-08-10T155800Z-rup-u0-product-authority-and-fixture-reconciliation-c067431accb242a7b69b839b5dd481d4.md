---
type: "Verification Evidence"
title: "RUP-U0 product authority and fixture reconciliation"
description: "Closes the focused Reference Game Runtime UI component fixture without Cartesian smoke, opaque hashes, or false desktop/backend claims."
timestamp: 2026-08-10T15:58:00Z
record_id: "c067431accb242a7b69b839b5dd481d4"
tags: ["ui", "runtime-ui", "reference-game", "product-fixture", "rup-u0"]
status: "complete"
producer_id: "codex-root"
run_id: "runtime-ui-product-slice"
related_plan: "docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "80a7270"
verified_by: "codex-root"
---

# Verification

- `CARGO_BUILD_JOBS=1 cargo nextest run --manifest-path reference-game/Cargo.toml --locked
  --test ui_product --test-threads=1 --no-tests fail --no-fail-fast` passed all eight tests with
  run ID `24f2711e-b1fc-45a5-b1fd-7f1dadbae3f4`; zero tests were skipped.
- `CARGO_BUILD_JOBS=1 cargo clippy --manifest-path reference-game/Cargo.toml --locked --test
  ui_product --no-deps -j 1 -- -D warnings` passed. The root `nara` dependency emitted only its
  pre-existing dead-code compiler warnings outside this unit.
- Direct `rustfmt --edition 2024 --check` passed for the five owned Rust files. Repository-wide
  write-mode formatting was deliberately not used because unrelated concurrent files are dirty.
- `git diff --cached --check` passed before commit, and commit `80a7270` contains only the eight
  RUP-U0 files named by the unit.

# Result

RUP-U0 is complete at commit `80a7270`. The focused fixture now distinguishes one successful
ordered trace from independent defeated and rejected cases, uses structured layout, semantic,
focus, hit and packet assertions, and exercises exactly one fractional-input case rather than a
state-by-target Cartesian product. No opaque snapshot hash is used as product evidence.

The Reference Game adapter now publishes the terminal root as modal only while terminal content is
visible. This product-owned lifecycle correction prevents a hidden terminal root from retaining a
modal barrier during the running HUD state without changing the shared UIF-U5 contracts.

The component presenter is explicitly test-only evidence. It plans the exact latest frame, reserves
presentation authority, binds a backend outcome, retains the full backend acceptance identity, uses
the managed runtime's real `RuntimeGeneration`, and commits a matching receipt. It does not claim
WGPU submission, native-window presentation, or platform accessibility ingress.

# Review

- Simplification and standards/specification passes removed a zero-test desktop gate, opaque
  presenter naming, synthetic state sorting, imprecise accessibility action selection, redundant
  packet clones and an unused text-layout key.
- The final independent correctness review found one P1: the component presenter used a fixed
  runtime generation. The fixture was corrected to bind and assert the real managed generation and
  complete acceptance identity; the follow-up review reported no findings, residual risks, or test
  gaps.
- A standards finding that treated rejection-log additions as a suffix was rejected after checking
  the owning implementation: `UiRouteRejectionLog` is newest-first, so the fixture correctly
  inspects the new prefix.

# Boundaries

This record is component/product-fixture evidence only. It does not close R4-R8 product validation,
desktop/WGPU presentation, AccessKit/Winit ingress, source-data roundtrip, headless delivery or
packaged launch. Those obligations remain in RUP-U1 and RUP-U2.

# Follow-up

RUP-U1 must drive one managed runtime from authored HUD through terminal state, exact Retry
activation and successor presentation. Pointer, keyboard, gamepad and platform accessibility must
produce exactly one typed command against a matching receipt. The component presenter may remain
supplementary but cannot substitute for the named first-party backend and platform paths.

# Citations

- `docs/plans/2026-08-08-002-feat-runtime-ui-product-slice-plan.md`
- `docs/architecture/reference-game-ui-product-slice.md`
- `docs/knowledge/engineering/verification/2026-08/2026-08-10T145838Z-uif-u5-foundation-conformance-baseline-70b35505e8da4cb388c36d3ede602259.md`
