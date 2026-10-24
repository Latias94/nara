---
type: "Verification Evidence"
title: "Public exact Scene instance end and source-context design review"
description: "Qualified empty-instance public retirement and shared authoring path; source origin carrier trial and occurrence integration remain open."
timestamp: 2026-10-24T03:11:11Z
record_id: "a91caf241959454f8674b9b04eabc141"
producer_id: "codex"
run_id: "session-01a1629b-instance-end"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "test/compiled-asset-domain-export"
git_commit: "5b3c99f"
verified_by: "serial nextest; public module consumer; scoped Clippy; rustdoc; independent native static reviews"
---

# Verification

All Cargo runs were root-owned and serial with `CARGO_BUILD_JOBS=1`, reusing the checkout target.

- The initial public integration test did not compile because
  `nara_scene::retire_and_despawn_scene_instance` did not exist (E0432). This is an API-availability
  red observation, not a failing runtime invariant or proof that the existing internal transaction
  was incorrect.
- `cargo nextest run --locked -p nara_scene --test instance_retirement --test-threads=1`:
  5 passed (`a5984a80`).
- `cargo nextest run --locked -p nara_scene --features serde --test-threads=1`:
  287 passed, 1 skipped (`764a66e9`), including the new public tests and existing authoring/projection
  tests. No new native Editor qualification is inferred from this window-free run.
- The separately compiled module consumer's existing `scene_spawn` test now also ends the exact
  instance, checks entity removal and retained tombstones, and rejects a stale retry. Its serial
  nextest run passed 1 test (`2a5e4f0c`) using its own manifest/lockfile and the shared target.
- `cargo clippy --locked -p nara_scene --all-targets --all-features -- -D clippy::correctness`:
  passed with existing warnings, not a warning-clean claim.
- `cargo test --locked -p nara_scene --doc --all-features`: 3 passed.
- Root `cargo fmt --all`, focused external-consumer rustfmt and `git diff --check` pass.

# Result

The public Scene owner now ends a whole exact runtime activation, including empty instances and
instances whose last entity was already individually retired. Foreign/stale receipts and native
lifecycle refusal preserve other activations. Identity history stays retained. The ordinary
authoring clear operation uses this same path and keeps its existing diagnostics/state behavior;
disposable projection clear retains its distinct epoch proof and domain release.

This is a prerequisite of the source/occurrence replacement, not that replacement. No occurrence
claim, bound source origin, canonical Prefab resolver result or new UI key semantics is implemented.
The public collision characterization remains incumbent behavior. The full foundation goal is Active.

# Evidence

Read-only design reviewer `01a16bd3-b0d5-72a2-950f-3e1621d0a034` found three P2 specification gaps:
occurrence uniqueness must not include source as an escape axis; empty instances require a public
exact end operation; and Scene admission refusal cannot be conflated with later UI candidate
rejection. All three were corrected in the Proposed design in `0b1f101`; the current code implements
only the existing runtime-instance end prerequisite. ADR 0109's whole-candidate rejection remains.

Five native read-only simplification/static review lanes covered reuse, quality/correctness,
efficiency, test value and public-contract comments. They found no actionable defect or worthwhile
behavior-preserving simplification. Proof tests were preserved. These were source reviews, not
independent Cargo reruns. All six research/review lanes and the design reviewer were closed.

Code review: skipped (ce-code-review unavailable) - the separate controller
`01a16bdb-6773-7b83-896d-5a237907a0c5` terminated without a receipt because its child context could
not discover native dispatch/collection tools. It did no substantive review. The root performed
the final owning diff scan; the completed native static reviews above are the actual coverage.
No external coding/review provider, push or publication was used.

The read-only source-carrier investigation `01a16bd9-459e-7851-ab1f-705640fa4fe8` confirmed:

- `ProjectInfo.stable_id` is optional text and starter product contracts bind it to a slug; it is
  not already a durable source universe. Effective settings preserve it rather than validate a UUID.
- Selected Scene loading reads/decodes/expands the Scene without admitting its asset metadata.
  Retained metadata catalogues scan assets/Prefabs, and absent Prefab metadata is legal today.
- `AssetRef` is a request, not a proven canonical source/product binding. Prefab resolution may
  map StableId to a path but returns a bare document and expansion retains the requested reference.
- File custody and settings/session lineage serve separate authority/change-detection purposes.

The recommended carrier candidate is a dedicated, authored, non-nil `project.source_universe` UUID,
generated only during explicit creation or supplied by the author, never minted on load. Named
origins use universe/name/product; stable-asset origins use stable asset/product without adding a
project universe; deliberately location-addressed origins use universe/full project-relative
path/product. No type or format is frozen by this recommendation. A proposed per-document location
policy list still needs an ergonomics comparison against a smaller explicit admission policy; it
must not become another redundant source registry merely to make metadata loss distinguishable.

# Follow-up

Continue the complete direct/file-backed source-origin trial, not only value equality tests. Decide
the explicit location-versus-stable policy and durable carrier, then replace source-bound candidates,
canonical resolver results, occurrence publication/replacement/end and Runtime/Canvas projections
together. Keep primary versus named products distinct, path aliases owner-proven, and metadata loss
an explicit refusal rather than identity downgrade. A bare candidate must not survive as an
identity-free persistent-spawn shortcut.

Use the public instance-end operation in the trial. Occurrence claims belong inside Scene's
exclusive end/replacement transaction, including empty instances; no caller-side cleanup registry.
Complete the ADR 0038/0058 reconciliation and ADR 0102 neutral conformance refreeze before claiming
the source fix. Do not accept ADR 0083's whole importer/package/copy/migration proposal by implication.
R2 complete queries/gestures/panel preparation, R5b overlays and later waves remain open.

# Citations

- [Public end implementation](../../../../../crates/nara_scene/src/spawn.rs)
- [Public end tests](../../../../../crates/nara_scene/tests/instance_retirement.rs)
- [Independent consumer](../../../../../tests/fixtures/module-consumer/tests/scene_spawn.rs)
- [Proposed source/occurrence design](../../../../architecture/scene-source-and-occurrence-identity-design.md)
- [Project info](../../../../../crates/nara_project/src/sections.rs)
- [Starter product contract](../../../../../src/project_workflow/starter/product_contract.rs)
- [Project source loading](../../../../../src/project_content.rs)
- [Metadata catalogue](../../../../../src/project_content/catalogue.rs)
- [Prefab resolver](../../../../../crates/nara_scene/src/prefab.rs)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
