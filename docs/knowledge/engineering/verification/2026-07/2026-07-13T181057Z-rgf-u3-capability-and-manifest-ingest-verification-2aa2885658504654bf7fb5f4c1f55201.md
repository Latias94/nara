---
type: "Verification Evidence"
title: "RGF-U3 capability and manifest ingest verification"
description: "RGF-U3 closed its feature surface, manifest authority, CLI privacy, and Server regression evidence on the active refactor branch."
timestamp: 2026-07-13T18:10:57Z
record_id: "2aa2885658504654bf7fb5f4c1f55201"
tags: ["rgf-u3", "verification", "capabilities", "manifest", "server"]
status: "verified"
producer_id: "codex-root"
run_id: "019f5096-ee46-7571-a208-be491cc72786"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "4709689"
verified_by: "cargo-nextest,cargo-check,cargo-fmt"
---

# Verification

RGF-U3 was verified from the dirty pre-commit worktree based on `4709689`. The
verification covered the root feature graph and public preludes, bounded
manifest parsing and authority-error lowering, the reference-game manifest CLI,
Server command timing/order, optional backend examples, and the wider workspace.

# Result

- The exact U3 focused gates passed: 61 domain tests, 16 root capability and
  project-composition tests, four reference-game manifest tests, and all three
  minimal desktop feature examples.
- Root no-default, default, serde-only, and all-feature `--all-targets` checks
  passed. The all-feature root run passed 83 tests.
- The independent reference-game check passed and its full run passed 15 tests.
- `cargo check --workspace --locked` passed. The final workspace nextest run
  passed 623 tests with three existing conditional skips.
- Root, reference-game, and public-prelude fixture formatting checks passed;
  `git diff --check` and the winit/wgpu dependency-boundary searches passed.

# Evidence

- `tests/product_capabilities.rs` owns the locked dependency-tree matrix and the
  positive/negative public-prelude consumer harness.
- `tests/fixtures/public-prelude/` is an independent Rust 1.95 workspace with a
  committed lockfile. Explicit module paths prove negative fixtures fail at the
  gameplay-prelude boundary rather than because a type was not compiled.
- `src/project_host.rs` exposes a manifest-specific authority-error lowering
  boundary. `tests/project_composition.rs` proves static codes and privacy across
  Debug, Display, serde diagnostics, and tracing.
- `reference-game/src/bin/headless.rs` now opens the committed manifest through
  a directory capability. `reference-game/tests/project_manifest_ingest.rs`
  proves random-cwd success and privacy-safe absolute/missing override failures.
- Root library tests retain authoritative Server tick admission, canonical
  ordering, explicit task configuration, and nonblocking worker behavior.

# Follow-up

After the U3 commit is independently reviewed and precisely staged, continue
with RGF-U11. U11 repairs the confirmed surface/provider lifetime violation
before U10 and before any desktop product evidence.

# Citations

- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md`
- `docs/architecture/adr/0079-root-product-capabilities-and-placeholder-domain-retirement.md`
- `docs/architecture/adr/0070-capability-oriented-filesystem-substrate.md`
- `docs/migrations/2026-07-engine-foundation.md`
