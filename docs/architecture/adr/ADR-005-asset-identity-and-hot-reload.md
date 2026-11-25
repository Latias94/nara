# ADR-005 Asset identity and hot reload

**Accepted on 2026-11-24**

- Identify persistent assets with `AssetId(Uuid)`. Track a small RON `<asset>.meta` sidecar beside
  each source with the ID, importer, and settings. Generate a UUID v7 at first discovery. Move the
  asset and its sidecar together so references survive renames and moves.
- Resolve IDs through `nara_asset` and load typed `Handle<T>` values asynchronously. Use
  `AssetSource` for development files, cooked `.npak` entries, and memory-backed test inputs.
- Use upstream `notify` under the optional `hot-reload` feature. Retire `vendor/notify`. Reload changed
  source scenes, prefabs, shaders, materials, and textures in the editor and development Play session.
- Key incremental cooking under `.nara/cache/` by source bytes, sidecar bytes, importer version,
  and target platform. Traverse referenced dependencies at Cook time so Export contains the assets
  the game needs.
- Keep CPU image import, reload, Cook, and local artifact caching in `nara_asset`.
  `nara_render` owns prepared image views, upload, and GPU residency.
