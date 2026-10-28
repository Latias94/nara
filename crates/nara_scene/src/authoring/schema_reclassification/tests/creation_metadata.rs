use super::*;
use nara_reflect::ComponentSchemaOwnerSnapshot;

fn creation_pair(next_initial_value: Option<u64>) -> (AuthoringSchemaBasis, AuthoringSchemaBasis) {
    let previous = ComponentSchemaOwnerSnapshot::admit(
        owner(RECOVERED_OWNER),
        ComponentSchemaCatalog {
            components: vec![schema(RECOVERED), schema("test.reclassification.Extra")],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let mut current = ComponentSchemaCatalog::successor_of(previous.current()).unwrap();
    current.components = previous.current().components().to_vec();
    current
        .components
        .iter_mut()
        .find(|schema| schema.id() == &ComponentTypeId::new(RECOVERED))
        .unwrap()
        .authoring_initial_value = next_initial_value
        .map(|value| ComponentValue::map([("value", ComponentValue::U64(value))]));
    let next = ComponentSchemaOwnerSnapshot::admit(
        owner(RECOVERED_OWNER),
        current,
        Some(previous.current().clone()),
    )
    .unwrap();
    let admit = |snapshot| {
        let bundle = AuthoringSchemaBundle::from_owner_snapshots([snapshot]).unwrap();
        let adapter =
            AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
                .unwrap();
        AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
    };
    (admit(previous), admit(next))
}

fn source() -> SceneDocument {
    SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("root"))
            .with_component(ComponentTypeId::new(RECOVERED), record(7, RECOVERED_OWNER)),
        SceneEntityRecord::new(id("old-created")),
        SceneEntityRecord::new(id("new-created")),
    ])
}

fn create(entity: &str, value: u64) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::AddComponent {
        entity: id(entity),
        component: ComponentTypeId::new(RECOVERED),
        value: record(value, RECOVERED_OWNER),
    }])
}

fn set(value: u64) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: id("root"),
        component: ComponentTypeId::new(RECOVERED),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("value"),
        value: ComponentValue::U64(value),
    }])
}

#[test]
fn creation_metadata_direct_and_owned_keep_source_and_real_creation_history() {
    let (previous, next) = creation_pair(Some(2));
    let kind = SchemaReclassificationKind::CreationMetadata {
        owner: owner(RECOVERED_OWNER),
    };
    for owned in [false, true] {
        each_source!(&source(), &previous, |session| {
            assert!(
                session
                    .apply_authoring_patch(&create("old-created", 1))
                    .applied
            );
            let first = session.document().clone();
            let first_content = session.content_state();
            assert!(session.apply_authoring_patch(&set(42)).applied);
            let second = session.document().clone();
            assert!(session.apply_authoring_patch(&set(99)).applied);
            let third = session.document().clone();
            assert!(session.undo_authoring().applied);
            assert_eq!(session.history_status().undo_depth, 2);
            assert_eq!(session.history_status().redo_depth, 1);
            let stale_edit = session
                .prepare_authoring_edit(session.revision(), &add("stale-edit"))
                .unwrap();
            let before = source_snapshot!(session);
            let mut work = SchemaReclassificationWork::default();
            let prepared = if owned {
                let incumbent = session
                    .retain_schema_reclassification_source(&mut work)
                    .unwrap();
                session
                    .capture_schema_reclassification(&next, &kind, &mut work, incumbent)
                    .unwrap()
                    .prepare(&mut work)
                    .unwrap()
            } else {
                session
                    .prepare_schema_reclassification(&next, &kind, &mut work)
                    .unwrap()
            };
            assert_eq!(prepared.readiness(), AuthoringDocumentReadiness::Complete);
            assert_source_unchanged!(session, before);
            session.commit_schema_reclassification(prepared).unwrap();
            assert_source_unchanged!(session, before);
            assert_eq!(session.authoring_schema_basis(), Some(next.clone()));
            assert!(!session.commit_authoring_edit(stale_edit).applied);
            assert_source_unchanged!(session, before);

            assert!(session.undo_authoring().applied);
            assert_eq!(session.document(), &first);
            assert_eq!(session.content_state(), first_content);
            assert!(session.undo_authoring().applied);
            assert_eq!(session.document().roots, source().roots);
            assert_eq!(session.document().entities, source().entities);
            // History restores the actual V1-created value, even though public creation
            // now requires V2's different complete initial value.
            assert!(session.redo_authoring().applied);
            assert_eq!(session.document(), &first);
            assert!(session.redo_authoring().applied);
            assert_eq!(session.document(), &second);
            assert!(session.redo_authoring().applied);
            assert_eq!(session.document(), &third);

            let before_creation = session.document().clone();
            assert!(
                !session
                    .apply_authoring_patch(&create("new-created", 1))
                    .applied
            );
            assert_eq!(session.document(), &before_creation);
            assert!(
                session
                    .apply_authoring_patch(&create("new-created", 2))
                    .applied
            );
            let new_creation = session.document().clone();
            assert!(session.undo_authoring().applied);
            assert_eq!(session.document(), &before_creation);
            assert!(session.redo_authoring().applied);
            assert_eq!(session.document(), &new_creation);
        });
    }
}

#[test]
fn removing_creation_template_preserves_direct_and_owned_concrete_history() {
    let (previous, next) = creation_pair(None);
    let kind = SchemaReclassificationKind::CreationMetadata {
        owner: owner(RECOVERED_OWNER),
    };
    assert!(
        next.schema(&ComponentTypeId::new(RECOVERED))
            .unwrap()
            .authoring_initial_value()
            .is_none()
    );
    for owned in [false, true] {
        each_source!(&source(), &previous, |session| {
            let original = session.document().clone();
            let original_content = session.content_state();
            assert!(
                session
                    .apply_authoring_patch(&create("old-created", 1))
                    .applied
            );
            let created = session.document().clone();
            let created_content = session.content_state();
            assert!(
                session
                    .apply_authoring_patch(&ScenePatchDocument::new([
                        ScenePatchOperation::RemoveComponent {
                            entity: id("old-created"),
                            component: ComponentTypeId::new(RECOVERED),
                        },
                    ]))
                    .applied
            );
            assert_eq!(session.document(), &original);
            let removed_content = session.content_state();
            assert!(session.undo_authoring().applied);
            assert_eq!(session.document(), &created);
            assert_eq!(session.history_status().undo_depth, 1);
            assert_eq!(session.history_status().redo_depth, 1);
            let before = source_snapshot!(session);
            let mut work = SchemaReclassificationWork::default();
            let prepared = if owned {
                let incumbent = session
                    .retain_schema_reclassification_source(&mut work)
                    .unwrap();
                session
                    .capture_schema_reclassification(&next, &kind, &mut work, incumbent)
                    .unwrap()
                    .prepare(&mut work)
                    .unwrap()
            } else {
                session
                    .prepare_schema_reclassification(&next, &kind, &mut work)
                    .unwrap()
            };
            assert_eq!(prepared.readiness(), AuthoringDocumentReadiness::Complete);
            assert_source_unchanged!(session, before);
            session.commit_schema_reclassification(prepared).unwrap();
            assert_source_unchanged!(session, before);
            assert_eq!(session.authoring_schema_basis(), Some(next.clone()));

            // Removing the template denies new creation without changing either history branch.
            let refused = session.apply_authoring_patch(&create("new-created", 1));
            assert!(!refused.applied);
            assert!(refused.inverse.is_none());
            assert_source_unchanged!(session, before);

            assert!(session.undo_authoring().applied);
            assert_eq!(session.document(), &original);
            assert_eq!(session.content_state(), original_content);
            // Redo creation restores its concrete V1 payload without a current template.
            assert!(session.redo_authoring().applied);
            assert_eq!(session.document(), &created);
            assert_eq!(session.content_state(), created_content);
            assert!(session.redo_authoring().applied);
            assert_eq!(session.document(), &original);
            assert_eq!(session.content_state(), removed_content);
            // Undo removal restores that same payload through the retained inverse.
            assert!(session.undo_authoring().applied);
            assert_eq!(session.document(), &created);
            assert_eq!(session.content_state(), created_content);
        });
    }
}

#[test]
fn creation_metadata_wrong_intent_owner_and_budget_never_publish() {
    let (previous, next) = creation_pair(Some(2));
    let correct = SchemaReclassificationKind::CreationMetadata {
        owner: owner(RECOVERED_OWNER),
    };
    each_source!(&source(), &previous, |session| {
        assert!(
            session
                .apply_authoring_patch(&create("old-created", 1))
                .applied
        );
        assert!(session.apply_authoring_patch(&set(42)).applied);
        assert!(session.undo_authoring().applied);
        let before = source_snapshot!(session);
        for kind in [
            DeclarationExtension,
            SchemaReclassificationKind::CreationMetadata {
                owner: owner(EXISTING_OWNER),
            },
        ] {
            let mut work = SchemaReclassificationWork::default();
            assert!(
                session
                    .prepare_schema_reclassification(&next, &kind, &mut work)
                    .is_err()
            );
            assert_eq!(work.usage(), SchemaReclassificationUsage::default());
            assert_source_unchanged!(session, before);
        }
        let mut measured = SchemaReclassificationWork::default();
        drop(
            session
                .prepare_schema_reclassification(&next, &correct, &mut measured)
                .unwrap(),
        );
        let usage = measured.usage();
        for (work, retained, budget) in [
            (usage.work_bytes - 1, usage.peak_retained_bytes, "work"),
            (usage.work_bytes, usage.peak_retained_bytes - 1, "retained"),
        ] {
            let mut allowance = work_with_limits(work, retained);
            let report = session
                .prepare_schema_reclassification(&next, &correct, &mut allowance)
                .unwrap_err();
            assert_limit(&report, budget);
            assert_source_unchanged!(session, before);
            assert_eq!(session.authoring_schema_basis(), Some(previous.clone()));
        }
        assert!(session.redo_authoring().applied);
        assert!(session.undo_authoring().applied);
    });
}
