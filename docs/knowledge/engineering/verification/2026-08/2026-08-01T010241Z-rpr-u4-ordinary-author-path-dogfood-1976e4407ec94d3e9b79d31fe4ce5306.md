---
type: "Verification Evidence"
title: "RPR-U4 ordinary author-path dogfood"
description: "Records reference-game and renamed-root adoption of the public product recipe plus local no-checkout candidate smoke."
timestamp: 2026-08-01T01:02:41Z
record_id: "1976e4407ec94d3e9b79d31fe4ce5306"
tags: ["rpr-u4", "product-recipe", "reference-game", "verification"]
status: "verified"
producer_id: "codex-root"
related_plan: "docs/plans/2026-08-01-001-refactor-product-readiness-delivery-reset-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "8b4e7f38b034d960291cfb4599ebae0564382d28"
verified_by: "codex-root"
---

# Verification

The verified subject is code commit `8b4e7f38b034d960291cfb4599ebae0564382d28` on
`refactor/engine-foundation-contracts`.

- The reference game constructs one replayable `ProductRecipe` for its persistent gameplay schema
  and reuses it through headless, desktop, and Editor Host paths.
- The independently locked renamed-root fixture adds a runtime-only plugin and a typed configured
  schema contribution through the public recipe, then executes a real file-backed `HeadlessRun`.
- Normal callers no longer assemble plugin definitions, provider lists, candidates, runtime
  publication, or retirement ledgers. Raw definitions remain only in explicit engine-owned probes
  and fault-injection tests.
- The parallel manual raw-App boot implementation and its duplicate command fixture were removed.
  The renamed-root boundary test was reduced from 543 to 182 lines by deleting its `syn`-based
  alias/path/macro policy engine and retaining direct dependency, execution, surface, and guide
  assertions.

# Result

RPR-U4 is complete. The ordinary compiled-Rust recipe is now exercised by the real reference
product and an independently locked renamed-root consumer. Package installation, independently
versioned distribution, file-backed plugin settings, and missing-package recovery remain OQ-045;
this unit does not claim those workflows.

# Evidence

- `cargo fmt --all -- --check` passed.
- The locked reference-game default suite passed 58/58 tests.
- The locked reference-game desktop suite passed 90/90 tests.
- The host-parity profile passed 2/2 tests, preserving the same bounded outcome and fault across
  headless, desktop, and Editor paths.
- `module-consumer` passed 1/1 direct-domain test.
- The final root `runtime_runner_contract` passed 4/4 tests, including the nested independently
  locked renamed-root build and runtime journey.
- The final focused reference-game composition/runtime run passed 8/8 tests after the public-surface
  tightening and test simplification.
- Release builds completed for `headless.exe`, `desktop.exe`, and `desktop_render_probe.exe` with
  `CARGO_BUILD_JOBS=1` and the existing reference-game target directory.
- The retained package tools created and verified a Windows x86_64 no-checkout transport for the
  exact subject revision. The archive SHA-256 is
  `677c7446fcafaae23ce6fe35b41e46423356f6638b7add9fed3e2b8920087356`; it contains 18 files,
  expands to 49,479,719 bytes, produced the stable
  `nara-reference-game.wave-summary-v1` headless result, and completed the desktop candidate smoke.
- No measurement, evidence-ingest, approval, or release script was added.

# Follow-up

Activate RPR-U5 and re-evaluate product readiness from current product-owned observations without
restoring the retired U9 evidence framework. The retained package/smoke scripts remain justified by
the no-checkout artifact boundary; further checks must map directly to an author action or product
decision.

# Citations

- `reference-game/src/lib.rs`
- `reference-game/tests/runtime_drive.rs`
- `tests/fixtures/runtime-runner/renamed-root/`
- `tests/runtime_runner_contract.rs`
- `docs/architecture/open-questions.md#oq-045-plugin-package-contribution-and-official-product-recipe-ergonomics`
- `docs/benchmarks/runtime-ownership-baseline.md`
