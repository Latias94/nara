---
type: "Work Progress"
title: "Canonical source and typed UI qualification with frame-independent content maintenance"
description: "Records source association and UI ownership corrections, actual root owner and native results, reviewed forward contracts and remaining consumer gates."
timestamp: 2026-10-30T00:24:35Z
record_id: "25781c8bea5c4c639412a82d4651a700"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-23-foundation-architecture-rebaseline-plan.md"
git_branch: "refactor/foundation"
git_commit: "286853e"
---

# Summary

The complete foundation Goal remains active. The canonical source-location and typed UI Image
replacement is not yet committed or qualified as a complete unit. `refactor/foundation` is now
`286853e`; `main` remains `4c405c1`. Only the two reviewed render/design documentation commits
`b76f0f3` and `286853e` have landed since main. Preserve the large authorized working diff and the
original untracked `vendor/`; do not push or open a PR.

# Details

## Owning Corrections

- Product lookup no longer forces a location-authored Prefab to become a StableAsset document.
  `SourceDocumentKey::validate_asset_source` owns borrowed kind/UUID/location validation;
  `ResolvedPrefabSource::with_source_asset` attaches the actual admitted record without changing
  the canonical key. Runtime and Authoring cache/fresh paths use it. Late metadata discovery
  also validates before snapshot publication. Independent source review approved this boundary.
- Typed UI replacement is source-ready and independently reviewed: Named and provider-scoped
  process-local External IDs are disjoint; only `UiVisual::image_asset` retains a typed persistent
  binding. Background replacement clears it. Automatic capture joins actual prepared AssetIds,
  without string aliases or AssetServer dependence. Canvas borrows the current catalogue/scope
  and rejects any scratch server identity/type-claim mutation. Later Image publication changes
  its catalogue generation and retries. Kernel image keys never become content or GPU leases.
- Root broad verification exposed an integration regression: removing per-drive Editor TileSet
  reconciliation assumed a scheduled Runtime frame, but driver Image publication can finish with
  no visible preview. Private `EditorTileSetInput::{Sources,Reconcile}` now separates explicit
  scoped source admission from maintenance of already bound dependencies. `advance` runs the
  same owner after import/hydration and revocation. Maintenance carries no scope and clones no
  dependency demands. Independent review approved ordering, revocation and last-good behavior.
- The user prioritizes deciding foreseeable long-term contracts now. A separate pinned-reference
  review produced four concrete additions: contribution registration/withdrawal custody, exact
  captured content to Host imports, exact render output to adapter-owned UI packets, and immutable
  future content sets. Parent corrected the initial suggestion so content owners do not carry
  GPU epochs, procedural content needs no Product, and display occurrence owns clip/DPI/input
  mapping separately from output identity. The final document diff passed independent review.
  ADR 0121 remains Proposed; these are not implemented render features. Commit `286853e` contains
  only ADR 0121, the foundation plan and its reassessment link. ADR 0007/base-guide clarification
  remains with the in-progress source unit.

## Actual Verification

All Cargo calls use Rust 1.95.0, one build job, serial execution and the root `target`.

| Scope | Actual result | Nextest run |
| --- | --- | --- |
| Corrected real custom/separated/nested source roots | 1 passed, 6 unselected | `97f823b3-7287-4030-b253-742102e5ab4f` |
| Final source association and Prefab/source/conversion integrations | 42 passed, 0 skipped across seven binaries | `5eef8ebc-d1c8-4036-8751-ec438df5bc64` |
| Independent source-location consumer | 4 passed, 0 skipped | `801da359-5d63-4032-ae20-edbb4fe7fc24` |
| Kernel image identity and compile ordering | 18 passed, 0 skipped | `17901ecc-3f6c-48f8-bbdd-791bfc9eeecf` |
| UI adapter all features | 106 passed, 0 skipped | `0ce8f990-3be2-4d98-a32f-a80c8e4d2a7e` |
| UI render owner all features | 38 passed, 0 skipped | `01f39a83-a715-482d-9825-944393ca132a` |
| Root all-feature library plus five integrations, before maintenance repair | 1,045 passed, 3 failed, 4 skipped; 1,048 executed | `1def23bf-89c7-4e37-b253-2e21b14a1c72` |
| Corrected authoring Image workflows plus Canvas identity gate | 13 passed, 971 unselected | `3b3d7e8f-2a9b-4295-a71b-fc992914a8af` |
| Runtime/Image publication, revocation and content-refresh affected cohort | 206 passed, 778 unselected | `92344c0c-d16a-4490-8102-81b474414ac3` |
| Independent UI with pure manual/provider image consumer | 5 passed, 0 skipped | `6422a00a-316a-46d8-a559-40be76bca9b9` |
| Ordinary native-Editor library suite | 251 passed, 35 ignored | `380527e9-8e93-423f-86a8-5cf5b50d1aac` |
| Selected actual Windows/DX12 Editor trajectories | 4 passed, 282 unselected | `d97454b3-efcd-4c19-ae34-015f11e45e6c` |
| Reference Game default feature suite | 95 passed, 2 failed, 0 skipped | `f30daab2-a781-421f-9546-e916cdcda194` |

The four actual native trajectories are Canvas pick/move/resize/Save/Reopen, Scene Image reimport,
Canvas priority through retained Scene Close, and views detach/minimize/reopen/device-recover/retire.
They use hidden native windows and actual DX12, not monitor-scanout or other-platform acceptance.

The broad root failures were two no-visible-Scene TileSet regressions and one old local-path
fixture assertion. All three pass in the affected 206 successor; do not report a fabricated
single all-green 1,048 run. A compile-only failure before the broad run was a removed test getter;
the helper now explicitly names `source_path`. A UI compile failure was a missing local
`ComponentFieldPathError` mapping; the subsequent complete UI suite passes.

Reference Game failures are being corrected locally: its direct App fixture must submit startup
and the recipe-owned binding definition together, rather than installing an unrelated default
binding. Its legacy Vec2 test must assert exact captured v1 source separately from migrated v2
effective output. A first correction with a separately installed binding correctly rejected a
divergent recipe definition (run `7be3db44-636a-4748-b3fb-e805285b0924`: 8/9 passed). The current
correction combines the real recipe and MinimalPlugins in one submission; its rerun is pending.
Do not weaken plugin or source-preservation checks for these fixtures.

Before the latest fixture changes, root `cargo fmt --all` and `git diff --check` pass. The changed
Markdown check found 1,153 local targets with none missing. Recheck formatting after final edits.
Independent workspace locks were refreshed with Cargo metadata for the actual UI Image dependency;
some also shed an already obsolete Asset-to-Tasks dependency. No dependency version update was
requested. Full workspace all-target check, independent broader changed consumers, neutral
executables, Reference Game Editor refresh and correctness Clippy remain pending.

## Continuity And Resources

All workers/reviewers from this unit have been closed. None retains a write scope. No reference
checkout or vendor files were changed. Cargo was still running only for the Reference Game
correction when this record was filled; poll the exact live tool session or actual processes.
The most recent free space was roughly 11 GiB on F. Reuse root target; never start concurrent
Cargo. Existing deps/incremental are NTFS-compressed. Do not recursively delete computed artifact
paths; preserve caches and use verified compression only if needed with Cargo stopped.

# Next Action

Finish Reference Game fixture corrections and remaining serial verification, inspect the complete
intended diff, capture final evidence and precisely commit this source/UI unit before locally
fast-forwarding main. Do not claim Source Scene Door dependencies, processed input/target delivery,
render graph implementation, custom-drawn Editor or the complete foundation Goal finished.

# Citations

- `progress/2026-10/2026-10-29T175710Z-source-location-review-corrections-and-typed-ui-image-binding-replacement-b7fd68aeb4ba4783b01ce02acb355ab8.md`
- `docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md`
- `docs/architecture/adr/0007-asset-identity-and-import-pipeline.md`
- `docs/architecture/adr/0033-asset-import-and-render-resource-preparation-seam.md`
- `docs/architecture/adr/0038-scene-prefab-authoring-identity-and-provenance.md`
- `docs/architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md`
- `docs/architecture/adr/0121-composable-render-programs-and-host-execution.md`
