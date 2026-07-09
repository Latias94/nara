---
type: "Engineering Log"
title: "Material/2D M2 Image Sampler Removal"
description: "M2 implementation checkpoint for nara_material, image sampler removal, material-aware sprite/tilemap batching, and wgpu cache split."
tags: ["material", "image", "sprite", "tilemap", "wgpu"]
timestamp: 2026-07-09T16:31:28+08:00
status: "passed"
---

# Material/2D M2 Image Sampler Removal

Implemented the material/sampler boundary above images.

- `nara_material` now owns sampler/filter/address modes, alpha mode, inline 2D material descriptors, semantic image refs, and material keys.
- `nara_image` now treats `ImageAsset` and `PreparedImageResource` as image content/import identity only.
- `nara_sprite` and `nara_tilemap` now expose material-first runtime wrappers while persistent codecs still serialize semantic asset references.
- `nara_sprite_render` now resolves image handles into `SpriteMaterialKey` values and splits/merges batches by image, sampler, alpha mode, and tint.
- `nara_render_wgpu` now caches image texture uploads by prepared image snapshot separately from sampler/material bind groups.

Verification is recorded in [2026-07-09-material-2d-m2.md](../../verification/2026-07-09-material-2d-m2.md).
