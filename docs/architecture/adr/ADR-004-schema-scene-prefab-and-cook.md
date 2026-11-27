# ADR-004 Schema, scene, prefab, and cooked formats

**Accepted on 2026-11-24**

Replace `bevy_reflect` with `#[derive(Schema)]` for type and field metadata, schema hashes, dynamic
field access, and RON and binary codecs. A canonical `FieldPath` addresses the same field in the
inspector, prefab overrides, and future animation. Use compact `PackedU16`, `PackedU32`, `PackedI32`,
`PackedF32`, and byte payloads for bulk data.

## Authoring

- Use compact `.scene.ron` and `.prefab.ron` with a flat entity table, stable UUIDs, asset aliases,
  deterministic ordering, and default elision. Store sparse overrides by `FieldPath`.
- Derive nested prefab entity UUIDs from the instance UUID and the child UUID using UUID v5. Remap
  internal entity references and preserve external ones. Detect cycles during expansion and report
  them as diagnostics. Revert removes an override. Apply changes the prefab and clears the override
  as one editor operation.
- Preserve unknown component payloads losslessly when a project Schema is unavailable. Show them
  read-only, allow an explicit undoable removal, and round-trip them when unrelated fields change.
  This is the v2 successor to the unknown-component contract in v0 ADR 0090.

## Delivery

- Cook prefabs and overrides into flat `.nscn` data with an `NSCN` header, asset dependency IDs,
  entity UUID and parent-index rows, and columns keyed by component type. Use compact binary
  payloads and contiguous packed arrays. Instantiate ECS components without an intermediate value tree.
  Derived Schema declares the native Postcard codec in the catalog. A handwritten Schema must
  declare a matching codec before it can be cooked or registered for native loading.
- Place cooked assets in a deterministic `.npak` archive with a versioned `NPAK` header and a
  directory sorted by `AssetId`. Support optional per-entry LZ4 compression and independent reads.
  Use the same virtual UUID paths for project directories, archives, and in-memory sources.
- Cook PNGs to a binary RGBA8 image value and sprite material descriptors to stable shader UUIDs
  and linear RGBA uniforms. Validate each referenced WGSL fragment against the stock 2D material
  interface before including it in the archive.
- Let the game own its save snapshots separately from its cooked asset and scene formats.

`nara_scene::authoring` owns compact authoring validation, prefab expansion, and atomic saves.
Documents carry stable document and entity UUIDs. Component payloads use schema version one by
default, with explicit version overrides for other revisions. `nara_reflect::Schema` supplies
field metadata, typed codecs, and defaults. Projects can also construct worlds in Rust.
