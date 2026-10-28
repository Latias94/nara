---
title: Installed owner creation-metadata evolution
date: 2026-10-29
status: qualified for creation-only data-owner evolution
related_plan: 2026-10-23-foundation-architecture-rebaseline-plan.md
---

# Installed Owner Creation-Metadata Evolution

## Complete Consumer

Continue the full foundation Goal after the closed native Prefab template import unit. Install
EnemyStats V1, adopt its generation-one owner and author existing values. Install V2 with the
same owner, component, field identities, shapes and wire version, but a different complete
`authoring_initial_value`. Its catalog advances one generation and carries its complete
predecessor. Explicitly retire V1 execution, prepare/review/adopt that metadata successor, then
approve and enable V2. Old values and history remain unchanged; new creation uses V2's initial
value. Edit, Undo/Redo, Save, physically Close and Open again through public Session/Application.

ADR 0118 already separates creation metadata from stored-value semantics. ADR 0114 requires a
separate existing-owner evolution transaction; the additive adoption operation does not cover
it. The [read-only audit](../knowledge/engineering/subagents/2026-10/2026-10-28T214506Z-installed-existing-owner-evolution-is-a-separate-transaction-after-additive-adoption-6b701a39429c41ddbc273c80bab9474c.md)
locates the four current refusal boundaries. Existing V1/V2 algorithm-only tests remain useful
but do not establish owner evolution.

This unit does not migrate stored fields or values. General structural/value changes need a
complete consumer to determine their history and persistence policy. Catalog generation is
lineage, not an old file-format version. Canonical release/project formats stay at version 1,
with no compatibility reader, conversion layer or new migration guide.

## Owners And Ordered Replacement

1. Reconcile the existing-owner transaction in ADRs 0090/0114/0118 and affected release contracts
   before changing admission. Keep package identity, semantic owner identity and executable
   approval distinct. Do not require Proposed ADR 0120 or completion of the custom-drawn UI.
2. Extend the existing bounded release capture/manifest/inspection to carry code-free predecessor
   bytes. Reuse Reflect's complete catalog and lineage validation. At project preparation, prove
   that candidate predecessor equals the actual admitted current catalog. A self-consistent
   candidate pair alone cannot establish that relation. Update all affected unreleased producers
   and consumers directly; retain one canonical reader.
3. Express a creation-metadata-only successor through the existing bundle/source preparation
   owners. Keep other owners' complete snapshots and Adapter evidence exactly intact. Permit
   exactly the intended creation change while preserving stable owner/component/field identities
   and stored-value semantics. For the affected owner, keep the Adapter implementation identity,
   version and coverage, but prepare new exact generation/fingerprint/predecessor-bound evidence
   from the validated successor catalog through its actual Adapter owner. Include that evidence
   in the same successor basis; do not reuse or relabel V1's opaque evidence. Define this bounded
   successor admission separately from additive reclassification, retaining additive equality
   guards. Reuse the same history preparation kernel and one shared successor basis; do not
   introduce a second history/Schema interpreter.
4. Prepare the complete retained open set, including inactive dirty Scene/Prefab documents and
   both Undo/Redo branches. Preserve source, content/saved identities, history entries/costs and
   selection. Capture current composition against actual working Prefabs. History replayability
   remains distinct from all-history composition validity under ADR 0090.
5. Require the affected V1 route to be explicitly disabled and its physical tasks returned before
   writing the Schema successor. Enforce this in Session admission, including revalidation at
   confirmation. Reuse existing Tasks, publication, whole-set adoption and reconciliation. New
   metadata grants no executable approval. V2 enablement remains a separate original-basis
   selection operation after successful adoption.
6. Exercise the actual independent C# release and public Application paths, then update guides,
   ledger and immutable evidence. Independently review the changed contracts and implementation,
   run appropriate serial nextest, feature/static and affected native checks, commit precise
   units and ff local main. No push or PR.

## Acceptance And Failure States

- Review and preparation write nothing. Wrong predecessor, unsupported declaration differences,
  stale source/project/Schema/open-set facts, proof failure or budget exhaustion publish neither
  partial Schema nor partial document classifications. V1 remains explicitly disabled if the
  caller already disabled it; refusal does not restart execution.
- Adoption preserves existing payloads, source revisions, saved checkpoints and history. Both
  retained history directions replay through the original owner kernel. Old queries/prepared
  proposals reject their old basis. A new component uses the changed initial value and one
  ordinary source transaction for Undo/Redo.
- Keep published-without-adoption, uncertain publication, cancellation and physical Close
  distinct. Reconcile from actual publication facts with a new review; do not manufacture a
  rollback or silently refresh an old candidate.
- If Schema adoption succeeds but V2 probe or desired-selection publication fails, report the
  new Schema with an inactive, retryable tool. Neither the old route nor its old approval may
  become current merely because execution failed. Bundle and selection files are separate
  durable owners, not a cross-file atomic transaction.
- Close/Open both after Schema adoption but before V2 enablement and after successful V2
  editing/Save. Restore the actual owner generation and values. Open observes desired selection
  without executing it; ordinary recipe refresh preserves the non-recipe owner.

Begin window-free. Select a real native trajectory only for affected interactions or lifecycle
claims; it does not establish new graphical upgrade controls. Nara's own custom-drawn frontend
will consume the same semantic operation. Parallel V1-serving/V2-probing, field/value migration
and general package authority over an owner remain separate design questions.

## Qualified Result

Reflect's strict relation and paired catalog reader are `9a4ee37`; whole-source/history, release,
Session/Application and actual C# consumers are `34f8371`. The
[verification record](../knowledge/engineering/verification/2026-10/2026-10-28T223626Z-installed-creation-only-owner-successor-preserves-sources-history-and-separate-execution-approval-57dec00a6525410f9abb79ec24a99e69.md)
records 1,887 final library passes with 39 explicit skips, all 37 independent consumers, actual
managed cancellation, both selected C# Application cases and the selected Windows/DX12 facet/panel
trajectory. Correctness Clippy, independent Scene integration, minimal features and root/fixture
format checks pass. The new upgrade workflow uses window-free public Session/Application;
existing native interaction evidence does not qualify new graphical upgrade controls.

Direct and owned Scene/Prefab preparation retain old concrete creation history even when the
successor removes its template. Ordinary new creation then refuses. Actual dirty Prefab capture,
unsupported predecessor/definition refusal, active/pending V1 refusal, published-result Cancel/
reconciliation and V2 probe failure retain their separate owners and tested facts.

This bounded unit is complete; general stored-field/value migration, executable gameplay
publications, complete management UI and the full foundation Goal remain open. The independent
[delivery-boundary audit](../knowledge/engineering/subagents/2026-10/2026-10-28T225432Z-source-carrying-export-still-lacks-an-independent-verified-runtime-content-entry-b9156a4b7e2043f09f3ab41b78cf3bf8.md)
records the next resource-facet/Runtime-content investigation without adopting its design or
claiming processed/source-free delivery.

## References

Pinned source comparison informs the semantics rather than supplying Nara qualification:

- Godot `81ab961f91066a63bcbf3ec340962bb1a1ada558`,
  `modules/gdscript/gdscript_compiler.cpp::_compile_class` and
  `modules/gdscript/gdscript.cpp::GDScriptInstance::reload_members`, copies compatible existing
  members by name in its keep-state path. Its Editor placeholder path is different:
  `GDScript::_update_exports` and `core/object/script_language.cpp::PlaceHolderScriptInstance`
  keep explicit overrides while unoverridden properties read the new defaults. Nara's complete
  stored payload is explicit data even when its value equals an old creation initial. Do not
  infer an absent override or reset `100` to `250` from equality with the old initial value.
- Bevy `85503941e488152711a4695bbe248e9ed9f453fc`,
  `crates/bevy_ecs/src/reflect/component.rs::apply_or_insert_mapped` and
  `reflect/mod.rs::from_reflect_with_fallback`, distinguishes applying to an existing component
  from constructing then applying. Derive `#[reflect(default)]` fills an omitted field. Those
  operations do not establish persistent authoring evolution, Undo continuity or a catalog
  successor protocol. Keep Nara's creation initial separate from missing-field `default_value`.

The read-only comparison did not run either engine or qualify their runtime reload, persistence
or history. Nara's fresh Adapter evidence and separate executable approval retain their own
owner contracts and actual consumer gates.

- [Additive adoption](2026-10-28-authoring-owner-adoption-plan.md)
- [Local lifecycle](2026-10-28-local-source-tool-release-plan.md)
- [Native Prefab import](2026-10-29-native-prefab-template-import-plan.md)
- [Full foundation plan](2026-10-23-foundation-architecture-rebaseline-plan.md)
- [ADR 0114](../architecture/adr/0114-standalone-generic-editor-authoring-and-execution-boundary.md)
- [ADR 0118](../architecture/adr/0118-editor-workspaces-frontends-and-creation-templates.md)
- [ADR 0090](../architecture/adr/0090-unavailable-schema-and-lossless-authoring.md)
- [ADR 0098](../architecture/adr/0098-schema-owner-lineage-and-active-runtime-composition.md)
