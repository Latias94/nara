---
type: "Work Progress"
title: "Source location review corrections and typed UI Image binding replacement"
description: "Records bounded domain verification, source-path preadmission, forward render contracts and the active typed UI Image owner replacement."
timestamp: 2026-10-29T17:57:10Z
record_id: "b7fd68aeb4ba4783b01ce02acb355ab8"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/foundation"
git_commit: "b76f0f3"
---

# Summary

The complete foundation Goal remains active. The canonical source-location replacement is still
uncommitted and not qualified as a complete unit. `refactor/foundation` now includes the separate
reviewed documentation commit `b76f0f3`; `main` remains `4c405c1`. Do not push or open a PR.

# Details

- The user's forward-design priority is implemented in the render plan and Proposed ADR 0121:
  stable Feature incarnations/candidate-local resources, semantic output and resolve wiring,
  independent candidate publication/failure groups, and retained shader-generation interleaving.
  This is design refinement, not rendering implementation. Requested Astra max failed its usage
  limit before work; a current Codex agent completed the bounded pinned Bevy/Godot review instead.
- Independent source-owner review found missing preallocation charges for canonical Image/TileSet
  paths and an existing unowned resolved-alias lifetime. Both were fixed and independently rereviewed:
  `resolve_reference_path` takes the actual root and returns `(AssetPath, WorkReservation)`;
  Prefab hit drops/refunds, miss transfers into charged cache; TileSet result owns path work through
  retained transfer; Image atomically transfers its canonical path into the retained import plan.
  `WorkReservation.previous` and invalid baseline assertions were removed because other queues can
  retire independently. Checked subtraction and admission remain.
- Parent also preadmits native source keys and stable-metadata path scratch, removes the redundant
  format-then-clone allocation, and transfers `native_source_work_bytes` into snapshot metadata.
  Successful queued paths drop before refund. These later parent changes still need root tests.
- Scene/tooling APIs now consistently name `asset_resolution`, including conversion wrappers;
  no aliases or hidden database ROOT were added. Removed unused source-path getters and redundant
  preview-owned scope: each projection and the actual `EditorAssetScope` retain their own facts.
- Complete real-project tests now cover custom/separated/nested roots, same local Image/Prefab
  names, all Path/StableId/primary Product selectors, non-activating unrelated metadata, exact
  source receipts after snapshot drop and mutation refusal. They are awaiting execution.

Actual bounded verification:

- Earlier core162, Image/Watch195 (one skipped), TileSet98 pass as recorded in predecessor.
- Domain cohort run `47406266-2eae-4eea-86ee-06e7525bd58a`: 1,217 run, 1,216 pass, one new
  `component_binding_export` fixture failure, three skipped. The fixture required AssetServer even
  for pure validation. It now defers real record reservation through the existing prepared codec
  contract. Its complete 20-test binary passes in run `0f04baf6-c5a8-4f09-84d6-e5b33c9a77b4`.
  No other domain behavior changed after that broad run; do not invent a single all-green run.
- Root all-feature library check passed before the latest unused-field cleanup and typed UI
  replacement. The no-UI test check found migrated last-good path and borrow call sites; a bounded
  worker fixed its three test files. Five `project_tooling`-absent errors came from the selected
  feature combination; the current Source test command includes `project-workflow-cli` instead.
- Current serial Cargo is root Source tests (log in temporary directory), including
  `project_content::` and `project_scene_content`/delivery/boundary integration binaries. It was
  still building at capture; no root execution result is claimed here.
- With Cargo stopped, NTFS compression of `target/debug/deps` recovered about 11 GiB without
  deleting artifacts. Parent stopped its exact compact helper after partial progress; no helper
  remains. New artifacts inherit compression. Keep using root target and serial Cargo.

## Active UI Image Replacement

Independent review rejected a string-based intermediate: arbitrary opaque Product-shaped names
could masquerade as persistent bindings, canonical prefixes exceeded the widget-key limit, and
one new test incorrectly admitted unsupported image borders. Do not restore that design for a
green build.

Selected contract, adopted in ADR 0102 but not yet qualified:

- Kernel `UiImageId::new` always creates Named. `external::<T>(token)` carries only a process-local
  provider `TypeId` and exact numeric token, with no Any payload, string grammar, registry or new
  allocator. First-party T is `ImageAsset`; token is its actual process-unique AssetId.
- `nara_ui::UiVisual` privately retains `Handle<ImageAsset>` only through explicit `image_asset`.
  Plain opaque/manual images remain drawable via explicit snapshot insertion and refuse asset
  persistence. Replacing a brush clears its old binding even if IDs compare equal.
- Codec/preparation reuses actual scope/resolver and Sprite-style deferred ownership; export uses
  the typed handle and existing AssetRef policy. Primary/auxiliary Runtime Products need no source
  database. Text/image borders stay unsupported. Pure authoring choices encode typed leaves.
- Canvas takes actual mutable decode context. Parent wires current runtime database/scope and
  AssetServer through borrowed `EditorCanvasImageSources`; only an actual cache miss clones a
  scratch server. `ui_preview.prepare` now receives these borrowed facts. Root prepared-image
  capture inserts exact External IDs instead of local/path/stable string aliases.

Active writer: Huygens `01a188a5-a3ed-70f1-8afd-a43ad7c5c708`, exclusive Kernel identity/lib/tests,
UI authoring/codec/composition/canvas/tests/Cargo dependency, UI-render images/integration/packet
tests, `tests/project_content_boot.rs` and `reference-game/tests/editor_ui_refresh.rs`. Do not
overwrite or fmt until source-ready. Parent owns root `authoring_runtime.rs`, `authoring_images.rs`,
`ui_authoring.rs`, `ui_preview.rs` wiring and its embedded tests. Read-only reviewer Carson
`01a188b3-b5a6-74b0-9ecc-47eeefce01ad` is challenging this selected boundary; ask for final rereview
after source-ready. All earlier workers were closed.

# Next Action

Finish and independently review the typed UI replacement, migrate remaining root/private callers,
freeze writers and inspect diff before fmt. Run relevant Source root, UI kernel/adapter/render,
independent source-location/neutral UI, Reference Game and selected native qualification serially.
Record exact failures and limits, not a broad completion claim. Commit qualified coherent units
and locally ff main; Source Scene Door dependencies, input/target delivery and render/custom UI
remain later complete workflows in the active foundation Goal.

# Citations

- `progress/2026-10/2026-10-29T165812Z-canonical-source-locations-and-bound-dependency-integration-in-progress-ca6a465101fd4a01ac81129384857e92.md`
- `docs/architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md`
- `docs/architecture/adr/0121-composable-render-programs-and-host-execution.md`
- `docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md`
