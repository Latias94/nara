use super::{
    AuthoringDocumentReadiness, AuthoringDocumentState, AuthoringRecordLocation, DiagnosticReport,
    PreparedSceneAuthoringPatch, SceneDocument, ScenePatchDocument, ScenePatchOperation,
    ScenePatchReport, UnavailableRecordManifest, UnavailableRecordManifestEntry,
    classify_scene_document, component_allows_composition, diagnostic_error,
    is_authoring_field_operation,
};
use std::{collections::BTreeSet, sync::Arc};

enum StructureAdmission<'a> {
    Direct,
    History,
    PrefabConversion(&'a crate::authoring::prefab_conversion::PrefabConversionFootprint),
}

pub(super) fn prepare_scene_authoring_structure(
    document: &SceneDocument,
    state: &AuthoringDocumentState,
    patch: &ScenePatchDocument,
    history: bool,
) -> Result<PreparedSceneAuthoringPatch, ScenePatchReport> {
    prepare_structure(
        document,
        state,
        patch,
        if history {
            StructureAdmission::History
        } else {
            StructureAdmission::Direct
        },
    )
}

pub(super) fn prepare_conversion_structure(
    document: &SceneDocument,
    state: &AuthoringDocumentState,
    patch: &ScenePatchDocument,
    footprint: &crate::authoring::prefab_conversion::PrefabConversionFootprint,
) -> Result<PreparedSceneAuthoringPatch, ScenePatchReport> {
    footprint.admit_patch(patch)?;
    prepare_structure(
        document,
        state,
        patch,
        StructureAdmission::PrefabConversion(footprint),
    )
}

fn prepare_structure(
    document: &SceneDocument,
    state: &AuthoringDocumentState,
    patch: &ScenePatchDocument,
    admission: StructureAdmission<'_>,
) -> Result<PreparedSceneAuthoringPatch, ScenePatchReport> {
    let history = matches!(admission, StructureAdmission::History);
    if patch.format_version != ScenePatchDocument::CURRENT_FORMAT_VERSION
        || patch.is_empty()
        || patch.operations.len() > super::MAX_AUTHORING_EDIT_OPERATIONS
    {
        return Err(authoring_structure_rejection(
            "Authoring composition requires a bounded nonempty transaction",
        ));
    }
    for operation in &patch.operations {
        if !matches!(admission, StructureAdmission::PrefabConversion(_)) {
            validate_operation_shape(operation, history)?;
        }
    }
    let field_operations = patch
        .operations
        .iter()
        .filter(|operation| is_authoring_field_operation(operation));
    let fields = if field_operations.clone().next().is_some() {
        super::field_edits_from_operations(field_operations)?
    } else {
        Vec::new()
    };
    admit_replay_cost(patch)?;
    let mut fields = fields.iter();
    let mut candidate = document.clone();
    let mut created = BTreeSet::new();
    let mut inverses = Vec::with_capacity(patch.operations.len());
    for (operation_index, operation) in patch.operations.iter().enumerate() {
        if is_authoring_field_operation(operation) {
            let edit = fields
                .next()
                .expect("each field operation has one parsed edit");
            let (next, inverse) =
                if created.contains(&(edit.entity.clone(), edit.component.clone())) {
                    super::prepare_admitted_field_edit(candidate, state, edit)?
                } else {
                    super::prepare_field_edit(candidate, state, edit)?
                };
            candidate = next;
            inverses.push(inverse.operations);
            continue;
        }
        match &admission {
            StructureAdmission::PrefabConversion(footprint) => {
                footprint.admit_operation(&candidate, operation)?;
                validate_conversion_record(state, operation)?;
            }
            _ => validate_structure_operation(&candidate, state, operation, history, &created)?,
        }
        let inverse = crate::patch::apply_authoring_structure_operation(
            &mut candidate,
            operation,
            operation_index,
        )
        .map_err(|diagnostic| {
            let mut diagnostics = DiagnosticReport::default();
            diagnostics.push(diagnostic);
            ScenePatchReport {
                applied: false,
                inverse: None,
                diagnostics,
            }
        })?;
        match operation {
            ScenePatchOperation::AddComponent {
                entity, component, ..
            } => {
                created.insert((entity.clone(), component.clone()));
            }
            ScenePatchOperation::RemoveComponent { entity, component } => {
                created.remove(&(entity.clone(), component.clone()));
            }
            ScenePatchOperation::RestoreEntity {
                entity,
                descendants,
                ..
            } => {
                // Only owner-created history can restore records. Their fields still cross
                // the ordinary schema/version/capability checks before any further edit.
                for record in std::iter::once(entity).chain(descendants.iter()) {
                    created.extend(
                        record
                            .components
                            .keys()
                            .cloned()
                            .map(|component| (record.id.clone(), component)),
                    );
                }
            }
            ScenePatchOperation::RemoveEntity { .. } => {
                if !created.is_empty() {
                    for operation in &inverse {
                        if let ScenePatchOperation::RestoreEntity {
                            entity,
                            descendants,
                            ..
                        } = operation
                        {
                            for record in std::iter::once(entity).chain(descendants.iter()) {
                                for component in record.components.keys() {
                                    created.remove(&(record.id.clone(), component.clone()));
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        inverses.push(inverse);
    }
    if crate::format::validate_scene_publication_shape(&candidate, state.limits).is_err() {
        return Err(authoring_structure_rejection(
            "Authoring structure exceeds the persistent format boundary",
        ));
    }
    candidate.canonicalize();
    let candidate_state = classify_scene_document(
        &candidate,
        state.source.clone(),
        state.kind,
        Arc::clone(&state.basis),
        state.limits,
    )
    .map_err(|error| ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics: error.into_diagnostics(),
    })?;
    let expected_manifest = if patch.operations.iter().any(|operation| {
        matches!(
            operation,
            ScenePatchOperation::AddEntity { .. }
                | ScenePatchOperation::RemoveEntity { .. }
                | ScenePatchOperation::RestoreEntity { .. }
                | ScenePatchOperation::MoveEntity { .. }
        )
    }) {
        manifest_after_topology(&state.unavailable_manifest, &candidate)
    } else {
        state.unavailable_manifest.clone()
    };
    if !preserves_unedited_declarations(
        &expected_manifest,
        &candidate_state.unavailable_manifest,
        patch,
    ) {
        return Err(authoring_structure_rejection(
            "Authoring structure changed the complete unavailable-record manifest",
        ));
    }
    let inverse = ScenePatchDocument::new(inverses.into_iter().rev().flatten());
    admit_replay_cost(&inverse)?;
    if inverse.operations.iter().any(is_authoring_field_operation) {
        super::field_edits_from_operations(
            inverse
                .operations
                .iter()
                .filter(|operation| is_authoring_field_operation(operation)),
        )?;
    }
    Ok(PreparedSceneAuthoringPatch {
        document: candidate,
        state: candidate_state,
        patch: patch.clone(),
        report: ScenePatchReport {
            applied: true,
            inverse: Some(inverse),
            diagnostics: DiagnosticReport::default(),
        },
    })
}

fn validate_conversion_record(
    state: &AuthoringDocumentState,
    operation: &ScenePatchOperation,
) -> Result<(), ScenePatchReport> {
    let record = match operation {
        ScenePatchOperation::AddEntity { entity, .. }
        | ScenePatchOperation::RestoreEntity { entity, .. } => entity,
        _ => return Ok(()),
    };
    for (component, value) in &record.components {
        let Some(entry) = state.basis.schemas.get(component) else {
            return Err(authoring_structure_rejection(
                "Conversion insertion requires an admitted component Schema",
            ));
        };
        if entry.adapter.is_none()
            || !entry
                .schema
                .has_capability(nara_reflect::ComponentCapability::Scene)
            || value.version != entry.schema.version()
            || value.owner() != Some(&entry.owner)
        {
            return Err(authoring_structure_rejection(
                "Conversion insertion requires a complete current data-authoring record",
            ));
        }
    }
    Ok(())
}

fn admit_replay_cost(patch: &ScenePatchDocument) -> Result<(), ScenePatchReport> {
    crate::patch::bounded_scene_patch_cost(patch, crate::ScenePatchApplyLimits::default())
        .map(|_| ())
        .ok_or_else(|| {
            authoring_structure_rejection(
                "Authoring construction or its inverse exceeds the replay work budget",
            )
        })
}

fn validate_operation_shape(
    operation: &ScenePatchOperation,
    history: bool,
) -> Result<(), ScenePatchReport> {
    match operation {
        ScenePatchOperation::AddEntity { entity, .. }
            if !entity.components.is_empty()
                || !entity.children.is_empty()
                || entity.prefab.is_some() =>
        {
            Err(authoring_structure_rejection(
                "Generic authoring creates only an empty entity",
            ))
        }
        ScenePatchOperation::RestoreEntity { .. } if !history => Err(
            authoring_structure_rejection("Only owner-created history can restore records"),
        ),
        ScenePatchOperation::ReplaceComponent { .. } => Err(authoring_structure_rejection(
            "Whole-record replacement is unavailable for generic authoring",
        )),
        _ => Ok(()),
    }
}

fn validate_structure_operation(
    document: &SceneDocument,
    state: &AuthoringDocumentState,
    operation: &ScenePatchOperation,
    history: bool,
    created: &BTreeSet<(crate::SceneEntityId, nara_reflect::ComponentTypeId)>,
) -> Result<(), ScenePatchReport> {
    match operation {
        ScenePatchOperation::AddEntity { .. } => {}
        ScenePatchOperation::MoveEntity { .. } => {}
        ScenePatchOperation::SetPrefabInstance { .. } => {}
        ScenePatchOperation::RemoveEntity { entity, .. } => {
            let empty_history_inverse = history
                && document.entities.iter().any(|record| {
                    &record.id == entity
                        && record.components.is_empty()
                        && record.children.is_empty()
                        && record.prefab.is_none()
                });
            if !empty_history_inverse && !authoring_deletion_closure_is_complete(document, state) {
                return Err(authoring_structure_rejection(
                    "Entity deletion requires a Complete document without Prefab anchors",
                ));
            }
        }
        ScenePatchOperation::RestoreEntity { .. }
            if history && authoring_deletion_closure_is_complete(document, state) => {}
        ScenePatchOperation::AddComponent {
            component, value, ..
        } => {
            let Some(entry) = state.basis.schemas.get(component) else {
                return Err(authoring_structure_rejection(
                    "Component has no admitted authoring schema",
                ));
            };
            if entry.adapter.is_none()
                || !component_allows_composition(&entry.schema)
                || value.version != entry.schema.version()
                || (if history {
                    value.owner().is_some_and(|owner| owner != &entry.owner)
                } else {
                    value.owner() != Some(&entry.owner)
                })
                || (!history && entry.schema.authoring_initial_value() != Some(&value.value))
            {
                return Err(authoring_structure_rejection(
                    "Component creation requires its exact admitted complete template",
                ));
            }
        }
        ScenePatchOperation::RemoveComponent { entity, component } => {
            let available = created.contains(&(entity.clone(), component.clone())) || state.record_facts.iter().any(|fact| {
                fact.readiness.is_available() && &fact.component == component
                    && matches!(&fact.location, AuthoringRecordLocation::EntityComponent { entity: owner } if owner == entity)
            });
            if !available
                || !state
                    .basis
                    .schemas
                    .get(component)
                    .is_some_and(|entry| component_allows_composition(&entry.schema))
            {
                return Err(authoring_structure_rejection(
                    "Component removal requires a current data-editable direct record",
                ));
            }
        }
        _ => {
            return Err(authoring_structure_rejection(
                "This structural operation is unavailable for generic authoring",
            ));
        }
    }
    Ok(())
}

fn authoring_deletion_closure_is_complete(
    document: &SceneDocument,
    state: &AuthoringDocumentState,
) -> bool {
    state.readiness == AuthoringDocumentReadiness::Complete
        && document
            .entities
            .iter()
            .all(|entity| entity.prefab.is_none())
}

/// Rebinds only source-local placement facts. The classifier must still reproduce every exact
/// source, record, value, reference and overlay location from the original manifest.
fn manifest_after_topology(
    manifest: &UnavailableRecordManifest,
    document: &SceneDocument,
) -> UnavailableRecordManifest {
    let placements = crate::patch::authoring_entity_placements(&document.roots, &document.entities);
    let mut expected = manifest.clone();
    for entry in &mut expected.entries {
        match entry {
            UnavailableRecordManifestEntry::Record(record) => match &record.location {
                AuthoringRecordLocation::EntityComponent { entity } => {
                    record.placement = placements.get(entity).cloned();
                }
                AuthoringRecordLocation::OverlayComponent { anchor, .. } => {
                    record.anchor_placement = placements.get(anchor).cloned();
                }
            },
            UnavailableRecordManifestEntry::OverlayTarget(target) => {
                target.anchor_placement = placements.get(target.anchor()).cloned();
            }
        }
    }
    expected
}

fn preserves_unedited_declarations(
    expected: &UnavailableRecordManifest,
    candidate: &UnavailableRecordManifest,
    patch: &ScenePatchDocument,
) -> bool {
    let replaced = patch
        .operations
        .iter()
        .filter_map(|operation| match operation {
            ScenePatchOperation::SetPrefabInstance { entity, .. } => Some(entity),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    if replaced.is_empty() {
        return expected == candidate;
    }
    // The patch owner compares each complete expected declaration and retains both snapshots
    // in the exact inverse. Only those actual instance anchors may change their overlay facts.
    // Direct unavailable records on the same entity remain protected.
    let unedited = |entry: &&UnavailableRecordManifestEntry| {
        let location = match entry {
            UnavailableRecordManifestEntry::Record(record) => record.location(),
            UnavailableRecordManifestEntry::OverlayTarget(target) => target.location(),
        };
        match location {
            AuthoringRecordLocation::EntityComponent { .. } => true,
            AuthoringRecordLocation::OverlayComponent { anchor, .. } => !replaced.contains(anchor),
        }
    };
    expected
        .entries
        .iter()
        .filter(unedited)
        .eq(candidate.entries.iter().filter(unedited))
        && expected
            .overlays
            .iter()
            .filter(|(anchor, _)| !replaced.contains(anchor))
            .eq(candidate
                .overlays
                .iter()
                .filter(|(anchor, _)| !replaced.contains(anchor)))
}

pub(super) fn authoring_structure_rejection(summary: &'static str) -> ScenePatchReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic_error(
        "scene.authoring-structure-unavailable",
        summary,
    ));
    ScenePatchReport {
        applied: false,
        inverse: None,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        AuthoringDocumentSource, SceneAuthoringHistoryLimits, SceneAuthoringSession, SceneDocument,
        SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
        ScenePatchOperation, ScenePlacement,
    };
    use nara_asset::AssetRef;
    use nara_reflect::AuthoringSchemaBundle;

    #[test]
    fn oversized_structural_inverse_is_refused_even_with_larger_history_limits() {
        use super::super::{
            AuthoringClassificationBasis, AuthoringDocumentKind, PrefabAuthoringDocumentCandidate,
            SceneAuthoringDocumentCandidate, classify_scene_document,
        };
        use crate::{
            PrefabAuthoringSession, PrefabDocument, SceneComponentRecord, SceneFileLimits,
        };
        use nara_core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits};
        use nara_reflect::{
            AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId, ComponentFieldPath,
            ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion,
            ComponentTypeId, ComponentValue, ComponentValueKind, PersistentComponentSchemaOwnerId,
        };
        use std::sync::Arc;

        let component = ComponentTypeId::new("game.LargeNote");
        let owner = AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new("game.notes").unwrap(),
            ComponentSchemaCatalog {
                components: vec![
                    ComponentSchema::new(
                        component.clone(),
                        "Large Note",
                        ComponentSchemaVersion::ONE,
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                    .with_fields([ComponentFieldSchema::required(
                        ComponentFieldId::new("text"),
                        "Text",
                        ComponentFieldPath::from_fields(["text"]),
                        ComponentValueKind::String,
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
                ],
                ..Default::default()
            },
            None,
        );
        let adapter =
            super::super::AuthoringAdapterReceipt::from_schema_owner(&owner, &Default::default())
                .unwrap();
        let bundle = AuthoringSchemaBundle::new([owner]).unwrap();
        let basis = Arc::new(AuthoringClassificationBasis::new(&bundle, &[adapter]).unwrap());
        let root = SceneEntityId::new("root").unwrap();
        let child = SceneEntityId::new("child").unwrap();
        let document = SceneDocument::new(
            [root.clone()],
            [
                SceneEntityRecord::new(root.clone())
                    .with_children([child.clone()])
                    .with_component(
                        component.clone(),
                        SceneComponentRecord::new(
                            ComponentSchemaVersion::ONE,
                            ComponentValue::map([(
                                "text",
                                ComponentValue::String("x".repeat(65 * 1024 * 1024)),
                            )]),
                        ),
                    ),
                SceneEntityRecord::new(child),
            ],
        );
        let file_limits = SceneFileLimits::default()
            .with_encoded_bytes(ByteLimit::new(128 * 1024 * 1024).unwrap())
            .with_shape(SerdeShapeLimits::new(
                DepthLimit::new(64).unwrap(),
                ItemLimit::new(1024).unwrap(),
                ItemLimit::new(1024).unwrap(),
                ByteLimit::new(128 * 1024 * 1024).unwrap(),
                ByteLimit::new(128 * 1024 * 1024).unwrap(),
            ))
            .with_component_value_bytes(ByteLimit::new(128 * 1024 * 1024).unwrap());
        let history_limits = SceneAuthoringHistoryLimits::new(
            8,
            1024,
            1024,
            256 * 1024 * 1024,
            1024,
            1024,
            128 * 1024 * 1024,
        )
        .unwrap();
        for kind in [AuthoringDocumentKind::Scene, AuthoringDocumentKind::Prefab] {
            for remove_entity in [false, true] {
                // Construct already-classified source facts here to avoid a 65 MiB wire fixture.
                // Forward removal is tiny; only its retained inverse crosses the replay limit.
                let state = classify_scene_document(
                    &document,
                    AuthoringDocumentSource::new(AssetRef::path("large-source.json").unwrap()),
                    kind,
                    Arc::clone(&basis),
                    file_limits,
                )
                .unwrap();
                let patch = ScenePatchDocument::new([if remove_entity {
                    ScenePatchOperation::RemoveEntity {
                        entity: root.clone(),
                        from: ScenePlacement::current(&document, &root).unwrap(),
                    }
                } else {
                    ScenePatchOperation::RemoveComponent {
                        entity: root.clone(),
                        component: component.clone(),
                    }
                }]);
                let (rejected, unchanged) = match kind {
                    AuthoringDocumentKind::Scene => {
                        let session = SceneAuthoringSession::from_published_authoring_document_with_history_limits(
                            SceneAuthoringDocumentCandidate { document: document.clone(), state }.publish(),
                            history_limits,
                        );
                        let revision = session.revision();
                        let rejected = session.prepare_authoring_edit(revision, &patch).is_err();
                        (
                            rejected,
                            session.document() == &document
                                && session.revision() == revision
                                && session.history_status().undo_depth == 0,
                        )
                    }
                    AuthoringDocumentKind::Prefab => {
                        let prefab =
                            PrefabDocument::new(document.roots.clone(), document.entities.clone());
                        let session = PrefabAuthoringSession::from_published_authoring_document_with_history_limits(
                            PrefabAuthoringDocumentCandidate { document: prefab.clone(), state }.publish(),
                            history_limits,
                        );
                        let revision = session.revision();
                        let rejected = session.prepare_authoring_edit(revision, &patch).is_err();
                        (
                            rejected,
                            session.document() == &prefab
                                && session.revision() == revision
                                && session.history_status().undo_depth == 0,
                        )
                    }
                };
                assert!(unchanged);
                assert!(
                    rejected,
                    "{kind:?}, remove entity={remove_entity}: accepted an unreplayable inverse"
                );
            }
        }
    }

    #[test]
    fn subtree_deletion_refuses_an_oversized_inverse_without_publishing() {
        let root = SceneEntityId::new("root").unwrap();
        let child = SceneEntityId::new("child").unwrap();
        let document = SceneDocument::new(
            [root.clone()],
            [
                SceneEntityRecord::new(root.clone()).with_children([child.clone()]),
                SceneEntityRecord::new(child),
            ],
        );
        let bundle = AuthoringSchemaBundle::new([]).unwrap();
        let published = SceneDocumentCandidate::decode_json_bytes(
            document.to_json_string().unwrap().as_bytes(),
        )
        .unwrap()
        .admit_authoring(
            AuthoringDocumentSource::new(AssetRef::path("scenes/bounded.scene.json").unwrap()),
            &bundle,
            &[],
        )
        .unwrap()
        .publish();
        let limits = SceneAuthoringHistoryLimits::new(8, 16, 128, 4096, 16, 128, 1).unwrap();
        let mut session =
            SceneAuthoringSession::from_published_authoring_document_with_history_limits(
                published, limits,
            );
        let revision = session.revision();
        let history = session.history_status();
        let report = session.apply_authoring_patch(&ScenePatchDocument::new([
            ScenePatchOperation::RemoveEntity {
                entity: root.clone(),
                from: ScenePlacement::current(&document, &root).unwrap(),
            },
        ]));
        assert!(!report.applied);
        assert_eq!(session.document(), &document);
        assert_eq!(session.revision(), revision);
        assert_eq!(session.history_status(), history);
    }
}
