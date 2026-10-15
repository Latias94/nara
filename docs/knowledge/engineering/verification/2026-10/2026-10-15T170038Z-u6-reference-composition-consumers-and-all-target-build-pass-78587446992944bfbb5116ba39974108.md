---
type: "Verification Evidence"
title: "U6 Reference composition consumers and all-target build pass"
description: "Verification Evidence for U6 Reference composition consumers and all-target build pass."
timestamp: 2026-10-15T17:00:38Z
record_id: "78587446992944bfbb5116ba39974108"
status: "verified"
producer_id: "codex-root"
run_id: "nara-foundation-20261015"
related_plan: "docs/plans/2026-10-15-1636-refactor-2d-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "25c29de"
verified_by: "codex-root"
---

# Verification

Reference Game, `CARGO_BUILD_JOBS=1`, shared `CARGO_TARGET_DIR=F:/SourceCodes/Rust/nara/target`,
locked/offline commands. `cargo fmt --all --manifest-path reference-game/Cargo.toml` passed.

# Result

- `cargo check --manifest-path reference-game/Cargo.toml --all-features --all-targets
  --locked --offline`: passed, 2m21s.
- Nextest `41c18bb6-2a19-4b51-82c0-09ddda7c5c35`, session `72749`, selected `authoring`,
  `headless_tracer`, `runtime_core`, `authoring_bundle`, and `desktop`: 41 passed, one old
  authoring fixture assertion failed, 0 skipped, 4.293 seconds execution.
- Nextest `a0ab52be-1438-40b0-b14f-83aee6aec030`, session `7374`, rechecked all seven authoring
  tests after correcting that fixture: seven passed, 0 skipped, 0.236 seconds.

# Evidence

The fixture assumed source replacement incremented a global numeric revision. Existing
`AuthoringHistory::replace_source` instead creates a fresh source identity at generation zero.
The corrected assertion checks that new identity and verifies a subsequent field patch keeps
the source identity and advances one generation. No production authoring semantics changed.

Real consumers now contribute systems and schema through the same typed contribution; the
Reference plugin no longer repeats provider registration inside its own build/preflight.
The preceding 198-test checkpoint includes actual compiled Reference Check, custom native type
rename/migration, strict Repair/Export, and both product observations.

# Follow-up

Native supervised-Play canary session `93873` is running. U12 final native evidence and focused
cluster static checks remain pending. U6 owned-path commit must preserve overlapping pre-existing
index/worktree changes; this evidence does not assert a standalone committed HEAD or U7-U13 done.

# Citations

- reference-game/src/lib.rs
- reference-game/tests/authoring.rs
- reference-game/tests/headless_tracer.rs
- reference-game/tests/runtime_core.rs
- docs/knowledge/engineering/2026-10/2026-10-15T163819Z-compiled-project-tooling-and-host-integration-pass-all-198-focused-tests-6a8e83bd2e48414b9573ab094aa2240e.md
