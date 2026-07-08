---
type: "Memory Event"
title: "Verification: U8 scene and prefab stable asset preflight implemented"
description: "U8 wired scene/prefab asset references through ProjectAssetDatabase-aware component decode context before World mutation."
timestamp: 2026-07-08T11:57:16Z
event_kind: "Verification"
---
# Event

U8 scene/prefab stable asset preflight implemented. `nara_reflect` now exposes `ComponentDecodeContext` for asset-aware component preflight and `ComponentEncodeContext` for export policy. `nara_scene` validates and spawns with optional `ProjectAssetDatabase`, cloning `AssetServer` into a scratch preflight context and writing it back only after the full scene/prefab preflight succeeds.

Sprite and tilemap codecs now parse `AssetRef::StableId`, resolve known stable IDs into typed handles during preflight, diagnose unknown IDs with entity/component/field/asset context, and keep path refs working without a project database. Scene export defaults to path refs and can explicitly emit stable IDs with `AssetRefExportPolicy::StableIdWhenKnown`.

# Verification

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo check --workspace --features serde`
- `cargo nextest run -p nara_scene -p nara_sprite -p nara_tilemap`
- `cargo nextest run -p nara`
- `cargo nextest run --workspace`
- `cargo run -q --features serde --example scene_prefab_roundtrip`
- `git diff --check`

# Impact

Persistent scene/prefab data can now carry semantic path or stable asset references without serializing runtime `AssetId` values. Failed asset resolution no longer allocates scene entities or commits partial `AssetServer` state into the target world.

# Citations

- [Plan](../../../plans/2026-07-08-004-feat-asset-render-resource-seam-plan.md)
- [ADR 0033](../../../architecture/adr/0033-asset-import-and-render-resource-preparation-seam.md)
- [Foundation](../../../architecture/nara-foundation.md)
