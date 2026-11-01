# R12 Component Registry Construction And Admitted Ownership

**Status:** Implemented as `7f69d53` and qualified for the migrated public/independent workflows
and selected Windows/DX12 trajectories. Full foundation work remains open.
**Parent:** [Foundation rebaseline R12](2026-10-23-foundation-architecture-rebaseline-plan.md#r12-consumer-integration-and-canonical-contract-consolidation).
**Scope:** One ownership and ergonomics unit within the existing foundation plan, preserving R5's
Schema, migration and retained-definition contracts. This is not a separate roadmap.
**Evidence:** [Owning, public, independent and native qualification](../knowledge/engineering/verification/2026-11/2026-11-01T033745Z-consuming-registry-admission-and-exact-app-scene-ownership-qualification-c1678b22b0424f55b3dd73f149d8e7e2.md).

## Problem And Source Evidence

At the starting baseline `5acab02`, successful registration leaves a public `ComponentRegistry`
in Building state. Calling `schema` then returns `None`, including when the requested declaration and binding were registered
successfully. Repeated fixture mistakes expose a real ordering burden: callers must know that an
apparently ordinary lookup is meaningful only after a separate admission operation.

This behavior was intentional in that implementation. The replacement must preserve admission,
while making its prerequisite visible in ownership and parameter types.

The source anchors below describe that starting implementation, rather than the replacement's
current symbol locations.

| Starting fact | Source evidence |
| --- | --- |
| One public type contains Building, Frozen and an internal transition state. | [Registry state and public type](../../crates/nara_reflect/src/registry.rs#L836). |
| Registration updates a candidate; freeze validates before moving it into an immutable `Arc`. | [Schema registration](../../crates/nara_reflect/src/registry.rs#L1364), [freeze](../../crates/nara_reflect/src/registry.rs#L1185), [complete validation](../../crates/nara_reflect/src/registry.rs#L2184). |
| Building lookups report absence, while other reads report `NotFrozen`; migration can report `UnknownComponentId` for the same phase error. | [Queries](../../crates/nara_reflect/src/registry.rs#L1722), [migration](../../crates/nara_reflect/src/registry.rs#L1818), [explicit Building-query test](../../crates/nara_reflect/tests/registry_contract.rs#L1015). |
| The immutable executable authority already exists, with shared identity, Schema queries and typed codec execution. | [Snapshot definition](../../crates/nara_reflect/src/registry.rs#L853), [snapshot reads](../../crates/nara_reflect/src/registry.rs#L893), [codec execution](../../crates/nara_reflect/src/registry.rs#L1001). |
| `from_snapshot` creates a Frozen wrapper over the same data; it does not thaw or rebuild bindings. | [Constructor](../../crates/nara_reflect/src/registry.rs#L1150). |
| Failed freeze supports repairing the original candidate and retrying. | [Missing-binding repair](../../crates/nara_reflect/tests/registry_contract.rs#L455), [migration-chain repair](../../crates/nara_reflect/tests/registry_contract.rs#L1329), [loaded-catalog repair](../../crates/nara_reflect/tests/catalog_format.rs#L416). |
| Provider callbacks operate on a private owner candidate before validated aggregate merge. | [Provider contract](../../crates/nara_reflect/src/provider.rs#L438), [candidate construction and validation](../../crates/nara_reflect/src/registry.rs#L1934), [merge ordering](../../crates/nara_reflect/src/registry.rs#L1277). |

No audited production consumer requires thawing a published definition. Consumers either build a
new candidate or retain an admitted snapshot. The combined type mainly lets the App registration
operation dispatch between fresh construction and preloaded receipt validation.

Retaining the combined public type and changing lookups to return `NotFrozen` would improve error
reporting at lower migration cost, but would retain the ordering obligation in every reader.
The existing snapshot and actual consumer split justify removing that obligation structurally.

## Selected Ownership Shape

Use `ComponentRegistryBuilder -> ComponentRegistrySnapshot`. Keep the existing snapshot as the
only immutable native-definition authority. Remove the mixed public `ComponentRegistry`, its
`from_snapshot` constructor and stored registry/snapshot dual views. Do not retain a compatibility
alias, a forwarding facade, or a second immutable registry type.

- The builder owns mutable declarations, bindings, migrations and provider contributions. Move
  registration validation and owner-catalog candidate construction here. Any necessary declaration
  observation must be explicitly named as candidate inspection and convey no admission authority.
- The snapshot owns admitted Schema queries, field/type/native-binding lookups, capability checks,
  migrations and codec execution. Move the remaining read and migration operations here, using
  the same immutable data and existing validators rather than synchronizing parallel tables.
- Readers and Scene execution entries accept the snapshot. A builder cannot satisfy their
  parameters. `schema == None` then means absence from an admitted definition.
- Keep raw owner-local construction distinct from provider composition. An admitted raw native
  definition still cannot publish product composition/executable fingerprints without the
  required receipts; `SchemaCompositionUnavailable` remains a meaningful operation error.
- Preserve typed exact-`T` codec preparation, one-to-one native binding and explicit error versus
  actual entity/component absence. Snapshot admission still does not certify a target World.

Prefer a consuming `ComponentRegistryBuilder::freeze(self)` returning the snapshot on success.
Consuming `finish(self)` is an equivalent naming alternative; choose one operation, not two
overlapping public entries. The implementation's failure value must return ownership of the
builder together with the unaltered original `ComponentRegistryError`, with a direct way to
extract both. A bare `Result<ComponentRegistrySnapshot, ComponentRegistryError>` that destroys
the failed candidate is insufficient. Missing bindings or migration edges must remain repairable
on that returned builder before retry. Validation failure publishes no snapshot.

Successful consumption leaves no reusable builder over the published definition. Do not add
`thaw`, `into_builder`, snapshot-to-builder reconstruction, copy-on-write editing or a generic
public typestate framework. A successor is constructed from explicit owner declarations/history
and newly registered bindings. Existing snapshots retain their exact original definitions.

## App And Provider Ownership

Keep lifecycle dispatch in a private App owner, with these mutually exclusive states:

| Private state | Allowed work and transition |
| --- | --- |
| `Building(builder)` | Provider collision preflight and isolated owner-candidate registration. The registry plugin's successful finish admits the complete builder and transitions to `Admitted`. |
| `Preloaded(snapshot)` | Resolve trusted source receipts and compare them with the exact retained definition. Do not replay provider validation or registration callbacks. Successful registry finish transitions to `Admitted` with the same snapshot allocation. |
| `Admitted(snapshot)` | Immutable reads and receipt validation where required by existing composition behavior. No registration callback, mutation or transition back to Building. |

The current owner creates a build seed, validates the declared provider set, freezes, then installs
runtime authority: [build](../../crates/nara_reflect/src/plugin.rs#L276),
[finish](../../crates/nara_reflect/src/plugin.rs#L329). Preserve that ordering and fallible plugin
finish semantics. Builder retry does not introduce automatic App seal retry or partial Runtime
activation. A failure must not make the App Ready or install the registry runtime authority.

Keep installation identity and structural mutation observation on the stable private App owner.
Its internal state transition must preserve the installation's instance token and must not look
like resource removal/reinsertion. Retain the mutation-revision hooks and runtime-generation/fault
checks. Comparing only snapshot pointers would miss replacing the owner with a new wrapper over
the same definition. The current [authority check](../../crates/nara_reflect/src/plugin.rs#L149)
and [replacement/reinsertion probes](../../crates/nara_reflect/src/plugin.rs#L473) establish the
required distinction between definition identity and installation identity.

Keep owner presence independently observable from admitted-snapshot availability. Only a World
with no registry owner installation may use the standalone Scene path. A present Building or
unfinished Preloaded owner has no admitted Runtime authority yet and must reject preparation,
commit and export, even when the caller supplies a separately admitted snapshot. A query that
returns no snapshot cannot by itself distinguish that refusal from the standalone case. Preserve
this fact through a narrow owner/basis query or equivalent private Scene integration, without
reintroducing a public mixed-state registry. The current
[World query](../../crates/nara_reflect/src/plugin.rs#L215),
[Scene basis selection](../../crates/nara_scene/src/spawn/preparation.rs#L77) and
[export selection](../../crates/nara_scene/src/export.rs#L208) rely on this distinction under
[ADR0081's registry-only rejection](../architecture/adr/0081-schema-source-stable-identity-catalog-and-runtime-binding.md#L247).

Provider callbacks take the appropriate builder borrow. Preserve private candidate identity,
owner/mode checks, source-catalog agreement, duplicate-claim checks and merge-after-success.
Errors and unwinding before merge leave every aggregate index and receipt unchanged; this is
registry atomicity, not rollback of arbitrary external callback effects. Retain the
[complete aggregate probes](../../crates/nara_reflect/src/registry.rs#L3823) and
[error/panic tests](../../crates/nara_reflect/tests/registry_contract.rs#L757).

Separate builder admission from immutable receipt validation in provider operations. The App
helper can dispatch privately; plugin authors should not need a public mixed-state registry for
preflight. The existing [preflight contract](../../crates/nara_reflect/src/provider.rs#L577)
distinguishes source/collision checks from registration callbacks, and the
[preloaded test](../../crates/nara_reflect/tests/registry_contract.rs#L479) requires no callback
replay. Trusted source resolution for receipt comparison remains permitted.

## Consumer Migration And Dependency Order

Reconcile the owning ADR boundary first, then implement the Reflect owner and App lifecycle,
migrate downstream readers, and qualify the complete consumer path. The rows below describe
dependencies, not independently shippable layers of compatibility adapters.

| Order | Real consumers | Required migration and preserved failure behavior |
| --- | --- | --- |
| 1 | `nara_reflect` registry, provider definitions and exports | Introduce the builder; consolidate admitted reads/migrations with existing snapshot codec execution. Migrate [registry-based reference collection](../../crates/nara_reflect/src/asset_reference.rs#L356) to the admitted input. Preserve atomic validation and recoverable errors. Update [provider callback types](../../crates/nara_reflect/src/provider.rs#L14) and [public/hidden exports](../../crates/nara_reflect/src/lib.rs#L93). |
| 2 | Component inventory and first-party registration helpers | Change the [inventory-generated callback](../../crates/nara_reflect/src/inventory.rs#L57) and direct registration helpers to builder parameters. [Scene registration and plugin preflight](../../crates/nara_scene/src/scene_components.rs#L66) and [Transform registration](../../crates/nara_transform/src/lib.rs#L163) demonstrate both paths; migrate Sprite, Tilemap, UI and all remaining schema-owning call sites through the same compiler-visible contract. |
| 3 | App registry plugin and schema-provider plugin helpers | Implement private Building/Preloaded/Admitted ownership and provider-oriented preflight dispatch. Ordinary finish remains fallible; preloaded missing/divergent receipts reject without replacing the snapshot. Preserve installation identity and sticky runtime fault detection. |
| 4 | Scene validation, migration, preparation and bindings | Replace mixed registry parameters with snapshot references in [migration](../../crates/nara_scene/src/migration.rs#L16), [validation](../../crates/nara_scene/src/validation.rs#L119) and [preparation](../../crates/nara_scene/src/spawn/preparation.rs#L66). Remove [the stored native wrapper](../../crates/nara_scene/src/component_bindings.rs#L123). Preserve exact selection identity, old-preparation rejection and operation-specific errors. |
| 5 | Host validation/content and authoring preview | Remove [SchemaValidationInput's dual storage](../../src/project_host/composition.rs#L359) and [the rebuilt Frozen wrapper](../../src/project_host/composition.rs#L1209). Update the [content identity check](../../src/project_content.rs#L1325) to compare the sole snapshot with selected bindings. Keep provider rejection/panic and freeze rejection distinct. Migrate [preview construction](../../src/project_host/authoring/adapter.rs#L68) and [Editor preview storage](../../src/project_host/runtime/editor/preview.rs#L295) to snapshots without wrapper reconstruction. |
| 6 | Tooling, Editor models, facades and independent callers | Migrate [workspace readers](../../crates/nara_tooling/src/workspace.rs#L828), [root prelude exports](../../src/lib.rs#L222), examples and fixtures. Replace [default Building placeholders](../../editor/src/frontend/imgui/authoring.rs#L851) used as empty read definitions with explicitly admitted empty snapshots or existing metadata-only inputs as appropriate. Keep runtime and metadata authority distinct. |

The public direct consumer currently constructs and returns a mixed type:
[module-consumer helper](../../tests/fixtures/module-consumer/src/lib.rs#L1). Its
[inventory successor workflow](../../tests/fixtures/module-consumer/tests/component_inventory.rs#L163)
and [Scene preparation identity cases](../../tests/fixtures/module-consumer/tests/scene_preparation.rs#L340)
must exercise the replacement without private hooks. The
[renamed-root inventory consumer](../../tests/fixtures/derive-dependencies/renamed-root/src/lib.rs#L50)
must continue to compile and run through `engine` alone. Follow the repository's existing fixture
harnesses rather than creating a new dependency resolver or source-analysis script.

Mechanical migrations in shared consumer files must be integrated with their owning work. This
unit does not redesign rendering, Recipe composition or the native qualification gate, and does
not add a scheduling dependency to unrelated foundation work.

## Accepted Contract Reconciliation

Before implementing the ownership change, amend the
[ADR 0081 Freeze and Runtime Ownership section](../architecture/adr/0081-schema-source-stable-identity-catalog-and-runtime-binding.md#freeze-and-runtime-ownership).
Its current text explicitly places registration/freeze on the registry and specifies a
Building-to-Frozen state machine with `NotFrozen` snapshot acquisition. Replace those carrier
requirements with consuming builder admission, recoverable failure and private App lifecycle
dispatch. Retain complete atomic validation, registry-plugin finish ownership, immutable native
execution, fresh Runtime construction for structural changes and exact retained-definition checks.

[ADR 0119](../architecture/adr/0119-schema-first-semantic-metadata-authority.md#decision) continues
to own Schema-first semantic authority and optional Bevy reflection adapters.
[ADR 0006](../architecture/adr/0006-scene-and-prefab-data-model.md#explicit-native-and-data-representations)
continues to require explicit native/data selection, complete native binding and per-apply
World eligibility. Neither contract grants execution to a builder or to a metadata-only
`ComponentSchemaOwnerSnapshot`. Keep that existing data-only owner family separate; no new
representation, reload protocol or relaxation of native admission is part of this unit.

ADR0081 selects this construction/admission split. Implementation `7f69d53` replaces the mixed
public type and migrates the actual consumers; the linked evidence records qualification and
limits. Directly affected guides, exports and current architecture references use that contract.

## Bounded Verification And Completion

The requirements below define this unit's acceptance. The linked evidence records executed checks,
including original integration failures and their focused corrections. The contract amendment
alone does not qualify the replacement or trigger unrelated product acceptance.

Run Cargo serially in the existing target directory with `CARGO_BUILD_JOBS=1` to qualify the
replacement. Prefer focused nextest selections, and use `cargo fmt --all` after reviewing
concurrent changes. Select coverage from the final affected diff and existing harnesses.

| Evidence | Required result |
| --- | --- |
| Reflect owner tests and compile-fail examples | Builder cannot be passed to admitted query/Scene/codec entries; snapshots expose no registration methods. Replace phase-error query tests with type-level misuse coverage. Preserve canonical catalogs, binding/lineage/capability checks, typed codecs and actual absence semantics. |
| Failure and identity regressions | Missing-binding and migration failures return builder plus the original error, permit repair/retry and publish nothing on failure. Provider error/panic preserves the aggregate. Clones retain pointer identity; independently rebuilt equivalent definitions remain distinct. Retained old definitions remain usable within their original contract. |
| App and Scene owner tests | Cover ordinary installation, preloaded exact receipts, missing/divergent receipts, no callback replay, build/finish failure, owner replacement/reinsertion and sticky runtime faults. Distinguish absent owner from installed-but-unadmitted Building/Preloaded owners: a separately admitted caller snapshot must not enable standalone preparation/commit or export before finish. Preserve exact native/data selections, prepared-definition checks and target-World eligibility. Reuse the [plugin authority tests](../../crates/nara_reflect/src/plugin.rs#L611). |
| Independent public consumers | Run the direct module helper, inventory successor/repair and Scene preparation cases through public builder/snapshot entries. Run the renamed-root inventory fixture and affected public-prelude/UI module fixtures. Demonstrate registration, one successful admission, queries/migration/codec execution and admitted Scene use with no compatibility alias or engine-private bypass. |
| Host, Tooling and Editor integration | Exercise content validation, explicit independent preview definitions, property edit/Undo, Save/Reopen and preview replacement/close through the migrated snapshot readers. Verify errors remain truthful and no path silently substitutes an equivalent or empty definition. |

After the implementation changes App/Scene/Editor consumers, use the existing
[native Editor qualification protocol](../verification/native-editor-qualification.md), with its
environment prerequisites and actual-execution requirements. Select these affected trajectories:

- `native_game_input_reopen_paused_refocus_resize_and_step_reach_presented_runtime` for admitted
  startup, Game execution and reopen through the migrated App/Scene path.
- `native_retained_scene_receipt_survives_prefab_navigation_and_bound_close` and
  `native_ui_canvas_priority_survives_closing_retained_scene` for retained definition custody,
  Scene/Prefab/Canvas navigation, replacement and close.
- `native_complete_prefab_conversion_reviews_commits_and_round_trips_scene_and_prefab` for
  Schema-driven edits, Undo/Redo and actual Save/Reopen through snapshot consumers.

Record the exact source revision, selected tests and actual outcomes. Apply any additional
protocol coverage required by the final changed behavior or release claim; do not claim the full
native gate from this focused selection. Missing desktop/adapter or other-platform evidence
remains unqualified. This plan neither reruns nor reinterprets another unit's native evidence.

Completion requires deletion of the mixed public type, `from_snapshot`, duplicate stored native
registry views and obsolete phase-only reader paths, with all affected callers and documentation
using the replacement. Search for remaining references as a deletion check, distinguishing the
still-valid `ComponentRegistrySnapshot` name and historical documents. Do not retain a deprecated
alias to make that check pass. Broader cleanup of Bevy TypeRegistry storage, Schema publications,
dynamic representations, renderer contracts and unrelated scripts remains outside this unit.
