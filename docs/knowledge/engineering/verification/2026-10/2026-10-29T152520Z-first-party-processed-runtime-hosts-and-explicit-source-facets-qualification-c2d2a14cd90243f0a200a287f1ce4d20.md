---
type: "Verification Evidence"
title: "First-party processed Runtime Hosts and explicit source facets qualification"
description: "Qualifies original-plan codecs, shared source/processed startup and travel, finite Scene dependencies, native residency, budgets and retained custody; records the source-location successor."
timestamp: 2026-10-29T15:25:20Z
record_id: "c2d2a14cd90243f0a200a287f1ce4d20"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md"
git_branch: "refactor/foundation"
git_commit: "44470ed"
supersedes: "3defca0f0fbf4f979d7066029882eae9"
---

# Verification

Qualify production `44470ed`, with the fixture-only cleanup `4f1be66`, against the ordered
asset-delivery plan. Tests ran in the existing Windows 11 / PowerShell 7 checkout on Rust
1.95.0, with serial Cargo, `CARGO_BUILD_JOBS=1` and the root `target`. Independent workspaces
also used that target. The tests ran over the final intended source tree before commit; no
production changes followed those results. Later changes only record evidence and successor
design. Local main was `76a8571` before integration. No push or PR is authorized or performed.

# Result

The first-party Runtime Host now accepts the original immutable processed base for startup,
later Scene requests and game-facing typed CPU collections. Source and processed input share
the existing candidate publication, native retirement, cancellation, retry/discard and physical
cleanup owners. Image/TileSet values publish into the actual stock stores before startup Scene
materialization and in the same exclusive Scene replacement tail during travel.

The original configuration plan freezes exact family/codec bindings, versioned definition IDs,
roles and predecode limits in a separate canonical codec fingerprint. Catalogues require that
agreement; format names cannot select another Rust implementation. Each member is decoded once
per prepared family/definition, including compound Products and several formats in one family.
Required Scene members are admitted as flat retained data, including finite A/B back-edges;
only the requested root instantiates. Dependency membership grants no public travel permission.

Native Asset/Image storage is independent of explicit source execution. Processed composition
has no dummy database, source queues, importer or watcher; source-backed consumers explicitly
select source facets or initialize their own actual source driver. The superseded coupled
Image configuration helper and source-only replacement wrapper are removed without aliases.

Independent source, compound CPU, actual stock extraction/Image preparation and native TileSet
consumers pass. A new Door component drives processed A-to-B-to-A requests after the captured
files disappear. Hidden dependency travel, malformed dependencies and missing/Soft actual
Required edges refuse while preserving current content.

# Evidence

## Final executed cohorts

| Scope | Result | Nextest run |
| --- | --- | --- |
| Root library, Tilemap library and selected Reflect asset-reference tests, all features | 1,082 passed; 127 outside selection/default skipped; 401.943 s | `9574f9d7-42fd-467a-a2e1-294086a3f3d7` |
| Six root public integrations: plugin composition, Image import limits, Runtime Scene activation, Scene product surface/content and startup surface | 80 passed, zero skipped; 110.254 s | `94d6511b-549f-49cf-a42e-5056de41fd9b` |
| Independent Scene/asset-domain consumers, 11 binaries | 84 passed, zero skipped; 10.474 s | `8a06edf9-3b57-4256-a441-d082a9b8e4d5` |
| Reference Game asset task flow, project content boot and plugin composition | 6 passed, zero skipped; 0.850 s | `b43acf4f-3f9f-4aad-b0ac-881e17a1aa08` |
| Three critical hidden-window Windows/DX12 Editor trajectories | 3 passed; 283 unselected; 8.424 s | `2743c7ff-0cc4-4bca-91dd-ba174ffbe43b` |
| Independent source/Editor and corrected fixture consumers, eight binaries | 55 passed, zero skipped; 10.162 s | `14e82a94-2d67-4da9-86a2-14eb36bb6e3c` |

The final root selection was:

```powershell
cargo +1.95.0 nextest run -p nara -p nara_reflect -p nara_tilemap --all-features --locked --lib -E 'package(nara) | package(nara_tilemap) | test(asset_reference::)' --test-threads=1 --no-fail-fast
```

The independent 84 cover bundle/import/source workflows, original-plan codec refusal and limits,
processed Host cancellation/last-good/multiple formats, Runtime Image/Scene/TileSet delivery and
six Door Scene reference cases. Their selected binaries are `bundle`, `compound_workflow`,
`import`, `workflow`, `runtime_base`, `processed_host`, `processed_scene_references`,
`runtime_image_delivery`, `runtime_tileset_delivery`, `runtime_scene_assets` and
`runtime_scene_delivery` in `tests/fixtures/module-consumer`.

The final 55 select `scene_preparation`, `prefab_conversion`, `durable_identity`,
`domain_reconciliation`, `image_reconciliation`, `source_import`, `compound_import` and
`snap_local_position`. They cover real Source import and existing native retirement/publication
consumers after fixture borrow/Clippy corrections, not just compiled test annotations.

The native command followed the [qualification procedure](../../../../verification/native-editor-qualification.md)
with `WGPU_BACKEND=dx12`, `-p nara-editor --all-features --lib --locked --run-ignored only` and
exact selection of `native_resume_redraw_rebases_paused_idle_before_the_owner_drive`,
`native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime` and
`native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts`. All three actually
executed. This qualifies those view/input/recovery/retirement paths; it does not establish
monitor scanout, other platforms or the full ignored native suite.

## Static and feature checks

- Correctness Clippy passes for root and Asset/Watch/Image/Reflect/Sprite/SpriteRender/Tilemap,
  all features and targets, with `-D clippy::correctness` (3m21s). Existing other warnings remain.
- Independent module-consumer workspace Clippy passes all targets with `-D warnings` (14.43s).
  A dependency's existing Editor dead-code warning is reported under Cargo dependency lint caps.
- Reference Game all-target correctness Clippy passes (58.33s), retaining existing other warnings.
- Asset/Watch/Image/Reflect/Sprite/Tilemap library checks pass with no default features (29.83s).
  Root passes with only `runtime-2d,serde` and no default features (28.74s).
- Root, independent and Reference Game `cargo fmt --all -- --check` pass. Final diff whitespace
  and changed/new-document link/fence checks pass. The two independent lock changes only remove
  the now dev-only `nara_tasks` dependency from `nara_asset`; no dependency version was upgraded.

## Review corrections and failed attempts

Review found actual owning defects and qualified their replacements:

- Sprite's source-only auxiliary refusal also rejected already admitted Runtime auxiliary
  Products. Runtime now requires existing complete typed bindings; Source keeps its real lookup.
- Image residency omitted value/state association. A newly installed empty value store could
  survive preparation and panic after Scene commit. Passive exact mutation baselines now admit
  before publication and reject replacement stores; returning the original owner recovers.
- Unpublished fresh bindings established permanent per-ID store associations even when abandoned.
  `prepare_mutation` now observes without binding; actual first mutation establishes association.
  The immediate managed `mutation_baseline` contract remains explicit. Abandonment and competing
  preparation regressions cover both outcomes.
- Three complete AssetServer copies and staged growth were uncharged. The borrowed owner cost
  query covers all five maps, anonymous/type claims and both path strings before cloning. A real
  small-budget base with 4,096 unrelated bindings refuses without leaked reservation or mutation.
  Aggregate/local drop order keeps catalogue copies, publication scratch and escaped pixels under
  their metadata owner through startup refusal, replacement failure and successful publication.
- Native Image/TileSet cardinality and actual Required Image dependency are checked independently
  of codec-provided inventory. Lax codec wrappers cannot use extra aliases or another Scene edge
  to disguise an omitted native dependency.
- Graph scratch overlapped Scene decode and retained indices undercharged requested capacity.
  Root Scene decode now precedes graph reservation; flat indices retain their original requested
  charge, then sequential dependency decoders reuse released work. Finite Scene back-edges no
  longer enter the asset-only branch or create mutually owning Scene objects.
- Tilemap membership used an uncharged typed cell vector and tree. It now parses once into an
  exact-capacity selection vector, sorts in place and validates duplicate coordinates before
  atlas lookup. The owner cost query includes path/UUID decode scratch even for empty cells.
  Reflect supplies the borrowed closed-grammar query; static color labels eliminate success-path
  formatting. Long-path, primary/auxiliary, unsorted/duplicate and nonallocating-query tests pass.

Euler's final independent source review approves the reference/cell cost and caller reservation
lifetimes with no blocker. The review itself ran no Cargo. Its longer-horizon finding is that
third-party native publication is not yet an open facet; the plan requires a typed successor
through the same preparation/baseline/commit owner before Material interfaces freeze.

Earlier runs are not silently counted as final coverage. The broad owner/root run had 1,321
passes and one faulty store-recovery fixture; the later focused run had 302 passes and one
startup-refusal fixture missing a real Image dependency. Both fixtures were corrected and their
actual production trajectories pass in the final root cohort. The earlier independent run's
unknown-codec expectation and malformed typed Transform fixture were corrected before final84.
Strict consumer Clippy initially rejected shared-helper, redundant update/clone, explicit borrow
drop and large typed-error fixture lints. The fixture-only correction preserves tests and public
domain errors through narrow documented exceptions; final strict Clippy and final55 pass.

## Workspace resources

No source, `vendor/`, reference checkout or cache was deleted. Automatic approval rejected a
verified recursive incremental-cache delete with only `blocked by policy`; no alternate delete
was attempted. NTFS transparent compression of the exact repository `target/debug/incremental`
recovered space while preserving artifacts. The owned compression helper was stopped after
sufficient recovery; the final Source/Editor cohort passed afterwards. Available F space rose
from about 1.15 GiB after native linking to about 41 GiB at that boundary. This is workspace
maintenance, not an engine memory or performance measurement.

# Follow-up

The complete foundation Goal stays active. This unit does not qualify source-authored Door
loading, a processed input-map resource, generic native publication, target build selection,
a Cargo/source/cache-free packaged Player, CAS/streaming, render composition or Nara's own
custom-drawn Editor UI. No compatibility reader, alias or migration guide was introduced.

The source successor must correct canonical source locations, authored aliases and
capability-relative reads before adding Scene discovery. Independent source review traced
cross-root same-name collisions, missing real Scene metadata discovery, mode-based Product
request refusal and sibling Authoring/Save/Export gaps. The
[preferred location design](../../../../architecture/source-location-and-reference-resolution-design.md)
compares three representations and records the complete consumer migration. It is Proposed
design, not an implementation or Accepted contract. The ordered plan retains finite effective
Scene closure, direct Door Product requests, declared-root authority and flat retained custody.

Continue on `refactor/foundation`, locally fast-forward completed units to main, reuse root
target and keep Cargo serial. Preserve untracked `vendor/`, all user changes and read-only
`repo-ref/`. Do not push or declare the overall foundation complete.

# Citations

- [Ordered delivery plan](../../../../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Asset identity/ownership](../../../../architecture/adr/0007-asset-identity-and-import-pipeline.md)
- [Import/render seam](../../../../architecture/adr/0033-asset-import-and-render-resource-preparation-seam.md)
- [Runtime base guide](../../../../guides/runtime-content-base.md)
