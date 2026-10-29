---
type: "Verification Evidence"
title: "Compound product continuity and independent Runtime format qualification"
description: "Records scoped product identities, explicit authored rebind, bounded processed text format, actual two-Scene source consumption and predecessor-aware Editor publication."
timestamp: 2026-10-29T00:40:54Z
record_id: "a2d6d4837fd44218b7cb87f67cb38063"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
git_branch: "feat/compound-asset-runtime-facets"
git_commit: "9ba8624"
related_plan: "docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md"
---

# Verification

Continue the full active foundation Goal from `main@97f4712`. This unit implements the named
compound text consumer and lower Runtime format/identity boundary, not complete R6a/R7. All
commands use Rust 1.95.0 on Windows 11, serial Cargo with `CARGO_BUILD_JOBS=1` and the existing
root target, including the independently declared fixture workspace. No remote publication,
vendor access or reference checkout edits occur.
The reviewed implementation is committed locally as `9ba8624`.

# Result

`AssetDomain` names a typed CPU family independently of its source or Runtime operations.
`SourceAssetImporter` retains settings and source preparation; `RuntimeAssetCodec<P>` supplies
explicit format kind/version, conservative encoded/work/retained limits and pure encode/decode.
Typed collection lookup requires the family rather than source capability. Private Host storage,
generation leases, source receipts and Scene publication remain under their existing owners.
Source-only implementations do not acquire Runtime support and Runtime-only implementations need
no source/settings facade. Separate codec types can serve one family's product.

`ImportedProductId` distinguishes primary from non-nil auxiliary UUIDs. `AssetProductRef` combines
one non-nil source ID and product ID, with strict discriminated wire forms and canonical UUID
text. Kind, name, path, artifact digest and runtime handle are not identity fields. ADR 0122 adopts
this narrow semantic slice; larger ADRs 0083/0087 remain Proposed outside the extracted scope.

The independent `consumer.text-bundle` parses bounded raw rows without intrinsic IDs and one
auxiliary suffix through the scoped reader. Its authored settings contain complete row assignments
bound to exact raw bytes plus bounded known-retired IDs. Initial authoring supplies IDs explicitly.
Rebind consumes `Continue`/`New` decisions, never guesses names/order/content or allocates cache
identity, retains old retirements and retires removed active IDs. Changed bytes require rebind;
equal-byte edits remain indistinguishable. The current lineage does not prove unlimited history.

The concrete format-1 `consumer_text_bundle` Runtime codec contains only cooked names/text and
stable references in product-ID order. It validates envelope, shape, size, entries, identities,
ordering, strings and semantic limits. Its input has no source, sidecar, suffix or cache reader.
Tests drop authoring inputs before decode and cover exact maximum multibyte text and JSON escaping.

Editor metadata Reimport now admits the actual previous leaf, retains that candidate and asks its
selected facet to validate previous-to-candidate settings in the existing worker before source
preparation. Two typed settings work allowances precede admission; the extra work releases before
ordinary parsing. Raw candidate/predecessor and encoded transport are reserved before task spawn.
Pending and a concrete queued-input owner share one lease, with inputs dropping before their
lease on success, unwind or cancelled disposal. Original source and metadata object/digest fences,
physical return, file cleanup and uncertain-publication behavior are retained.

The actual external Editor imports two products, Reimports removal into retirement, freshly
reopens, then rejects an otherwise-valid candidate that erases retirement. Dispatch is explicitly
Started, the exact domain transition diagnostic is observed and old metadata bytes remain equal.
Source-only Check/Play and actual two-Scene travel consume separate selected entries from one
bundle through the existing Scene owner. Source move/rebuild preserves products; copied source IDs
collide until the copy receives a new enclosing identity. Stale continuity rejects strict Check.

# Evidence

| Scope | Actual result |
| --- | --- |
| Asset full all-feature library | 107 passed, no skips; build 5.58s, run 6.080s |
| Final root/Asset focused libraries | 125 passed; 880 outside-selection skips; build 2m31s, run 9.419s |
| Final independent resource and Editor consumers | 44 passed, no skips, six binaries; build 25.94s, run 5.423s |
| Final compound project/family cases after the Runtime-only family assertion | Three passed, no skips; build 38.11s, run 0.394s |
| Root/Asset all-feature/all-target correctness Clippy | Passed; 1m35s; unrelated warnings retained |
| Both independent packages, all-target correctness Clippy | Passed; 50.56s; unrelated warnings retained |
| Asset library without default/serde features | Passed; 1.59s |
| Root no-default `runtime-2d,serde,tooling` library | Passed; 27.55s; warnings retained |
| Pure compound/source/Runtime package library | Passed; 2.28s; normal dependency tree excludes root/Scene/2D/Editor |
| Workspace and independent fixture format checks | Passed |

Full Asset library: `96148e70-62e2-4ca0-8ee2-6f1a6ee138e3`.
Root/source-import/source-Export/Asset focused cohort:
`02b8a344-6a81-40f8-96f7-fa199d6fb2bc`:

```text
cargo +1.95.0 nextest run -p nara -p nara_asset --all-features --locked --lib
  -E 'package(nara_asset) | test(source_import_refresh) | test(source_asset_import) | test(project_workflow::export::source_tests)'
  --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail
```

Final independent consumers: `3285c340-ca4d-48ec-ad51-d84fd455af36`:

```text
cargo +1.95.0 nextest run --manifest-path tests/fixtures/module-consumer/Cargo.toml
  -p nara-asset-domain-consumer -p nara-editor-panel-module-consumer --locked
  --test bundle --test compound_workflow --test import --test workflow
  --test source_import --test compound_import --test-threads=1 --no-fail-fast
  --status-level fail --final-status-level fail
```

This includes 19 bundle cases, three actual compound project workflows, six prior pure input
cases, seven prior source workflows, eight prior Editor import cases and the actual compound
Editor transition. Root's new six transition cases additionally prove no source parsing/read on
rejection, successful publication, bounded panic, replaced source/metadata refusal, pre-decode
work refusal and queued-retention cancellation/drop behavior. Final compound project run
`990e4f71-1597-46d2-a65d-46672d7b15cc` additionally queries a family with the same Product but no
source importer and confirms it cannot retrieve another family's collection.

## Reviews And Failed Approaches

Read-only design reviewers `01a184f5-2e49-78d1-90df-5bbeb0dd32a4` and
`01a184f5-3072-7463-97c2-e53e8d685aeb` challenged source-coupled product types and candidate-only
continuity validation. Their concrete counterexamples motivated the family/codec split, exact
raw-byte binding, explicit decisions, real predecessor hook and limited history claims.

Core reviewer `01a18503-4f4c-7212-8934-b8fe489655c0` found no residual scope/equality/wire/codec
or budget contract defect. Initial core tests exposed tagged unit `Primary` accepting extra
fields despite the enum's unknown-field attribute; the strict empty struct variant fixes it.
Reference source text also receives strict canonical validation. The full Asset run covers those
corrections. No alternate version reader is retained.

Bundle reviewer `01a18510-8111-7523-ae4d-f8636860ef31` found no actual data correctness flaw.
Its maximum-value regression suggestion became the multibyte/control-character round trip in the
final bundle tests. Source digest is revision evidence only, suffix changes affect values rather
than product identity, and concrete parsing checks limits before constructing unbounded data.

Editor reviewer `01a18518-b405-7d51-aa8e-b028dda13d8a` found two P2 defects in the new external
test, with no confirmed production regression. Run `3967961e-c40d-484d-b2e3-48ac3c978f4d` passed
42 of 43 and reproduced the first: the initial asynchronous Open had no frame, but the test
unwrapped it. Use public `Application::advance` across lifecycle states. The second accepted any
terminal failure; assert dispatched Started and the exact
`consumer.text-bundle.transition-invalid` diagnostic along with byte equality. The reviewer
confirmed both fixes. Final 44-test execution includes them and the additional copy-identity case.

The first compound integration build used nonexistent `SceneProductTransactionLimits::default`;
its fixture now supplies actual explicit limits through `new`. This was a fixture compile error,
not a product failure. The first 33-test source/bundle run
`fc19e8b9-79e0-42d4-be0a-f1620e733fce` passed before later boundary and Editor cases were added.

# Follow-up

Continue the full ordered plan with admitted Runtime configuration, immutable base catalog and
both startup/later Scene acquisition. Decode-only round trips and source-backed two-Scene Play
do not establish source-free delivery. The current Caption declares a bundle `AssetRef` and a
consumer-owned product selector; this does not qualify general Schema product-reference traversal
or strict Check of arbitrary subproduct targets. That actual reference boundary remains an
integration obligation for the Runtime catalog workflow.

Target executable selection/cook/optional runner separation, source/cache-inaccessible execution,
actual target/schema/digest refusal, last-good delivery candidates, local CAS/group publication,
server stripping, remote services and patch/DLC retain their own scope. Existing single-text
source format remains a distinct simpler consumer, not a legacy reader for the compound format.
No source format compatibility layer, migration guide or custom-drawn UI qualification is added.
No native widget/input or built-executable export qualification is rerun or broadened by this
UI-neutral worker/format unit; the actual Editor evidence is the window-free Application path.

# Citations

- [Ordered plan](../../../../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md)
- [Full foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR 0122](../../../../architecture/adr/0122-scoped-imported-product-identity-and-authored-continuity.md)
- [ADR 0103](../../../../architecture/adr/0103-concrete-domain-owned-asset-import-execution.md)
- [Public source/Runtime facet guide](../../../../guides/project-asset-domains.md)
- [Independent bundle](../../../../../tests/fixtures/module-consumer/asset-domain-consumer/src/bundle.rs)
