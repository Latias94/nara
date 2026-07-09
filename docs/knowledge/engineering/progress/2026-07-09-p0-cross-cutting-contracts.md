---
type: Work Progress
title: P0 Cross-Cutting Engine Contracts
timestamp: 2026-07-09T18:41:08+08:00
tags:
  - architecture
  - adr
  - asset
  - diagnostics
---

# Summary

The P0 cross-cutting contract pass accepted four architecture decisions:

- ADR 0035: `nara.toml` is the file-backed project settings authority, with explicit code-first
  resource/plugin overrides for embedded apps.
- ADR 0036: event/message/resource queues are allowed when their lifecycle class, retention,
  cleanup stage, producer, consumer, and replay/diagnostic role are explicit.
- ADR 0037: asset load/reload request, cache, failure, and lifetime policy is owned by the asset
  seam; GPU objects remain backend cache entries.
- ADR 0038: scene/prefab authoring identity is provenance-aware; prefab-expanded projections must
  write back through explicit override or convert-to-local flows.

# Code Impact

- `nara_asset` now depends on `nara_diagnostic`.
- `AssetReloadDiagnostics` records source-change scheduling failures.
- `resolve_asset_source_changes` no longer discards `SourceChangeResolver` errors.
- Root `nara` facade re-exports `AssetReloadDiagnostics`.

# Verification

See [verification/2026-07-09-p0-cross-cutting-contracts.md](../verification/2026-07-09-p0-cross-cutting-contracts.md).

# Citations

- [ADR 0035](../../architecture/adr/0035-project-manifest-and-runtime-settings-authority.md)
- [ADR 0036](../../architecture/adr/0036-event-message-and-resource-queue-lifetime.md)
- [ADR 0037](../../architecture/adr/0037-asset-load-request-cache-and-lifetime-policy.md)
- [ADR 0038](../../architecture/adr/0038-scene-prefab-authoring-identity-and-provenance.md)
- [Asset reload diagnostics implementation](../../../crates/nara_asset/src/reload.rs)
