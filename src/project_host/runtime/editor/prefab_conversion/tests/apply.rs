use nara_asset::AssetSourcePath;
use nara_reflect::ComponentPropertyPath;
use nara_scene::{
    PrefabComponentIntent, PrefabEntityTarget, PrefabOverlay, PrefabPropertyClaim,
    ScenePatchDocument, ScenePatchOperation, SourceDocumentKey,
};
use nara_tooling::{
    EditorPersistenceAttemptId, EditorPersistenceOperation, EditorPrefabInspectionId,
};

use super::*;
use crate::project_host::runtime::editor::SaveTestPhase;

#[test]
fn apply_rejects_working_content_mixed_with_an_unacknowledged_save() {
    let project = apply_project(29);
    let mut editor = project.editor();
    let containing = open_source(&mut editor, SCENE);
    let destination = open_source(&mut editor, BASE);
    let original = source_state(&editor, containing);
    let saved = original.document.entities[0].prefab.clone().unwrap();
    let mut changed = saved.clone();
    changed.overrides = Some(overlay(41));
    let outcome = dispatch(
        &mut editor,
        EditorAction::Mutate(EditorDocumentMutation::ApplyScenePatch {
            document: containing,
            base_revision: original.revision,
            patch: ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
                entity: id("anchor"),
                expected: Some(saved),
                value: Some(changed.clone()),
            }]),
        }),
    );
    assert!(matches!(outcome, EditorActionOutcome::Workspace(report) if report.applied));
    let gate = editor.hold_next_save(SaveTestPhase::AfterPublication);
    let (attempt, revision) = queue_save(&mut editor, containing);
    replay(&mut editor, containing, true);
    assert!(gate.wait_until_started());

    let restored = source_state(&editor, containing);
    assert_eq!(restored.document, original.document);
    assert_eq!(restored.saved_digest, original.saved_digest);
    assert!(!restored.dirty);
    let published = editor
        .authoring_source_selection(containing)
        .unwrap()
        .receipt
        .unwrap();
    assert_ne!(
        restored.saved_digest,
        Some(crate::project_host::persistence::editor_digest(
            published.digest()
        ))
    );
    let mut expected_disk = original.document.clone();
    expected_disk.entities[0].prefab = Some(changed);
    let disk = fs::read(project.root.join(SCENE)).unwrap();
    assert_eq!(
        SceneDocumentCandidate::decode_json_bytes(&disk)
            .unwrap()
            .to_json_string()
            .unwrap(),
        expected_disk.to_json_string().unwrap(),
    );

    let (operation, address) = inspect_amount(&mut editor, containing);
    assert_eq!(
        prepare_apply(&mut editor, operation, address, destination),
        InspectOutcome::Rejected {
            inspection: Some(operation),
            reason: InspectRejection::ApplySourceSaveRequired,
        }
    );
    assert_eq!(source_state(&editor, containing), restored);
    assert_eq!(editor.prefab_inspection.view().phase, InspectPhase::Ready);
    gate.release();
    settle_save(&mut editor, attempt, containing, revision);
    assert!(source_state(&editor, containing).dirty);
    close(&mut editor);
}

#[test]
fn apply_cannot_adopt_an_older_pending_save_but_can_reuse_its_confirmed_content() {
    let project = apply_project(0);
    let mut editor = project.editor();
    let containing = open_source(&mut editor, SCENE);
    let destination = open_source(&mut editor, BASE);
    let unrelated = open_source(&mut editor, OTHER);
    let original = source_state(&editor, containing);
    let base_revision = editor.document_query_basis(destination).unwrap().revision;
    let outcome = dispatch(
        &mut editor,
        EditorAction::Mutate(EditorDocumentMutation::ApplyPrefabPatch {
            document: destination,
            base_revision,
            patch: ScenePatchDocument::new([ScenePatchOperation::SetField {
                entity: id("leaf-0"),
                component: payload_type(),
                component_version: ComponentSchemaVersion::ONE,
                field: ComponentFieldId::new("amount"),
                value: ComponentValue::U64(29),
            }]),
        }),
    );
    assert!(matches!(outcome, EditorActionOutcome::Workspace(report) if report.applied));
    let gate = editor.hold_next_save(SaveTestPhase::BeforePublication);
    let (attempt, captured) = queue_save(&mut editor, destination);
    replay(&mut editor, destination, true);
    replay(&mut editor, destination, false);
    assert!(gate.wait_until_started());
    let live_revision = editor.document_query_basis(destination).unwrap().revision;
    assert_ne!(live_revision, captured);
    assert!(source_state(&editor, destination).dirty);

    let (operation, address) = inspect_amount(&mut editor, containing);
    assert_eq!(
        prepare_apply(&mut editor, operation, address, destination),
        InspectOutcome::Rejected {
            inspection: Some(operation),
            reason: InspectRejection::ApplySourceSaveRequired,
        }
    );
    assert_eq!(source_state(&editor, containing), original);
    assert_eq!(editor.prefab_inspection.view().phase, InspectPhase::Ready);
    assert!(matches!(
        inspection(
            &mut editor,
            InspectAction::Cancel {
                inspection: operation
            }
        ),
        InspectOutcome::Accepted {
            phase: InspectPhase::Terminal,
            ..
        }
    ));
    assert!(matches!(
        inspection(
            &mut editor,
            InspectAction::Acknowledge {
                inspection: operation
            }
        ),
        InspectOutcome::Accepted {
            phase: InspectPhase::Idle,
            ..
        }
    ));
    gate.release();
    settle_save(&mut editor, attempt, destination, captured);
    let checkpoint = source_state(&editor, destination);
    assert_eq!(checkpoint.revision, live_revision);
    assert_eq!(checkpoint.saved_revision, captured);
    assert!(!checkpoint.dirty);

    edit(&mut editor, unrelated, "unrelated-edit");
    let gate = editor.hold_next_save(SaveTestPhase::AfterPublication);
    let (attempt, unrelated_revision) = queue_save(&mut editor, unrelated);
    editor.drive_editor_frame(Duration::ZERO);
    assert!(gate.wait_until_started());
    let (operation, address) = inspect_amount(&mut editor, containing);
    assert!(matches!(
        prepare_apply(&mut editor, operation, address, destination),
        InspectOutcome::Accepted {
            phase: InspectPhase::PreparingEdit,
            ..
        }
    ));
    wait(&mut editor, |editor| {
        !editor.prefab_inspection.work_pending()
    });
    assert!(
        editor
            .prefab_apply_review(operation)
            .unwrap()
            .destination_edit
            .is_none()
    );
    assert!(matches!(
        inspection(
            &mut editor,
            InspectAction::Commit {
                inspection: operation
            }
        ),
        InspectOutcome::Accepted {
            phase: InspectPhase::Validating,
            ..
        }
    ));
    editor.drive_editor_frame(Duration::ZERO);
    let target = editor.prefab_inspection.view().apply_destination.unwrap();
    assert_eq!(
        (target.document, target.revision, target.changed),
        (destination, live_revision, false)
    );
    assert!(matches!(
        inspection(
            &mut editor,
            InspectAction::FinishApply {
                inspection: operation
            }
        ),
        InspectOutcome::Accepted {
            phase: InspectPhase::Validating,
            ..
        }
    ));
    editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        editor.prefab_inspection.view().result,
        Some(InspectResult::Applied { destination, .. }) if destination == target
    ));
    assert_eq!(source_state(&editor, destination), checkpoint);
    assert_ne!(
        source_state(&editor, containing).document,
        original.document
    );
    assert!(editor.persistence.has_pending_document(unrelated));
    gate.release();
    settle_save(&mut editor, attempt, unrelated, unrelated_revision);
    close(&mut editor);
}

#[test]
fn apply_reports_a_stale_containing_revision_before_asking_to_save_the_destination() {
    let project = apply_project(0);
    let mut editor = project.editor();
    let containing = open_source(&mut editor, SCENE);
    let destination = open_source(&mut editor, BASE);
    let original = source_state(&editor, containing);
    let disk = fs::read(project.root.join(BASE)).unwrap();

    let (operation, address) = inspect_amount(&mut editor, containing);
    assert!(matches!(
        prepare_apply(&mut editor, operation, address, destination),
        InspectOutcome::Accepted {
            phase: InspectPhase::PreparingEdit,
            ..
        }
    ));
    wait(&mut editor, |editor| {
        !editor.prefab_inspection.work_pending()
    });
    assert!(
        editor
            .prefab_apply_review(operation)
            .unwrap()
            .destination_edit
            .is_some()
    );
    assert!(matches!(
        inspection(
            &mut editor,
            InspectAction::Commit {
                inspection: operation
            }
        ),
        InspectOutcome::Accepted {
            phase: InspectPhase::Validating,
            ..
        }
    ));
    editor.drive_editor_frame(Duration::ZERO);
    let target = editor.prefab_inspection.view().apply_destination.unwrap();
    assert!(target.changed);
    assert!(source_state(&editor, destination).dirty);

    edit(&mut editor, containing, "other-edit");
    replay(&mut editor, containing, true);
    let stale = source_state(&editor, containing);
    assert_eq!(stale.document, original.document);
    assert_ne!(stale.revision, original.revision);
    assert!(!stale.dirty);
    assert!(matches!(
        inspection(
            &mut editor,
            InspectAction::FinishApply {
                inspection: operation
            }
        ),
        InspectOutcome::Accepted {
            phase: InspectPhase::Validating,
            ..
        }
    ));
    editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        editor.prefab_inspection.view().result,
        Some(InspectResult::ApplyStopped { destination, failure: Some(InspectRejection::StaleSource), .. })
            if destination == target
    ));
    assert_eq!(source_state(&editor, containing), stale);
    assert_eq!(fs::read(project.root.join(BASE)).unwrap(), disk);
    close(&mut editor);
}

#[test]
fn apply_checks_the_destination_domain_before_exposing_review() {
    let project = Project::new(1, false);
    let mut intent = overlay(0);
    let PrefabComponentIntent::Properties { claims, .. } = &mut intent.components[0] else {
        unreachable!();
    };
    claims[0] = PrefabPropertyClaim {
        path: ComponentPropertyPath::new(ComponentFieldId::new("annotation")),
        intent: PrefabPropertyIntent::Absent,
    };
    write_containing(&project, intent);
    let mut record = payload(0);
    record.set_value(ComponentValue::map([
        ("amount", ComponentValue::U64(0)),
        ("annotation", ComponentValue::String("before".into())),
    ]));
    let ui = SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("layer", ComponentValue::String("base".into()))]),
    );
    project.write_source(
        BASE,
        &SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id("leaf-0")).with_component(payload_type(), record),
            SceneEntityRecord::new(id("ui-root"))
                .with_component(ComponentTypeId::new("nara.ui.UiRoot"), ui),
        ]),
    );
    let mut editor = project.editor();
    let containing = open_source(&mut editor, SCENE);
    let destination = open_source(&mut editor, BASE);
    let original = source_state(&editor, containing);
    let target = source_state(&editor, destination);
    assert!(editor.prefab_uses_ui_authoring_domain(destination));
    let (operation, mut address) = inspect_amount(&mut editor, containing);
    address.property = ComponentPropertyPath::new(ComponentFieldId::new("annotation"));
    assert!(matches!(
        prepare_apply(&mut editor, operation, address, destination),
        InspectOutcome::Accepted {
            phase: InspectPhase::PreparingEdit,
            ..
        }
    ));
    wait(&mut editor, |editor| {
        !editor.prefab_inspection.work_pending()
    });
    assert!(
        matches!(
            editor.prefab_inspection.view().result,
            Some(InspectResult::Failed {
                reason: InspectRejection::InvalidProperty,
                ..
            })
        ),
        "{:?}",
        editor.prefab_inspection.view()
    );
    assert_eq!(source_state(&editor, containing), original);
    assert_eq!(source_state(&editor, destination), target);
    close(&mut editor);
}

fn amount_property() -> ComponentPropertyPath {
    ComponentPropertyPath::new(ComponentFieldId::new("amount"))
}

fn overlay(amount: u64) -> PrefabOverlay {
    let mut overlay = PrefabOverlay::new(SourceDocumentKey::project_location(
        UNIVERSE.parse().unwrap(),
        AssetSourcePath::new(BASE).unwrap(),
        SourceDocumentKind::Prefab,
    ));
    overlay.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(id("leaf-0")),
        component: payload_type(),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: amount_property(),
            intent: PrefabPropertyIntent::Set(ComponentValue::U64(amount)),
        }],
    });
    overlay
}

fn apply_project(base_amount: u64) -> Project {
    let project = Project::new(1, false);
    write_containing(&project, overlay(29));
    project.write_source(
        BASE,
        &SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("leaf-0"))
            .with_component(payload_type(), payload(base_amount))]),
    );
    project
}

fn write_containing(project: &Project, overrides: PrefabOverlay) {
    let mut anchor = SceneEntityRecord::new(id("anchor"));
    let mut instance = PrefabInstance::forest(AssetRef::path("base.prefab.json").unwrap());
    instance.overrides = Some(overrides);
    anchor.prefab = Some(instance);
    project.write_source(SCENE, &SceneDocument::from_ordered_roots([anchor]));
}

fn inspect_amount(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
) -> (EditorPrefabInspectionId, EditorPrefabPropertyAddress) {
    let basis = editor.document_query_basis(document).unwrap();
    let InspectOutcome::Accepted {
        inspection: operation,
        ..
    } = inspection(
        editor,
        InspectAction::Inspect {
            basis,
            anchor: id("anchor"),
        },
    )
    else {
        panic!("the selected occurrence must start inspection");
    };
    wait(editor, |editor| !editor.prefab_inspection.work_pending());
    let target = editor
        .prefab_effective_observation(operation)
        .unwrap_or_else(|error| {
            panic!(
                "inspection failed: {error:?}; {:?}",
                editor.prefab_inspection.view()
            )
        })
        .targets()
        .find(|target| target.entity.entity() == &id("leaf-0"))
        .unwrap()
        .clone();
    (
        operation,
        EditorPrefabPropertyAddress {
            entity: target,
            component: payload_type(),
            property: amount_property(),
        },
    )
}

fn prepare_apply(
    editor: &mut EditorProjectSession,
    operation: EditorPrefabInspectionId,
    address: EditorPrefabPropertyAddress,
    destination: EditorDocumentId,
) -> InspectOutcome {
    let destination = editor.document_query_basis(destination).unwrap();
    inspection(
        editor,
        InspectAction::PrepareApply {
            inspection: operation,
            address,
            destination,
            limits: Default::default(),
        },
    )
}

fn queue_save(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
) -> (EditorPersistenceAttemptId, SceneAuthoringRevision) {
    let base_revision = editor.document_query_basis(document).unwrap().revision;
    let outcome = dispatch(
        editor,
        EditorAction::Persistence(EditorPersistenceAction::Save {
            document,
            base_revision,
        }),
    );
    assert!(matches!(
        outcome,
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
    ));
    assert_eq!(
        editor.persistence_view().operation(),
        Some(EditorPersistenceOperation::Saving {
            document,
            captured_revision: base_revision,
        })
    );
    (editor.persistence_view().attempt().unwrap(), base_revision)
}

fn settle_save(
    editor: &mut EditorProjectSession,
    attempt: EditorPersistenceAttemptId,
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
) {
    wait(editor, |editor| {
        editor.persistence_view().result().is_some()
    });
    assert!(
        matches!(editor.persistence_view().result(), Some(EditorPersistenceResult::Saved {
        document: target, revision: saved, ..
    }) if target == document && saved == revision),
        "{:?}",
        editor.persistence_view()
    );
    assert!(matches!(
        dispatch(editor, EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult { attempt })),
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Acknowledged { attempt: acknowledged })
            if acknowledged == attempt
    ));
    editor.drive_editor_frame(Duration::ZERO);
}
