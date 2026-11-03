use super::*;

use nara_reflect::{ComponentSchemaVersion, ComponentTypeId};
use nara_scene::{AuthoringDocumentReadiness, AuthoringRecordReadiness};

fn encode_prefab(
    encoding: DocumentEncoding,
    prefab: &PrefabDocument,
) -> Result<Vec<u8>, nara_scene::SceneFormatError> {
    match encoding {
        DocumentEncoding::Json => prefab.to_json_string(),
        DocumentEncoding::Ron => prefab.to_ron_string(),
    }
    .map(String::into_bytes)
}

#[test]
fn prefab_open_shares_scene_basis_and_preserves_unavailable_records_through_save_and_reopen() {
    for (path, encoding) in [
        ("reference-ui.prefab.json", DocumentEncoding::Json),
        ("reference-ui.prefab.ron", DocumentEncoding::Ron),
    ] {
        let root = TestRoot::new();
        let directory = root.capability();
        let (bundle, adapter) = scene_bundle_and_name_adapter();
        let basis = AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap();
        let containing = ScenePersistenceHost::open_authoring(
            &directory,
            "startup.scene.json",
            AuthoringDocumentSource::new(
                nara_asset::AssetRef::path("scenes/startup.scene.json").unwrap(),
            ),
            &basis,
        )
        .unwrap();
        let retained_basis = containing.session.authoring_schema_basis().unwrap();
        assert_eq!(retained_basis, basis);

        let entity = SceneEntityId::new("root").unwrap();
        let name = ComponentTypeId::new("nara.scene.Name");
        let visibility = ComponentTypeId::new("nara.scene.Visibility");
        let owner =
            PersistentComponentSchemaOwnerId::try_new(SCENE_COMPONENTS_SCHEMA_OWNER_ID.as_str())
                .unwrap();
        let prefab = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(entity.clone())
            .with_component(
                name.clone(),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::String("Original".to_owned()),
                )
                .with_owner(owner.clone()),
            )
            .with_component(
                visibility.clone(),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::String("visible".to_owned()),
                )
                .with_owner(owner),
            )]);
        let bytes = encode_prefab(encoding, &prefab).unwrap();
        fs::write(root.path.join(path), &bytes).unwrap();
        let source = AuthoringDocumentSource::new(
            nara_asset::AssetRef::path(format!("prefabs/{path}")).unwrap(),
        );
        let binding = EditorDocumentSourceBinding::new(path);
        let OpenedPrefabPersistence {
            mut host,
            session,
            digest,
        } = PrefabPersistenceHost::open_authoring(
            &directory,
            binding.clone(),
            source.clone(),
            &retained_basis,
        )
        .unwrap();
        assert_eq!(session.document(), &prefab);
        assert_eq!(session.authoring_source(), Some(&source));
        assert_eq!(
            session.authoring_schema_basis(),
            Some(retained_basis.clone())
        );
        assert_eq!(
            session.authoring_readiness(),
            Some(AuthoringDocumentReadiness::Degraded)
        );
        let facts = session.authoring_record_facts().unwrap();
        assert_eq!(
            facts
                .iter()
                .find(|fact| fact.component() == &name)
                .unwrap()
                .readiness(),
            AuthoringRecordReadiness::Available
        );
        assert_eq!(
            facts
                .iter()
                .find(|fact| fact.component() == &visibility)
                .unwrap()
                .readiness(),
            AuthoringRecordReadiness::KnownUnbound
        );
        let manifest = session.unavailable_record_manifest().unwrap().clone();
        assert_eq!(manifest.record_count(), 1);
        assert_eq!(digest, editor_digest(ContentDigest::of_bytes(&bytes)));
        let source_file = host.read_source().open().unwrap();
        assert!(host.same_source_object(&source_file).unwrap());
        assert_eq!(
            host.accepted_source_observation().unwrap().2,
            ContentDigest::of_bytes(&bytes)
        );

        let (mut workspace, mut authority) = EditorWorkspace::new_hosted();
        let document = workspace
            .open_prefab_session(path, binding, session)
            .unwrap()
            .opened_document
            .unwrap();
        assert!(
            authority
                .bind_opened_source_digest(&mut workspace, document, digest)
                .applied
        );
        let report =
            workspace.apply_authoring_command(EditorWorkspaceCommand::ApplyAuthoringFieldEdit {
                document,
                base_revision: workspace.prefab(document).unwrap().revision(),
                edit: AuthoringFieldEdit::single(
                    entity,
                    name.clone(),
                    ComponentSchemaVersion::ONE,
                    ComponentFieldId::new("value"),
                    ComponentValue::String("Renamed".to_owned()),
                )
                .unwrap(),
            });
        assert!(report.applied, "{:#?}", report.diagnostics);
        assert!(matches!(
            host.save_owned(
                PrefabSaveCandidate {
                    checkpoint: authority.capture(&workspace, document).unwrap(),
                },
                None,
            ),
            PrefabSaveOutcome::Rejected(EditorPersistenceRejection::PreservationMismatch)
        ));
        assert_eq!(fs::read(root.path.join(path)).unwrap(), bytes);

        let PrefabSaveOutcome::Saved { commit, .. } = host.save_owned(
            PrefabSaveCandidate {
                checkpoint: authority.capture(&workspace, document).unwrap(),
            },
            workspace
                .prefab(document)
                .unwrap()
                .session()
                .authoring_readmission(),
        ) else {
            panic!("the real Prefab Host must save against its retained authoring basis");
        };
        assert!(authority.commit(&mut workspace, commit).applied);
        assert!(matches!(
            host.prepare_reopen(None),
            Err(EditorPersistenceRejection::PreservationMismatch)
        ));
        let PrefabReopenOutcome::Opened(reopened) = host
            .prepare_reopen(
                workspace
                    .prefab(document)
                    .unwrap()
                    .session()
                    .authoring_readmission(),
            )
            .unwrap()
            .prepare()
            .admit()
        else {
            panic!("the saved Prefab must reopen without a native registry");
        };
        let OpenedPrefabReopen {
            session,
            file,
            content_digest,
            ..
        } = *reopened;
        assert_eq!(session.authoring_source(), Some(&source));
        assert_eq!(session.authoring_schema_basis(), Some(retained_basis));
        assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
        assert_eq!(
            session.document(),
            workspace.prefab(document).unwrap().session().document()
        );
        host.commit_reopen(file, content_digest);

        let observation = host.accepted_source_observation();
        let mut changed = session.document().clone();
        changed.entities[0].components.remove(&visibility).unwrap();
        fs::write(
            root.path.join(path),
            encode_prefab(encoding, &changed).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            host.prepare_reopen(session.authoring_readmission())
                .unwrap()
                .prepare()
                .admit(),
            PrefabReopenOutcome::Rejected(EditorPersistenceRejection::PreservationMismatch)
        ));
        assert_eq!(host.accepted_source_observation(), observation);
    }
}
