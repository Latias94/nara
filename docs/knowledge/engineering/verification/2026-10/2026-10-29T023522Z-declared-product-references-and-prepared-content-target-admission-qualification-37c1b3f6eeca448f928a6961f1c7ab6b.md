---
type: "Verification Evidence"
title: "Declared product references and prepared content target admission qualification"
description: "Qualifies complete product references through shared Schema traversal, strict prepared-product checks, source copying and cached compiled Editor Play refusal and recovery."
timestamp: 2026-10-29T02:35:22Z
record_id: "37c1b3f6eeca448f928a6961f1c7ab6b"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md"
git_branch: "feat/declared-asset-product-references"
git_commit: "c3c6d33"
---

# Verification

Continue the full active foundation Goal from `main@3e3f962`. This unit closes the actual
declared-product reference prerequisite of the Runtime base plan. It does not implement
source-free delivery or complete R6/R7. Commands use Rust 1.95.0 on Windows 11, serial Cargo,
`CARGO_BUILD_JOBS=1` and the existing root target, including independent fixture builds. No
remote publication, vendor access or reference-checkout mutation occurs.
The reviewed implementation is committed locally as `c3c6d33`.

# Result

`AssetRef::Product(AssetProductRef)` preserves complete source-scoped primary/auxiliary identity
through the existing AssetRef Schema leaf, default/nested traversal and rewrite. Path and StableId
remain primary selectors. Variant accessors do not silently lower Product to its enclosing source.
Asset database lookup proves only source identity. Existing primary-only typed handles, Image,
TileSet and native Prefab consumers reject auxiliary selection before reservation/publication.
Explicit Prefab PRIMARY requires a matching actual stable source proof.

Three strict identity record visitors support their own JSON/RON encodings, arbitrary field
order and named RON records while rejecting unknown/duplicate/missing fields, positional arrays,
noncanonical product UUIDs and nil identities. The canonical component-value encoder/decoder are
small public leaf functions used by handwritten codecs and field-edit payloads. Derive and
budgeted rewrite delegate to that same grammar. No narrower AssetProductRef field codec, second
Schema walker or compatibility reader is added.

The content Host collects required targets once during effective source discovery, admits
temporary collection/map work before allocation and deduplicates complete identity. After the
real domain prepares a value, `AssetDomain::contains_product` checks its selected targets inside
the existing budget/panic boundary. It is a trusted nonallocating pure query, defaulting to
PRIMARY. The bundle supplies auxiliary membership. Missing targets prevent snapshot publication.
`ProjectAssetCollection::get` remains a primary query; compound consumers explicitly query the
enclosing source, then pass the complete reference to their own product API.

Cached TrustedCompiled Editor Play rechecks edited effective selections against those same
prepared products. Its temporary content ticket and contained private batch query preserve the
existing owner and resource generation. It does not reimport source or restrict edits to a cached
list of formerly selected IDs. The public Session regression starts valid Play, stops, edits to
a missing auxiliary target, observes exact preparation refusal with unchanged document and
escaped resources, then changes to another existing unselected product and starts successfully.

Both Scene export policies retain complete Product values including PRIMARY. Unknown payload,
missing Adapter, conversion provenance and history remain lossless. Prefab closure copying
remaps authorized enclosing stable identities without rekeying internal products; complete
mapping takes precedence and destinations without stable identity refuse. Its final loader
accepts a rewritten nested PRIMARY via the actual destination stable key even when its preferred
lookup spelling is Path or StableId. Existing destination admission prevents ambiguity.

The actual two-Scene consumer uses one persistent Product AssetRef instead of an opaque selector
string. It rejects a missing product before travel, preserves current resources and then commits
the valid Scene. A copied Scene actually selects `(copy_id, FIRST)` through public `load_scene`
and strict Check. A domain membership panic remains bounded and releases candidate reservations.
The current frontend's scalar draft displays/edits complete identities with canonical wire
helpers; this does not select a different future custom-drawn Editor UI.

# Evidence

| Scope | Actual result |
| --- | --- |
| Final Asset all-feature library, including JSON/RON record correction | 122 passed, no skips; build 32.97s, run 5.247s |
| Final Reflect library and five reference/derive integration binaries | 185 passed, no skips; build 7.00s, run 6.950s |
| Scene/Sprite/Tilemap/UI all-feature libraries | 632 passed, no skips; build 2m22s, run 39.041s |
| Scene source/format/expansion/history/conversion/export and Editor draft integration | 70 passed; 753 outside-selection skips; build 1m36s, run 3.314s |
| Initial broad root library and five content/Editor binaries | 973 passed, four failed, four ignored; build 5m01s, run 389.237s; all failures subsequently corrected as described below |
| Focused corrected root/source-boundary cases | 24 non-budget cases passed in the 33-case correction run; its five failures came from the too-small temporary test allowance described below |
| Final budget boundaries and adaptive-reservation regression | Nine passed, no skips; build 9.01s, run 3.552s |
| Final independent resource and Editor consumers | 46 passed, no skips, six binaries; build 16.68s, run 5.823s |
| Focused actual compound/cached Editor workflow before the final joint run | Five passed, no skips; run 0.761s |
| Root, six lower owners and Editor, all-feature/all-target correctness Clippy | Passed; 2m43s; existing non-correctness warnings retained |
| Both independent packages, all-target correctness Clippy | Passed; 45.95s; existing warnings retained |
| Asset/Scene no-default library check | Passed; 6.96s |
| Root no-default `runtime-2d,serde,tooling` library check | Passed; 16.45s; existing feature-dependent warnings retained |
| Independent Reference Game default-feature all-target check | Passed; 1m29s; existing warnings retained; compilation only |
| Pure domain package library and normal dependency boundary | Passed; 2.30s; no root/Scene/2D/Editor normal dependency |
| Root, independent fixture and Reference Game formatting | All passed |

Asset: `7bfd88d5-92ed-45f0-b7b2-f91153f2b412`; Reflect:
`6c4ae88a-b193-452d-8875-544d5b0260dd`; four owner libraries:
`87269f39-83ba-4b7f-b89e-4919ebc8d79f`; Scene/Editor integration:
`419c0bbb-f758-4ccb-9718-6dc055996b68`; initial root:
`8bf186a1-9557-4940-b30a-a50273f7ffdb`; focused correction:
`fbcfd7dd-64d6-41fa-99aa-6927bcf03e93`; final budget:
`fbf8f70d-dfd0-4d01-b200-9f7222114389`; independent final:
`20a90ee1-611e-409d-be11-aaf902dbead9`; focused compound:
`dcc8cea5-cdc0-4baa-a17b-6a8e8ccf6ec3`.

The final independent command is:

```text
cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml
  -p nara-asset-domain-consumer -p nara-editor-panel-module-consumer --locked
  --test bundle --test compound_workflow --test import --test workflow
  --test source_import --test compound_import --test-threads=1 --no-fail-fast
  --status-level fail --final-status-level fail
```

## Review Corrections And Failed Approaches

Independent reviewer `01a18535-5445-76b2-af7d-51c399abe662` found the real cached Editor Play
target gate omission and authored its public regression. Its final source review confirms the
prepared-product fix. Reviewer `01a1853f-eee0-7d41-a574-dd4472d5537f` found forced map decoding
incompatible with emitted RON records and implemented strict symmetric visitors and six record
regressions. Reviewer `01a18542-87e6-7a72-b172-60a611b8b133` found rewritten PRIMARY closure
lookup rejection and implemented the actual stable-destination fallback and nested-source tests.
All reviewers froze their writes before final formatting/testing.

The first Scene/native build found a nonexistent top-level PersistentFieldCodec call. Use the
canonical leaf APIs rather than importing derive internals; remove the newly added unreachable
Encoding-error variant. The following Reflect build exposed a test's implicit trait import;
its explicit test-only import preserves the derive contract comparison. Early independent
compilation also exposed the frontend's old exhaustive two-variant encoder; canonical helpers
and complete scalar text drafts replace it. Final selected suites cover these changes.

The first broad root run's new stable-product test encountered deliberately invalid unreferenced
metadata used by its existing path-laziness proof. Preserve that original proof, then write valid
metadata before requesting an explicit stable product, which requires the complete identity
catalogue. The unknown-component assertion expected a retired migration diagnostic; preserve the
actual unknown-component ID and no-publication assertion.

The other two initial root failures assumed default compiler spare reservation was irreducible
required work. The production compiler intentionally lowers that allowance to actual shared
Host capacity. Exact/one-byte-short tests now use a 1 MiB compiler allowance below their fixed
decode peak, while a new default-limit regression proves safe shrinking. The first 64 KiB test
allowance was below the fixture's actual roughly 222 KiB compiler work and correctly refused;
the final nine-case suite passes without changing production limits or removing refusal checks.

The first 46-test independent run passed 45 and failed the new cached Play test before reaching
Play: generic property reads require authoring admission, which the native TrustedCompiled
Session does not possess. Use that mode's public navigation and Inspector model to observe its
exact source field instead of widening authority for the test. The final real Play refusal,
unchanged field/frame/resources and SECOND recovery all execute successfully. A panic during the
failed earlier test interrupted its normal explicit Close; do not present that abort as a product
buffer-overflow finding. Final execution completes the existing close owner.

# Follow-up

Continue the complete ordered Runtime base/config/catalog implementation. `nara_project` owns
admitted profile-resolved Runtime values; source-only input paths must refuse rather than
disappear. Bind a trusted expected base digest, target and selected recipe/Schema before Runtime
construction. Startup and later Scene requests use the same immutable base, with no source,
metadata or cache fallback. Preserve source and base proofs separately; do not fabricate source
lineage or accept a catalog's self-supplied expectation.

Actual runtime-only startup/later Scene loading, executable selection, cooked Scene format,
local CAS/group publication, remote services and DLC remain open. This unit does not rerun native
window/input/DX12 or built-executable export qualification and establishes no new other-platform
claim. Existing real filesystem unlink and reviewed tool-artifact reclamation are unchanged;
imported project sources remain outside installation disposal authority. No migration guide,
obsolete reader or compatibility alias is added. Keep the full foundation Goal active.

# Citations

- [Ordered delivery plan](../../../../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md)
- [Full foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Scoped product decision](../../../../architecture/adr/0122-scoped-imported-product-identity-and-authored-continuity.md)
- [Domain guide](../../../../guides/project-asset-domains.md)
- [Persistent component guide](../../../../guides/persistent-components.md)
- [Independent complete workflow](../../../../../tests/fixtures/module-consumer/asset-domain-consumer/tests/compound_workflow.rs)
- [Initial integration continuity](../../progress/2026-10/2026-10-29T013709Z-declared-product-reference-integration-and-cached-editor-play-admission-in-progress-0d7fa57efb5843c3a54807a0256bed32.md)
