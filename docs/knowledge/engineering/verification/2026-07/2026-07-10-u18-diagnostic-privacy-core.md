---
type: "Verification Evidence"
title: "U18 diagnostic privacy core verification"
description: "Commit 6a70847 completed and verified the bounded privacy-safe diagnostics and pressure core."
timestamp: 2026-07-10T13:29:15Z
record_id: "f4b3a25eaa664c4d9d48ca78ae2fcff7"
resource: "nara engine foundation"
tags: ["u18", "diagnostics", "privacy", "pressure", "m1"]
status: "complete"
producer_id: "codex-root"
related_plan: "docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "6a70847"
verified_by: "codex-root"
---

# Verification

U18 replaced raw-string diagnostic construction with a bounded, privacy-classified observation
contract and committed the integrated caller migration as `6a70847`. Verification covered the core
crate, `serde` serialization, migrated project/asset/scene/tooling/facade callers, architecture
evidence, and adversarial review.

# Result

- `Diagnostic`, `DiagnosticReport`, and `RuntimeDiagnostics` enforce entry, byte, field, and text
  budgets with typed outcomes and saturating sticky accounting.
- Engine identities and summaries reject runtime-owned or unsafe text. Public/project-relative,
  sensitive, and secret fields have distinct constructors; sensitive and secret fields retain no
  raw value.
- Runtime dedupe, expiry, eviction, tracing cursors, and tombstone compaction use bounded indexed
  storage. The final byte-pressure regression proves the ADR's `order <= 2 * live` bound.
- `RuntimePressureSnapshots` is a separate headless-safe numeric resource and cannot apply producer
  overload policy.
- Asset, project, scene, tooling, examples, facade exports, and product-plan composition use the
  canonical API. No compatibility shim remains.
- ADRs 0009, 0048, 0049, 0056, and 0068 remain honestly `partial`; U31 still owns stabilized runtime
  producer bridges.

# Evidence

- `cargo nextest run -p nara_diagnostic --test-threads 1`: 50/50 passed.
- `cargo nextest run -p nara_diagnostic --features serde --test-threads 1`: 51/51 passed.
- `cargo clippy -p nara_diagnostic --all-features --all-targets -- -D warnings`: passed.
- `cargo test -p nara_diagnostic --all-features --doc`: two compile-fail doctests passed.
- Focused serial tests passed for `nara_asset` (35), `nara_project` (28), `nara_scene` (36),
  `nara_tooling` (25), and root `nara` (54).
- `cargo check --workspace --all-features --all-targets --locked` passed with
  `CARGO_BUILD_JOBS=1` during integration; the final post-review
  `cargo check --workspace --locked` also passed.
- `cargo fmt --all -- --check` and `git diff --check` passed.
- `cargo nextest run -p nara --test architecture_docs --test-threads 1`: 5/5 passed, run ID
  `48f96492-66d2-450f-b0a6-c0ee6818e845`.
- Two independent final read-only reviews reported no P0/P1/P2 finding after the report-consumption,
  expiry-index, and byte-eviction tombstone fixes.
- A stale-symbol search found no code use of `RuntimeDiagnosticContext`,
  `RuntimeDiagnosticDomain`, `with_dedupe_key`, `.diagnostics()`, or owned report extraction.
- Broad multi-package nextest was deliberately not rerun after the recorded host-memory exhaustion;
  all test commands were single-package or single-target with one test thread.

# Follow-up

Run the M1 decision gate sequentially. U31 later bridges asset, watcher, task, window, render,
project, and editor typed outcomes into this core and publishes source-owned pressure measurements.
Do not mark ADR 0009, 0048, or 0068 implemented before those bridge tests exist.

# Citations

- `docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md`
- `docs/migrations/2026-07-engine-foundation.md`
- `docs/architecture/adr/0009-diagnostics-errors-and-logging.md`
- `docs/architecture/adr/0048-runtime-diagnostics-and-observability-bus.md`
- `docs/architecture/adr/0068-global-resource-budgets-metrics-and-diagnostic-privacy.md`
- `crates/nara_diagnostic/src/contract_tests.rs`
