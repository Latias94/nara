---
type: "Verification Evidence"
title: "NUI-U13 renderer-neutral ECS and Scene UI adapter"
description: "Verifies canonical runtime UI authoring, structured Scene and Prefab identity, completed hierarchy order, headless frame and action projection, reference-game data migration, and atomic Retry replacement."
timestamp: 2026-08-06T05:17:15Z
record_id: "075ab24c755744c1b6bc506b3795ec6b"
producer_id: "codex-root"
source_session: "019fcacd-bf84-7ee1-85a5-a6b920b331e2"
related_plan: "docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md"
git_branch: "refactor/engine-foundation-contracts"
verified_by: "codex-root"
---

# Verification

NUI-U13 closes the legacy ECS Runtime UI execution tree and proves one renderer-neutral
authoring adapter over completed hierarchy plus the immutable UI kernel. This evidence follows the
corrected unit boundary: packet outcomes and GPU pixels belong to NUI-U14, while the exhaustive
six-state and three-physical-target product matrix belongs to NUI-U15.

# Result

Passed. The reference game now loads its HUD and terminal structure from
`reference-game/prefabs/reference-ui.prefab.json`; Rust projects typed gameplay facts and consumes
typed Retry intents without constructing a second UI tree. Canonical version-1 components, stable
runtime and structured Scene/Prefab identity, completed structural order, headless frame/action
projection, Retry generation replacement, and old public-surface removal are verified.

# Evidence

- `cargo nextest run --locked -p nara_text -p nara_ui_kernel -p nara_ui_widgets -p nara_ui -p nara_input -p nara_scene --test-threads=1`: 200 passed, 0 failed across 19 test binaries.
- `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --test first_wave -E 'test(/retry/)' --test-threads=1`: 7 passed, 0 failed. This includes atomic candidate rejection, runtime-only projectile retirement, normalized startup/Retry state, stable replacement identity, and successful semantic Retry.
- `cargo nextest run --manifest-path reference-game/Cargo.toml --locked --test project_content_boot --test-threads=1`: 1 passed, 0 failed. Structured expanded-scene provenance is retained and charged to the Project Content budget.
- `cargo check --manifest-path reference-game/Cargo.toml --all-targets --all-features --locked -j 1`: passed for every supported reference-game target and feature.
- `cargo check --locked -p nara --all-features -j 1`: passed for the root facade and complete feature surface.
- `crates/nara_ui/src/{authoring,codec,runtime,adapter_tests}.rs` own the replacement component vocabulary, canonical codecs, per-surface projection/session state, and focused adapter evidence.
- `crates/nara_scene/src/{provenance,runtime_hierarchy,spawn,product_transaction_tests}.rs` preserve structured source/projection identity and combine Scene plus runtime-only hierarchy retirement into one failure-atomic replacement.
- `reference-game/src/runtime_ui.rs`, `reference-game/src/systems.rs`, and `reference-game/prefabs/reference-ui.prefab.json` prove the data-versus-Rust boundary and exact Retry coalescing path.
- Searches of compiled reference/example/UI entry points find no `UiPanel`, `UiPanelMaterial`, `ComputedUiLayouts`, `UiPointerRoute`, `with_sibling_order`, or `HierarchyConstructionWriter` use.

# Follow-up

Treat NUI-U5B as the dependency-ordered composition checkpoint immediately following U13, then
complete NUI-U14's packet-resource mismatch, atlas saturation, retention/eviction, backend outcome,
device-reset, and GPU readback matrix. Do not use NUI-U14 or NUI-U15 evidence retroactively as an
U13 closure condition.

# Citations

- `docs/plans/2026-08-04-002-refactor-ordered-ui-kernel-and-engine-boundary-correction-plan.md#nui-u13-replace-the-ecs-runtime-ui-adapter`
- `docs/architecture/reference-game-ui-product-slice.md#unit-boundary-decision`
- `docs/architecture/adr/0102-renderer-neutral-ui-text-widget-and-presented-frame-topology.md`
- `docs/architecture/adr/implementation-status.md`
- `docs/migrations/2026-07-engine-foundation.md#nui-u13-1-renderer-neutral-runtime-ui-authoring-adapter`
