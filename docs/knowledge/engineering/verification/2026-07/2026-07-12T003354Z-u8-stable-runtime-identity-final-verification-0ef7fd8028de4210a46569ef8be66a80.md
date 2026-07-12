---
type: "Verification Evidence"
title: "U8 stable runtime identity final verification"
description: "Focused and workspace evidence for the completed U8 identity contract."
timestamp: 2026-07-12T00:33:54Z
record_id: "0ef7fd8028de4210a46569ef8be66a80"
resource: "nara stable runtime identity"
tags: ["u8", "identity", "verification"]
status: "passed"
producer_id: "codex-root"
run_id: "goal-019f5096"
source_session: "019f4f36-42c9-7043-92b5-661311b14e21"
related_plan: "docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "9263d8c"
verified_by: "codex-root"
---

# Verification

Verify the U8 requirements and scenarios from the engine-foundation plan at `9263d8c`, including
world-scoped allocation, duplicate scene instances, typed lookup outcomes, fork/restore remaps,
tombstones, failure-atomic scene replacement/export, reflected reference serialization, gameplay
command targets, and bounded tooling observations.

# Result

Passed. ADR 0058 is implemented and every in-repo scene, gameplay, reflection, tooling, and facade
consumer uses the shared `nara_identity` vocabulary. ADR 0076 remains partial because U9
schema-aware component observations and the U16 runtime host are intentionally still open.

# Evidence

- `cargo nextest run -p nara_identity -p nara_gameplay -p nara_reflect -p nara_scene -p
  nara_tooling`: 171 passed, 0 skipped on 2026-07-12.
- `cargo nextest run -p nara_identity -p nara_gameplay -p nara_reflect -p nara_scene -p
  nara_tooling -p nara --test stable_runtime_identity --test scene_play_mode`: 8 passed, 0 skipped
  on 2026-07-12.
- `cargo nextest run --workspace --all-features`: 618 passed, 3 configured skips at `9263d8c`.
- `cargo check --workspace`, `cargo fmt --all -- --check`, and strict no-dependency clippy for
  `nara_reflect` plus `nara_scene` passed at `9263d8c`.
- `tests/stable_runtime_identity.rs` and canonical serde tests prove stable serialized references
  contain no Bevy `Entity`, world-domain ID, process pointer, or runtime asset handle.
- Stale-contract searches find no duplicate gameplay/scene identity owner, `SceneEntityMap`, raw
  `WorldSnapshot`, or removed map accessors in code, tests, and examples. The only
  `PersistentRuntimeId` definition is the canonical `nara_identity` owner.

# Follow-up

Enter U9 before U10/U16. U9 must freeze the registry, enforce operation-level field capabilities,
add strict bounded persistent readers and canonical envelopes, and preserve failure-atomic
publication. Do not restore raw-entity tooling snapshots or duplicate identity aliases.

# Citations

- `docs/plans/2026-07-10-001-refactor-engine-foundation-contracts-plan.md#u8-stable-runtime-identity-and-entity-references`
- `docs/architecture/adr/0058-stable-runtime-identity-and-entity-references.md`
- `docs/architecture/adr/0076-play-runtime-debug-control-and-observation.md`
- `docs/migrations/2026-07-engine-foundation.md#u8-1-world-scoped-runtime-identity-and-stable-entity-references`
