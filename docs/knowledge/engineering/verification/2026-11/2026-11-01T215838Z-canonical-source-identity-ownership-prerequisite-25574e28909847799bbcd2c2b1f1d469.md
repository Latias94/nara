---
type: "Verification Evidence"
title: "Canonical source identity ownership prerequisite"
description: "Pure source IDs paths and document keys move to Identity; Asset retains metadata comparison, with owning public independent and portable evidence."
timestamp: 2026-11-01T21:58:38Z
record_id: "25574e28909847799bbcd2c2b1f1d469"
producer_id: "codex-foundation-main"
run_id: "prefab-extraction-representation-20261102"
related_plan: "docs/plans/2026-11-02-prefab-extraction-plan.md"
git_branch: "refactor/foundation"
---

# Verification

Pure identity ownership prerequisite of the complete
[CP2 extraction unit](../../../../plans/2026-11-02-prefab-extraction-plan.md), based on
`8b25d838963e604c2baaf914e676a0bed9efeb30`. The containing commit identifies the exact replacement.
Windows11, Rust1.95.0, serial Cargo/jobs1 and the existing root target. No vendor changes or remote
publication. This prerequisite neither implements extraction nor closes the full foundation Goal.

# Result

Identity owns canonical StableAssetId, AssetSourcePath, path validation and SourceDocumentKey
representations with unchanged wire formats and modeled costs. Asset and Scene export the same
canonical values. Identity depends on neither Asset nor App; metadata association is now
AssetRecord::validate_source_document_key. Asset retains authored aliases, explicit scopes,
catalogue case policy and metadata/file authority. Scene retains source/Prefab interpretation.
The old Scene key/wire files and duplicate Asset definitions are removed, with no aliases/readers
for a previous representation.

An independent consumer constructs Identity values, passes them through Asset metadata validation
and actual ResolvedPrefabSource association via the Scene facade without conversions. Existing
alias, repeated-source and durable identity consumers exercise the same canonical values.

# Evidence

| Check | Result | Scope |
| --- | --- | --- |
| Final Identity/Asset/Scene all-feature libraries | 772 passed, zero skipped | `1e71113a-20a4-4995-8c28-d5d9652a6898` |
| Public Prefab source association | 3 passed, zero skipped | `5cd888c0-55f0-47d4-b628-236beeafd12a` |
| Independent source locations, Prefab sources and durable identity | 15 passed, zero skipped | `822a9fe0-b1a7-419e-9a60-c375314ac498`; includes direct canonical Identity→Asset→Scene consumption |
| Root source capture, upgrade inventory/recovery and late association | 220 passed, 1133 excluded | `34cb4f53-152e-41ee-a68b-df0b39823a50`; no-default project-workflow-cli/tooling/presentation-2d union |
| Asset/Identity documentation | 4 passed | Three actual Asset compile-fail controls and one Identity path example |
| Owner all-feature library compilation | passed | Identity, Asset and Scene |
| Root all-feature library compilation | passed | Includes all five moved metadata-comparison consumers |
| Root no-default-feature library compilation | passed | Minimal facade remains independent |
| Owner no-default-feature library compilation | passed | Identity, Asset and Scene; existing feature-specific warnings remain |
| wasm32-unknown-unknown all-feature library compilation | passed | Identity and Asset, including relocated getrandom wasm_js configuration; compile-only |
| Owner correctness Clippy, all features/all targets | passed | Identity, Asset and Scene with `-D clippy::correctness`; ordinary existing/style warnings remain |

The initial owner run `169efa4e-ef63-4172-a343-94a8693dc76d` passed772 before the path-join
preallocation correction; only the final run above is attributed to the final helper. The
SourceDocumentKey retained formula is unchanged, with checked string overhead and overflow.
AssetSourcePath's separate capacity accounting remains unchanged. The join helper reserves the
complete result once; complete-segment scope suffixes cannot silently turn an internal validation
failure into an ordinary out-of-scope result.

All19 tracked Identity-consumer lockfiles were resolved serially offline and their original
package/version/source/checksum facts compared with a temporary baseline. Every existing package
fact is unchanged. One stale schedule fixture needed serde_json, itoa and zmij records because
unchanged nara_project already requested Asset serde. The first offline resolution selected newer
cached serde_json/zmij; these newly added records were pinned explicitly to the original root
1.0.150/1.0.21, with itoa1.0.18. Final added facts match the original root exactly. This is neither
a dependency upgrade nor a claim that the stale lock originally described that complete feature
graph. Other lockfile changes describe only the owning dependency edges.

Final root and independent module-consumer formatting checks and `git diff --check` passed.
Current architecture/plan documents had1203 local file links checked before final qualification
links were added; final links point to this record. Engineering memory validation passed with
existing legacy warnings and its derived rollups were current. No broad style-warning cleanup
or unrelated native rerun was included.

## Independent Review

Popper `01a19908-419b-71c1-b8c7-c96c13f7965f` statically reviewed the complete moved types,
serde/features, exact metadata comparison, cost formulas and facade identity. It found three
remaining Root calls to the removed method in source capture, upgrade inventory and recovery.
All were changed to the AssetRecord method with identical error mapping. Its rereview found no
assigned P1/P2 and no remaining old call across crates/src/editor/tests/reference-game/samples/
examples. The final Root build/tests above include those fixes. Review was read-only and the
reviewer is closed.

The complete CP2 representation plan has a separate read-only adversarial review by Maxwell
`01a198fb-d9f2-79c2-b486-e09dcc662673`, with no assigned P1/P2; it does not substitute for source
model implementation or production tests. Alternative recommendations, the ordinary mixed-order
counterexample and the actual Rust generic inference probe are captured in the predecessor.

# Follow-up

The source-model replacement remains open: explicit bound-root/forest topology, source-owned
typed placements, inline authored references, contextual defaults and closed flat compiled output,
then complete declaration/occurrence transformation and actual project publication/recovery.
Native must reject authored-only references after defaults and before all codecs/World mutation.
No native rendering/input behavior changed in this pure-value prerequisite, so it makes no new
native Editor/platform execution claim. Wasm compilation does not prove browser execution.

# Citations

- [Representation and implementation checkpoint](../../progress/2026-11/2026-11-01T212648Z-prefab-extraction-representation-selected-after-root-ordering-and-reference-value-counterexamples-044888fa29f7401b91a5904e8a230cb1.md)
- [Identity contract](../../../../architecture/adr/0058-stable-runtime-identity-and-entity-references.md)
- [Asset contract](../../../../architecture/adr/0007-asset-identity-and-import-pipeline.md)
- [Independent consumer](../../../../../tests/fixtures/module-consumer/tests/source_locations.rs)
- TEMP logs: `nara-source-identity-final-owner-tests.log`,
  `nara-source-identity-public-association-tests.log`,
  `nara-source-identity-root-capture-upgrade-tests.log`, `nara-source-identity-doc-tests.log`,
  `nara-source-identity-minimal-check.log`, `nara-source-identity-wasm-check.log`,
  `nara-source-identity-independent-tests.log` and `nara-source-identity-root-check.log`.
