---
type: "Verification Evidence"
title: "RGF-U12 authorized startup content verification"
description: "Commit f341255 closes bounded authorized scene, prefab, and image startup-content publication as an immutable budget-leased snapshot."
timestamp: 2026-07-18T03:58:46Z
record_id: "ac05708ff3764164865e4cae207bf5a3"
tags: ["rgf-u12", "verification", "project-content", "filesystem", "budgets"]
status: "verified"
producer_id: "codex-root"
run_id: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
source_session: "019f4ede-b40a-77c3-8336-c6f713f3fa86"
related_plan: "docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md"
git_branch: "refactor/engine-foundation-contracts"
git_commit: "f341255"
verified_by: "focused nextest,workspace nextest,workspace check,wasm check,clippy,fmt,static boundary audit,independent review"
---

# Verification

RGF-U12 was verified against commit `f341255` on
`refactor/engine-foundation-contracts`. The reviewed scope is the public
`ProjectContentLoader` path from one host-issued project `DirectoryCapability` and one matching
`ProjectSettingsCandidate`/`RuntimePlan` pair into an immutable `ProjectContentSnapshot`. The
loader follows only the startup scene's path-addressed prefab and image closure and publishes no
App, runtime, service, native binding, source capability, or target `World`.

# Result

Passed. The reference game now commits a canonical startup scene, prefab, image metadata record,
and PNG source that load from randomized current/home directories through the root product API.
The snapshot carries the settings lineage, frozen schema fingerprint/generation, content revision
and digest, source-upgrade flag, original and expanded scene documents, prefab documents, and
imported image values behind one retained budget lease.

- Scene, prefab, asset metadata, reflected `AssetRef`, and PNG boundaries reject unknown,
  malformed, unsupported, escaped, cyclic, stable-ID-only, over-budget, or hierarchy-dependent
  input before snapshot publication.
- Twelve tracked budget kinds plus aggregate bytes expose active and high-water observations.
  Queue, in-flight, handle, work, artifact, and retained charges release on every failure path;
  retained snapshot charges release exactly when the last cloned snapshot owner drops.
- Snapshot clones share immutable documents and image pixel storage. `ImageAsset` is intentionally
  non-`Clone`, verified by a compile-fail fixture, so imported pixel ownership cannot escape the
  snapshot lease through a public value clone.
- Static AST and type-identity guards reject ambient filesystem/path authority, module/include
  redirection, hidden filesystem tokens, whole-root indexing, snapshot authority/service/runtime
  fields, and macro/import/type-alias bypasses in the content boundary.
- The schema fingerprint remains World-independent. Target-World required-component, hook, and
  observer eligibility remains RGF-U29 and is not claimed by this unit.

# Evidence

- The final root content boundary and budget suite passed 38/38 after review hardening. The broader
  U12 root composition/content suite passed 49/49.
- `cargo nextest run --locked -p nara_fs -p nara_project -p nara_reflect -p nara_asset -p nara_image -p nara_scene --test-threads=1`:
  264 passed, 3 declared conditional skips, no failures.
- `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --test project_manifest_ingest --test project_content_boot --test prefab_startup --test-threads=1`:
  6 passed.
- `cargo nextest run --workspace --locked --test-threads=1`: 807 passed, 3 declared conditional
  skips, no failures.
- `cargo check --workspace --locked` and
  `cargo check --manifest-path reference-game/Cargo.toml --locked --all-targets`: passed.
- `cargo check --locked -p nara_fs --target wasm32-unknown-unknown`: passed. The only warning is
  the pre-existing cfg-specific dead-code warning for `as_os_str`.
- `cargo check --locked -p nara --no-default-features --features runtime-2d,desktop-winit,render-wgpu --example windowed_sprites`:
  passed.
- `cargo fmt --all -- --check`, `git diff --cached --check`, root/reference-game format checks,
  and focused Clippy with only documented pre-existing lint allowances: passed.
- Adversarial review findings were resolved for pre-budget path allocation, snapshot-payload
  escape, retained-byte accounting, all-kind lease release, aggregate contention diagnostics,
  canonical asset metadata output, strict `AssetRef` shape, cross-platform path iteration, and
  AST/type/macro boundary bypasses. The final independent review reported no P0, P1, or P2 finding.

# Follow-up

RGF-U12 closure does not close the active plan. RGF-U29 must still prove the static provider and
per-apply target-World eligibility boundary. U12 and U29 then converge before RGF-U26 freezes the
manual materialization baseline and RGF-U24 owns the concrete Host start transaction. Runtime
materialization must consume the snapshot's leased values without reopening project source.

# Citations

- `docs/plans/2026-07-12-001-refactor-reference-game-driven-foundation-plan.md#u12-build-an-authorized-immutable-project-content-snapshot`
- `src/project_content.rs`
- `src/project_content/budget.rs`
- `tests/project_content_boot.rs`
- `tests/project_content_boundary.rs`
- `tests/project_content_limits.rs`
- `reference-game/tests/project_content_boot.rs`
- `reference-game/tests/prefab_startup.rs`
- Commit `f341255`
