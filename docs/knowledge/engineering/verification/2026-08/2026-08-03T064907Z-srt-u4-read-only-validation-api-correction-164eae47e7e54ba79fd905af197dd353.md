---
type: "Verification Evidence"
title: "SRT-U4 read-only validation API correction"
description: "Supersedes the first U4 verification after detailed API review found hierarchy validation could register missing component types and the provisional replacement result was hidden and discardable."
timestamp: 2026-08-03T06:49:07Z
record_id: "164eae47e7e54ba79fd905af197dd353"
tags: ["srt-u4", "verification", "correction", "hierarchy", "api"]
status: "verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-08-02-001-refactor-startup-scene-activation-and-atomic-retry-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "5a90f55"
verified_by: "Codex detailed API review plus serial focused Cargo gates"
supersedes: "5175acec97f041c2af61585559c01e04"
---

# Verification

This record corrects and supersedes the first SRT-U4 verification. The implementation at
`25e7f6c` made complete hierarchy validation proportional to hierarchy participants, but created
its filtered query state through mutable `World::query`. On a World where `Parent` or `Children`
was not registered, validation could extend the component registry despite being a read-only
semantic check. The advanced product replacement was also exported while hidden from rustdoc and
its diagnostic-bearing result was not marked `must_use`.

Commit `5a90f55` restores `validate_hierarchy(&World)`, initializes filtered query state with
`QueryState::try_new`, and treats absent relationship types as an empty participant set without
registration. It removes `doc(hidden)` from the provisional advanced replacement and marks its
`SceneSpawnReport` result `must_use`.

# Result

Passed at `5a90f55`.

- Empty-World validation returns success without changing the component count or registering
  `Parent`/`Children`; sparse and corrupted relationship fixtures retain complete reverse-edge,
  missing-entity, duplicate, and cycle coverage.
- The replacement entry remains absent from the ordinary prelude, but its provisional advanced
  contract is now discoverable and callers cannot silently discard success or rejection.
- Detailed review withdrew the zero-limit finding after confirming R8/AE5 require zero input
  entries, not a zero-capacity `ItemLimit`.
- A receipt-owning reference-game run resource does not need to pass through
  `SceneProductOverlayWriter::replace_resource`. SRT-U5 holds that private owner in
  `World::resource_scope`; a rejected Scene transaction automatically restores the unchanged
  owner, while success updates the new `SpawnedSceneInstance` without failure before the exclusive
  system returns. This matches the plan's explicit Scene-then-Run commit sequence and remains an
  SRT-U5 acceptance test rather than a new generic Scene callback.
- Runtime-only overlay component types remain a trusted advanced composition responsibility:
  ordinary products register them during plugin build/seal, while the runtime writer resolves an
  existing `ComponentId` and never mutates the registry. No provider or registration-provenance
  registry was added.

# Evidence

- `cargo nextest run -p nara_hierarchy -p nara_scene --locked --test-threads=1`: 120 passed.
- Strict all-target Clippy passed for `nara_hierarchy`, `nara_scene`, and root with the repository's
  explicit pre-existing lint allowances.
- `cargo fmt --all` and `git diff --check` passed.
- No Cargo commands ran concurrently. `tests/architecture_docs.rs` was intentionally not run.

The broader serial U4 evidence in the superseded record remains applicable to the unchanged ECS,
identity, reflect, Scene transaction, and reference-game paths; this record is the authoritative
closure revision for hierarchy immutability and advanced API discoverability.

# Follow-up

SRT-U5 is active. It must prove the private receipt-owning run-resource scope, ordinary build/seal
registration of every runtime-only overlay type, exact projectile ownership, and same-tick
coherence while replacing the reference game's copied reset template.

# Citations

- `docs/knowledge/engineering/verification/2026-08/2026-08-03T053651Z-srt-u4-atomic-product-scene-replacement-5175acec97f041c2af61585559c01e04.md`
- `crates/nara_hierarchy/src/validation.rs`
- `crates/nara_hierarchy/src/tests.rs`
- `crates/nara_scene/src/product_transaction.rs`
- `crates/nara_scene/src/spawn.rs`
- Git commit `5a90f55`
