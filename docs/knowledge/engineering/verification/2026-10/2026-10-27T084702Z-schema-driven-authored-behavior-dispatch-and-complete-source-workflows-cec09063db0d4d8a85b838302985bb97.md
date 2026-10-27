---
type: "Verification Evidence"
title: "Schema driven authored Behavior dispatch and complete source workflows"
description: "Records external catalog admission, three authored callback bindings, prepared source Attach/history/disk persistence and repeated Prefab identity proof."
timestamp: 2026-10-27T08:47:02Z
record_id: "cec09063db0d4d8a85b838302985bb97"
producer_id: "codex-root"
run_id: "schema-driven-behavior-2026-10-27"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "experiment/schema-driven-behaviors"
---

# Verification

Successor to the fixed-field private language experiment at `0c1fcd6`. It replaces that harness
in place, adds external existing-format catalog/Scene fixtures and reuses the shared authoring
basis and compiler. Production engine/Editor code is unchanged. The complete foundation Goal
remains active; ADR 0120 remains Proposed and its production implementation not-started.
No remote operation occurred and untracked `vendor/` was not read, changed or staged.

# Result

Both languages explicitly register Mover, Sensor and Label by Schema/version. Definition reads
the admitted canonical catalog owner. Native capture iterates admitted fields through the
existing property resolver and declared-reference plan, without per-Behavior native field/type
dispatch. Label's stable `text` differs from `wire_text`; optional gain is declared externally
and defaults only when absent. Callback availability checks precede fresh Runtime preparation.

Complete selected source workflows exercise actual Remove, Attach from the exact declared
template, a stable-field edit, Undo/Redo, JSON/RON disk Save/Reopen and direct/dual-Prefab language
execution. Native export independently marks original actor/friend pairs and asserts their exact
remapped Sensor references. Bad bindings preserve a separate active predecessor; foreign and
retired Invocation tokens reject. A third Schema needs catalog/source and language callback
changes, not a native extraction branch. No public SDK, migration lineage or VM reload is claimed.

The original Attach probe correctly rejected arbitrary records without a creation template.
The consumer now supplies an external complete template, attaches it exactly and edits afterward;
no production invariant was waived. The parent and Faraday identified a weak bad-target test
that stopped at the count cap. The corrected Sensor batch reaches the exact foreign-target
error after a valid first command, while the target Name stays unchanged.
Faraday's final read-only review closes that finding and the stale-token coverage gap with no
remaining P1/P2 in the declared scope. Both worker and reviewer are closed.

# Evidence

Windows 11 x64 / PowerShell, Rust 1.95.0, .NET SDK 10.0.400 and runtime/hostfxr 10.0.11.
mlua 0.12.2 asserts Lua 5.4; netcorehost 0.22.0 uses explicit hostfxr without nethost download.
All Cargo work is serial with `CARGO_BUILD_JOBS=1` and the root `target` reused.

```text
python tests/fixtures/behavior-languages/run.py
```

Run `33cdeebc-b691-4e43-a4e0-2a8b5f02ba86`: **3 passed, 0 skipped**. Both language tests
execute direct Scene, repeated Prefab and failure cases; the third rejects invalid catalogs.
Managed incremental build 0.81 seconds with zero warnings/errors; Rust build 7.13 seconds and
execution 0.216 seconds. These are test-stage observations, not language performance evidence.

```text
cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml -p nara-scene-module-consumer --test behavior_representation --locked --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail
```

Shared factory regression run `f11f5f00-1e07-4b7f-8e0b-3636b23fd2ed`: **9 passed, 0 skipped**;
build 6.27 seconds, execution 0.262 seconds. Existing root feature warnings remain.

```text
cargo +1.95.0 clippy --manifest-path tests/fixtures/behavior-languages/Cargo.toml --test languages --locked -- -D clippy::correctness
cargo +1.95.0 fmt --manifest-path tests/fixtures/behavior-languages/Cargo.toml --all -- --check
```

Focused correctness Clippy passed in 18.50 seconds without warnings; formatting passed.
All 778 local links across nine affected/new documents resolve. Python runner syntax and
`git diff --check` passed.
No production/native acceptance was rerun for this private fixture/shared-test-support change.

# Follow-up

Author code iteration, actual domain-access cost, VM-local versus envelope state, safe-point
instance cleanup, actual assets, public representation ownership and code lifetime remain open.
The fixed generation-one catalog and callback availability list do not establish catalog migration,
cross-owner dependency/ABI compatibility or a permission boundary. Reusable/asynchronous snapshots
are unsupported. Do not promote the private roster or Name-only command tail to a universal SDK.

# Citations

- [Plan](../../../../plans/2026-10-27-schema-driven-behavior-plan.md).
- [Successor report](../../../../research/2026-10-27-schema-driven-behavior-comparison.md).
- [Fixture](../../../../../tests/fixtures/behavior-languages/README.md).
- [ADR 0120](../../../../architecture/adr/0120-schema-publications-and-execution-representations.md).
- Franklin authored only the two language modules; parent integrated and executed checks.
- Faraday independently reviewed read-only, without Cargo or file edits.
