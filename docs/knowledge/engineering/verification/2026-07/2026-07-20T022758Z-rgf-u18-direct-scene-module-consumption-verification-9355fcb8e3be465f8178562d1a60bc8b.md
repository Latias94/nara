---
type: "Verification Evidence"
title: "RGF-U18 direct scene module consumption verification"
description: "Commit de4834e proves locked direct nara_scene consumption through documented public prerequisites without the root facade."
timestamp: 2026-07-20T02:27:58Z
record_id: "9355fcb8e3be465f8178562d1a60bc8b"
tags: ["rgf-u18", "verification", "module-consumer", "scene", "dependency-boundary"]
status: "verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "de4834e226f459184315fe12e4a7bc78fe70b59f"
verified_by: "focused nextest,independent check,clippy,fmt,diff review"
---

# Verification

RGF-U18 was reviewed at implementation commit `de4834e` on
`refactor/engine-foundation-contracts` against R3, R30, and AE16. The review rejected a nested
Cargo test that added roughly ninety seconds of lock/build overhead, retained a fast AST boundary
gate, and kept the independent Cargo invocation as the authoritative compilation check.

# Result

RGF-U18 passed its implementation, dependency-boundary, and review gates.

- `module-consumer` is an independent resolver-v3 workspace with its own lockfile and direct
  dependencies only on `nara_scene`, `nara_reflect`, and test-only `bevy_ecs`.
- The consumer decodes, publishes, validates, and spawns the committed canonical-v1 RON fixture
  through public APIs into a caller-owned registry and `bevy_ecs::World`.
- Spawn evidence preserves the stable `enemy` and `player` scene identities plus the expected
  `Name` and `Visibility` values.
- Root boundary tests reject the facade, workspace dependency inheritance, `[patch]`, private Nara
  dependencies, undeclared source roots, module path redirects, and hidden source inclusion.
- Removing the documented `nara_reflect` prerequisite fails at its explicit import. Cargo metadata
  confirms one workspace member and no root `nara` package in the dependency graph.
- The module README documents exact prerequisites and limits the claim to the direct scene slice;
  it does not claim arbitrary module or cross-engine composition.

# Evidence

- `cargo nextest run --locked -p nara --test module_consumer_boundary --test-threads=1` -> 6 passed.
- `cargo check --manifest-path module-consumer/Cargo.toml --locked --all-targets` passed.
- `cargo nextest run --manifest-path module-consumer/Cargo.toml --locked --test-threads=1` -> 1
  passed.
- `cargo clippy --manifest-path module-consumer/Cargo.toml --locked --all-targets -- -D warnings`
  passed.
- `cargo nextest run --locked -p nara --test architecture_docs --test-threads=1` -> 7 passed
  after the foundation and implementation-ledger updates.
- The focused root test target passed strict Clippy with only the repository's documented existing
  allowances for `nara_app` and `nara_asset`; unqualified strict Clippy remains blocked by existing
  `double_must_use`, `too_many_arguments`, and `derivable_impls` findings outside U18.
- `cargo fmt --all -- --check`, the independent consumer format check, staged diff review, and
  `git diff --check` passed.
- Engineering-memory validation passed. Derived `current-state.md` and `log.md` remain stale by
  design because both have concurrent working-tree edits; a later integration owner must render
  them from the immutable shards.

# Follow-up

RGF-U15 may now add hosted Windows/Linux feedback for the locked root, reference-game, and
module-consumer workspaces. U18 does not authorize a compatibility facade, arbitrary standalone
modules, cross-engine interoperability, or a stable dynamic ABI.

# Citations

- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u18-prove-direct-nara_scene-module-consumption`
- `module-consumer/Cargo.toml`
- `module-consumer/tests/scene_spawn.rs`
- `tests/module_consumer_boundary.rs`
- `crates/nara_scene/README.md`
- Commit `de4834e`
