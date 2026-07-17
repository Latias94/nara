---
type: "Verification Evidence"
title: "RGF-U5 managed runtime correction verification"
description: "Supersedes the first U5 verification after raw managed World access exposed a safe Bevy change-detection bypass; ff2e02a structurally seals the scope and re-verifies the six correction regressions."
timestamp: 2026-07-17T05:10:52Z
record_id: "5a58cbaf030041fe8ead8cfdf4119c51"
tags: ["rgf-u5", "verification", "correction", "runtime", "fault-boundary"]
status: "verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "ff2e02a"
verified_by: "codex-root,u5-final-api-review,u5-final-adversarial"
supersedes: "5b37e0cb30ca4d24bb1b30fc98dc7e47"
---

# Verification

This record corrects and supersedes the first RGF-U5 verification. The earlier implementation at
`ff6b111` still exposed raw mutable `World` access through managed candidate, driver, and close
scopes. A caller could use Bevy's safe `bypass_change_detection` API to replace the fallback error
handler, consume an observer failure, restore the handler, and pass the epoch checks. That made the
earlier RV-03 closure claim too strong even though the other five reported regressions were closed.

Commit `ff2e02a` removes raw mutable `World` access from those managed scopes, protects the runtime
fault reporter, fallback handler, and private bridge revision from typed mutable access, binds
Nara-managed observers to the canonical handler explicitly, and validates the private managed
generation and fault authority around every startup and core schedule. Native Rust systems that
explicitly install their own per-system, per-command, or global fallback policy remain trusted code
that owns that policy; they are outside the default managed-error guarantee rather than an
untrusted sandbox surface.

# Result

| Review regression | Final result |
|---|---|
| Ready candidate publishes a pre-existing sticky fault as Running | Closed. Promotion publishes and reads the reporter atomically and initializes `Faulted`; state projection also observes later sticky faults. |
| Required task or service failure lacks a production bridge | Closed. Real task terminals and service outcomes enter domain integration systems, where required versus optional policy is known; `nara_tasks` intentionally owns execution mechanics rather than guessing domain criticality. |
| Abnormal Drop permanently leaks through `mem::forget` | Closed. Incomplete owners enter bounded, observable, owner-thread-affine quarantine with retry and fail-fast capacity limits. |
| Managed scopes can temporarily replace the fault bridge | Closed. Candidate access is immutable and driver/close mutation is typed and guarded; the public API has no mutable-World field, return, alias, or `DerefMut`/`AsMut`/`BorrowMut` conversion. |
| A pending Stop destroys Winit surface-retirement authority | Closed. Stop first exposes `Stopping`; Winit retires surfaces and releases providers before runtime close runs. |
| The example helper fails to retry initial `RetirementIncomplete` | Closed. It drives until `Retired` or its deadline, and incomplete retirement invokes retry. |

Two independent final reviews of the exact correction diff found no remaining P0, P1, or P2 in
these six regressions. ADR 0084 remains Proposed; this trial evidence does not accept the ADR.

# Evidence

- Implementation commit: `ff2e02a9ea087e32a00d90cde3b9e883dbc20c68`.
- Six-regression code diff fingerprint before commit:
  `298b821956df9e920c87a09afab8c8158527cc5b` using
  `git diff HEAD --binary | git hash-object --stdin` over the related code paths.
- `cargo nextest run --workspace --locked`: 788/788 passed, with three declared conditional skips.
- `cargo check --workspace --locked`: passed.
- `windowed_clear`, `windowed_sprites`, and `runtime_ui_panel`: all passed their documented
  no-default-feature check commands under `--locked`.
- `cargo test --doc --workspace --locked`: passed.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- The AST boundary regression parses all public impls, fields, aliases, trait methods, and mutable
  conversion impls; it rejects renamed raw-World return paths instead of relying on token names.
- Focused regressions cover pre-promotion faults, real required and optional task/service outcomes,
  bounded quarantine, hidden change-detection replacement, observer/default-command capture,
  pending-Stop surface retirement, and incomplete-retirement retry.

# Follow-up

- RGF-U5 is closed at `ff2e02a` under the active plan's correction gate.
- The historical closure review and first verification remain immutable evidence but must be read
  through this superseding record for RV-03 and the U5 closure revision.
- RGF-U28, RGF-U12, and RGF-U29 are eligible parallel successors only when admitted by the active
  registration; this record does not start them.
- ADR 0084 remains Proposed until RGF-U23 performs the independent decision review.

# Citations

- `docs/knowledge/engineering/verification/2026-07/2026-07-17T014655Z-rgf-u5-managed-runtime-verification-5b37e0cb30ca4d24bb1b30fc98dc7e47.md`
- `docs/knowledge/engineering/subagents/2026-07/2026-07-17-rgf-u5-runtime-closure-review.md`
- `docs/knowledge/engineering/subagents/2026-07/2026-07-16-rgf-u5-runtime-code-review.md`
- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md`
- `docs/architecture/adr/0084-executable-runtime-ownership-and-isolation.md`
- `crates/nara_app/src/runtime.rs`
- `crates/nara_app/src/lib.rs`
- `tests/runtime_instance.rs`
- `tests/runtime_driver_boundary.rs`
- `crates/nara_winit/src/tests.rs`
- `examples/support/runtime_retirement.rs`
