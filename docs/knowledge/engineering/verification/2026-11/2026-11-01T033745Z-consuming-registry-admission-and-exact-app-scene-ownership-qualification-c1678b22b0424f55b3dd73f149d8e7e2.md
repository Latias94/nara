---
type: "Verification Evidence"
title: "Consuming registry admission and exact App Scene ownership qualification"
description: "Builder to immutable snapshot replacement, failed-candidate recovery, installed-but-unadmitted refusal and complete migrated consumers qualified on Windows with scoped DX12 evidence."
timestamp: 2026-11-01T03:37:45Z
record_id: "c1678b22b0424f55b3dd73f149d8e7e2"
producer_id: "codex-foundation-main"
run_id: "registry-ownership-20261101"
source_session: "01a1629b-6c28-7203-87c5-85328214df4a"
related_plan: "docs/plans/2026-11-01-component-registry-ownership-plan.md"
git_branch: "refactor/foundation"
git_commit: "7f69d53488864289e922e609dbb3ff484e4dd44a"
---

# Verification

Implementation `7f69d53488864289e922e609dbb3ff484e4dd44a` replaces the mixed public registry with
consuming construction and the existing immutable admitted definition. This is the complete
[registry ownership unit](../../../../plans/2026-11-01-component-registry-ownership-plan.md)
inside the active foundation Goal, not completion of that Goal or new Schema evolution support.
Tests ran against the same implementation before commit; the final language-fixture change is
rustfmt-only import wrapping. No production source changed after its recorded checks.

Environment: Windows11, Rust `+1.95.0`, PowerShell7, jobs1, one serial Cargo invocation at a time
in the checkout, with the original root target reused by independent workspaces. Native tests use
the actual DX12 backend, `NARA_WGPU_FORCE_FALLBACK=0` and
`DEAR_IMGUI_REQUIRE_WGPU_ADAPTER=1`. No push or PR was created.

# Result

- `ComponentRegistryBuilder::freeze(self)` publishes the sole `ComponentRegistrySnapshot` or
  returns the unchanged builder and original error through `ComponentRegistryFreezeError`.
  Queries, migrations and codec execution require the snapshot; registration requires the builder.
  No mixed-type alias, `from_snapshot`, thaw or phase-only reader errors remain.
- Private App Building/Preloaded/Admitted dispatch preserves installation identity and mutation
  revision. Preloaded receipt validation does not replay provider callbacks. Failed admission
  restores the candidate without retrying failed App activation or publishing Runtime authority.
- Provider private candidate replacement/mode/owner and aggregate failure-atomicity remain checked.
  Exact definition identity remains separate from content equality and target-World eligibility.
- Scene preparation, retained commit, export and actual data read/set/remove refuse installed but
  unadmitted owners. Only genuinely owner-free Worlds use the explicit standalone path. Explicit
  preview retains its separate execution definition without losing the actual managed Host basis.
- Scene bindings and `SchemaValidationInput` no longer retain duplicate native wrappers/views.
  Domain registration, Root, Tooling, Editor, examples, independent consumers, Reference and samples
  use the replacement directly. Current ADRs/guides describe this ownership instead of old phases.

# Evidence

| Scope | Command shape and actual result |
| --- | --- |
| Reflect owner | `nextest run -p nara_reflect --all-features --locked`:395 passed, no skips, run `4d446fa5-a20a-44c7-b00c-8c1fe3e77428`. |
| Scene and affected domains | All-feature Scene/Tooling/Transform/Sprite/Tilemap/Render/UI cohort:1,219 run,1,218 passed,1 new fixture assertion failed,9 skipped, run `09930e63-12bf-49e3-aa1f-92c53a7ee4f4`. Current Bevy initializes internal resource entities; the new rejection test now compares actual before/after membership. Its focused rerun passed1/1,489 filtered, run `f80b393a-099d-460d-86be-bbdb65dd62b3`. No production check was weakened. |
| Root and Editor/public | All-feature Root/Editor libraries plus13 selected integration binaries:1,668 passed,43 skipped, run `f8fac87d-98ee-40e7-ac2a-f4026a5c53ce`,523.609s. Includes plugin/data composition, content boot, authoring, file workspace, Inspector, exact patches, serialization/identity, persistence, Play and public compile boundaries. |
| Selected native | The4 tests below all execute and pass, run `00eb7883-ab30-48dd-ab83-820bad33a3d1`,13.981s;1,499 nonselected cases excluded. Exact Root/Editor all-feature library union reused existing binaries. |
| Owner compile-fail docs | `test -p nara_reflect -p nara_scene --all-features --doc --locked`:7 Reflect and9 Scene cases pass;1 existing inventory-template example ignored. Cases cover builder query/codec/Scene misuse, consumed builder reuse and snapshot registration refusal. |
| Independent direct consumer | Locked module-consumer inventory/manual-codec/Scene-preparation/data/property tests:30 passed,0 skips, run `b5dd1fe3-5ff9-4ea1-b7af-f05b5c69307f`. Includes two failed admissions repaired on the returned builder while an original Scene definition remains retained. Its3 new compile-fail docs also pass. |
| Renamed Root consumer | Locked offline renamed-root fixture:7 passed,0 skips, run `7fadbf81-c3b6-4e7e-a72f-14ebd881df8e`. The preexisting stale Input dependency lock needed a minimal serde/serde_json/itoa/zmij refresh; added JSON versions match the root rather than updating unrelated dependencies. |
| Reference Game | All-feature authoring/first-wave/plugin/Prefab-startup/UI-roundtrip/Vec2-migration selection:40 passed,0 skips, run `eb9b0361-dd1b-4891-9ec0-b42346b3cd12`,7.093s. All-feature executable targets also built as part of this command. |
| Strict independent lint | Module-consumer workspace, all targets, locked `clippy -- -D warnings` passes. One old projection fixture now supplies an explicit database/scope resolver instead of an obsolete default-object argument; no production compatibility path was added. |
| Workspace lint/build | Root workspace all features/all targets locked `clippy -- -D clippy::correctness` passes,5m30s. Other existing warning categories remain; this is not a warning-free workspace claim. |
| Independent builds | Locked all-feature/all-target checks pass for renamed Runtime Runner, stock-composition consumer, behavior-language trial, collect-game workspace and two-level-game. These compile checks do not rerun managed child language execution or all GPU effects. |
| Minimal/portable | Root library checks pass with no default features, with runtime-core, and on wasm32-unknown-unknown with `runtime-2d,presentation-2d,presentation-ui,tooling,serde`. The wasm result is compile-only; platform-specific unused-code warnings remain. |
| Formatting and deletion | Root and8 affected independent workspace fmt checks pass. The language trial needed one import-wrap correction under its own fmt configuration. Staged whitespace checks and118 current-contract local links pass. Rust source/consumers have no old mixed `ComponentRegistry`, old provider merge entry or phase-only reader errors. Historical/superseded documents retain their original names and evidence. |

Native tests selected from the existing protocol:

- `native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime`
- `native_retained_scene_receipt_survives_prefab_navigation_and_bound_close`
- `native_ui_canvas_priority_survives_closing_retained_scene`
- `native_complete_prefab_conversion_reviews_commits_and_round_trips_scene_and_prefab`

Independent static reviews found no P1/P2 in final Core/App and Scene ownership. Dalton's
additional real-data-record review confirmed the new test uses a genuine admitted owner/value;
the test itself passes in the domain cohort. These reviews are source evidence, separate from
runtime execution. Recoverable integration mistakes and original failed assertions remain recorded
in the [progress lineage](../../progress/2026-11/2026-11-01T030917Z-registry-public-and-dx12-consumers-pass-before-final-independent-and-feature-checks-c82a8b32f14745cba8cabdb0c035b463.md).

# Follow-up

The full foundation Goal remains active. Coordinated Schema/value/claim evolution and source
extraction, joint motion/executor selection, durable material generations, editable Nara-drawn
Inspector, broader toolkit/platform and independent human-author evidence retain their own gates.
Optional embedded Bevy reflection storage is a separate traced-consumer cleanup, not an automatic
consequence of this ownership split. Proposed0120/0121 are not promoted by this qualification.

Native scope is the selected current frontend on Windows/DX12. There is no new monitor scanout,
OS IME/accessibility, Linux/macOS runtime or performance/latency claim. Existing ignored cases were
not silently counted as passing. No broad requalification is needed solely for this evidence commit.

# Citations

- [Registry ownership unit](../../../../plans/2026-11-01-component-registry-ownership-plan.md)
- [ADR0081](../../../../architecture/adr/0081-schema-source-stable-identity-catalog-and-runtime-binding.md)
- [Public component guide](../../../../guides/persistent-components.md)
- [Scene admission regressions](../../../../../crates/nara_scene/tests/registry_admission.rs)
- [Independent retained-definition consumer](../../../../../tests/fixtures/module-consumer/tests/component_inventory.rs)
- [Native procedure](../../../../verification/native-editor-qualification.md)
