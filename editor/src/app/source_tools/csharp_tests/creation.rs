//! Actual managed owner evolution through the window-free public Application boundary.

use nara::{
    fs::RelativePath, project_host::EditorSchemaOwnerAdoptionId,
    reflect::ComponentSchemaOwnerChange,
};

use super::*;

const PREDECESSOR: &str = "enemy-stats.predecessor.catalog.json";

fn begin_evolution(
    application: &mut EditorApplication,
    installed: &InstalledSourceTool,
) -> EditorSchemaOwnerAdoptionId {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match application.prepare_installed_schema_owner(
            &installed.directory,
            installed.receipt,
            ComponentSchemaOwnerChange::CreationMetadata,
        ) {
            Ok(operation) => return operation,
            Err(EditorSchemaOwnerAdoptionError::Busy) => {
                assert!(
                    Instant::now() < deadline,
                    "Application evolution remained Busy"
                );
                application.advance(Duration::ZERO);
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("Application evolution admission failed: {error:?}"),
        }
    }
}

fn assert_stats(
    application: &mut EditorApplication,
    document: EditorDocumentId,
    entity: &SceneEntityId,
    health: i64,
    armor: i64,
) {
    for (field, expected) in [("health", health), ("armor", armor)] {
        assert_eq!(
            read_property(application, document, entity, STATS, field).unwrap(),
            EditorPropertyValue::Present(ComponentValue::I64(expected))
        );
    }
}

fn commit_bonus(
    application: &mut EditorApplication,
    document: EditorDocumentId,
    entity: &SceneEntityId,
    bonus: i64,
) {
    let basis = application.document_query_basis(document).unwrap();
    let activation = application
        .source_tool_view()
        .unwrap()
        .active
        .unwrap()
        .id()
        .clone();
    let EditorSourceToolActionOutcome::Requested(Ok(request)) = source_action(
        application,
        EditorSourceToolAction::Request {
            activation,
            basis: basis.clone(),
            intent: EditorSourceToolIntent::Edit {
                entity: entity.clone(),
            },
            parameters: BTreeMap::from([("bonus".into(), ComponentValue::I64(bonus))]),
        },
    ) else {
        panic!("The actual C# edit request was not admitted");
    };
    let view = settle_tool(application);
    assert_eq!(view.review.as_ref().unwrap().request, request);
    let inverse = {
        let review = application.source_tool_review().unwrap().unwrap();
        assert_eq!(review.request(), &request);
        assert_eq!(review.patch().operations.len(), 2);
        review.inverse().clone()
    };
    assert_eq!(application.document_query_basis(document).unwrap(), basis);
    let EditorSourceToolActionOutcome::Accepted(Ok(committed)) =
        source_action(application, EditorSourceToolAction::Accept { request })
    else {
        panic!("The actual C# review was not accepted");
    };
    assert!(committed.report.applied, "{:?}", committed.report);
    assert_eq!(committed.report.document, Some(document));
    assert!(committed.created_entity.is_none());
    assert_eq!(
        committed
            .report
            .patch_report
            .as_ref()
            .unwrap()
            .inverse
            .as_ref(),
        Some(&inverse)
    );
    assert!(application.source_tool_review().unwrap().is_none());
}

fn save(application: &mut EditorApplication, document: EditorDocumentId) {
    let revision = application.document_query_basis(document).unwrap().revision;
    let frame = application.current_frame().unwrap();
    let history = frame.workspace().document(document).unwrap().history;
    let submitted = application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision: revision,
            })],
        ),
        Duration::ZERO,
    );
    assert!(matches!(
        submitted.actions(),
        [EditorApplicationActionDispatch::Dispatched(
            EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
        )]
    ));
    let deadline = Instant::now() + TIMEOUT;
    loop {
        application.advance(Duration::ZERO);
        let frame = application.current_frame().unwrap();
        if frame.persistence().operation().is_none() {
            assert!(matches!(
                frame.persistence().result(),
                Some(EditorPersistenceResult::Saved { document: actual, revision: saved, .. })
                    if actual == document && saved == revision
            ));
            let tab = frame.workspace().document(document).unwrap();
            assert!(!tab.dirty);
            assert_eq!(tab.history, history);
            return;
        }
        assert!(Instant::now() < deadline, "Application Save did not settle");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
#[ignore = "run behavior-languages/run.py for actual .NET tool artifacts"]
fn csharp_source_tool_creation_metadata_evolution_preserves_existing_data_and_history() {
    let directory = PathBuf::from(std::env::var_os("NARA_SOURCE_TOOL_V1_DIR").unwrap());
    let successor_directory = PathBuf::from(
        std::env::var_os("NARA_SOURCE_TOOL_CREATION_V2_DIR")
            .expect("Runner supplies the C# tool with its embedded generation-two catalog"),
    );
    let executable = PathBuf::from(std::env::var_os("NARA_SOURCE_TOOL_DOTNET").unwrap());
    let owner_id = PersistentComponentSchemaOwnerId::try_new("trial.enemy-stats").unwrap();
    let predecessor_bytes = fs::read(successor_directory.join(PREDECESSOR)).unwrap();
    let owner = ComponentSchemaOwnerSnapshot::from_json_catalogs(
        owner_id.clone(),
        &fs::read(directory.join("enemy-stats.catalog.json")).unwrap(),
        None,
        Default::default(),
    )
    .unwrap();
    let successor = ComponentSchemaOwnerSnapshot::from_json_catalogs(
        owner_id,
        &fs::read(successor_directory.join("enemy-stats.catalog.json")).unwrap(),
        Some(&predecessor_bytes),
        Default::default(),
    )
    .unwrap();
    assert_eq!(owner.current().generation(), 1);
    assert_eq!(successor.current().generation(), 2);
    assert_eq!(successor.predecessor(), Some(owner.current()));
    let component = ComponentTypeId::new(STATS);
    let initial = ComponentValue::map([
        ("wire_health", ComponentValue::I64(250)),
        ("wire_armor", ComponentValue::I64(25)),
    ]);
    assert_eq!(
        successor
            .schema(&component)
            .unwrap()
            .authoring_initial_value(),
        Some(&initial)
    );
    let mut definition = successor.schema(&component).unwrap().clone();
    assert_eq!(definition.version, ComponentSchemaVersion::ONE);
    definition.authoring_initial_value = owner
        .schema(&component)
        .unwrap()
        .authoring_initial_value()
        .cloned();
    assert_eq!(&definition, owner.schema(&component).unwrap());

    let native =
        AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())
            .unwrap();
    let bundle = AuthoringSchemaBundle::from_owner_snapshots(
        native.owners().iter().cloned().chain([owner.clone()]),
    )
    .unwrap();
    let enemy = SceneEntityId::new("enemy").unwrap();
    let guide = SceneEntityId::new("guide").unwrap();
    let source = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(enemy.clone()).with_component(
            component.clone(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    ("wire_health", ComponentValue::I64(73)),
                    ("wire_armor", ComponentValue::I64(9)),
                ]),
            )
            .with_owner(owner.owner_id().clone()),
        ),
        SceneEntityRecord::new(guide.clone()),
    ]);
    let fixture = PersistenceFixture::new();
    let bundle_path = fixture.root.join("catalogs/authoring.bundle.json");
    let scene_path = fixture.root.join("worlds/entry.scene.json");
    let selection_path = fixture.root.join(SELECTION);
    fs::write(&bundle_path, bundle.to_json_string().unwrap()).unwrap();
    fs::write(&scene_path, source.to_json_string().unwrap()).unwrap();
    let original_bundle = fs::read(&bundle_path).unwrap();
    let original_source = fs::read(&scene_path).unwrap();
    let installed = install_tool(&fixture.root, &directory, &executable);
    let installed_successor = install_tool_release(
        &fixture.root,
        &successor_directory,
        &executable,
        "installed-creation-v2",
        "2",
        Some(PREDECESSOR),
    );
    let previous_leaf = installed
        .root
        .open_file(&RelativePath::new("catalogs/authoring.bundle.json").unwrap())
        .unwrap();
    assert_eq!(fs::read(&bundle_path).unwrap(), original_bundle);
    let mut application = fixture.open_with_expected_state(EditorApplicationState::AuthoringReady);
    let opened = settle_tool(&mut application);
    assert!(opened.active.is_none());
    assert!(opened.selection.as_ref().unwrap().desired.is_none());
    let document = application
        .current_frame()
        .unwrap()
        .workspace()
        .active_document
        .unwrap();
    enable_installed_tool(&mut application, &installed);
    let active = settle_tool(&mut application);
    let enabled = active.selection.as_ref().unwrap().desired.clone().unwrap();
    assert!(enabled.enabled);
    assert_eq!(enabled.installed.directory.as_ref(), installed.directory);
    assert_eq!(enabled.installed.receipt, installed.receipt);
    assert_selection(&active, &enabled, true);
    let old_activation = active.active.as_ref().unwrap().id().clone();
    let enabled_bytes = fs::read(&selection_path).unwrap();
    commit_bonus(&mut application, document, &enemy, 5);
    commit_bonus(&mut application, document, &enemy, 7);
    assert_stats(&mut application, document, &enemy, 85, 9);
    replay(&mut application, document, false);
    assert_stats(&mut application, document, &enemy, 78, 9);
    let before = application
        .current_frame()
        .unwrap()
        .workspace()
        .document(document)
        .unwrap()
        .clone();
    assert!(before.dirty);
    assert_eq!(before.history.undo_depth, 1);
    assert_eq!(before.history.redo_depth, 1);
    let old_basis = application.document_query_basis(document).unwrap();
    let old_edit = application
        .prepare_authoring_edit(
            &old_basis,
            &ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                entity: SceneEntityRecord::new(
                    SceneEntityId::new("stale-before-evolution").unwrap(),
                ),
                placement: ScenePlacement::last(ScenePlacementOwner::Root),
            }]),
        )
        .unwrap();

    // Inspection identifies the affected active V1 owner before any durable write.
    let refused = begin_evolution(&mut application, &installed_successor);
    let failed = settle_schema_owner(&mut application, EditorSchemaOwnerAdoptionPhase::Terminal);
    assert_eq!(failed.operation, Some(refused));
    assert_eq!(failed.change, ComponentSchemaOwnerChange::CreationMetadata);
    assert_eq!(
        failed.outcome,
        Some(EditorSchemaOwnerAdoptionOutcome::Failed(
            EditorSchemaOwnerAdoptionError::ActiveTool,
        ))
    );
    assert_eq!(
        application.schema_owner_adoption_failure().unwrap().phase(),
        EditorSchemaOwnerAdoptionPhase::Preparing
    );
    assert!(application.schema_owner_adoption_publication().is_none());
    assert_eq!(
        application.document_query_basis(document).unwrap(),
        old_basis
    );
    assert_eq!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .document(document)
            .unwrap(),
        &before
    );
    assert_eq!(fs::read(&bundle_path).unwrap(), original_bundle);
    assert_eq!(fs::read(&scene_path).unwrap(), original_source);
    assert_eq!(fs::read(&selection_path).unwrap(), enabled_bytes);
    assert_eq!(
        application.source_tool_view().unwrap().active.unwrap().id(),
        &old_activation
    );
    application
        .acknowledge_schema_owner_adoption(refused)
        .unwrap();

    // Disable revokes a real admitted invocation and waits for the task owner's return.
    let EditorSourceToolActionOutcome::Requested(Ok(revoked)) = source_action(
        &mut application,
        EditorSourceToolAction::Request {
            activation: old_activation.clone(),
            basis: old_basis.clone(),
            intent: EditorSourceToolIntent::Edit {
                entity: enemy.clone(),
            },
            parameters: BTreeMap::from([("bonus".into(), ComponentValue::I64(99))]),
        },
    ) else {
        panic!("The actual V1 invocation was not admitted");
    };
    let selection = application.source_tool_selection_view().unwrap();
    let disable = application
        .disable_installed_source_tool(&selection.basis)
        .unwrap();
    let disabled = settle_tool(&mut application);
    assert!(!disabled.has_pending_work() && !disabled.process_cleanup_failed);
    let mut desired_disabled = enabled.clone();
    desired_disabled.enabled = false;
    assert_selection(&disabled, &desired_disabled, false);
    let disable_result = disabled
        .selection
        .as_ref()
        .unwrap()
        .disable
        .as_ref()
        .unwrap();
    assert_eq!(disable_result.attempt, disable);
    assert_eq!(
        disable_result.outcome,
        Some(EditorSourceToolSelectionOutcome::Disabled)
    );
    assert!(matches!(
        source_action(
            &mut application,
            EditorSourceToolAction::Accept { request: revoked }
        ),
        EditorSourceToolActionOutcome::Accepted(Err(EditorSourceToolError::Disabled))
    ));
    let disabled_bytes = fs::read(&selection_path).unwrap();
    assert_ne!(disabled_bytes, enabled_bytes);

    let operation = begin_evolution(&mut application, &installed_successor);
    let review = settle_schema_owner(&mut application, EditorSchemaOwnerAdoptionPhase::Review);
    assert_eq!(review.operation, Some(operation));
    assert_eq!(review.change, ComponentSchemaOwnerChange::CreationMetadata);
    assert_eq!(review.owner.as_deref(), Some("trial.enemy-stats"));
    assert_eq!(review.documents, 1);
    assert_eq!(review.outcome, None);
    assert!(!review.reconciliation);
    assert!(application.schema_owner_adoption_publication().is_none());
    assert_eq!(
        application.document_query_basis(document).unwrap(),
        old_basis
    );
    assert_eq!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .document(document)
            .unwrap(),
        &before
    );
    assert_eq!(fs::read(&bundle_path).unwrap(), original_bundle);
    assert_eq!(fs::read(&scene_path).unwrap(), original_source);
    assert_eq!(fs::read(&selection_path).unwrap(), disabled_bytes);
    application.adopt_schema_owner(operation).unwrap();
    let adopted = settle_schema_owner(&mut application, EditorSchemaOwnerAdoptionPhase::Terminal);
    assert_eq!(adopted.operation, Some(operation));
    assert_eq!(adopted.change, ComponentSchemaOwnerChange::CreationMetadata);
    assert_eq!(
        adopted.outcome,
        Some(EditorSchemaOwnerAdoptionOutcome::Adopted)
    );
    let basis = application.document_query_basis(document).unwrap();
    assert_eq!(basis.revision, old_basis.revision);
    assert_ne!(basis.schema, old_basis.schema);
    assert_eq!(
        basis
            .schema
            .as_ref()
            .unwrap()
            .schema(&component)
            .unwrap()
            .authoring_initial_value(),
        Some(&initial)
    );
    let mut reclassified = application
        .current_frame()
        .unwrap()
        .workspace()
        .document(document)
        .unwrap()
        .clone();
    reclassified.authoring_schema_basis = before.authoring_schema_basis.clone();
    assert_eq!(reclassified, before);
    assert_stats(&mut application, document, &enemy, 78, 9);
    assert_eq!(
        read_property_at(&mut application, old_basis.clone(), &enemy, STATS, "health"),
        Err(EditorDocumentQueryRejection::StaleSchema)
    );
    let rejected = application.commit_authoring_edit(old_edit, Duration::ZERO);
    assert!(matches!(
        rejected.actions(),
        [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))]
            if !report.applied
    ));
    assert_eq!(application.document_query_basis(document).unwrap(), basis);
    let adopted_bundle = fs::read(&bundle_path).unwrap();
    let expected = AuthoringSchemaBundle::from_owner_snapshots(
        native.owners().iter().cloned().chain([successor.clone()]),
    )
    .unwrap();
    assert_eq!(
        AuthoringSchemaBundle::from_json_bytes(&adopted_bundle).unwrap(),
        expected
    );
    let actual_leaf = installed
        .root
        .open_file(&RelativePath::new("catalogs/authoring.bundle.json").unwrap())
        .unwrap();
    let publication = application.schema_owner_adoption_publication().unwrap();
    assert!(publication.previous_identity().is_some());
    assert!(publication.published_identity().is_some());
    assert_ne!(
        publication.previous_identity(),
        publication.published_identity()
    );
    // The installation root and Application admission use distinct capability sessions.
    // Compare their held native objects through the filesystem owner, not scoped ID equality.
    assert!(!actual_leaf.same_object(&previous_leaf).unwrap());
    assert_eq!(
        publication.verified_content_digest(),
        Some(ContentDigest::of_bytes(&adopted_bundle))
    );
    assert_eq!(fs::read(&scene_path).unwrap(), original_source);
    assert_eq!(fs::read(&selection_path).unwrap(), disabled_bytes);
    assert_selection(
        &application.source_tool_view().unwrap(),
        &desired_disabled,
        false,
    );
    application
        .acknowledge_schema_owner_adoption(operation)
        .unwrap();

    // Both retained history directions still replay their original stored values.
    replay(&mut application, document, false);
    assert_stats(&mut application, document, &enemy, 73, 9);
    assert!(
        !application
            .current_frame()
            .unwrap()
            .workspace()
            .document(document)
            .unwrap()
            .dirty
    );
    replay(&mut application, document, true);
    assert_stats(&mut application, document, &enemy, 78, 9);
    replay(&mut application, document, true);
    assert_stats(&mut application, document, &enemy, 85, 9);
    replay(&mut application, document, false);
    assert_stats(&mut application, document, &enemy, 78, 9);
    assert_eq!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .document(document)
            .unwrap()
            .history,
        before.history
    );

    // Creation uses V2 data while executable approval remains disabled.
    let basis = application.document_query_basis(document).unwrap();
    let mut draft = application.begin_authoring_construction(&basis).unwrap();
    draft
        .add_component(guide.clone(), component.clone())
        .unwrap();
    let prepared = application
        .prepare_authoring_construction(&basis, &draft)
        .unwrap();
    let committed = application.commit_authoring_construction(prepared, Duration::ZERO);
    assert!(matches!(
        committed.submission().actions(),
        [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))]
            if report.applied
    ));
    assert_stats(&mut application, document, &guide, 250, 25);
    assert_stats(&mut application, document, &enemy, 78, 9);
    replay(&mut application, document, false);
    assert_eq!(
        read_property(&mut application, document, &guide, STATS, "health"),
        Err(EditorDocumentQueryRejection::Source(
            AuthoringReadError::ComponentNotFound
        ))
    );
    replay(&mut application, document, true);
    assert_stats(&mut application, document, &guide, 250, 25);
    assert_selection(
        &application.source_tool_view().unwrap(),
        &desired_disabled,
        false,
    );
    assert_eq!(fs::read(&scene_path).unwrap(), original_source);
    assert_eq!(fs::read(&selection_path).unwrap(), disabled_bytes);
    assert_eq!(fs::read(&bundle_path).unwrap(), adopted_bundle);
    save(&mut application, document);
    let intermediate_source = fs::read(&scene_path).unwrap();
    assert_ne!(intermediate_source, original_source);
    close(&mut application);
    drop(application);

    // Fresh Open observes the new Schema and durable disabled V1 selection without a V2 probe.
    let mut application = fixture.open_with_expected_state(EditorApplicationState::AuthoringReady);
    let opened = settle_tool(&mut application);
    assert_selection(&opened, &desired_disabled, false);
    assert!(opened.attempt.is_none() && opened.request.is_none() && opened.review.is_none());
    assert_eq!(fs::read(&selection_path).unwrap(), disabled_bytes);
    assert_eq!(fs::read(&bundle_path).unwrap(), adopted_bundle);
    assert_eq!(fs::read(&scene_path).unwrap(), intermediate_source);
    let document = application
        .current_frame()
        .unwrap()
        .workspace()
        .active_document
        .unwrap();
    let frame = application.current_frame().unwrap();
    let tab = frame.workspace().document(document).unwrap();
    assert!(!tab.dirty && tab.history.undo_depth == 0 && tab.history.redo_depth == 0);
    assert_eq!(
        tab.authoring_schema_basis
            .as_ref()
            .unwrap()
            .schema(&component)
            .unwrap()
            .authoring_initial_value(),
        Some(&initial)
    );
    assert_stats(&mut application, document, &enemy, 78, 9);
    assert_stats(&mut application, document, &guide, 250, 25);

    let attempt = enable_installed_tool(&mut application, &installed_successor);
    let active = settle_tool(&mut application);
    let enabled_successor = active.selection.as_ref().unwrap().desired.clone().unwrap();
    assert!(enabled_successor.enabled);
    assert_selection(&active, &enabled_successor, true);
    assert_eq!(active.attempt.as_ref(), Some(&attempt));
    assert_eq!(
        enabled_successor.installed.directory.as_ref(),
        installed_successor.directory
    );
    assert_eq!(
        enabled_successor.installed.receipt,
        installed_successor.receipt
    );
    assert_ne!(active.active.as_ref().unwrap().id(), &old_activation);
    assert_eq!(fs::read(&bundle_path).unwrap(), adopted_bundle);
    let enabled_successor_bytes = fs::read(&selection_path).unwrap();
    // This artifact doubles the bonus; catalog evolution alone never ran that code.
    for (entity, old_health, health, armor) in [(&enemy, 78, 92, 9), (&guide, 250, 264, 25)] {
        commit_bonus(&mut application, document, entity, 7);
        assert_stats(&mut application, document, entity, health, armor);
        replay(&mut application, document, false);
        assert_stats(&mut application, document, entity, old_health, armor);
        replay(&mut application, document, true);
        assert_stats(&mut application, document, entity, health, armor);
    }
    assert_eq!(fs::read(&scene_path).unwrap(), intermediate_source);
    save(&mut application, document);
    let saved_source = fs::read(&scene_path).unwrap();
    assert_ne!(saved_source, intermediate_source);
    close(&mut application);
    drop(application);

    let mut application = fixture.open_with_expected_state(EditorApplicationState::AuthoringReady);
    let opened = settle_tool(&mut application);
    assert_selection(&opened, &enabled_successor, false);
    assert!(opened.attempt.is_none() && opened.request.is_none() && opened.review.is_none());
    assert_eq!(fs::read(&bundle_path).unwrap(), adopted_bundle);
    assert_eq!(fs::read(&selection_path).unwrap(), enabled_successor_bytes);
    assert_eq!(fs::read(&scene_path).unwrap(), saved_source);
    let document = application
        .current_frame()
        .unwrap()
        .workspace()
        .active_document
        .unwrap();
    assert_stats(&mut application, document, &enemy, 92, 9);
    assert_stats(&mut application, document, &guide, 264, 25);
    let frame = application.current_frame().unwrap();
    let tab = frame.workspace().document(document).unwrap();
    assert!(!tab.dirty && tab.history.undo_depth == 0 && tab.history.redo_depth == 0);
    close(&mut application);
}
