---
type: "Verification Evidence"
title: "M1 runtime safety gate continued"
description: "U1-U5, U18, and U25 pass the sequential low-memory milestone gate; M1 decision is continue."
timestamp: 2026-07-11T01:52:41Z
record_id: "6170a50ac21a4da1b2d87860f1a5d35f"
resource: "nara engine foundation"
tags: ["m1", "runtime-safety", "u4", "gameplay", "lifecycle", "time", "tasks", "diagnostics", "filesystem"]
status: "complete"
producer_id: "codex-root"
related_plan: "docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "8ba9384"
verified_by: "codex-root"
supersedes: "f20c564eb0b140f68d60d29adee70f"
---

# Verification

The M1 milestone table requires U1-U5, U18, and U25 to prove ADR governance, terminal plugin
failure containment, per-tick fixed time, authoritative gameplay commands, bounded tasks,
privacy-safe diagnostics, and capability-bound filesystem IO. The final gate was run sequentially
with `CARGO_BUILD_JOBS=1`, one package/target at a time, and one nextest thread because an earlier
broad nextest run exhausted host memory.

# Result

Decision: **continue**.

- Built-in plugin lifecycle retains retry only before mutation, terminally poisons after committed
  failure, and preserves reverse once-only cleanup ownership.
- Fixed clocks advance once per authoritative tick and preserve declared desktop/server debt policy.
- U4 commit `8ba9384` replaces the frame command vector with bounded canonical
  `(tick, source, source_sequence)` admission, current-gated Consume/Capture, engine-owned Ack, and
  sticky terminal quarantine. Public ingress cannot impersonate `LocalAction`.
- Task work remains bounded, threaded in production, terminally typed, cooperatively cancellable,
  deterministically integrated, and finitely shut down.
- Diagnostics retain only classified bounded state and expose separate headless pressure snapshots.
- The Windows host proved handle-bound filesystem identity, reparse rejection fixtures, atomic
  replacement, locks, digests, and explicit guarantee tiers. Three live symlink/junction tests were
  skipped because they require host privileges; their default public-open reparse-tag fixtures
  cover the same fail-closed classification without overstating privileged evidence.
- None of M1's revise/abort triggers occurred: lifecycle cleanup ownership is proven, tasks require
  no global type-erased result bus, and the supported Windows host provides a capability-bound IO
  tier with unsupported guarantees failing explicitly.

# Evidence

- `cargo fmt --all -- --check`: passed.
- `cargo nextest run -p nara_app --test-threads 1`: 52/52 passed.
- `cargo nextest run -p nara_tasks --test-threads 1`: 28/28 passed.
- `cargo nextest run -p nara_fs --test-threads 1`: 29/29 passed; 3 privileged tests skipped by
  their declared platform precondition.
- `cargo nextest run -p nara_diagnostic --features serde --test-threads 1`: 51/51 passed.
- `cargo nextest run -p nara_gameplay --features serde --test-threads 1`: 27/27 passed.
- `cargo nextest run -p nara --test-threads 1`: 55/55 passed, including 5/5 architecture tests and
  exact ServerPlugins command-stream assertions.
- `cargo check --workspace`: passed with optional workspace adapter crates compiled.
- `cargo check -p nara --features serde --all-targets`: passed.
- `cargo run -p nara --example headless_server`: passed.
- Strict `nara_gameplay` Clippy, root all-target checks, command stale-symbol searches, and
  `git diff --check` passed during U4 integration.
- U4 code commit: `8ba9384`. ADR 0057 now has implementation evidence; ADR 0024 remains honestly
  partial only because its canonical seeded RNG resource is still an explicit future trigger.

# Follow-up

M1 opens U8. Run the two-world runtime identity fork/reload spike before U6/U9 or downstream
document and Play Mode consumers depend on identity. In parallel, assess the user's proposed
Seven Billion Humans-style pause/step/timeline and domain `ExecutionCursor` protocol against U8 and
U16 boundaries. Record a separate replay/debug ADR if the evidence supports it; do not conflate
asset hot reload with Rust machine-code replacement or promise reverse execution without bounded
checkpoints and deterministic forward replay.

# Citations

- `docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md`
- `docs/architecture/adr/0057-authoritative-fixed-tick-and-command-ingress.md`
- `docs/architecture/adr/implementation-status.md`
- `docs/migrations/2026-07-engine-foundation.md`
- `crates/nara_app/src/lib.rs`
- `crates/nara_gameplay/src/queue.rs`
- `crates/nara_tasks/src/runtime.rs`
- `crates/nara_diagnostic/src/contract_tests.rs`
- `crates/nara_fs/tests/filesystem_contract.rs`
