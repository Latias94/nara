---
type: "Verification Evidence"
title: "Tracked local artifact builds and complete-group publication qualification"
description: "Finite typed compound and Image execution, current-input reuse, whole-group publication and original Runtime custody."
timestamp: 2026-10-30T23:16:10Z
record_id: "d3fcd08e15e04d45812ae09d7c81e944"
producer_id: "codex-foundation-main"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-31-local-artifact-build-plan.md"
git_branch: "refactor/foundation"
git_commit: "ec7fe1b"
---

# Verification

Qualify the finite local-build outcome under ADRs 0007/0103, not the whole Proposed 0087 graph
or complete target executable/package delivery. Starting branch and main were `ec7fe1b`;
the implementation was uncommitted while these checks ran. The final 15 changed Rust files,
ordered by repository path and hashed as path, NUL, bytes, NUL, have SHA-256
`8f981ea841dbe51b16187c8569a3f71bd3ead32eb82ccafdb992e5bbeaf63431`.
Rust 1.95.0, Windows 11, one Cargo at a time, CARGO_BUILD_JOBS=1 and the existing root target
were used, including the independent workspace. No native GPU trajectory ran for this unit.

# Result

The lower optional `SourceAssetBuild` facet preserves source-only settings requirements.
Root `asset_build` owns the explicitly selected typed Host, tracked capabilities/inputs and
local store. Actual normalized metadata is both hashed and executed. Raw/presence observations,
processed group/member/format/Product observations and actual Runtime reference demands remain
separate. Product, seed, closed recipe, byte digest and complete manifest retain distinct meaning.

Store publication uses existing filesystem capabilities and one original cooperative lock:
blobs, manifest, then a single index replacement. It verifies complete captured bytes before
reuse and preserves separate seed hints/successful-recipe history, including A-to-B-to-A
nondeterminism rejection. Success exposes actual replacement/lock/directory-sync receipts;
post-index failure/panic returns uncertain with that receipt. No CAS or sandbox claim follows
from file identity or advisory lock tiers. Each store has its own budget; escaped group clones
share their original lease, not a global root registry.

Independent compound and Image consumers execute real imports/cooks, two clean stores, current
input/settings/target changes, optional presence/deletion, fresh-process reuse and cache removal.
The three-blob Scene/Scene/bundle group exercises every publication stage and managed Runtime
travel through the retained pack. Actual Image pixels/encoding feed the existing catalogue,
Headless stock Image publication and escaped prepared pixels. This is CPU/Runtime custody
evidence, not GPU cooking, framebuffer readback or native presentation.

# Evidence

| Check | Actual result |
| --- | --- |
| Store owner tests, real capabilities/publication/limits | 20/20 passed; `c532d512-8f47-4a49-9b66-829513367f98` |
| Root + Asset all-feature libraries | 1252 passed, 4 existing skipped; `7cc3d6a0-e809-4389-bfe1-bc4f571fe8fd` |
| Final independent durable builds, Image cooks and existing import/bundle regressions | 57/57 passed; `850414e7-3307-4db4-8018-5ad79e44cc51` |
| Workspace all-feature/all-target correctness Clippy | Passed; existing non-correctness warnings retained |
| Two independent durable consumer binaries, Clippy `-D warnings` | Passed |
| Asset non-serde and root runtime-2d/serde minimal library checks | Passed |
| Root project-workflow-only library check | Passed |
| Root/independent formatting, whitespace and affected local documentation targets | Passed |

The final independent cohort includes 19 compound-build tests and 11 Image-build tests, plus
27 pre-existing import/bundle tests. It covers a real child-process hit without source execution,
actual compiled-tool digests, semantic continuity-set ordering, exact Product membership,
required/optional read errors, corrupt generated objects, clear-cache identity and old owners.
Store tests additionally exercise exact/one-short work/retention/encoded limits, refund/escape,
independent owners, true lock contention, changed lock binding, cleanup custody and strict wire
preflight. No existing budget threshold was widened.

Independent review identified and closed four execution P2s: output member limits incorrectly
applied to raw replay, path cloning before admission, stale metadata observation flags after a
miss, and discarded successful publication receipts. `InputSnapshot::Missing/Present` removes
independent optional file/digest/byte states. Revalidation maps actual content/length drift to
`Stale` while preserving authority/I/O errors. A proposed AfterIndex panic concern was ruled
out by the store's local catches, passing owner faults and a public facade regression.

Image review closed two P2s: flattening upstream definitions rejected a valid repeated cooker,
and tuple bindings could refund work before dropping decoded pixels on error. Current cook seeds
name only the current operation; exact processed group digests recursively bind upstream facts.
Private `WorkingValue<T>` couples decoded/encoded values with leases in safe field-drop order.
Public tests prove sequential same-cooker reuse and changed upstream implementation with equal
output bytes. That latter case deliberately models an upstream definition change using the real
captured member through public commit; it is not a separately rebuilt executable experiment.

Failure history is retained: the first production check found two String/AssetPath mistakes in
metadata paths; they were corrected without changing the shared path API. The first public run
passed 24/25 and exposed metadata growth classified as a read limit instead of stale input.
After correction, 25/25 passed. The final 57-test cohort includes all later ownership/recipe fixes.
The 1252-test root cohort preceded the final Image-only fixes; final consumer execution and
workspace Clippy cover those fixes. No unexecuted broader native result is inferred.

All four implementation/review lanes ended. Pauli closed execution findings; Huygens closed
store/Image findings after rereading the corrections. Kant independently reconciled the same
research ZIP with the current plan and found no missing work unit or required roadmap reorder.
All 91 archive manifest entries matched length/SHA-256; its ten research files were identical
to the existing imported versions. Supporting snapshots did not replace current owners.

# Follow-up

The bounded local-build R6b outcome is qualified. The complete foundation Goal remains active.
General dependency scheduling/reconciliation, remote cache, a VFS, portable crash guarantees,
reproducible executable selection, the packaged Player and full target delivery remain separate.
Rendering, editable Nara-drawn Editor UI, joint motion/execution and authored Schema/Prefab
evolution retain their existing complete consumer gates. No compatibility reader, erased
importer registry, global budget manager or migration guide was added.

This additive build unit changes no existing native view/input/cadence/retirement flow, so it
does not rerun the predecessor's GPU/native trajectories. No throughput, RSS, general streaming
performance, Linux/macOS or automatic hermeticity is claimed. A bounded NTFS compression pass
preserved artifacts and finished before further Cargo; it compressed five files and recovered
about 2.5 GiB without deletion. No push or PR is authorized or performed.

# Citations

- [Selected design](../../../../architecture/local-artifact-build-design.md)
- [Unit plan](../../../../plans/2026-10-31-local-artifact-build-plan.md)
- [Accepted finite contract](../../../../architecture/adr/0103-concrete-domain-owned-asset-import-execution.md#finite-typed-local-builds-2026-10-31-refinement)
- [Consumer guide](../../../../guides/project-asset-domains.md#finite-local-artifact-builds)
- [Pinned source study](../../../../research/2026-10-31-local-build-recipes-and-publication.md)
- [Research disposition](../../../../research/2026-10-30-engine-direction-reconciliation.md)
- [In-progress rationale](../../progress/2026-10/2026-10-30T213010Z-tracked-local-artifact-execution-and-complete-group-store-implementation-in-progress-610ba9175c1b41bebdaa3f8f0c86bd79.md)
