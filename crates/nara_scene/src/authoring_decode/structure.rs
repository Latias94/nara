use super::{
    AuthoringDocumentReadiness, AuthoringDocumentState, AuthoringRecordLocation, DiagnosticReport,
    PreparedSceneAuthoringPatch, SceneDocument, ScenePatchDocument, ScenePatchOperation,
    ScenePatchReport, UnavailableRecordManifest, UnavailableRecordManifestEntry,
    classify_scene_document, component_allows_composition, diagnostic_error,
};
use std::sync::Arc;

pub(super) fn prepare_scene_authoring_structure(
    document: &SceneDocument,
    state: &AuthoringDocumentState,
    patch: &ScenePatchDocument,
    history: bool,
) -> Result<PreparedSceneAuthoringPatch, ScenePatchReport> {
    // Structural commands are one operation. Field-only transactions retain their existing
    // bounded multi-write contract, and history never broadens the public mutation vocabulary.
    if patch.format_version != ScenePatchDocument::CURRENT_FORMAT_VERSION
        || patch.operations.len() != 1
    {
        return Err(authoring_structure_rejection(
            "Authoring composition requires one explicit structural operation",
        ));
    }
    let operation = &patch.operations[0];
    match operation {
        ScenePatchOperation::AddEntity { entity, .. } => {
            if !entity.components.is_empty()
                || !entity.children.is_empty()
                || entity.prefab.is_some()
            {
                return Err(authoring_structure_rejection(
                    "Generic authoring creates only an empty entity",
                ));
            }
        }
        ScenePatchOperation::MoveEntity { .. } => {}
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
            let available = state.record_facts.iter().any(|fact| {
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
    let mut candidate = document.clone();
    let inverse = crate::patch::apply_authoring_structure_operation(&mut candidate, operation, 0)
        .map_err(|diagnostic| {
        let mut diagnostics = DiagnosticReport::default();
        diagnostics.push(diagnostic);
        ScenePatchReport {
            applied: false,
            inverse: None,
            diagnostics,
        }
    })?;
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
    let expected_manifest = if matches!(
        operation,
        ScenePatchOperation::AddEntity { .. }
            | ScenePatchOperation::RemoveEntity { .. }
            | ScenePatchOperation::RestoreEntity { .. }
            | ScenePatchOperation::MoveEntity { .. }
    ) {
        manifest_after_topology(&state.unavailable_manifest, &candidate)
    } else {
        state.unavailable_manifest.clone()
    };
    if candidate_state.unavailable_manifest != expected_manifest {
        return Err(authoring_structure_rejection(
            "Authoring structure changed the complete unavailable-record manifest",
        ));
    }
    Ok(PreparedSceneAuthoringPatch {
        document: candidate,
        state: candidate_state,
        patch: patch.clone(),
        report: ScenePatchReport {
            applied: true,
            inverse: Some(ScenePatchDocument::new(inverse)),
            diagnostics: DiagnosticReport::default(),
        },
    })
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
/// source, record, value, reference and nested-patch location from the original manifest.
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
                AuthoringRecordLocation::PatchComponent { anchor, .. } => {
                    record.anchor_placement = placements.get(anchor).cloned();
                }
            },
            UnavailableRecordManifestEntry::PatchTarget(target) => {
                target.anchor_placement = placements.get(&target.anchor).cloned();
            }
        }
    }
    expected
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
