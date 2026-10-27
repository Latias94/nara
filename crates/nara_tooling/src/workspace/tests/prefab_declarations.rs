use super::*;
use nara_scene::{
    PrefabAddition, PrefabDeclarationError, PrefabDeclarationLimits, PrefabEntityTarget,
    PrefabInstance, PrefabInstanceClaim, PrefabInstancePath, PrefabInstancePathSegment,
    PrefabOverlay, PrefabRevert, SourceDocumentKey, SourceDocumentKind,
};

fn root_path() -> PrefabInstancePath {
    PrefabInstancePath::new(entity_id("anchor"))
}

fn source_document() -> SceneDocument {
    let base = SourceDocumentKey::named(
        "5e7c0f41-a1f7-4ce4-bb69-e4927f4f661b".parse().unwrap(),
        "base",
        SourceDocumentKind::Prefab,
    )
    .unwrap();
    let mut overlay = PrefabOverlay::new(base.clone());
    overlay
        .suppressions
        .push(PrefabEntityTarget::source(entity_id("inherited")));
    let mut addition = PrefabAddition::new(entity_id("nested"));
    addition.prefab = Some(PrefabInstance {
        source: AssetRef::path("prefabs/nested.prefab.json").unwrap(),
        overrides: Some(PrefabOverlay::new(base)),
    });
    overlay.additions.push(addition);
    overlay.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(entity_id("detached")),
        value: None,
    });
    let mut anchor = SceneEntityRecord::new(entity_id("anchor"));
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path("prefabs/base.prefab.json").unwrap(),
        overrides: Some(overlay),
    });
    SceneDocument::from_ordered_roots([anchor, SceneEntityRecord::new(entity_id("peer"))])
}

fn schema_basis() -> AuthoringSchemaBasis {
    AuthoringSchemaBasis::admit(&authoring_basis_bundle("Prefab Revert"), &[]).unwrap()
}

fn open_document(
    workspace: &mut EditorWorkspace,
    kind: EditorDocumentKind,
    title: &str,
    basis: &AuthoringSchemaBasis,
) -> EditorDocumentId {
    let source = source_document();
    let report = match kind {
        EditorDocumentKind::Scene => {
            let path = format!("scenes/{title}.scene.json");
            let published = SceneDocumentCandidate::decode_json_bytes(
                source.to_json_string().unwrap().as_bytes(),
            )
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::path(&path).unwrap()),
                basis,
            )
            .unwrap()
            .publish();
            workspace
                .open_authoring_scene_document(
                    title,
                    EditorDocumentSourceBinding::new(path),
                    published,
                )
                .unwrap()
        }
        EditorDocumentKind::Prefab => {
            let path = format!("prefabs/{title}.prefab.json");
            let published = PrefabDocumentCandidate::decode_json_bytes(
                PrefabDocument::new(source.roots, source.entities)
                    .to_json_string()
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap()
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::path(&path).unwrap()),
                basis,
            )
            .unwrap()
            .publish();
            workspace
                .open_authoring_prefab_document(
                    title,
                    EditorDocumentSourceBinding::new(path),
                    published,
                )
                .unwrap()
        }
        EditorDocumentKind::Extension(_) => unreachable!("entity-bearing source fixture"),
    };
    report.opened_document.unwrap()
}

fn document_value(workspace: &EditorWorkspace, document: EditorDocumentId) -> SceneDocument {
    match &workspace.documents[&document] {
        EditorDocumentSlot::Scene(slot) => slot.session.document().clone(),
        EditorDocumentSlot::Prefab(slot) => slot.session.document().instantiate(),
        EditorDocumentSlot::Extension(_) => unreachable!("entity-bearing source fixture"),
    }
}

fn assert_basis_rejected(
    workspace: &EditorWorkspace,
    basis: &EditorDocumentQueryBasis,
    rejection: EditorDocumentQueryRejection,
    preparation_code: &str,
) {
    // A malformed selector and exhausted content limits must not mask stale authority.
    let path = PrefabInstancePath::new(entity_id("absent"));
    let limits = PrefabDeclarationLimits::new(1, 1, 1).unwrap();
    assert_eq!(
        workspace.observe_prefab_instance(basis, &path, limits),
        Err(rejection)
    );
    let report = workspace
        .prepare_prefab_revert(basis, &path, PrefabRevert::Overlay, limits)
        .unwrap_err();
    assert!(!report.applied);
    assert_eq!(report.document, Some(basis.document));
    assert!(report.patch_report.is_none());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|entry| entry.code().as_str() == preparation_code),
        "{:?}",
        report.diagnostics
    );
}

#[test]
fn observe_and_prepare_non_active_sources_preserve_navigation_and_one_undo_owner() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let mut workspace = EditorWorkspace::new();
        let document = open_document(&mut workspace, kind, "source", &schema_basis());
        assert!(
            workspace
                .apply_authoring_command(EditorWorkspaceCommand::SelectEntity {
                    document,
                    entity: Some(entity_id("peer")),
                })
                .applied
        );
        let background = open_document(
            &mut workspace,
            EditorDocumentKind::Scene,
            "background",
            &schema_basis(),
        );
        assert!(
            workspace
                .apply_authoring_command(EditorWorkspaceCommand::SelectEntity {
                    document: background,
                    entity: Some(entity_id("peer")),
                })
                .applied
        );
        let before = workspace.authoring_model();
        let original = document_value(&workspace, document);
        let basis = workspace.document_query_basis(document).unwrap();
        let observation = workspace
            .observe_prefab_instance(&basis, &root_path(), PrefabDeclarationLimits::default())
            .unwrap();
        assert_eq!(observation.revision, basis.revision);
        assert_eq!(Some(observation.schema), basis.schema);
        assert_eq!(observation.instance, original.entities[0].prefab);
        assert_eq!(observation.nested_instances.len(), 2);
        let nested = PrefabInstancePath {
            anchor: entity_id("anchor"),
            steps: vec![PrefabInstancePathSegment::Addition { index: 0 }],
        };
        assert!(
            workspace
                .observe_prefab_instance(&basis, &nested, PrefabDeclarationLimits::default(),)
                .unwrap()
                .instance
                .unwrap()
                .overrides
                .is_some()
        );
        let prepared = workspace
            .prepare_prefab_revert(
                &basis,
                &root_path(),
                PrefabRevert::Overlay,
                PrefabDeclarationLimits::default(),
            )
            .unwrap();
        assert_eq!(prepared.document(), document);
        assert_eq!(prepared.document_kind(), kind);
        assert_eq!(prepared.base_revision(), basis.revision);
        assert!(prepared.is_prefab_revert());
        assert!(matches!(&prepared.patch().operations[..],
            [ScenePatchOperation::SetPrefabInstance { entity, expected, value }]
            if entity == &entity_id("anchor")
                && expected == &observation.instance
                && value.as_ref().unwrap().overrides.is_none()));
        assert_eq!(workspace.authoring_model(), before);
        assert_eq!(document_value(&workspace, document), original);

        let committed = workspace.commit_authoring_edit(prepared);
        assert!(committed.applied, "{:?}", committed.diagnostics);
        assert_eq!(workspace.active_document(), Some(background));
        assert_eq!(
            workspace.documents[&document].selection().top_entity(),
            Some(&entity_id("peer"))
        );
        assert_eq!(
            workspace.documents[&background].selection().top_entity(),
            Some(&entity_id("peer"))
        );
        assert_eq!(
            workspace.documents[&document].history_status().undo_depth,
            1
        );
        assert_eq!(
            workspace.documents[&background].history_status().undo_depth,
            0
        );
        let changed = document_value(&workspace, document);
        assert_ne!(changed, original);
        let undo = workspace.apply_authoring_command(EditorWorkspaceCommand::Undo {
            document,
            base_revision: committed.revision.unwrap(),
        });
        assert!(undo.applied, "{:?}", undo.diagnostics);
        assert_eq!(document_value(&workspace, document), original);
        assert_eq!(
            workspace.documents[&document].history_status().undo_depth,
            0
        );
        assert!(!workspace.documents[&document].is_dirty());
        let redo = workspace.apply_authoring_command(EditorWorkspaceCommand::Redo {
            document,
            base_revision: undo.revision.unwrap(),
        });
        assert!(redo.applied, "{:?}", redo.diagnostics);
        assert_eq!(document_value(&workspace, document), changed);
        assert_eq!(
            workspace.documents[&document].history_status().undo_depth,
            1
        );
        assert_eq!(workspace.active_document(), Some(background));
    }
}

#[test]
fn different_schema_admissions_and_missing_schema_reject_before_source_reads() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let mut workspace = EditorWorkspace::new();
        let schema = schema_basis();
        let document = open_document(&mut workspace, kind, "source", &schema);
        let basis = workspace.document_query_basis(document).unwrap();
        let before = workspace.authoring_model();
        let original = document_value(&workspace, document);
        let independent = schema_basis();
        assert_ne!(schema, independent);
        for schema in [None, Some(independent)] {
            let wrong_schema = EditorDocumentQueryBasis {
                schema,
                ..basis.clone()
            };
            assert_basis_rejected(
                &workspace,
                &wrong_schema,
                EditorDocumentQueryRejection::StaleSchema,
                "tooling.workspace-stale-schema-basis",
            );
        }
        assert_eq!(workspace.authoring_model(), before);
        assert_eq!(document_value(&workspace, document), original);
    }
}

#[test]
fn matching_paths_and_shared_schema_do_not_transfer_scene_or_prefab_ownership() {
    let mut workspace = EditorWorkspace::new();
    let schema = schema_basis();
    let documents = [
        open_document(&mut workspace, EditorDocumentKind::Scene, "first", &schema),
        open_document(&mut workspace, EditorDocumentKind::Scene, "second", &schema),
        open_document(&mut workspace, EditorDocumentKind::Prefab, "first", &schema),
        open_document(
            &mut workspace,
            EditorDocumentKind::Prefab,
            "second",
            &schema,
        ),
    ];
    let before = workspace.authoring_model();
    for owner in documents {
        let basis = workspace.document_query_basis(owner).unwrap();
        for document in documents {
            if document == owner {
                continue;
            }
            let wrong_owner = EditorDocumentQueryBasis {
                document,
                ..basis.clone()
            };
            assert_basis_rejected(
                &workspace,
                &wrong_owner,
                EditorDocumentQueryRejection::StaleIncarnation,
                "tooling.workspace-stale-document-incarnation",
            );
        }
        assert_eq!(document_value(&workspace, owner), source_document());
    }
    assert_eq!(workspace.authoring_model(), before);
}

#[test]
fn stale_prepared_revert_and_captured_basis_reject_even_after_undo_restores_content() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let mut workspace = EditorWorkspace::new();
        let document = open_document(&mut workspace, kind, "source", &schema_basis());
        let original = document_value(&workspace, document);
        let basis = workspace.document_query_basis(document).unwrap();
        let pending = workspace
            .prepare_prefab_revert(
                &basis,
                &root_path(),
                PrefabRevert::Overlay,
                PrefabDeclarationLimits::default(),
            )
            .unwrap();
        let accepted = workspace
            .prepare_prefab_revert(
                &basis,
                &root_path(),
                PrefabRevert::Overlay,
                PrefabDeclarationLimits::default(),
            )
            .unwrap();
        assert!(workspace.commit_authoring_edit(accepted).applied);
        let before = workspace.authoring_model();
        assert_basis_rejected(
            &workspace,
            &basis,
            EditorDocumentQueryRejection::StaleRevision,
            "tooling.workspace-stale-document-revision",
        );
        assert!(!workspace.commit_authoring_edit(pending).applied);
        assert_eq!(workspace.authoring_model(), before);
        let revision = workspace.document_revision(document).unwrap();
        assert!(
            workspace
                .apply_authoring_command(EditorWorkspaceCommand::Undo {
                    document,
                    base_revision: revision,
                })
                .applied
        );
        assert_eq!(document_value(&workspace, document), original);
        assert_basis_rejected(
            &workspace,
            &basis,
            EditorDocumentQueryRejection::StaleRevision,
            "tooling.workspace-stale-document-revision",
        );
        assert_eq!(
            workspace.documents[&document].history_status().undo_depth,
            0
        );
    }
}

#[test]
fn reopen_same_content_and_schema_invalidates_observation_and_pending_revert() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let (mut workspace, mut authority) = EditorWorkspace::new_hosted();
        let document = open_document(&mut workspace, kind, "source", &schema_basis());
        let basis = workspace.document_query_basis(document).unwrap();
        let pending = workspace
            .prepare_prefab_revert(
                &basis,
                &root_path(),
                PrefabRevert::Overlay,
                PrefabDeclarationLimits::default(),
            )
            .unwrap();
        let digest = EditorDocumentDigest::new(7, [0x5a; 32]);
        match kind {
            EditorDocumentKind::Scene => {
                let session = workspace.scene(document).unwrap().session();
                let candidate = SceneDocumentCandidate::decode_json_bytes(
                    session.document().to_json_string().unwrap().as_bytes(),
                )
                .unwrap();
                let reopened = session.try_reopen_authoring_candidate(candidate).unwrap();
                authority
                    .publish_reopened_session(
                        &mut workspace,
                        document,
                        basis.revision,
                        reopened,
                        digest,
                    )
                    .unwrap();
            }
            EditorDocumentKind::Prefab => {
                let source = workspace.documents[&document]
                    .source_binding()
                    .unwrap()
                    .clone();
                let session = workspace.prefab(document).unwrap().session();
                let candidate = PrefabDocumentCandidate::decode_json_bytes(
                    session.document().to_json_string().unwrap().as_bytes(),
                )
                .unwrap();
                let reopened = session.try_reopen_authoring_candidate(candidate).unwrap();
                authority
                    .publish_reopened_prefab_session(
                        &mut workspace,
                        document,
                        basis.revision,
                        source,
                        reopened,
                        digest,
                    )
                    .unwrap();
            }
            EditorDocumentKind::Extension(_) => unreachable!("entity-bearing source fixture"),
        }
        let current = workspace.document_query_basis(document).unwrap();
        assert_eq!(basis.schema, current.schema);
        assert_ne!(basis.revision.source_id(), current.revision.source_id());
        let before = workspace.authoring_model();
        assert_basis_rejected(
            &workspace,
            &basis,
            EditorDocumentQueryRejection::StaleIncarnation,
            "tooling.workspace-stale-document-incarnation",
        );
        assert!(!workspace.commit_authoring_edit(pending).applied);
        assert_eq!(workspace.authoring_model(), before);
        assert_eq!(document_value(&workspace, document), source_document());
    }
}

#[test]
fn identical_generic_patch_does_not_acquire_source_owned_revert_origin() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let mut workspace = EditorWorkspace::new();
        let document = open_document(&mut workspace, kind, "source", &schema_basis());
        let basis = workspace.document_query_basis(document).unwrap();
        let before = workspace.authoring_model();
        let revert = workspace
            .prepare_prefab_revert(
                &basis,
                &root_path(),
                PrefabRevert::Overlay,
                PrefabDeclarationLimits::default(),
            )
            .unwrap();
        let lookalike = workspace
            .prepare_authoring_edit(document, basis.revision, revert.patch())
            .unwrap();
        assert_eq!(revert.patch(), lookalike.patch());
        assert_eq!(revert.inverse(), lookalike.inverse());
        assert!(revert.is_prefab_revert());
        assert!(!lookalike.is_prefab_revert());
        assert_eq!(workspace.authoring_model(), before);
        assert_eq!(
            workspace.documents[&document].history_status().undo_depth,
            0
        );
    }
}

#[test]
fn source_observation_and_preparation_refusals_preserve_owner_results_and_state() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let mut workspace = EditorWorkspace::new();
        let document = open_document(&mut workspace, kind, "source", &schema_basis());
        let basis = workspace.document_query_basis(document).unwrap();
        let before = workspace.authoring_model();
        let original = document_value(&workspace, document);
        let missing = PrefabInstancePath::new(entity_id("absent"));
        for (path, limits, error, code) in [
            (
                missing,
                PrefabDeclarationLimits::default(),
                PrefabDeclarationError::AnchorNotFound,
                "scene.authoring-prefab-anchor-not-found",
            ),
            (
                root_path(),
                PrefabDeclarationLimits::new(1, 1, 1).unwrap(),
                PrefabDeclarationError::BudgetExceeded,
                "scene.authoring-prefab-budget-exceeded",
            ),
        ] {
            let rejection = EditorDocumentQueryRejection::PrefabDeclaration(error);
            assert_eq!(rejection.to_string(), error.to_string());
            assert_eq!(
                std::error::Error::source(&rejection)
                    .unwrap()
                    .downcast_ref::<PrefabDeclarationError>(),
                Some(&error)
            );
            assert_eq!(
                workspace.observe_prefab_instance(&basis, &path, limits),
                Err(rejection)
            );
            let report = workspace
                .prepare_prefab_revert(&basis, &path, PrefabRevert::Overlay, limits)
                .unwrap_err();
            assert!(!report.applied);
            assert_eq!(report.document, Some(document));
            let source_report = report.patch_report.as_ref().unwrap();
            assert!(
                source_report
                    .diagnostics
                    .iter()
                    .any(|entry| entry.code().as_str() == code)
            );
            assert_eq!(report.diagnostics, source_report.diagnostics);
        }
        assert_eq!(workspace.authoring_model(), before);
        assert_eq!(document_value(&workspace, document), original);
    }
}
