# Authored collision derivation

## Contract and scope

Complete the generated-collision workflow in the foundation plan's R10 using the independent
motion workspace. ADR0095 owns the plugin-specific domain; ADR0117 owns persistent configuration
versus native reconciliation; Scene owns source, occurrence and structured Prefab provenance.
This qualifies one concrete Rapier authoring integration, not a production backend selection.

One persistent `CollisionDefinition` describes a translation-only box: center, positive bounded
half width/height and fixed/dynamic behavior. Register its exact native codec through the ordinary
Schema inventory. Durable Scene/Prefab data contains only that definition, never Rapier handles,
runtime Entity IDs, solver caches or debug output.

A private domain owner derives real Rapier bodies/colliders after ordinary Scene materialization.
Its reconciliation input is the complete selected set of live Scene receipts and their accepted
authoring revisions. Resolve those receipts in the actual World and copy typed `SceneEntitySource`
provenance; do not parse expanded entity IDs or invent another persistent identity service.
Attribute body, collider and debug roles to source, occurrence, component/property and revision.

Validate the complete input before native mutation. Unsupported geometry, missing/stale source
membership and duplicate input refuse with structured diagnostics. Invalid authoring patches keep
their old document revision; a bypassed invalid native value must also refuse derivation. Retire
removed or changed native pairs before publishing their replacements. Preserve unchanged pairs
and solver state for unchanged live entities. The existing public Scene synchronization replaces
that occurrence's activation; this consumer derives fresh pairs for the replacement rather than
infer native continuity from equal source IDs. Synchronize only dirty selected sources, preserving
other occurrences' entities and dynamics. An in-place changed definition also rebuilds its pair.
This finite authoring policy does not promise seamless physical editing or native rollback.

Debug geometry is an immutable numeric capture derived from the actual current collider/body,
not a second mutable shape authority. Retained old captures keep their exact data; native handles
from removed or replaced outputs no longer belong to the current solver. Complete unload and
managed shutdown retire all owned bodies/colliders. A retained debug capture cannot resurrect them.

## Alternatives

- A codec that creates native bodies conflates persistent decoding with lifecycle effects and
  prevents data-only editing. Keep codec preparation free of solver mutation.
- Private post-spawn derivation reuses the admitted native binding and current Scene membership.
  Select it for this consumer, which has no measured startup/cook/delivery pressure.
- A domain cook remains conditional on a concrete delivery or startup requirement. Do not create
  a generic baker, portable Physics provider, writer ACL or second Scene coordinator.

## Complete consumer and verification

1. Build and materialize a Scene/Prefab through public Schema/Scene interfaces; derive actual
   Rapier body/collider/debug output and observe real dynamic motion/contact through fixed steps.
2. Edit dimensions through the ordinary authoring patch, reconcile and check the native shape,
   debug vertices, revision and original output retirement. Invalid edits preserve the old state.
3. Duplicate a definition and instantiate the same source at another occurrence. Prove distinct
   generated membership and structured attribution, including a Prefab projection.
4. Remove a component/entity and unload one occurrence. Verify exact remaining membership and
   preservation of unrelated bodies. Hold an old debug capture across edit/removal/close.
5. Undo/Redo and Save/Reopen the authored definition, bind it through the exact original codec,
   and reconstruct output without any serialized native handle. Reject a missing native binding.
6. Exercise managed close, invalid native input and stale/foreign receipts. Run focused serial
   nextest, fixture formatting and correctness Clippy; independently review before local commit.

Reuse Rust 1.95.0, root `target` and `CARGO_BUILD_JOBS=1`. No concurrent Cargo. Record only actual
qualification in the owning ADR/ledger and one verification shard. Full R10, affine authoring,
physical audio devices and broader foundation work remain separate.

## Qualification

Implemented in the independent motion workspace without production engine edits. The final
default and feature-unified cohorts pass 30/30 and 31/31, including eight authored collision
controls. Actual collider/debug observations accompany public edit/history/disk-Reopen, and the
complete selected Entity set accompanies every structural removal/unload control. A late-invalid
mixed batch retains all native state; repaired same-Entity retry rebuilds only its own pair.
Independent native-ownership and testing reviews close with no remaining P1/P2. The
[verification](../knowledge/engineering/verification/2026-11/2026-11-03T170523Z-authored-collision-definitions-retain-exact-source-attribution-through-real-rapier-derivation-and-retirement-b550d8d09ee34b72adfa1d18262eec6a.md)
retains exact commands, the initial integration failures, review corrections and support limits.
This closes this finite collision unit, not full R10 or the foundation rebaseline.
