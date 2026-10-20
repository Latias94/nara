---
type: "Verification Evidence"
title: "Current Scene ownership and decoded-generation custody qualification"
description: "Qualify the first P2 ownership slice and retain the concrete residency design limits."
timestamp: 2026-10-20T17:26:53Z
record_id: "0bf781c53d9e4cc3950bfb77b98d2b8a"
producer_id: "codex-root"
related_plan: "docs/plans/2026-10-20-persistent-two-level-game-plan.md"
git_branch: "main"
git_commit: "3a9284a"
---

# Scope

This record accompanies the first P2 implementation diff after P1 at `3a9284a`.
It qualifies current source/receipt custody, decoded-generation retention and Reference Game Retry.
It does not qualify hosted cross-level travel, resource residency retirement or checkpoint saves.

# Verified Results

Root alone ran Cargo with Rust 1.95.0 and `CARGO_BUILD_JOBS=1` in the shared checkout.

- `cargo +1.95.0 check -p nara --no-default-features --features runtime-core --all-targets
  --locked --offline` passes. Direct data-only ownership does not require Image, filesystem loading
  or a graphics backend. Existing feature-subset dead-code warnings remain.
- Root all-feature library plus content boot/boundary/limits/delivery/selected-Scene/public-surface
  cohorts run 343 tests: 342 pass, one public trybuild suite fails only because five expected
  diagnostics still name the renamed types; one separate test is skipped. Nextest run
  `b2c260a5-3e78-4270-a255-b891bd5877f5`, local log
  `target/current-scene-owner-content-nextest.log`.
- After inspecting actual compiler output and updating those five expectations, the public surface
  suite passes 2/2, including all seven negative compilation cases. Nextest run
  `84eda727-704b-413e-ba4b-f217b2bf97fb`, log
  `target/current-scene-public-surface-final.log`. No blanket golden overwrite was used.
- Reference Game all-feature library and seven selected integration binaries pass 37/37. This
  includes consecutive Retry generations, startup/Retry normalization, stable-asset Retry,
  lifecycle rejection, exact retirement, deterministic wave timing and unchanged headless CLI
  summary. Nextest run `b65f9206-27da-4d29-b5f4-8794c7d0fb80`, log
  `target/current-scene-reference-nextest.log`. Independent-workspace Cargo reuses root `target`.
- Final `cargo +1.95.0 check --workspace --all-features --all-targets --locked --offline` passes;
  log `target/current-scene-workspace-final-check.log`. Root and Reference `cargo fmt --all --
  --check` pass. Existing unused/dead-code warnings remain; this is not a warning-free lint claim.

New real-owner tests prove Direct A -> B -> Restart B -> A, stale/foreign rejection before the
product callback, typed preparation rejection preserving the old pair, startup-window guards,
Project-current to Direct rejection, same-source Project restart, and receipt synchronization.
Host startup source facts retire at publication (two reservations become one live-content lease).
An escaped Image remains readable and retains one decoded generation after Host close; its final
drop releases that generation and reservation. The loader refuses a third generation while escaped
A and B remain, refunds rejected work, and accepts C after the last A holder retires. This last
case is real content-loader evidence, not an implemented hosted travel trajectory.

Three Codex lanes implemented disjoint source-owner, generation and consumer responsibilities.
Cross-review found and corrected two Bevy 0.20 `SystemState::get` Result handling omissions before
Reference verification. Independent review of the root Host integration found no production
blocker. A premature Reference command used nonexistent test target names and exited before tests;
a later duplicate-build attempt was interrupted and rerun with the shared target. Neither is a
passing verification claim.

# Ownership And Next Design

`CurrentScene` is read-only; its private owner holds one retained `SceneSource` and matching
`SpawnedSceneInstance`. Restart/Direct Replace derive the frozen registry from the World, require
Finalized startup and the full expected receipt, and update source plus receipt before returning.
Reference membership remains a derived game cache; it does not choose the Retry source. Snapshot,
SceneSource and escaped Image share an acyclic generation token. No token points back to a snapshot.
Source-only receipts do not retain decoded generations. Host freshness keeps the original startup
admission revision separately from the current Scene and releases the decoded startup snapshot.

Hosted replacement remains explicitly rejected with `scene.current.project-residency-required`.
The next slice must share a single staged AssetServer between component decoding and resource
preparation; independent clones allocate different fresh handles. Use a narrow nominal asset-binding
input rather than exposing scratch Scene entities across calls. Prepare exact Image versions,
TileSet extent/atlas/retention and owned catalog deltas before commit. Retire only still-owned
publications, preserve foreign records and historical handle bindings, and retire even for an empty
destination. Publish B's admitted Image values into shared handles so current B does not retain A's
generation indefinitely. Native effects keep their separate degraded/retry semantics.

The later game uses ordinary private modules and the normal headless/desktop entries. Prepared
target inspection must precede logical save-reference admission. Game reconstruction of deletion
facts and dynamic objects is distinct from playable Ready. Do not add a universal Save trait,
whole-World serializer, generic scene manager or a third export probe binary.

The user-requested compiler deprecation and Bevy/Nara task dependency investigation remains ordered
after P1-P3. Coexisting package names alone do not prove duplicate work or justify a new scheduler.

# Sources And Limits

- [Plan](../../../../plans/2026-10-20-persistent-two-level-game-plan.md)
- [Proposed ADR 0089](../../../../architecture/adr/0089-runtime-scene-instance-loading-activation-unload-and-travel.md)
- [Accepted ADR 0117](../../../../architecture/adr/0117-2d-foundation-ownership-and-authoring-contracts.md)
- Bevy/Godot residency comparisons used pinned local `repo-ref` source; no latest-release claim.
- Hosted CI at P1 `3a9284a` is still in progress. Wasm portable-library compilation and the Linux
  module consumer pass; this is neither browser execution nor final Windows/Linux Root acceptance.
- These are focused public/runtime tests, not a new native Editor visual qualification, full
  workspace test run, performance benchmark or complete P2/P3 production claim.
