use super::*;
use nara_reflect::ComponentSchemaOwnerSnapshot;

fn creation_pair() -> (AuthoringSchemaBasis, AuthoringSchemaBasis) {
    let schema = ComponentSchema::new(
        ComponentTypeId::new(RECOVERED),
        "Creation metadata",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["value"]),
        ComponentValueKind::U64,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
    .with_authoring_initial_value(ComponentValue::map([("value", ComponentValue::U64(1))]));
    let owner = PersistentComponentSchemaOwnerId::try_new(RECOVERED_OWNER).unwrap();
    let previous = ComponentSchemaOwnerSnapshot::admit(
        owner.clone(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let mut current = ComponentSchemaCatalog::successor_of(previous.current()).unwrap();
    current.components = previous.current().components().to_vec();
    current.components[0].authoring_initial_value =
        Some(ComponentValue::map([("value", ComponentValue::U64(2))]));
    let next =
        ComponentSchemaOwnerSnapshot::admit(owner, current, Some(previous.current().clone()))
            .unwrap();
    let admit = |snapshot| {
        let bundle = AuthoringSchemaBundle::from_owner_snapshots([snapshot]).unwrap();
        let receipt =
            AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
                .unwrap();
        AuthoringSchemaBasis::admit(&bundle, &[receipt]).unwrap()
    };
    (admit(previous), admit(next))
}

fn kind() -> SchemaReclassificationKind {
    SchemaReclassificationKind::CreationMetadata {
        owner: PersistentComponentSchemaOwnerId::try_new(RECOVERED_OWNER).unwrap(),
    }
}

#[test]
fn creation_metadata_direct_and_owned_preserve_complete_dirty_set_and_checkpoints() {
    for owned in [false, true] {
        let (previous, next) = creation_pair();
        let (mut workspace, mut authority) = EditorWorkspace::new_hosted();
        let documents = open_pair(&mut workspace, &previous);
        for document in documents {
            apply_patch(&mut workspace, document, "saved");
            let checkpoint = authority.capture(&workspace, document).unwrap();
            assert!(
                authority
                    .commit(
                        &mut workspace,
                        EditorPersistenceCommit {
                            checkpoint,
                            digest: EditorDocumentDigest::new(17, [0x42; 32]),
                        },
                    )
                    .applied
            );
            apply_patch(&mut workspace, document, "dirty");
            apply_patch(&mut workspace, document, "redo");
            replay(&mut workspace, document, true);
            assert!(
                workspace
                    .apply_authoring_command(EditorWorkspaceCommand::SelectEntity {
                        document,
                        entity: Some(id("root")),
                    })
                    .applied
            );
        }
        assert!(
            workspace
                .apply_authoring_command(EditorWorkspaceCommand::SetActiveDocument {
                    document: documents[0],
                })
                .applied
        );
        let before = snapshot(&workspace);
        let prepared = if owned {
            workspace
                .capture_schema_reclassification(&previous, &next, &kind(), Default::default())
                .unwrap()
                .prepare()
                .unwrap()
        } else {
            workspace
                .prepare_schema_reclassification(
                    &previous,
                    &next,
                    &kind(),
                    &mut SchemaReclassificationWork::default(),
                )
                .unwrap()
        };
        assert_eq!(snapshot(&workspace), before);
        assert_eq!(prepared.schema_basis(), &next);
        assert_eq!(prepared.documents().collect::<Vec<_>>(), documents);
        prepared.validate(&workspace).unwrap();
        let scene = prepared.scene_source(&workspace, documents[0]).unwrap();
        let prefab = prepared.prefab_source(&workspace, documents[1]).unwrap();
        assert_eq!(scene.schema_basis(), next);
        assert_eq!(prefab.schema_basis(), next);
        assert_eq!(scene.unavailable_record_manifest().total_count(), 1);
        assert_eq!(prefab.unavailable_record_manifest().total_count(), 1);
        workspace
            .bind_schema_reclassification(prepared)
            .unwrap()
            .commit();
        let mut expected = before;
        for document in &mut expected {
            document.tab.authoring_schema_basis = Some(next.clone());
        }
        assert_eq!(snapshot(&workspace), expected);
        assert_eq!(workspace.active_document(), Some(documents[0]));
        for document in documents {
            replay(&mut workspace, document, true);
            assert_eq!(workspace.document_is_dirty(document), Some(false));
            replay(&mut workspace, document, false);
            replay(&mut workspace, document, false);
            assert_eq!(workspace.document_is_dirty(document), Some(true));
            assert_eq!(
                workspace.documents[&document].history_status().redo_depth,
                0
            );
        }
    }
}

#[test]
fn creation_metadata_owned_preparation_refuses_wrong_intent_budget_and_stale_complete_set() {
    let (previous, next) = creation_pair();
    let mut workspace = EditorWorkspace::new();
    let documents = open_pair(&mut workspace, &previous);
    for document in documents {
        apply_patch(&mut workspace, document, "undo");
        apply_patch(&mut workspace, document, "redo");
        replay(&mut workspace, document, true);
    }
    let before = snapshot(&workspace);
    for intent in [
        DeclarationExtension,
        SchemaReclassificationKind::CreationMetadata {
            owner: PersistentComponentSchemaOwnerId::try_new("test.wrong-owner").unwrap(),
        },
    ] {
        assert_refused(
            &workspace
                .capture_schema_reclassification(&previous, &next, &intent, Default::default())
                .unwrap_err(),
        );
        assert_eq!(snapshot(&workspace), before);
    }
    let prepared = workspace
        .capture_schema_reclassification(&previous, &next, &kind(), Default::default())
        .unwrap()
        .prepare()
        .unwrap();
    let usage = prepared.usage();
    drop(prepared);
    for (work, retained, budget) in [
        (usage.work_bytes - 1, usage.peak_retained_bytes, "work"),
        (usage.work_bytes, usage.peak_retained_bytes - 1, "retained"),
    ] {
        let limits = SchemaReclassificationLimits::new(
            ByteLimit::new(work).unwrap(),
            ByteLimit::new(retained).unwrap(),
        );
        let result = workspace
            .capture_schema_reclassification(&previous, &next, &kind(), limits)
            .and_then(|request| request.prepare());
        assert_limit(&result.unwrap_err(), budget);
        assert_eq!(snapshot(&workspace), before);
    }
    let request = workspace
        .capture_schema_reclassification(&previous, &next, &kind(), Default::default())
        .unwrap();
    apply_patch(&mut workspace, documents[1], "after-capture");
    let live = snapshot(&workspace);
    let prepared = request.prepare().unwrap();
    assert_refused(&prepared.validate(&workspace).unwrap_err());
    assert_refused(
        &workspace
            .bind_schema_reclassification(prepared)
            .unwrap_err(),
    );
    assert_eq!(snapshot(&workspace), live);
}
