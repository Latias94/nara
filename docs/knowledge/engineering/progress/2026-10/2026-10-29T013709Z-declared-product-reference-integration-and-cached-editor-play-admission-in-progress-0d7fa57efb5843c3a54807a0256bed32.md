---
type: "Work Progress"
title: "Declared product reference integration and cached Editor Play admission in progress"
description: "Records complete Product reference integration, two independent contract gaps and pending JSON/RON, cached Play and Prefab copy qualification."
timestamp: 2026-10-29T01:37:09Z
record_id: "0d7fa57efb5843c3a54807a0256bed32"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md"
git_branch: "feat/declared-asset-product-references"
git_commit: "3e3f962"
---

# Summary

Continue the full active foundation Goal from `main@3e3f962`; do not reduce completion to this
declared-reference prerequisite. Current source is uncommitted on
`feat/declared-asset-product-references`. Startup/later Scene source-free catalog delivery is
still open. Preserve the existing untracked `vendor/`, serial Cargo, root target, no remote
publication, English artifacts and Chinese conversation.

# Details

`AssetRef::Product(AssetProductRef)` uses the existing AssetRef Schema leaf, codec, collector and
rewrite. Path and stable selectors keep their primary meaning. Source DB resolution proves only
the enclosing source; primary-only handles, Image/TileSet and Prefab consumers reject auxiliary
selection. Product field lookup never silently discards the inner identity. No narrower typed
`AssetProductRef` field codec or second walker was introduced.

Root collects complete required targets during effective source discovery, admits/deduplicates
their map entries before allocation, and checks `AssetDomain::contains_product` on actual
prepared values before publication. The predicate is a trusted nonallocating pure query. The
concrete bundle overrides auxiliary membership. `ProjectAssetCollection::get` remains a primary
query; the consumer explicitly looks up the enclosing source and passes the complete reference
to `TextBundle::get`. The real Caption now stores one Product AssetRef instead of an opaque
selector string. Its travel test adds missing-target refusal before valid replacement and
preserves current content. A separate predicate-panic regression checks bounded failure and
candidate reservation release.

Independent root review `01a18535-5445-76b2-af7d-51c399abe662` confirmed that cached
`ProjectContentSnapshot::prepare_editor_startup_scene` could admit an edited missing auxiliary
target into actual TrustedCompiled Play. The parent implemented revalidation against the same
prepared domain collections with a temporary content-budget ticket, the existing effective
reference collector and contained private batch membership. No allowed-target subset cache,
new importer execution or public erased store was added. The reviewer is writing the public
Session edit/Play regression in `compound_workflow.rs` and extending the copy fixture to actually
reference the copied source.

The first cross-crate build found a call to nonexistent top-level `PersistentFieldCodec`.
Canonical `encode_asset_reference_value` and `decode_asset_reference_value` are now small public
leaf APIs for actual handwritten codecs and field-edit payloads. The sealed field codec and
budgeted rewrite delegate to that same grammar. Remove the newly added unreachable Encoding
error instead of retaining a failure branch for the infallible encoder. Native consumers no
longer import derive internals for this purpose.

Core review `01a1853f-eee0-7d41-a574-dd4472d5537f` found that forced `deserialize_map` rejects RON
records emitted by the serializers, including ordinary Path AssetRef. That reviewer is fixing
the three identity record visitors, retaining sequence/unknown/duplicate rejection, and adding
JSON/RON regressions. Its exclusive write scope is Asset `identity.rs`, `product_identity.rs`
and the Asset manifest's RON dev dependency. The root lockfile still needs generated reconciliation.

Scene/native review `01a18542-87e6-7a72-b172-60a611b8b133` found that PrefabCopy correctly rewrites
a nested source to Product PRIMARY but its final closure loader only matches the preferred
destination spelling. The reviewer is fixing exact-first lookup with admitted stable destination
fallback and actual nested Prefab source regressions. Its exclusive scope is `prefab_copy.rs`
and its tests. Other asset/Reflect/Scene/native workers have frozen and closed. Do not format or
overwrite these active scopes before their source-ready report.

Initial Asset all-feature library passed 116/116, run
`394619f9-ea93-4253-bb7e-d98a665e49c3`. Initial Reflect library plus declared-product, nested,
field/property-limit and reference-visitor binaries passed 185/185, run
`60189202-a28c-4f78-bab2-5db5d14b5bd8`. These precede the public leaf-helper extraction and RON
correction, so they are not final qualification. Scene/native library build session 61750
terminated 101 at the missing codec API; that call has been corrected but not rebuilt. No Cargo
is running at this record boundary. Workspace fmt ran once before the latest parent corrections;
final root/fixture fmt and build/test/Clippy remain. Five edited documents' 41 local links pass.

Runtime-config review `01a18529-0ca1-7223-a691-a95d2715731a` recommends `nara_project`-owned
RuntimeConfiguration with existing section validation/lowering and product invariants. Reuse
root composition admission, retain source and base proofs separately, and bind the trusted base
digest/target/Schema to the original recipe before Runtime construction. Do not fabricate a
ProjectManifest/source receipt or let the catalog provide its own expected digest. Projection
must refuse a remaining source-relative `input.action_map`; explicit profile clearing is valid.
No Runtime config or catalog implementation has started in this unit. These constraints are
recorded in the ordered plan.

The latest user explicitly requires `unlink`. Verified current real `DirectoryCapability::unlink_file`
and reviewed artifact reclamation remain present; project-owned imported copies are outside
installation disposal authority. Prefab convert-to-local remains a distinct operation. No unlink
implementation or qualification scope was removed by this unit.

# Next Action

Receive the three active source-ready reports, inspect their diffs, reconcile lockfiles offline,
format after writers freeze, then run Asset/Reflect/Scene/native-owner tests and actual public
compound/Editor workflows serially. Fix actual failures without dropping strict assertions.
Review final code, check affected feature combinations/Clippy, capture final qualification and
create precise local Conventional Commits before ff main. Do not push. Continue the full
Runtime base/config/catalog plan after this dependency unit; keep the foundation Goal active.

# Citations

- [Ordered plan](../../../../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md)
- [Full foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Scoped product decision](../../../../architecture/adr/0122-scoped-imported-product-identity-and-authored-continuity.md)
- [Canonical field guide](../../../../guides/persistent-components.md)
- [Asset domain guide](../../../../guides/project-asset-domains.md)
