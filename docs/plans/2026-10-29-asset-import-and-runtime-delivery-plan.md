# Asset Import Ownership And Processed Runtime Delivery

**Date:** 2026-10-29. **Status:** Source/import, compound continuity/format and declared-product admission qualified; Runtime catalog delivery open.
**Baseline:** `13a2744`.
**Parent:** [Foundation rebaseline](2026-10-23-foundation-architecture-rebaseline-plan.md), R6/R7.

## Problem

At the baseline, the compiled source-domain parser imports the root facade to name its input,
error and reservation types. Expressing a pure text importer therefore requires Scene and 2D
features. Registered project domains also rely on source preparation as their only Runtime input.
Source-carrying Export
correctly runs without the development directory, but still reads package-local source and
authoring metadata during startup and later Scene acquisition.

The [delivery audit](../knowledge/engineering/subagents/2026-10/2026-10-28T225432Z-source-carrying-export-still-lacks-an-independent-verified-runtime-content-entry-b9156a4b7e2043f09f3ab41b78cf3bf8.md)
identifies these distinct gaps. Build-machine Cargo and an explicit source-carrying mode remain
valid. Preserve actual filesystem unlink and reviewed artifact reclamation; imported project
copies remain outside installation disposal authority.

## Ordered Development

1. Move the executing source/import contract into `nara_asset` under its existing serde feature.
   Keep typed settings/product, immutable primary bytes, bounded auxiliary reads and domain
   diagnostics. Preserve the caller's concrete read errors. Migrate Check, source-backed Play,
   Editor metadata validation, source Export and independent consumers, then remove the root-owned
   parser protocol without aliases. Root retains source receipts, aggregate budgets, private
   heterogeneous storage, task custody and Scene publication.
2. Extend the independent text package with a compound text bundle and a concrete processed
   Runtime codec. Exercise two persistent subproducts and an auxiliary input. Separate product
   continuity from display name, ordering, content digest and runtime handle. If the source lacks
   intrinsic stable keys, require author-persisted continuity with explicit ambiguity handling;
   generated cache never supplies identity authority. Do not freeze a universal product executor
   or extend every asset reference before the complete consumer establishes its requirements.
3. Add one verified immutable local base catalog containing declared Runtime configuration,
   startup Scene, later Scene and processed resources. Use the existing Scene owner for a declared
   pass-through format or a specifically admitted Runtime representation. Startup and subsequent
   Scene requests capture the same base authority. Missing Runtime support or catalog failure
   refuses without source/import-cache fallback. Preserve existing transactional Scene commit and
   last-good products.
4. Separate the starter's fixed Cargo bins/profile/features and local acceptance runner from
   generic executable selection. Bind target artifacts and content compatibility explicitly.
   Foreign-target build-only evidence stays distinct from actual target execution.

Each step is a reviewable dependency unit, not completion of all R6/R7. Local CAS/group publication,
remote services, patch/DLC and universal all-platform qualification remain later work in the
foundation plan.

Step 1 is implemented as `c352614`, with independently consumed parsing, Check/Play/Scene and
Editor metadata workflows. The [qualification](../knowledge/engineering/verification/2026-10/2026-10-28T233934Z-source-import-contracts-below-project-hosts-with-typed-read-errors-e617bfdd9a104fb69bdf3a20936f7d57.md)
records actual tests, the corrected held-parser fixture, dependency proof and limits.

Step 2 has its [bounded qualification](../knowledge/engineering/verification/2026-10/2026-10-29T004054Z-compound-product-continuity-and-independent-runtime-format-qualification-a2d6d4837fd44218b7cb87f67cb38063.md)
under [ADR 0122](../architecture/adr/0122-scoped-imported-product-identity-and-authored-continuity.md).
The reviewed implementation is `9ba8624`.
The product family marker no longer implies a source importer. A separately selected Runtime
codec can serve that family's product. The compound consumer binds an explicit row assignment to
exact primary source bytes; its Editor facet also validates previous-to-candidate settings through
the actual metadata publication worker. A pure helper alone cannot establish that transition.
Startup/later Scene source-free catalog admission remains step 3.
That predecessor consumer used an ordinary declared bundle `AssetRef` plus a concrete text
selector. Its codec round trip alone did not qualify Schema traversal or required-target validation.

The Runtime catalog step first replaces that opaque selector with a complete declared product
reference in the existing asset-reference grammar. Reflect uses its existing asset leaf traversal
and rewrite, while the project Host validates the selected product against the actual prepared
domain value. Native primary-only stores and Prefab-source loading refuse auxiliary selections
instead of silently loading their enclosing primary. Copying a source remaps its enclosing source
identity and preserves its internal product-ID pattern. This reference integration is a
prerequisite of the named catalog consumer, not completion of source-free delivery.
This prerequisite is implemented as `c3c6d33`, with complete nested/default traversal and rewrite,
strict JSON/RON records, primary-only refusal, actual prepared-domain target checks, source-copy
scope and cached compiled Editor Play refusal/recovery. The
[qualification](../knowledge/engineering/verification/2026-10/2026-10-29T023522Z-declared-product-references-and-prepared-content-target-admission-qualification-37c1b3f6eeca448f928a6961f1c7ab6b.md)
records independent consumers, review corrections, actual verification and limits.

### Runtime Base Implementation Order

After the declared-product prerequisite, establish these owners in dependency order:

- `nara_project` admits a profile-resolved Runtime configuration without authoring paths,
  source identity, schema-bundle paths or import settings. Reuse its existing section validation,
  lowering and product invariants; do not deserialize an effective value and bypass admission.
  Projection refuses a remaining source-relative `input.action_map` until a concrete bounded
  input resource has its own Runtime representation; explicit profile clearing remains valid.
  Do not silently omit a requested input map.
- Composition consumes the same admitted Runtime settings for source and catalog entry. Keep
  source manifest/root lineage in the source candidate. Do not manufacture a `ProjectManifest`
  or source lineage for processed delivery.
- Bind the immutable base to a trusted expected catalog digest, target and this executable's
  selected recipe/Schema before constructing the Runtime. A catalog cannot authorize itself by
  supplying both its actual and expected digest. Source agreement and Runtime-base agreement
  retain distinct proofs.
- Prepare startup and later Scene requests from that exact admitted base and original plan.
  Select the Scene representation through the Scene owner, preserve complete product identities,
  and validate decoded domain products against their catalog identity. Reuse the existing Scene
  transaction and physical cleanup owners. No authoring source, metadata or cache fallback is
  permitted after selecting the processed base.

This ordering records current design constraints. The catalog format, processed Scene format and
complete runtime-input resource representation still need concrete owner implementation and
qualification; these bullets do not establish an implemented delivery path.

## Owning Decisions

[ADR 0103](../architecture/adr/0103-concrete-domain-owned-asset-import-execution.md) already admits
typed operation facets below root orchestration. Its source-domain refinement names the new
executing contract independently of Runtime and cook. Selecting the source facet does not select
Runtime decode or cook. [ADR 0110](../architecture/adr/0110-first-party-project-workflow-and-source-carrying-export.md)
continues to own explicit source-carrying delivery.

Reconcile the actually consumed identity, local catalog and build selection contracts before
steps 2-4. Proposed ADRs 0083/0086/0087/0088 are design evidence, not wholesale accepted authority.
Adopt narrowly scoped decisions only after their detailed contract and consumer are reviewable.

## Reference Constraints

Pinned Bevy `85503941e488152711a4695bbe248e9ed9f453fc` separates source readers, processing and
the processed output loader. Its glTF index-based labels do not prove rename/reorder continuity.
Pinned Godot `81ab961f91066a63bcbf3ec340962bb1a1ada558` exports imported resources plus Runtime
remap metadata; that metadata is distinct from authoring import settings. Its import IDs and
name/path fallbacks do not establish automatic continuity after arbitrary source edits.

Use those responsibility boundaries, not their complete watcher, processor, DCC matching or
platform-variant systems. Reference checkouts remain read-only.

## Verification

For step 1, compile the independent parser with `nara_asset` and serde only. Run its actual public
Check/Play/Scene-change tests, Editor metadata publication/cancellation tests and source Export
consumer. Cover concrete input failure propagation, domain diagnostics, reservation-before-call,
panic containment and retained generation release. Check affected feature combinations and
correctness Clippy. Run Cargo serially with the existing root target.

For subsequent delivery, require clean content builds with reproducible content digests, two
Scenes, unavailable authoring source/cache and no Cargo on the run path. Reject wrong target,
Schema, codec, duplicate identity and corrupted/missing members. Failed candidates preserve the
current admitted Scene and products. Qualify actual platform execution separately from static
checks, source-only tests and future custom-drawn Editor UI.
