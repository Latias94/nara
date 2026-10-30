# Local Build Recipes And Publication

Date: 2026-10-31. Bounded source study for
[R6b](../plans/2026-10-31-local-artifact-build-plan.md); no production qualification.
Pins: Bevy `85503941e488152711a4695bbe248e9ed9f453fc`,
Godot `81ab961f91066a63bcbf3ec340962bb1a1ada558`.

## Constraints From The References

Bevy source metadata names the selected processor and settings; its processing hash includes
raw metadata bytes. This does not prove semantic settings normalization. Godot resolves compatible
parameters/defaults and writes effective importer options in its concrete option order.
Nara must hash and execute the same normalized values, sorting only unordered collections and
preserving semantic sequence order.
Sources: [Bevy meta](../../repo-ref/bevy/crates/bevy_asset/src/meta.rs#L81),
[processor input hash](../../repo-ref/bevy/crates/bevy_asset/src/processor/mod.rs#L1073),
[Godot parameters](../../repo-ref/godot/editor/file_system/editor_file_system.cpp#L2901).

Bevy processor dependencies can come from processed reads. Godot texture imports can directly
read an additional normal-map source, while imported-resource dependency queries describe another
layer. Raw source bytes, processed member/format identity, optional presence observations and
Runtime Product references are distinct facts. A full input hash is not an output content hash.
Sources: [processed dependency promotion](../../repo-ref/bevy/crates/bevy_asset/src/processor/process.rs#L364),
[processed byte reads](../../repo-ref/bevy/crates/bevy_asset/src/loader.rs#L618),
[Godot normal map](../../repo-ref/godot/editor/import/resource_importer_texture.cpp#L808),
[full hash](../../repo-ref/bevy/crates/bevy_asset/src/meta.rs#L303).

Godot texture selection can depend on project settings and Host-preferred format. Export selects
among generated variants using target features/priorities. The effective consumed target facts
belong in Nara's executing recipe; a target label without actual behavior is insufficient.
Sources: [texture settings](../../repo-ref/godot/editor/import/resource_importer_texture_settings.cpp#L38),
[texture variants](../../repo-ref/godot/editor/import/resource_importer_texture.cpp#L959),
[export selection](../../repo-ref/godot/editor/export/editor_export_platform.cpp#L1603).

Group scheduling is not group atomic publication. Godot atlas import saves shared output then
member resources, with subsequent metadata updates. Bevy writes its destination directly and
uses unfinished-work logging for later reconstruction. Nara's complete success boundary must
follow all verified members and the manifest/index publication, with honest durability evidence.
Sources: [atlas output](../../repo-ref/godot/editor/import/resource_importer_texture_atlas.cpp#L312),
[member output](../../repo-ref/godot/editor/import/resource_importer_texture_atlas.cpp#L400),
[processor write protocol](../../repo-ref/bevy/crates/bevy_asset/src/processor/mod.rs#L1159).

Both references have skip behavior that is not proof of a current complete group. Bevy can reuse
an earlier dependency result before another update; Godot has mtime/invalid-import paths and
version checks with specific policies. Nara must distinguish current verified reuse, invalid
cache, failure and stale last-good. Native Rust remains trusted: these mechanisms do not
automatically hash every compiler/environment input.
Sources: [Bevy skip](../../repo-ref/bevy/crates/bevy_asset/src/processor/mod.rs#L1121),
[dependency updates](../../repo-ref/bevy/crates/bevy_asset/src/processor/mod.rs#L1646),
[Godot checks](../../repo-ref/godot/editor/file_system/editor_file_system.cpp#L562).

## Concrete Nara Consumers

The external TextBundle domain already consumes primary identity/settings/continuity and an
actual auxiliary suffix through `read_dependency`. Its source output has resolved text, while its
Runtime codec contains no authoring suffix path. The complete primary/auxiliary Product set shares
one actual member. A durable group must preserve this relationship rather than fabricate multiple
unused output files. Its continuity assignments/retired IDs need domain-semantic normalization,
not arbitrary Vec-order hashing.
Sources: [bundle importer](../../tests/fixtures/module-consumer/asset-domain-consumer/src/bundle.rs#L296),
[complete source workflow](../../tests/fixtures/module-consumer/asset-domain-consumer/tests/compound_workflow.rs#L331),
[Runtime pack consumer](../../tests/fixtures/module-consumer/asset-domain-consumer/tests/runtime_base.rs#L650).

Stock Image import and Runtime encode exist, but there is no existing target-sensitive transform.
The device texture-dimension limit is a real receiving constraint. Admission alone can prove
target-dependent rejection; two successful different outputs need an explicit real conversion
policy. Select a bounded CPU Image cook with `Reject` or proportional `FitNearest`, preserving
Product identity and exact nearest texels while stripping authoring provenance. Use a valid
4096-by-1 PNG and actual Runtime roundtrip for 4K/2K targets. This is separate from graphics
execution and must not silently shrink TileSet atlas semantics.
Sources: [Image import](../../crates/nara_image/src/import.rs#L665),
[Runtime format](../../crates/nara_image/src/runtime.rs#L117),
[device receiving bound](../../crates/nara_render_wgpu/src/texture.rs#L907).

The first source-provenance correction removes imaginary artifact paths and accurately names
recipe versus pixel content. It is a prerequisite, not completed CAS, target-build/export or
processed-dependency proof. The complete parent outcome still requires actual execution,
publication interruption/reopen and independent original Runtime custody.
