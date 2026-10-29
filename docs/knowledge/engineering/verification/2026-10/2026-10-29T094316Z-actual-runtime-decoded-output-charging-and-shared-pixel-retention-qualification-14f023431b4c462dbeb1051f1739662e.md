---
type: "Verification Evidence"
title: "Actual Runtime decoded-output charging and shared pixel retention qualification"
description: "Qualifies required concrete codec cost queries, upfront maximum admission and actual output refunds through compound data, many small Images, held large decode and last-owner native captures."
timestamp: 2026-10-29T09:43:16Z
record_id: "14f023431b4c462dbeb1051f1739662e"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md"
git_branch: "refactor/foundation"
git_commit: "a049e15"
---

# Verification

Continue the complete active foundation Goal from `be3b75c` on long-lived `refactor/foundation`.
The predecessor Image checkpoint is already in local main. Rust1.95.0, Windows11, PowerShell7,
serial Cargo, jobs1 and the existing root target apply, including independent workspaces.
This unit changes decoded-output accounting before stock TileSet/native residency. It does not
reduce the overall Goal or qualify the remaining first-party packaged delivery work.
Implementation is `a049e15`; subsequent evidence/ledger integration changes only documentation.

# Result

`RuntimeAssetCodec::runtime_retained_bytes` is required and bounded over the actual decoded value.
It counts inline product and owned modeled allocations, excludes encoded/base/Host-wrapper
storage, and allocates no product/query scratch. A bounded existing domain diagnostic can refuse
an unaccountable value. There is no compatibility default that silently charges a maximum.

The content Host admits the full codec work/output promise before invoking decode, then checks
complete Product membership and actual cost against both inline size and that original promise.
Only success refunds unused output and all temporary work through the existing DecodeLease.
Host wrapper storage excludes the already-counted inline product. The returned actual output
charge is inspectable through `RuntimeContentAsset::retained_bytes`; clones share one charge.
Independent decodes remain separately charged even for one Product. Failure, a query panic or an
underestimated promise cannot acquire post-allocation capacity or publish a value. Cost/Host
addition is checked; an overflowing domain query or excessive result refuses.

Image's concrete cost includes `size_of::<ImageAsset>()`, exact fixed-size pixel length, the
Arc allocation's Box slice handle and two counters. The Runtime decoder supplies no source or
opaque retention owner; querying such attached values refuses rather than guessing their cost.
TextBundle counts its entire compound map and cooked strings, not just the selected auxiliary
entry. Both remain logical ownership models, not allocator capacity, size classes or RSS.

Actual escaped CPU/content/prepared captures retain their original acquisition and its admitted
cost. Successful decode does not create a source lease, native publication right, generic
residency executor or second Scene transaction. The established native Scene/resource
failure/discard/commit workflows continue through their original owners.

# Evidence

| Scope | Actual result |
| --- | --- |
| Asset/Image all-feature library tests | 271 passed, one default skip; build1m28s/run17.481s |
| Root all-feature Runtime content catalog/budget selection | 30 passed,900 outside-selection skips; build1m52s/run3.574s |
| Two independent packages, eight selected binaries | 64 passed, no skips; build1m39s/run4.876s |
| Root/Asset/Image correctness Clippy | All features/all targets passed2m01s; existing style/large-error warnings remain |
| Both independent packages correctness Clippy | All targets/locked passed1m08s; existing warnings remain |
| Minimal feature checks | Asset/Image no-default, Image serde-only, root runtime-core+serde and runtime-2d+serde all passed |
| Final static checks | Root/independent formatting, changed-document links/fences and Git whitespace pass; memory validation passes with historical advisory warnings |

Nextest IDs: owners271 `cc61cf97-b0de-40b4-aa13-ade936708efe`; Host30
`ad5e1fed-9960-486b-8a4e-29205490b7eb`; independent64
`29f87505-9870-4197-8b99-b4f8fc5bb269`.

The owner command selects `nara_asset` and `nara_image`, all features, `--lib`. Root selects
`nara --all-features --lib -E 'test(runtime_content::)'`. Independent workspace selects
`nara-scene-module-consumer` and `nara-asset-domain-consumer`, and `runtime_image_delivery`,
`runtime_scene_assets`, `runtime_scene_delivery`, `runtime_base`, `bundle`, `compound_workflow`,
`workflow`, `import`, locked and single test thread. No full workspace, broader root or fresh
native Editor/GPU qualification is claimed for this accounting-only successor. Its predecessor
owns the earlier Image/GPU/native evidence; those counts are not reused as new execution.

The new Image consumer holds32 independent allocations across primary/auxiliary selections with
one captured base. Its cap equals one full maximum promise plus31 actual output charges and base
storage, below the old charge for two maximum outputs. The33rd request refuses before codec
entry, and returns only after the last acquisition/prepared clone releases the original output.
Actual decoder entry counting prevents a post-decode failure from impersonating upfront admission.

A separate actual4MiB decoded Image pauses at a channel gate. Competing acquisitions independently
exceed retained/work promises and refuse before decoder entry. Releasing the first decode refunds
unused output/work, then the second acquisition succeeds while the first actual output remains.
Existing atomic native resource/Scene publication and escaped pixel regressions also pass in the
same consumer. No unmeasured RSS or latency improvement follows from these logical boundaries.

Compound tests cover bounded multibyte/escaped strings, whole-product accounting, and one private
test codec's cost-query panic, typed refusal, over-promise result, below-inline result and
predecode below-inline promise. Each failure is followed by a complete normal acquisition at the
original exact full-decode cap. Existing wrong codec, membership panic, held concurrent decode,
same-base Scene acquisition and physical Scene output retention remain selected and pass.

## Independent Review And Integration

Darwin `01a186f6-b135-7851-ba3c-026fe4a53a50` owned Image cost/helper/tests; Zeno
`01a186f6-fc8f-74a1-8c75-aef137d0ba1e` owned the compound codec and external failure tests;
Bohr `01a186f7-37e6-77c3-a18e-386fa0c21984` owned the external Image consumers. Parent owned
the required trait, Host admission/refund, docs and all Cargo. Parent strengthened the paused
decode consumer from tiny pixels to a real4MiB output before its test binary compiled. All
workers froze before final formatting and are closed.

Beauvoir `01a186fc-a344-7953-9439-e88eaa7996c8` independently reviewed production admission,
cost honesty, panic/refusal, concurrency and drop ordering and found no confirmed P1/P2. Product
destruction precedes lease release both on failed local acquisition and in the successful shared
wrapper. The review is static evidence; actual executed bounds and recovery come from tests above.
Its source scope predates only test/doc formatting, not subsequent production changes.

Pinned read-only Bevy/Godot observations were added to the existing Image study: concrete upload
bytes and pixel length do not themselves supply Nara's complete CPU ownership/Host-wrapper cost.
No reference/vendor changes, new dependencies, lock rewrites, compatibility alias/reader, new
format version, migration guide, separate build output or remote publication occur in this unit.

# Follow-up

Keep the complete foundation Goal active. Continue source-independent TileSet descriptor/admission,
actual Image dependency/Required-edge and atlas/cell validation, then full first-party closed
source/processed Host, typed reads and native residency. Source facet removal, processed input,
target-build separation, CAS, R8/R9 and Nara-owned Editor UI remain open. A new decode still needs
its complete upfront maximum; successful output refund is not streaming, automatic eviction,
allocation interception or a guarantee that an arbitrary trusted codec reports honest costs.

# Citations

- [Ordered plan](../../../../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md)
- [Asset decision](../../../../architecture/adr/0007-asset-identity-and-import-pipeline.md)
- [Runtime codec contract](../../../../../crates/nara_asset/src/runtime_codec.rs)
- [Host acquisition](../../../../../src/runtime_content.rs)
- [Image Runtime codec](../../../../../crates/nara_image/src/runtime.rs)
- [Independent compound consumer](../../../../../tests/fixtures/module-consumer/asset-domain-consumer/tests/runtime_base.rs)
- [Independent Image consumer](../../../../../tests/fixtures/module-consumer/tests/runtime_image_delivery.rs)
- [Predecessor qualification](2026-10-29T090832Z-anonymous-and-processed-image-content-with-retained-native-publication-qualification-3e529af056d34a5caa2c12924c0c1014.md)
