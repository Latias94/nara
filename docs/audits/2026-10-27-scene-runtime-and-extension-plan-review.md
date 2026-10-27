# Scene Runtime And Extension Plan Review

**Baseline:** `664bb86135805abdd31f389ae6f037af0e141c87`.
**Scope:** two independent read-only reviews of the landed architecture and plans. Findings do
not qualify the concurrent implementation or supersede its fresh verification.

## Findings And Disposition

| Priority | Observed contract or counterexample | Owning correction |
| --- | --- | --- |
| P1 | App stores one domain authority validator, already occupied by Reflect. Scene cannot install its independent guard. | Compose immutable plugin-owned guards at the existing finish and managed safe points; reject duplicate plugin registrations and preserve canonical fault attribution. Current runtime integration prerequisite, not a new extension bus. |
| P1 | Advanced Scene replacement captures managed native authority but does not require its selected registry to be that authority. An empty candidate can avoid component binding conflicts. | Centralize ordinary/advanced selection checking before preparation. Preserve the explicit independent projection exception in ADRs 0081/0098. |
| P1 | Export enumerates selected native Schemas, so an empty frozen registry can omit live native persistent members without error. | Check actual persistent membership against the selected Scene-capable definitions before encoding; use the owning apply/binding facts, not a cached source roster. A removed native component remains absent. |
| P1 | Preflight supplies full limits to every component and clones current-version input before aggregate admission. Plain candidates bypass compiler budgeting. | Share one residual logical node/byte allowance across native and data values. Reuse Reflect's default plan, including true zero-byte residuals, before owning copies. Trusted migration/codec internals are not allocator-isolated. |
| P1 integration | Metadata validity is not managed data-definition authority or language execution readiness. | Install exact native/data selections through App composition. Prepare, commit, mutation and export revalidate the applicable selection. Language callbacks, code leases and physical retirement remain with their execution owner. |
| P1 acceptance | Carrier storage alone does not establish an ergonomic scheduled Rust consumer. | Add a scheduled consumer that reads and changes a data member through the guarded interface, preserves siblings, rejects stale entity commands and leaves source/history untouched. Start with explicit carrier-level scheduling and change granularity. |
| P1 follow-up | A toolkit panel is not a complete installable command, inline Inspector or replacement lifecycle. | Use the same authored data consumer in the semantic-tool and selected-language work. Extract only proven repeated UI-neutral logic from Session query/prepare/result owners. Custom-drawn UI controls, IME and layout remain Nara-owned R11 work. |
| P2 | Unbound `PreparedScene` cancels by drop; advanced `PreparedSceneReplacement` already owns allocated targets and requires explicit cleanup. | At the advanced-owner integration boundary, consider `StagedSceneReplacement` and explicit custody naming; do not hide World cleanup in Drop. |
| P2 | `SceneAssetBindings` is a World-bound reservation transaction, unlike immutable `SceneComponentBindings`. | Reassess `SceneAssetReservations` while integrating the product replacement owner. Avoid a giant context that merges the responsibilities. |
| P2 | Full-world live Scene export is distinct from source Save and from Godot packing a root. | Make its flattened current-live scope explicit in APIs/guides; decide final naming with the complete consumer. |
| P2 follow-up | Schema-first storage still retains Bevy TypeRegistry; source review found its direct consumers primarily in tests. | Perform a bounded R12 consumer audit. Remove unused storage/entry points or isolate an actual optional adapter; do not manufacture another crate without a consumer. |
| P2 follow-up | The ECS facade has selected facade-aware derives, but ordinary Event/observer authoring may still require direct substrate paths. | Use a renamed-dependency observer consumer to determine the missing public surface before adding wrappers. Do not fork all Bevy macros. |
| Documentation | R5b still presents the completed private comparison as its next action. | Preserve comparison history, point current work at the production declarative replacement and latest remaining workflow evidence. |

`nara_ecs::upstream` is already removed. The explicit `nara_ecs::bevy_ecs` substrate path is the
current choice; restoring a compatibility alias would recreate ambiguity without a consumer.

## Source Evidence

The baseline call paths are `crates/nara_app/src/lib.rs::__install_runtime_authority_validator`,
`runtime/owner.rs::RuntimeOwnerState`, `nara_reflect/src/plugin.rs`,
`nara_scene/src/spawn/preparation.rs::ScenePreparationDefinition`, `spawn.rs` advanced replacement,
`validation.rs::preflight_scene_with_context_options`, `export.rs::export_scene_with_options`,
`persistent_apply.rs` binding receipts, and `component_bindings.rs`. ADRs 0003/0084 own managed
execution; 0081/0098 own exact applicable definitions; 0006 owns complete persistent composition.

Pinned references remain Bevy `85503941e488152711a4695bbe248e9ed9f453fc` and Godot
`81ab961f91066a63bcbf3ec340962bb1a1ada558`. Bevy's `component/info.rs` permits components without
Rust TypeId but places layout/drop/thread-safety proof on the descriptor owner. Its dynamic ECS
example is a comparison for concrete query pressure, not a reason to weaken Nara's native
bijection. Godot's `scene/resources/packed_scene.cpp`, `scene/main/node.cpp` and
`core/io/resource.cpp` distinguish persistence owner, parentage, packing scope, shared resources
and instance-local duplication. Borrow those ownership concepts while preserving Nara's ECS
runtime and source/occurrence/activation identity separation. Godot's `editor_plugin.h` separates
Inspector/import/export contributions rather than treating panel mounting as tool completeness.

## Execution Recommendation

Keep the selected closed native/data family and one compiler/publication kernel. Finish the
shared owning-contract corrections and the headless public runtime consumer, then integrate the
actual product recipe/startup/replacement and selected optional language owner. Reuse that owner
and authored data for the tool workflow. A custom-drawn editor remains the product direction;
embedding a reference toolkit is neither required nor an acceptance gate for this Scene step.
Independent code review, affected verification and deletion of the superseded private helper
remain necessary before declaring the corresponding workflow complete.
