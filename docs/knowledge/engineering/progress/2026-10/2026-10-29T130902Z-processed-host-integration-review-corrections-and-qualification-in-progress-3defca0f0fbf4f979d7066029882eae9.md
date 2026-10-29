---
type: "Work Progress"
title: "Processed Host integration review corrections and qualification in progress"
description: "Captures actual partial verification, corrected publication association and metadata admission, native facet counterexamples and remaining qualification commands."
timestamp: 2026-10-29T13:09:02Z
record_id: "3defca0f0fbf4f979d7066029882eae9"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md"
git_branch: "refactor/foundation"
git_commit: "76a8571"
supersedes: "2c878dfe537f42eab9df31e120d1afb6"
---

# Summary

Continue the complete active foundation Goal and ordered processed-delivery plan. Local main and
refactor/foundation remain at 76a8571; this entire production successor is uncommitted. All source
writers are frozen. No push/PR, new branch/worktree, source rollback or vendor/reference changes.
Windows11/PowerShell7, Rust1.95.0, serial Cargo/jobs1/root target remain mandatory.

# Details

The first-party Source/Processed replacement is implemented in source: plan-owned exact codec
definitions/fingerprint, strict required catalog field, typed compound collections with complete
Product mappings, native Image/TileSet Required closure, closed startup/request contexts and
shared residency/publication/cleanup. Headless/desktop actions have processed constructors with
execution-only options. Independent managed-App Runtime Scene sources retain their existing
custody; hosted Runtime sources explicitly require the first-party residency transaction.
Default Asset/Image plugins contain no source resources; AssetSource/ImageSource and Watch are
explicit. Authoring Host explicitly initializes its real database/importer through its own driver.
The now test-only nara_asset->nara_tasks dependency was moved to dev-dependencies. Source record
and iterator callers, Reference Game reload test and current guides/ADRs were migrated without
compatibility aliases. All persistent Runtime formats remain version one.

Independent review found and resolved these concrete defects in source:

- Sprite still unconditionally refused auxiliary Products. The source-only refusal now requires
  an actual source database; native Runtime uses existing complete typed bindings. Added actual
  bound/unbound/wrong-type primary/auxiliary regression in nara_sprite.
- Equal codec fingerprints could process different family orders. Families now follow the minimum
  stable selected definition ID; members follow canonical catalog order. A permutation test covers it.
- Image residency checked revisions but not managed value/state association. A replaced empty
  Image store after retirement could cause commit_loaded StorageMismatch after Scene publication.
  ImagePublication now captures, validates and commits through AssetMutationBaseline. A real
  A->empty->new store->rejected A test preserves the complete baseline and recovers by returning
  the original store.
- Residency did not charge three complete live AssetServer clones. Owner cost queries now cover
  all paths, Products, anonymous/type claims and two staged binding-growth bounds before cloning.
  Three owner tests and a real processed-base small-budget/4096-unrelated-bindings test were added.
- Native cardinality independently requires exactly the actual Image/TileSet Product, rather than
  trusting lax codec aliases. TileSet native admission separately checks its actual Required Image
  edge, even if a codec omits inventory and another legitimate Scene edge acquires that Image.
- Graph work was reserved before Scene decode despite no graph allocations yet. Scene acquisition
  now precedes graph reservation; an exact-Scene-work-cap repeated preparation regression covers
  refund and sequential reuse.

Rawls added native lax-inventory/alias counterexamples plus a valid control in the independent
runtime_tileset_delivery fixture. He fixed its initial untagged Transform JSON by constructing
SceneComponentRecord from original admitted Schema/typed initial value. The independent manifest
now explicitly enables presentation-2d to run actual stock extraction and ImagePrepare without a
window/backend. An outdated wrong-codec test now expects CodecNotSelected during base open.

Actual results so far, preserving their pre-correction scope:

- Asset/Image/Watch all-feature library check passed35.10s; root all-feature library check passed14.35s.
- First focused root64:62passed,2failed (Sprite auxiliary refusal); run49db9dd7-0538-4494-a624-69c25ff6e908.
- After Sprite correction, processed startup5/5 passed;67f7e2c4-1c26-455a-b094-8e0f1acca8c0.
- Root full library960passed,4default skipped,386.195s;20399968-31b4-4459-8c5b-c90a6faa58a8.
  This predates the publication-association/cost/graph fixes and their new regression tests.
- Independent two packages/10binaries76:74passed,2failed,9.441s;c1681742-d412-48a9-b296-5531f83d7849.
  Passed the real processed compound CPU Host/cancel/last-good/multiformat workflow. Failures were
  the outdated Image wrong-codec expectation and malformed typed Transform fixture; both source-fixed,
  not rerun yet. The later native lax-wrapper tests were not part of that run.
- Root, independent and Reference Game fmt ran once; later root fmt covers review corrections.
  Independent fixtures need fmt again after Rawls's final native additions. No Clippy/native result yet.

At capture one Cargo command is still active, exec session53732:

`cargo +1.95.0 nextest run -p nara -p nara_asset -p nara_image -p nara_asset_watch -p nara_sprite -p nara_sprite_render --all-features --locked --lib --test-threads=1 --no-fail-fast --status-level fail --final-status-level fail`

It is recompiling the reviewed source. Log TEMP/nara-processed-final-owner-tests.log; no result yet.
Do not launch another Cargo, including cargo fmt, while it is active. Other TEMP logs:
nara-processed-independent-tests.log, nara-processed-host-all-lib.log, nara-processed-start-retest.log.

Only open agents: Socrates01a1878c-41a7-7f71-adb4-b7627de5c0b5 independently reviewing the new
AssetServer cost/metadata fix; Rawls01a18787-02bd-7343-a6de-1b4ca0e0e89f now read-only researching
the next ordered processed input-map unit. Their implementation files are frozen. Boole, Beauvoir,
Lagrange and Mencius are closed after delivery/review. No outstanding user approval.

# Next Action

Poll the active owner/root run to terminal; fix actual failures without relaxing invariants.
Then fmt the independent workspace and rerun actual independent binaries, including the two fixed
Image/TileSet binaries and new native counterexamples. Fresh source/compound Host coverage should
be preserved after the metadata correction. Run affected root public/plugin/image integrations,
Reference Game source task flow, correctness Clippy and minimal features. Qualify the three
selected native Windows/DX12 Editor trajectories under the existing procedure. All Cargo serial.
Review final diffs/evidence, record honest limits, make exact Conventional Commits and locally ff
main, then return to refactor/foundation. Do not mark the whole Goal complete: processed input map,
target build selection/packaged Player and later rendering/custom UI remain ordered work.

# Citations

- docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md
- docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md
- docs/architecture/adr/0007-asset-identity-and-import-pipeline.md
- docs/architecture/adr/0033-asset-import-and-render-resource-preparation-seam.md
- docs/architecture/adr/0103-concrete-domain-owned-asset-import-execution.md
- docs/verification/native-editor-qualification.md
