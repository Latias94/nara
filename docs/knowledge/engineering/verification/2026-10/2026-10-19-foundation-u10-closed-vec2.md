---
type: Verification Evidence
title: Foundation U10 closed Vec2 shapes and owner migration
description: Finite F32 vector records retain their wire encoding with explicit saved-owner lineage and external editing evidence.
timestamp: 2026-10-19T09:50:15Z
record_id: cf00ab1b570d413da7820a1c2f1e379c
producer_id: codex-foundation-u12-profile-config-probe
run_id: foundation-correction-20261019
related_plan: docs/plans/2026-10-19-1335-refactor-foundation-correction-plan.md
git_branch: refactor/foundation-contract-hardening
---

# Contract And Owner Scope

`PersistentFieldCodec` now describes `Vec2` as a closed record with required `x` and `y`
members, each in the finite F32 domain. The existing F64-backed x/y wire encoding is retained.
Missing or extra members and out-of-domain values reject. Existing recursive container codecs
carry this contract into Optional, List and Map descendants. `ComponentCodec` is now crate-private;
its unused public exports and convenience methods were retired. The public closure-based codec
registration APIs remain available to independent consumers.

The [owner inventory](../../../../audits/2026-10-19-vec2-schema-owner-inventory.md) identified
`reference-game.components` / `InitialVelocity2d` as the only existing persisted owner requiring
a new generation. Its component advances from v1 to v2, and its catalog and native binding
advance from v4 to v5. An explicit v1-to-v2 migration validates the complete closed value before
returning it, preserving valid values and rejecting malformed predecessors without dropping
members. Catalogs v1-v4 remain immutable. The v5 successor inherits predecessor tombstones;
it does not redeclare them.

Current Reference Game Scene/Prefab records use component v2. A saved component-v1 Scene
fixture remains available for upgrade evidence. Current authoring bundles for the Reference
Game and generic-editor image-import fixture were regenerated with the exact current and
predecessor catalogs, and packaging includes catalog v5 alongside the retained history.
Manual closed-record owners and consumers whose Vec2 values are runtime-only keep their
existing schema lineage. The new independent `consumer.math.Motion` owner starts at generation 1.

# Verification

The root agent executed the following runs on Windows with Rust 1.95.0 and serialized Cargo,
using `CARGO_BUILD_JOBS=1`. The independent consumer retained its own lockfile and reused the
shared build target. This shard records those reported executions; it does not claim an
additional test run.

- `79f4ba64-3ea6-4c5f-b2ec-b804642f71d2`: 68 focused reflection and Scene tests passed.
  Coverage includes the closed shape and unchanged encoding, required and extra members,
  numeric-domain rejection, recursive vectors, and the explicit owner-successor/migration
  requirement.
- `68c7c4d9-caf4-4dfb-b476-7992e761d63a`: all 12 independent consumer tests passed across
  five binaries. Two manual closure-codec cases preserve the public codec contract. The Vec2
  consumer exercises generic edits, Undo, Save and Reopen with a data-only authoring bundle
  and no native registry installed. Other Scene and U13 panel evidence remains separate.
- `af3ccd7d-0f69-4cef-8587-65113fc5bbbc`: all 38 Reference Game tests passed across
  `authoring`, `desktop_flow`, `first_wave`, `project_content_boot` and `vec2_schema_migration`.
  The saved-v1 fixture upgrades explicitly, saves and reopens as current; strict Check admits
  it without rewriting source bytes. The current v5 owner/binding matches, while missing,
  extra and out-of-domain vector records reject.

An earlier Reference Game run, `10bcf2df-bfc7-442d-86f9-889077b2781f`, passed 36 of 38 cases.
The two failures were traced to fixture expectations: the desktop catch-up scenario encountered
the configured `max_delta` cap, and the content boot metadata expectation omitted a charged
read. Their source-backed corrections preceded the final 38-pass run. They were not v5
schema-lineage failures.

`authoring_bundle --write` completed successfully before these recorded results. Its generated
output check has not yet been executed at this evidence boundary.

# Completion Boundary

Focused owner, saved-predecessor and independent consumer checks passed. The root agent still
must execute `authoring_bundle --check`, the final workspace gate and final review, and verify
the committed source state. This record does not close U10 or the enclosing goal, and it does
not claim native Inspector or platform qualification from the data-only consumer run.

# Sources

- [Foundation correction plan](../../../../plans/2026-10-19-1335-refactor-foundation-correction-plan.md)
- [Vec2 owner inventory](../../../../audits/2026-10-19-vec2-schema-owner-inventory.md)
- [Reflection math contracts](../../../../../crates/nara_reflect/tests/math_reflection.rs)
- [Saved-owner migration workflows](../../../../../reference-game/tests/vec2_schema_migration.rs)
- [Independent data-only Vec2 authoring](../../../../../tests/fixtures/module-consumer/tests/vec2_authoring.rs)
