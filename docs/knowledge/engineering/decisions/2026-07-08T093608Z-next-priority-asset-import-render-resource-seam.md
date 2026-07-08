---
type: "Decision"
title: "Next Priority Is Asset Import And Render Resource Preparation Seam"
tags: ["nara", "asset", "render", "next-priority", "architecture"]
timestamp: 2026-07-08T09:36:08Z
status: "accepted"
git_branch: "main"
related_adr: "../../../architecture/adr/0033-asset-import-and-render-resource-preparation-seam.md"
---

# Decision

The next implementation planning pass should prioritize the asset import and render resource
preparation seam before direct texture upload, material expansion, runtime UI rendering, or 3D mesh
work.

# Context

The scene/prefab serialization foundation introduced persistent `AssetRef` and typed runtime
handles. The renderer currently supports colored sprite/tilemap batches, but textured rendering
would be the first place where asset identity, import artifacts, hot reload, and backend GPU
resource lifetime meet.

# Rationale

Doing texture upload directly in sprite or wgpu code would create shallow convenience paths that
future UI, materials, atlases, fonts, and 3D assets would need to replace. A dedicated import and
render prepare seam keeps source identity, imported artifacts, and backend GPU caches separate.

# Next Action

Create the next implementation plan around:

- source-side `.meta` identity and stable `AssetRef::StableId` resolution;
- importer registry and imported artifact cache records;
- image/texture import as the first concrete importer;
- backend-neutral render resource preparation state in `nara_render`;
- wgpu texture/sampler/cache creation in `nara_render_wgpu`;
- sprite/tilemap texture usage through typed handles and prepared render resources.

# Citations

- [ADR 0033](../../../architecture/adr/0033-asset-import-and-render-resource-preparation-seam.md)
- [Current engineering state](../current-state.md)
- [Scene/prefab final verification](../verification/2026-07-08T091921Z-scene-prefab-serialization-foundation-final.md)
