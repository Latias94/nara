# Vec2 Schema Owner Inventory

This is the migration inventory for U10 of the
[foundation correction plan](../plans/2026-10-19-1335-refactor-foundation-correction-plan.md).
It records affected schema owners rather than treating a shared helper change as permission to
rewrite every catalog. Verification results belong in the implementation evidence.

| Owner or consumer | Previous contract | Required change |
| --- | --- | --- |
| `reference-game.components` / `InitialVelocity2d` | Catalog v4, native binding v4, component v1, derived opaque Vec2 map | Preserve catalogs v1-v4. Publish catalog v5 against v4, binding v5 and component v2 with an explicit v1-to-v2 migration validating the closed shape. Upgrade current Scene/Prefab records; retain a separate saved v1 fixture for strict Check and save/reopen evidence. |
| `nara.transform.components` | Hand-written closed numeric records | No helper-derived shape or schema change; retain the existing owner lineage. |
| `nara.sprite.components` | Hand-written closed numeric records | No helper-derived shape or schema change. |
| `nara.tilemap.components` | Hand-written closed numeric records | No helper-derived shape or schema change. |
| Render, Scene hierarchy and UI owners | Manual schemas or no persistent Vec2 helper field | No owner version change. |
| `collect-game.components` | Derived scalar configuration; Vec2 appears only in runtime `GameSnapshot` | No persistent shape change. |
| Reflection test probes, including `GeneratedProbe` | Ephemeral test schemas with no saved owner catalog | Use the new helper and extend recursive shape/codec assertions. No manufactured persistence lineage. |
| Independent module consumer | New `consumer.math.Motion` with Vec2, scalar and Vec<Vec2> | Start at component/owner generation 1 with a closed shape. Prove data-only editing without installing its native registry. Existing manual closure-codec consumer remains the external codec interface proof. |

Current authoring bundles for the Reference Game and the generic-editor image-import fixture carry
the affected owner's exact current/predecessor pair. They must be regenerated from the selected
compiled owner definitions, retaining historical catalogs unchanged. Packaging includes the new v5
catalog alongside the retained predecessors.

The x/y map wire encoding is unchanged. The accepted value grammar becomes narrower: both axes are
required finite F32 values stored as F64, and extra members reject. The owner migration preserves
well-formed values and rejects malformed predecessors before publication; it never silently strips
unknown members. External owners using the former derive helper must supply the same explicit
version/catalog/migration work for their own Vec2-containing fields and containers.
