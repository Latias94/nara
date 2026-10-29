---
type: "Verification Evidence"
title: "Anonymous and processed Image content with retained native publication qualification"
description: "Qualifies source-free Runtime Image decode, explicit managed Product admission, anonymous procedural render content and original output custody through CPU, public Scene, GPU and native Editor workflows."
timestamp: 2026-10-29T09:08:32Z
record_id: "3e529af056d34a5caa2c12924c0c1014"
producer_id: "codex-foundation-main"
run_id: "session-01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-10-29-asset-import-and-runtime-delivery-plan.md"
git_branch: "refactor/foundation"
git_commit: "14e5ab1"
---

# Verification

Continue the complete active foundation Goal and ordered asset/Runtime delivery from `3ac32a4`.
The same long-lived `refactor/foundation` branch and existing root target are used. Rust1.95.0,
Windows11, PowerShell7, serial Cargo and `CARGO_BUILD_JOBS=1` apply to every workspace. Native
tests select DX12. Implementation is `14e5ab1`; subsequent evidence/ledger integration changes
only documentation. No push/PR, vendor/reference modification or relocated output.

# Result

Image content owns actual extent/format/color, immutable fixed-size shared pixels and a cached,
domain-separated BLAKE3 content hash. Ordinary construction is anonymous. `from_product` names
persistent delivery content explicitly; `from_import` derives PRIMARY and retains actual import
provenance. General serde validates their association and recomputes content identity. It does
not trust a serialized hash or turn a missing identity into an invented source record.

Managed ImageContentSnapshot requires a present complete Product, matching typed native binding,
usable state, publication and exact slot. Anonymous content cannot acquire managed authority
through two absent associations or a remaining foreign Product binding. Code-first preparation
instead uses actual typed storage/version/slot and supports anonymous pixels. Prepared snapshots
keep content with its identity; the neutral RenderResourceContentHash replaces import-artifact
descriptor hashing. Logical Product, version, slot, content and GPU epoch retain distinct roles.

The `runtime_image` v1 wire is bounded Core metadata plus exact raw RGBA, with framing, strict
object shape, shared kind/version/engine admission, complete Product, dimensions and exact byte
length checked before pixel copying. Anonymous encode refuses. Decode supplies the named PRIMARY
or auxiliary Product, recomputes its hash and invents no path, source settings or import artifact.
The nonempty binary golden fixes metadata and raw framing. This qualifies the current RGBA8
contract, not mips/compression, a render graph or every future Image usage.

The independent public consumer imports a real PNG for production, encodes primary/auxiliary
members and removes producer files. One captured base supplies startup/later Scenes and Images;
native codecs resolve actual handles and reverse export. The existing Scene/native transaction
stages complete bindings plus actual owned Images/states/events. Refusal and discard preserve the
current Scene/resources; successful replacement publishes them together. The World has no source
database. CPU/content/prepared escapes keep the original Runtime acquisition reservation and base
alive until the last pixel owner releases them. This is not automatic first-party native residency.

Source import/Editor/TileSet-source paths still require real provenance. Common bytes/file/reload
admission refuses existing processed or mismatched values before read/decode budget admission.
Fresh TileSet dependencies validate source kind/path/stable ID before publication; repeated
reconciliation does not churn version, slot, state or events. Valid last-good remains until its
actual publication is revoked. Procedural examples and GPU fixtures now directly publish anonymous
pixels, removing synthetic source IDs/paths and RGBA-to-PNG-to-import choreography.

# Evidence

| Scope | Actual final result |
| --- | --- |
| Image/Render/TileSet/SpriteRender/UIRender all-feature cohort | 317 passed, one existing default skip; build1m49s/run22.364s |
| Root all-feature lib plus ten content/Scene/Play/composition integrations | 1087 passed, five existing default skips; build5m22s/run404.504s |
| Independent Image/reconciliation/native Scene resource consumer | Nine passed across three binaries, no skips; build1m23s/run0.808s |
| WGPU all-feature library with required adapter | 145 passed, four default skips; build3m56s/run82.813s |
| Windows/DX12 Editor critical three plus Image reimport and Canvas workflow | Five executed/passed;281 outside-selection skips; build3m23s/run12.485s |
| Reference Game all-feature selected twelve binaries | 61 passed, no skips; corrected build15.93s/run30.213s |
| No-default Image/Render | Lib check passed20.47s |
| Image serde-only | Lib check passed14.05s |
| Root no-default runtime-2d+serde | Lib check passed10.46s |
| Procedural palette/windowed Sprite examples | All-feature check passed30.46s |
| Eight affected owner/root/Editor packages correctness Clippy | All features/all targets passed3m45s; existing style/large-error warnings remain |
| Image/Render compile-fail doctests | Two passed, no skips; build11.03s, runs0.78s/1.97s |
| Public capability/dependency/prelude boundaries | Nine passed, no skips; corrected build1.92s/run93.434s |
| Final formatting and narrowed Render assertion cleanup | Root/independent/Reference formatting checks pass; two prepared-hash tests pass,35 outside-selection skips |

Nextest IDs: domains317 `cc61836d-fe7e-4fa0-af0b-0bc8dd682aa9`; root1087
`adfd3fee-b367-4bf2-8cd7-01660c20dd4d`; independent9
`e217b022-a135-4509-bd64-156f380e0c45`; WGPU145
`d0841b04-645f-4ab0-a968-a90fc1156ff2`; native5
`355354fe-6cbd-4eae-a44d-2cb574f4c987`; Reference61
`ffef5075-7107-4c3c-b6c0-2f96f9ff762f`; public9
`22701de7-4cb5-4410-8200-bba7e77df1f4`.

The final Render test-only cleanup removes one redundant owned-buffer hash assertion after
Clippy; focused run `c57e0d98-062b-4a0e-9fac-1dacdc0c0223` passes2/2 (build1m16s/run0.115s).
No production changes follow the broad results above. Changed-document links/fences and Git
whitespace checks pass. Memory validation passes with existing historical advisory warnings.

Root integrations select `image_import_limits`, `project_content_boot`, `project_content_limits`,
`project_content_delivery`, `project_scene_content`, `runtime_scene_activation`,
`workspace_play_runtime`, `project_runtime_boot`, `plugin_composition` and
`runtime_configuration_composition`, with `--lib`. Independent package
`nara-scene-module-consumer` selects `runtime_image_delivery`, `image_reconciliation` and
`runtime_scene_assets`. Domains select the five named packages with all features and their
ordinary default test selection. No full-workspace execution is claimed.

WGPU selects all features, `WGPU_BACKEND=dx12` and `DEAR_IMGUI_REQUIRE_WGPU_ADAPTER=1`.
Actual readbacks include anonymous old/new exact snapshots together, repeat without reupload,
native palette/tile ordering and UI images. Required-adapter tests do not silently skip missing
devices. The four default ignored cases are separate from this145-case execution.

Native execution selects the procedure's exact resume/redraw, Game input/reopen/resize/Step and
independent-view detach/recover/retire critical names, plus
`native_scene_reimport_replaces_presented_pixels_without_document_or_extent_changes` and
`native_canvas_widgets_pick_move_resize_and_save_reopen_real_source`. These are hidden actual
Windows/DX12 submissions/presentations, not monitor scanout or other-platform qualification.

Reference Game selects `first_wave`, `desktop_flow`, `desktop_parity`, `desktop_render`,
`ui_product`, `ui_authoring_roundtrip`, `project_manifest_ingest`, `plugin_composition`,
`asset_task_flow`, `image_asset_safety`, `project_content_boot` and `editor_ui_refresh`.
This includes the four directly affected tests beyond the predecessor57-case cohort.

## Review And Recorded Corrections

The earlier Image164/1, domains313/1 and independent9/0 runs preceded the anonymous constructor
correction and are not reused as final evidence. Huygens
`01a186a5-2357-7b41-9b4f-c724583024aa` identified two P2 source-admission defects and supplied
the common import guard, fresh TileSet guard and six real regression tests. Arendt
`01a186b3-4a1e-7a02-a3c0-0555617f487b` found no remaining confirmed P1/P2 in that repair scope.
The preceding independent codec/consumer review by Dalton retains its source-only scope.

Euclid `01a186ae-eee3-7871-8699-7f96ba4189c8` challenged the mandatory persistent Product in
ordinary Image construction using the actual anonymous handle/prepared paths and procedural
examples. Parent corrected the Image owner, managed admission and codec; Euclid migrated the
examples and Dewey `01a186bc-e895-7941-b53d-85da4121b916` updated the two bounded test modules.
Kant `01a186c3-0b75-7a73-b1f9-cb8dcf3d898b` removed fictional source metadata from the actual
GPU readback fixtures. Averroes `01a186c0-e1ab-7f60-9e29-514422666616` found no confirmed P1/P2
in the final identity/serde/authorization/retention contract and immediate consumers. All these
agents are closed; parent owns integration and all Cargo. Writers froze before formatting.

Initial root test compilation exposed three missed optional-source assertions in an Editor
source-only fixture. They now explicitly require actual provenance. Initial Reference Game61
ran60 passes and one failure: the old UI refresh test expected migration to overwrite the source
Prefab capture. It now checks the unchanged authored v1 declaration separately from the exact
source-bound effective v2 projection and canonical asset reference. The complete original
edit/history/Save/Reopen/Restart path remains, and the full61-case rerun passes. Production source
preservation was not weakened to satisfy that assertion. No skipped failure is reported as a pass.

The initial public-boundary9 run passed7 and failed2 because deeper presentation fixture locks
lacked the new Image/Render dependency edges. Offline workspace updates refreshed those two locks
and the affected renamed-root/sample locks without changing registry package versions. The full
nine-case rerun passes, including independently locked 2D/UI trees and positive/negative public
prelude compilation. The earlier shallow lock scan was incomplete; final recursive coverage
includes nested fixture workspaces. No second build directory was introduced.

# Follow-up

Keep the complete foundation Goal active. The ordered plan now includes concrete decoded-output
cost/refund before stock residency: generic base acquisition currently retains the codec's maximum
output promise per Image, even a small one. This is bounded but unsuitable as proof of many-image
residency. Preserve upfront admission and original escaped ownership rather than raise caps.

Next replace TileSet's source-only content admission, validate actual complete Image dependencies
against same-base Required edges, extent/atlas barriers and Scene cells, and separate source-only
auxiliary refusal from Runtime Product bindings. Then join first-party Host startup, later Scene
requests, typed CPU reads, selected codecs/publication and physical native residency. No fabricated
source records, second Scene transaction or source/cache fallback. Processed input maps, source
plugin facets, target-build selection, CAS, R8/R9 rendering and Nara-owned Editor UI remain open.
No compatibility reader, migration guide or remote publication is added.

# Citations

- [Ordered plan](../../../../plans/2026-10-29-asset-import-and-runtime-delivery-plan.md)
- [Foundation plan](../../../../plans/2026-10-23-foundation-architecture-rebaseline-plan.md)
- [Image decision](../../../../architecture/adr/0033-asset-import-and-render-resource-preparation-seam.md)
- [Runtime Image guide](../../../../guides/runtime-images.md)
- [Pinned Image study](../../../../research/2026-10-29-image-content-and-runtime-provenance.md)
- [Independent consumer](../../../../../tests/fixtures/module-consumer/tests/runtime_image_delivery.rs)
- [Native procedure](../../../../verification/native-editor-qualification.md)
- [Earlier WIP record](../../progress/2026-10/2026-10-29T081235Z-source-independent-runtime-image-ownership-and-final-qualification-in-progress-421a0d14cb7a41c7877c249ec9184b8a.md)
