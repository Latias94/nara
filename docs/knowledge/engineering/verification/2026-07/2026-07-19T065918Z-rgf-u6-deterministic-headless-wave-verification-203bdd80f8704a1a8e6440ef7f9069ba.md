---
type: "Verification Evidence"
title: "RGF-U6 deterministic headless wave verification"
description: "Commit db511a7 closes the authoritative reference-game wave, stable snapshots, terminal outcomes, privacy-safe CLI, and bounded shutdown."
timestamp: 2026-07-19T06:59:18Z
record_id: "203bdd80f8704a1a8e6440ef7f9069ba"
tags: ["rgf-u6", "verification", "reference-game", "headless", "cli"]
status: "verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "db511a7"
verified_by: "focused nextest,reference-game nextest,workspace nextest,workspace check,all-target check,optional examples,clippy,fmt,independent review"
---

# Verification

RGF-U6 was verified against commit `db511a7` on
`refactor/engine-foundation-contracts`. The reviewed scope completes the first deterministic
headless reference-game product path through the U24-owned public action: authorized project
content boot, stable gameplay state, terminal outcomes, privacy-safe CLI output, and bounded
runtime retirement.

# Result

Passed. The reference game now runs a complete deterministic wave using only the public root
product surface and committed project content.

- The startup scene consumes the committed enemy prefab and image closure, while scene-managed
  projectiles receive deterministic stable identities and retire through the identity domain before
  despawn.
- Semantic movement commands, pursuit, automatic fire, collision, damage, death, score, and
  `WaveOutcome::{Completed, Defeated}` are driven through the fixed tick. A same-tick player death
  and final enemy death resolves to `Defeated`.
- `WaveTickGate` admits a tick only after command/topology validation. Runtime faults reject the
  tick, prevent downstream simulation, and prevent a failed frame from replacing the last-good
  authoritative snapshot.
- Snapshots sort stable game/scene identities, carry tick/outcome/player/enemy/projectile/score
  state, and are published only for admitted ticks. The CLI starts from bundled input, accepts only
  a bounded `--max-ticks`, emits one stable JSON summary for terminal success, and emits static
  privacy-safe diagnostics with nonzero exit for all failure paths.
- `HeadlessRun` owns an owned command buffer and keeps admission, candidate publication, and
  bounded cleanup inside the root Host. Compile-fail and source-boundary tests reject unbounded
  iterator inputs and private lifecycle orchestration in ordinary callers.

This is implementation evidence for RGF-U6. It does not claim desktop rendering/input parity,
editor persistence, hosted artifact delivery, replay, or a universal external command-file
capability; those remain owned by later plan units.

# Evidence

- Root public contract gate: `cargo nextest run --locked -p nara --test reference_game_contract
  --test-threads=1` -> 3 passed.
- Reference-game U6 gate: `cargo nextest run --manifest-path reference-game/Cargo.toml --locked
  --test first_wave --test headless_cli --test headless_snapshot --test public_surface
  --test prefab_startup --test-threads=1` -> 18 passed.
- Root Host/runtime/content gate with `runtime-2d,serde`: -> 101 passed across the root library,
  `project_runtime_boot`, `project_host_boundary`, and `runtime_instance`.
- Scene and identity regression gate: `cargo nextest run --locked -p nara_identity -p nara_scene
  --test-threads=1` -> 99 passed.
- Full workspace gate: `cargo nextest run --workspace --locked --no-fail-fast --test-threads=1`
  -> 866 passed, 3 declared conditional skips, no failures.
- `cargo check --workspace --locked`, reference-game all-target check, and the three required
  optional backend example checks (`windowed_clear`, `windowed_sprites`, `runtime_ui_panel`) all
  passed.
- Targeted Clippy passed with only documented pre-existing allowances (`result_large_err`,
  `collapsible_if`, `double_must_use`, `too_many_arguments`, `derivable_impls`). Reference-game
  all-target Clippy passed with the existing `too_many_arguments`/`needless_return` allowances.
- `cargo fmt --all -- --check` and staged `git diff --cached --check` passed.
- Independent correctness/specification/reuse/quality/efficiency reviews found no remaining U6
  P0/P1. A final formal reviewer queue could not be completed because the local review service
  twice remained unavailable; the existing independent review evidence and all mechanical gates
  are retained as the coverage basis.

# Residual Risk

- Scene retirement is intentionally single-entity and synchronizes the parent/children projection
  per entity; a measured multi-entity wave pressure case may justify a future batch API.
- Cleanup retries are bounded per caller drive and paced by the CLI deadline; a long-lived desktop
  host should choose its own wake strategy.
- The wave proves semantic deterministic equality for the declared snapshot fields, not machine-level
  floating-point parity across all platforms.

# Follow-up

RGF-U13 is the next admitted unit. It owns the desktop-profile startup, native event-loop driving,
input parity, single-target render transaction, HUD, and truthful desktop shutdown behavior. U6
must not reopen Host publication choreography or add an external scenario/replay loader.

# Citations

- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u6-complete-the-authoritative-headless-wave`
- `reference-game/src/lib.rs`
- `reference-game/src/resources.rs`
- `reference-game/src/systems.rs`
- `reference-game/src/snapshot.rs`
- `reference-game/src/bin/headless.rs`
- `reference-game/tests/first_wave.rs`
- `reference-game/tests/headless_cli.rs`
- `reference-game/tests/headless_snapshot.rs`
- `tests/reference_game_contract.rs`
- `tests/project_runtime_boot.rs`
- `tests/project_host_boundary.rs`
- `crates/nara_identity/src/domain.rs`
- `crates/nara_scene/src/spawn.rs`
- Commit `db511a7`
