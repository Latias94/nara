---
type: "Verification Evidence"
title: "Closed Scene binding selection and shared strict data compilation"
description: "Qualifies explicit mixed Schema selection and public Prefab compilation while preserving separate runtime authority, data publication and export obligations."
timestamp: 2026-10-27T10:56:01Z
record_id: "263aa2e60c62429db4a8862d2df6e059"
producer_id: "codex-root"
run_id: "scene-component-bindings-2026-10-27"
related_plan: "docs/plans/2026-10-27-scene-data-representation-plan.md"
git_branch: "refactor/scene-data-representation"
verified_by: "01a17cf2-0432-7420-8b0c-3eb578a7c6e9"
supersedes: "7e4b079a81b64d149b8f031b8b2c84ea"
---

# Verification

Production selection/compiler successor to `a693fce`, under the ongoing
[Scene data-family plan](../../../../plans/2026-10-27-scene-data-representation-plan.md).
The full foundation Goal remains active. ADR 0120 remains Proposed; this is not a complete
runtime family, managed owner, language SDK or owned Editor UI qualification.

# Result

`SceneComponentBindings` retains an exact frozen native snapshot and explicitly admitted
data-owner snapshots. Construction rejects duplicate owners and overlapping active/tombstoned
claims across current/predecessor catalogs. Owner and declaration allowances are checked before
creating the derived indexes. Native owner enumeration now includes raw empty/tombstone-only
owners, not only provider receipts. Limits bound these new data indexes, not total CPU or all
already-owned metadata memory; native owner scanning remains unmeasured startup work.

`SchemaAuthority::Mixed` selects these definitions inside the existing semantic compiler.
Data records require an explicit matching durable owner and current version. Unknown, retired,
ownerless, foreign and unsupported-version records refuse rather than becoming native-preview
omissions. Missing native bindings cannot produce a frozen native snapshot and never choose data
implicitly. Native migration and ownerless native convenience retain their existing path.

Both public compiler entries use one `try_expand_with_authority` implementation. Raw Prefab
declarations are representation-neutral; the private enum variant was renamed from Native to
Raw. Classified Authoring sources do not transfer their basis into Mixed execution. Compiler
results remain document/provenance candidates, not runtime definition authority or file receipts.

Public tests use two data Schemas plus real native Name, without per-Schema native data types or
hidden preview helpers. Repeated/nested Prefabs preserve independent references, canonical
provenance, defaults, declarative overrides and actual selected-loader requests. Separate asset
coverage tests nested default Path and StableId references against selected metadata with no
native codec/AssetServer, rejecting missing metadata without a partial candidate.

# Evidence

| Actual command / scope | Result |
| --- | --- |
| Scene `cargo check --lib --locked`, no-default and all-features | Both passed |
| Scene final `nextest --all-features --lib` plus component_bindings, component_binding_assets, prefab_instance_expansion and prefab_sources | 415 passed, 0 skipped; run `d33abae0-7ecd-4159-94fa-108d747c2783` |
| Same final `nextest` library/integration selection with `--no-default-features` | 256 passed, 0 skipped; run `d381ebe1-fecf-445f-a48c-174672d37f8b` |
| Focused no-default new public tests | 17 passed, 0 skipped; run `5e01ee6f-c7d6-4074-a9a1-0b46345a59a9` (included in the final 256) |
| Root `cargo check -p nara --all-features --lib --locked` | Passed in 15.17 s |
| Scene all-features/all-targets correctness Clippy | Passed in 23.71 s; existing warnings remain |
| Final focused correctness Clippy for both new test targets | Passed in 1.17 s after oracle correction |
| `cargo +1.95.0 fmt --all -- --check` | Passed |

All Cargo commands used `+1.95.0`, root target and serial `CARGO_BUILD_JOBS=1`. No full workspace,
native Editor, actual language or standalone delivery rerun is claimed. Core code was unchanged
between the final feature-matrix results. The root check is build evidence, not a gameplay test.

Kierkegaard (`01a17cf2-0432-7420-8b0c-3eb578a7c6e9`) independently reviewed admission, limits,
owner/version, source authority, actual demand and native compatibility, with no confirmed
implementation finding in this scope. An asset oracle could originally miss skipped StableId
validation because both reference forms targeted the same absent asset. It was strengthened to
singleton Path, singleton StableId and combined lists before the final runs. The new test writer
and both review/research agents were closed. Original vendor and reference content were untouched.

# Follow-up

Continue actual runtime preparation/publication, not another comparison harness. Nietzsche's
read-only seam review (`01a17ceb-4b31-7b43-aa14-5c4bbd817134`) established these owning constraints:

- Extend `validation::PreparedSceneEntity` with a complete per-entity data group and retain it
  with `UnboundScenePublication`. Native flattening/receipt maps stay native-only. Reuse the
  existing value/reference validators rather than add an interpreter.
- `ComponentDecodeContext` holds limits, not a scene-wide debit accumulator. Current native
  preflight also migrates/clones before such aggregate admission. Native and data preparation
  need a shared residual allowance before owning copies, including defaults; passing the same
  full limits to each record is not aggregate enforcement.
- After target mapping, stage one lifecycle-free carrier per entity in the existing insertion
  plan, without inserting it before `declare_persistent_apply_targets` (fresh targets must still
  be empty). Reserve the carrier type against product overlays. Register/check its topology
  before target allocation after flush and recheck in the final exclusive publication tail.
- Install managed data selection through Scene-owned App/seal/startup authority. Capturing a
  caller's arbitrary bindings is not admission. Compare exact native/owner snapshots, not bytes
  or the binding container's address. Preserve the existing explicit-projection exception where
  selected native definition differs from managed native basis; do not globally equate them.
- Carrier members retain only their owner-local data snapshots, never the complete mixed
  binding/native codec composition or VM leases. Strict retirement scans actual archetypes;
  no carrier whitelist or remove-hook workaround is needed. Native claims must still be gone
  before final retirement, and additional retirements retain their separate guards.
- Extend the current-live export loop over registered Scene identities and inspect every
  actual carrier member, not just selected schemas or an original instance roster. Native-only
  export must reject encountered data even with an empty native catalog. Reuse actual-instance
  entity/persistent reference remapping; separately lower declared asset references through
  explicit immutable metadata under the chosen policy, without asset loading, preserving owner.

Relevant product entry points are `src/startup_scene.rs`, `src/startup_scene/replacement.rs` and
`src/project_host/runtime/content_residency/replacement.rs`. File-backed composition currently
preloads native Schema validation only; it does not automatically authorize data owners.
These are remaining implementation obligations, not runtime capabilities proved by compilation.

# Citations

- [Binding selection](../../../../../crates/nara_scene/src/component_bindings.rs)
- [Shared Schema authority](../../../../../crates/nara_scene/src/schema_authority.rs)
- [Public mixed compiler tests](../../../../../crates/nara_scene/tests/component_bindings.rs)
- [Default asset-reference tests](../../../../../crates/nara_scene/tests/component_binding_assets.rs)
- [Existing preparation owner](../../../../../crates/nara_scene/src/spawn/preparation.rs)
- [Existing final publication](../../../../../crates/nara_scene/src/spawn/publication.rs)
- [Existing live export](../../../../../crates/nara_scene/src/export.rs)
- [Representation design](../../../../architecture/scene-component-representations-design.md)
