---
type: "Verification Evidence"
title: "Canonical source locations and typed UI Image binding qualification"
description: "Records the complete scoped source-location replacement, exact public image inventory, real consumer/native evidence and remaining Source Scene work."
timestamp: 2026-10-30T01:58:32Z
record_id: "0ce5bafdc65f4893aa0079009668e41a"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md"
git_branch: "refactor/foundation"
git_commit: "a56a1d3"
---

# Verification

Qualify the canonical source-location and typed UI Image replacement under ADRs 0007, 0033,
0038 and 0102. This is one dependency unit of the complete foundation Goal, which remains active.
All Cargo operations use Rust 1.95.0, one build job, serial execution and the repository `target`.

# Result

The source-contract replacement is qualified as `a56a1d3`. `AssetSourcePath` is the canonical logical catalogue
location; `AssetPath` remains an authored alias. The borrowed resolver and explicit scope bind
metadata, reads, watch/reload, native identity, decode/encode, Authoring and source Export without
kind-guessed directories. Real separated and nested roots share local aliases without sharing
identities. Complete Prefab Product selection retains its admitted document key, including
ProjectLocation, with one borrowed source/metadata validator used before snapshot publication.

The UI checkpoint is refrozen for this changed image boundary. Typed asset selection stays in the
Scene/ECS adapter; manual and External kernel IDs do not imply persistence, residency or access.
Canvas uses the current real catalogue and refuses scratch identity/type-claim changes. Automatic
capture joins actual prepared AssetIds without string aliases or an AssetServer prerequisite.
An Editor maintenance correction advances retained TileSet dependencies after Image import,
hydration and revocation even when no ordinary Runtime frame or visible Scene advances.

Independent source, UI and maintenance review found no unresolved blocking findings after the
recorded corrections. This is not qualification of Source Scene dependencies, the render graph,
the complete custom-drawn Editor, all platforms or the complete foundation Goal.

## Public Image Inventory And Assertions

| Owning surface | Refrozen contract |
| --- | --- |
| Kernel `UiImageId` | `new` always creates Named; `external::<T>(u64)` creates a private provider-type/token equality key. `as_name() -> Option<&str>` replaces `as_str`; External identities are process-local and never serialized. Providers own non-reused tokens; the Image adapter uses the actual process-unique AssetId. |
| Adapter `UiVisual` | `image_asset(Handle<ImageAsset>, tint)` establishes the private typed binding; `image_handle()` borrows its Copy handle. `image(id, tint)` remains opaque. `with_background` clears the binding even for an equal ID; border/radii retain it. Export requires the real typed binding and explicit Asset encode context. |
| `UiAuthoringCanvasCompiler::compile` | Receives a mutable actual `ComponentDecodeContext`; root, node and button images follow the same resolution. Current Canvas Host rejects any new scratch binding or type claim and retries after actual Image catalogue publication. |
| `UiPreparedImages` and presentation | Explicit `insert` still serves arbitrary manual/provider IDs. Automatic capture keys actual snapshots by External Image AssetId, with no path/UUID/Product alias search or AssetServer gate. Packet retention pins the exact prepared snapshot. |
| Reflect/Scene source context | Decode/encode accept `with_asset_resolution` or an explicit standalone path scope. Source-aware Scene/tooling methods use `*_asset_resolution`; implicit database-root APIs are removed without aliases. |

The public constructors and result shapes are the inventory; the kernel's private representation
is not an extension contract. The independent manual-image consumer draws a Product-shaped Named
ID and two distinct providers with the same numeric token without Asset/ECS/renderer dependencies.
Adapter tests refuse persistence for opaque IDs and clear stale bindings. Runtime/Canvas tests
exercise long canonical paths, distinct roots and source-free primary/auxiliary Products with
actual pixels. Existing layout, interaction, exact presentation and packet-custody rules remain.
Image text/borders still refuse; this unit adds no new drawing primitive.

# Evidence

The complete 215-file Rust/manifest/lock input set was frozen before the final workspace checks.
LF-normalized aggregate SHA-256 is
`1f787542cb7e1d03dd4f56244032aee2c91166649c7486320cc2ec0ce44891f3`.
Each entry hashes normalized file bytes; sorted `<digest><two spaces><path><LF>` entries form the
aggregate. Staged blobs were checked against that inventory before the source commit. No Rust
behavior changed between the final affected reruns, examples and commit. Raw logs and the local
hash manifest remain outside tracked memory.

| Scope | Actual result | Nextest run |
| --- | --- | --- |
| Real custom separated/nested source roots | 1 passed, 6 unselected | `97f823b3-7287-4030-b253-742102e5ab4f` |
| Source association and Prefab/source/conversion integrations | 42 passed across 7 binaries | `5eef8ebc-d1c8-4036-8751-ec438df5bc64` |
| Independent source locations | 4 passed | `801da359-5d63-4032-ae20-edbb4fe7fc24` |
| Kernel image identity/compile order | 18 passed | `17901ecc-3f6c-48f8-bbdd-791bfc9eeecf` |
| UI adapter all features | 106 passed | `0ce8f990-3be2-4d98-a32f-a80c8e4d2a7e` |
| UI render | 38 passed | `01f39a83-a715-482d-9825-944393ca132a` |
| Root all-feature library and 5 integrations before maintenance correction | 1,045 passed, 3 failed, 4 skipped; 1,048 executed | `1def23bf-89c7-4e37-b253-2e21b14a1c72` |
| Corrected Image authoring plus Canvas identity gate | 13 passed, 971 unselected | `3b3d7e8f-2a9b-4295-a71b-fc992914a8af` |
| Affected runtime/Image/content refresh after correction | 206 passed, 778 unselected | `92344c0c-d16a-4490-8102-81b474414ac3` |
| Pure independent UI/manual Image consumer | 5 passed | `6422a00a-316a-46d8-a559-40be76bca9b9` |
| Ordinary Editor library | 251 passed, 35 ignored | `380527e9-8e93-423f-86a8-5cf5b50d1aac` |
| Selected actual Windows/DX12 Editor trajectories | 4 passed, 282 unselected | `d97454b3-efcd-4c19-ae34-015f11e45e6c` |
| Reference Game first default run | 95 passed, 2 failed | `f30daab2-a781-421f-9546-e916cdcda194` |
| Final affected Reference authoring/Vec2 migration | 9 passed | `92a364ee-397c-418c-8563-921ca046d375` |
| Reference Game Editor UI refresh | 1 passed | `584af0cb-1c79-4fcc-a517-4622943e7c9a` |
| Independent workflow first run | 63 passed, 1 failed; 64 executed | `3b64122f-fee0-40a3-9ffd-32450d1a766f` |
| Corrected independent nested-schema export policy | 1 passed | `e2b521ad-fdee-4b91-9b33-65ad02bd8eb8` |
| Actual CLI Check/Run/Repair/Export | 1 passed, 7 unselected; 1,133.508 seconds | `54e714f5-3097-4dc2-b910-bb3538575258` |

The root failures exposed missing no-visible-Scene TileSet progress and one old local-path
assertion; the affected 206 rerun covers the repair. Reference fixtures now submit the recipe's
actual startup definition together and distinguish exact captured v1 source from migrated v2
effective values. The independent nested-schema fixture explicitly selects StableId export
instead of expecting a stable reference from the default Path policy without metadata. These
corrections preserve production admission. The table does not fabricate one green broad rerun.

The actual native selection consists of:

- `native_canvas_widgets_pick_move_resize_and_save_reopen_real_source`
- `native_scene_reimport_replaces_presented_pixels_without_document_or_extent_changes`
- `native_ui_canvas_priority_survives_closing_retained_scene`
- `native_views_detach_minimize_reopen_recover_and_retire_with_exact_receipts`

These use hidden Windows native windows and actual DX12. They establish neither monitor scanout
nor Linux/macOS execution. The ordinary suite's ignored native cases are not claimed as passes.

Additional final gates:

- `cargo +1.95.0 check --workspace --all-features --all-targets --locked`: passed, 6m50s.
- `cargo +1.95.0 clippy --workspace --all-features --all-targets --locked -- -D clippy::correctness`:
  passed, 6m09s. Existing suggestion warnings remain.
- Minimal `nara --no-default-features --features runtime-ui` and
  `nara_scene --no-default-features` checks: passed.
- All-feature `layout_gallery`, `interaction_lab` and default headless
  `ui_presentation_gallery` executables: passed, including manual image presentation, exact action
  receipts and retained packet readability. The last is a headless conformance run, not another
  native-window result.
- Final `cargo fmt --all -- --check` for the root, Reference Game, independent module consumer
  and pure UI consumer workspaces: passed. Staged and unstaged `git diff --check`: passed.

Earlier Asset, Image/Watch, TileSet and domain qualification results and bounded failed attempts
remain in the linked progress records; they are not reclassified as a single final-source run.
All known implementation/review agents are closed. No push, PR, reference or vendor modification
is part of delivery. No build output was deleted; prior NTFS compression preserved existing files.

# Follow-up

Continue the same source owner with finite effective Scene data dependencies, direct primary
Scene Product requests resolved in preparation, exact selected closure receipts, escaped flat data
retention and dependency-only travel refusal. Authoring must include actually demanded open Scene
working copies under the exact nominal Schema basis, rather than add a temporary saved-only
model. ADR 0047 adopts that next contract; implementation and qualification remain outstanding.
Input/target delivery, render composition and Nara's own Editor UI remain separate open units.

# Citations

- [Owner plan](../../../../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md)
- [Source-location design](../../../../architecture/source-location-and-reference-resolution-design.md)
- [ADR 0102](../../../../architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md)
- [Predecessor progress](../../progress/2026-10/2026-10-30T002435Z-canonical-source-and-typed-ui-qualification-with-frame-independent-content-maintenance-25781c8bea5c4c639412a82d4651a700.md)
