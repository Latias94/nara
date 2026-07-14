---
type: "Verification Evidence"
title: "RGF-U10 bounded PNG ingest verification"
description: "Verified bounded PNG read, decode, publication, last-good reload, review disposition, and focused post-review gates."
timestamp: 2026-07-14T17:48:58Z
record_id: "cd08a4898f76409392fbaf093174ea5f"
tags: ["rgf-u10", "nara-image", "security", "verification"]
producer_id: "codex-root"
run_id: "20260714T170623-u10"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
verified_by: "cargo-nextest,cargo-check,ce-code-review"
---

# Verification

RGF-U10 replaces ambient image reads and generic decode/expansion with one audited static,
non-interlaced PNG path. File requests reserve their encoded ceiling before dispatch, read through
an opened `FileCapability`, atomically upgrade to the versioned logical peak before decoder
construction, and retain the publication charge until commit or candidate drop. Publication binds
the stable asset mapping, expected asset version, state revision, slot revision, and store identity.

# Result

The implementation and documentation are ready for an isolated U10 commit. Initial rejection
publishes no image, failed reload retains the last-good value and version, stale or cross-runtime
candidates cannot publish, and all tested terminal paths release their reservations exactly once.

A full `ce-code-review` pass selected correctness, testing, maintainability, project standards,
agent-native, learnings, security, performance, API-contract, reliability, and adversarial lenses.
Independent validation retained two findings: the peak test lacked an independent v1 golden oracle,
and migration guidance omitted the removal of deep-clone/equality traits from reservation-bearing
image values and candidates. Both were fixed. Correctness and security returned no findings; all
other proposed defects were rejected as pre-existing, unreachable, intentionally deferred, or
contrary to the explicit U10 rejection contract.

# Evidence

- `cargo fmt --all -- --check` and `git diff --check` pass.
- The four directly affected crates pass 154 tests; the `nara_image` serde matrix passes 55 tests.
- The root `runtime-2d` image contract passes 2 tests; the independent reference-game image safety
  target passes 1 test; the full independent reference game passes 16 tests.
- `cargo nextest run --workspace --locked --test-threads=1` passes 685 tests with three documented
  platform skips.
- `cargo check --workspace --locked`, the root no-default/default/coarse-feature matrix,
  all-features/all-targets, `asset_import_texture`, the three named desktop examples, and the
  independent reference-game checks pass.
- After review fixes, `cargo nextest run --locked -p nara_image --features serde --test-threads=1`
  passes 55 tests, including the independent numeric PNG memory-plan v1 oracle.
- Workspace Clippy remains blocked only by nine pre-existing `nara_asset` lints outside U10; Clippy
  is not an RGF-U10 gate.

# Follow-up

Keep `png` pinned to 0.18.1 until its allocation behavior and the memory-plan version are re-audited.
Treat cancellation during one bounded regular-file read, synchronous capability open during
`TaskUpdate`, and automatic retry under transient aggregate contention as measured Host/task-policy
questions, not silent U10 guarantees. The startup-content Host must provide bounded concurrency and
truthful retry/failure policy when RGF-U12 closes the real project boot path.

After the U10 commit, update the active plan around a thin `RuntimeInstance`, a minimal immutable
startup-content snapshot, pure plugin-plan resolution, and a separate Host construction integration
unit before growing the reference game.

# Citations

- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u10-bound-png-read-decode-and-publication`
- `crates/nara_image/src/import.rs`
- `crates/nara_image/src/import/png.rs`
- `crates/nara_image/src/import/publication.rs`
- `crates/nara_image/src/limits.rs`
- `crates/nara_image/tests/image_import_limits.rs`
- `docs/migrations/2026-07-engine-foundation.md#rgf-u10-1-bounded-png-ingest-and-publication`
