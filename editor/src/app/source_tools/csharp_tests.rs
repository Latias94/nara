//! Installed C# metadata adoption and durable selection through the public Application boundary.

use std::{
    collections::BTreeMap,
    fs,
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use nara::{
    fs::{
        CapabilityRights, ContentDigest, DirectoryCapability, HostCapabilityOptions,
        RelativeComponent, TrustMode,
    },
    project_host::{
        EditorSchemaOwnerAdoptionError, EditorSchemaOwnerAdoptionOutcome,
        EditorSchemaOwnerAdoptionPhase, EditorSchemaOwnerAdoptionView,
        EditorSourceToolInstallationOutcome, EditorSourceToolInstallationPlan,
        EditorSourceToolManagedHost, EditorSourceToolManagedHostDigests,
        PreparedEditorSourceToolRelease, advanced::built_in_schema_providers,
        open_explicit_product_project_root,
    },
    reflect::{
        AuthoringSchemaBundle, ComponentFieldId, ComponentPropertyPath, ComponentSchemaCatalog,
        ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        PersistentComponentSchemaOwnerId,
    },
    scene::{
        AuthoringReadError, AuthoringRecordReadiness, SceneComponentRecord, SceneDocument,
        SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation, ScenePlacement,
        ScenePlacementOwner,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorAuthoringProjection,
        EditorAuthoringQuery, EditorAuthoringQueryLimits, EditorAuthoringQueryPosition,
        EditorAuthoringQueryTarget, EditorAuthoringRow, EditorCloseDecision, EditorDocumentId,
        EditorDocumentMutation, EditorDocumentQueryBasis, EditorDocumentQueryRejection,
        EditorPersistenceAction, EditorPersistenceRequestResult, EditorPersistenceResult,
        EditorPropertyRead, EditorPropertyReadLimits, EditorPropertyTarget, EditorPropertyValue,
        EditorSourceToolAction, EditorSourceToolActionOutcome, EditorSourceToolAttemptId,
        EditorSourceToolDesiredSelection, EditorSourceToolError, EditorSourceToolIntent,
        EditorSourceToolSelectionError, EditorSourceToolSelectionOutcome,
        EditorSourceToolSelectionPhase, EditorSourceToolView,
    },
};

use crate::app::{
    EditorApplication, EditorApplicationActionDispatch, EditorApplicationState,
    persistence_tests::{PersistenceFixture, close},
};

const STATS: &str = "trial.enemy.EnemyStats";
const NAME: &str = "nara.scene.Name";
const TIMEOUT: Duration = Duration::from_secs(30);
const INSTALLATION: &str = "installed-tool";
const SELECTION: &str = ".nara/source-tools/selection.json";

struct InstalledSourceTool {
    root: DirectoryCapability,
    receipt: ContentDigest,
    host: EditorSourceToolManagedHost,
}

fn install_tool(project: &Path, artifacts: &Path, executable: &Path) -> InstalledSourceTool {
    let source = project.join("tool-release-source");
    fs::create_dir(&source).unwrap();
    let mut files = Vec::new();
    for member in [
        "EnemyStatsTool.dll",
        "EnemyStatsTool.deps.json",
        "EnemyStatsTool.runtimeconfig.json",
        "enemy-stats.catalog.json",
    ] {
        let bytes = fs::read(artifacts.join(member)).unwrap();
        fs::write(source.join(member), &bytes).unwrap();
        let digest = ContentDigest::of_bytes(&bytes);
        let hash: String = digest
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        files.push(serde_json::json!({
            "path": member,
            "length": digest.length(),
            "blake3": hash,
            "kind": "tool",
        }));
    }
    let manifest = serde_json::to_vec(&serde_json::json!({
        "format_version": 1,
        "package": "trial.enemy-stats",
        "release": "1",
        "host_target": "x86_64-pc-windows-msvc",
        "authority": "trusted_process",
        "protocol_version": 1,
        "owner": "trial.enemy-stats",
        "component": STATS,
        "catalog": "enemy-stats.catalog.json",
        "entry": "EnemyStatsTool.dll",
        "requires": [],
        "files": files,
    }))
    .unwrap();
    fs::write(source.join("nara-tool.json"), &manifest).unwrap();
    let captured = PreparedEditorSourceToolRelease::capture(
        &open_explicit_product_project_root(&source).unwrap(),
        ContentDigest::of_bytes(&manifest),
    )
    .unwrap();
    let handle = fs::OpenOptions::new()
        .read(true)
        .share_mode(7)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(project)
        .unwrap();
    let root = DirectoryCapability::from_host_handle(
        handle,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::Untrusted),
    )
    .unwrap();
    let plan = EditorSourceToolInstallationPlan::new(
        root.share(),
        RelativeComponent::new(INSTALLATION).unwrap(),
        captured,
    )
    .unwrap();
    let installation = match plan.publish() {
        EditorSourceToolInstallationOutcome::Published(installation) => installation,
        other => panic!("Actual project-root release publication failed: {other:?}"),
    };
    let receipt = installation.receipt_digest();
    drop(installation);
    let bootstrap = PathBuf::from(
        std::env::var_os("NARA_SOURCE_TOOL_BOOTSTRAP_DIR")
            .expect("Runner supplies the actual managed Host output directory"),
    );
    let digest = |path: &Path| ContentDigest::of_bytes(&fs::read(path).unwrap());
    let host = EditorSourceToolManagedHost::new(
        open_explicit_product_project_root(executable.parent().unwrap()).unwrap(),
        open_explicit_product_project_root(&bootstrap).unwrap(),
        EditorSourceToolManagedHostDigests {
            launcher: digest(executable),
            bootstrap: digest(&bootstrap.join("Nara.SourceToolHost.dll")),
            deps: digest(&bootstrap.join("Nara.SourceToolHost.deps.json")),
            runtimeconfig: digest(&bootstrap.join("Nara.SourceToolHost.runtimeconfig.json")),
        },
        &std::env::var("NARA_SOURCE_TOOL_FRAMEWORK").unwrap(),
        TIMEOUT,
    )
    .unwrap();
    InstalledSourceTool {
        root,
        receipt,
        host,
    }
}

fn enable_installed_tool(
    application: &mut EditorApplication,
    installed: &InstalledSourceTool,
) -> EditorSourceToolAttemptId {
    let selection = application.source_tool_selection_view().unwrap();
    let prepared = application
        .prepare_installed_source_tool_selection(
            installed.root.share(),
            INSTALLATION,
            installed.receipt,
            &installed.host,
        )
        .unwrap();
    let before = application.source_tool_selection_view().unwrap();
    assert_eq!(
        before, selection,
        "Host preparation must not activate or publish"
    );
    application
        .enable_installed_source_tool(&selection.basis, prepared)
        .unwrap()
}

fn assert_selection(
    view: &EditorSourceToolView,
    desired: &EditorSourceToolDesiredSelection,
    active: bool,
) {
    let selection = view.selection.as_ref().unwrap();
    assert_eq!(selection.phase, EditorSourceToolSelectionPhase::Idle);
    assert!(selection.error.is_none(), "{selection:?}");
    assert_eq!(selection.desired.as_ref(), Some(desired));
    assert_eq!(
        selection.active_origin.as_ref(),
        active.then_some(&desired.installed),
    );
    assert_eq!(view.active.is_some(), active);
}

fn settle_tool(application: &mut EditorApplication) -> EditorSourceToolView {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        application.advance(Duration::ZERO);
        let view = application.current_frame().unwrap().source_tool().clone();
        if !view.has_pending_work()
            && view.selection.as_ref().is_some_and(|selection| {
                selection
                    .operation
                    .as_ref()
                    .is_some_and(|operation| operation.outcome.is_some())
                    && selection
                        .disable
                        .as_ref()
                        .is_none_or(|disable| disable.outcome.is_some())
            })
        {
            assert!(
                view.attempt_error.is_none()
                    && view.request_error.is_none()
                    && view.selection.as_ref().is_some_and(|selection| {
                        selection.error.is_none()
                            && selection.phase == EditorSourceToolSelectionPhase::Idle
                    }),
                "{view:?}"
            );
            return view;
        }
        assert!(
            Instant::now() < deadline,
            "Actual C# child did not return: {view:?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn source_action(
    application: &mut EditorApplication,
    action: EditorSourceToolAction,
) -> EditorSourceToolActionOutcome {
    let frame = application.current_frame().unwrap();
    let submitted = application.submit_and_drive(
        EditorActionBatch::new(frame.revision(), [EditorAction::SourceTool(action)]),
        Duration::ZERO,
    );
    let [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::SourceTool(outcome))] =
        submitted.actions()
    else {
        panic!("Application lost the source-tool action outcome: {submitted:?}");
    };
    outcome.clone()
}

fn read_property(
    application: &mut EditorApplication,
    document: EditorDocumentId,
    entity: &SceneEntityId,
    component: &str,
    field: &str,
) -> Result<EditorPropertyValue, EditorDocumentQueryRejection> {
    let basis = application.document_query_basis(document).unwrap();
    read_property_at(application, basis, entity, component, field)
}

fn read_property_at(
    application: &mut EditorApplication,
    basis: EditorDocumentQueryBasis,
    entity: &SceneEntityId,
    component: &str,
    field: &str,
) -> Result<EditorPropertyValue, EditorDocumentQueryRejection> {
    let target = EditorPropertyTarget {
        basis,
        entity: entity.clone(),
        component: ComponentTypeId::new(component),
        property: ComponentPropertyPath::new(ComponentFieldId::new(field)),
    };
    let frame = application.current_frame().unwrap();
    let submitted = application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::ReadDocumentProperty(EditorPropertyRead {
                target,
                limits: EditorPropertyReadLimits::new(4096, 64, 4096).unwrap(),
            })],
        ),
        Duration::ZERO,
    );
    let [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::DocumentProperty(result)),
    ] = submitted.actions()
    else {
        panic!("Application lost the property read outcome: {submitted:?}");
    };
    result
        .as_ref()
        .map(|observation| observation.value.clone())
        .map_err(Clone::clone)
}

fn component_readiness(
    application: &mut EditorApplication,
    document: EditorDocumentId,
    entity: &SceneEntityId,
) -> AuthoringRecordReadiness {
    let basis = application.document_query_basis(document).unwrap();
    let frame = application.current_frame().unwrap();
    let submitted = application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::QueryDocumentAuthoring(EditorAuthoringQuery {
                position: EditorAuthoringQueryPosition::Start(EditorAuthoringQueryTarget {
                    basis,
                    entity: entity.clone(),
                    projection: EditorAuthoringProjection::Components,
                }),
                limits: EditorAuthoringQueryLimits::new(16, 16, 4096).unwrap(),
            })],
        ),
        Duration::ZERO,
    );
    let [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::DocumentAuthoring(Ok(
            page,
        ))),
    ] = submitted.actions()
    else {
        panic!("Application lost the component metadata observation: {submitted:?}");
    };
    assert!(page.next.is_none());
    page.rows
        .iter()
        .find_map(|row| match row {
            EditorAuthoringRow::Component {
                component,
                readiness,
                ..
            } if component == &ComponentTypeId::new(STATS) => Some(*readiness),
            _ => None,
        })
        .expect("The original EnemyStats source record must remain discoverable")
}

fn settle_schema_owner(
    application: &mut EditorApplication,
    phase: EditorSchemaOwnerAdoptionPhase,
) -> EditorSchemaOwnerAdoptionView {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        application.advance(Duration::ZERO);
        let view = application.schema_owner_adoption_view().unwrap();
        if view.phase == phase
            || matches!(
                view.phase,
                EditorSchemaOwnerAdoptionPhase::Terminal
                    | EditorSchemaOwnerAdoptionPhase::RequiresReconcile
            )
        {
            assert_eq!(view.phase, phase, "{view:?}");
            return view.clone();
        }
        assert!(
            Instant::now() < deadline,
            "Application metadata adoption did not settle: {view:?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn replay(application: &mut EditorApplication, document: EditorDocumentId, redo: bool) {
    let frame = application.current_frame().unwrap();
    let base_revision = application.document_query_basis(document).unwrap().revision;
    let mutation = if redo {
        EditorDocumentMutation::Redo {
            document,
            base_revision,
        }
    } else {
        EditorDocumentMutation::Undo {
            document,
            base_revision,
        }
    };
    let submitted = application.submit_and_drive(
        EditorActionBatch::new(frame.revision(), [EditorAction::Mutate(mutation)]),
        Duration::ZERO,
    );
    let [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] =
        submitted.actions()
    else {
        panic!("Application lost the history outcome: {submitted:?}");
    };
    assert!(report.applied, "{report:?}");
    assert_eq!(report.document, Some(document));
}

#[test]
#[ignore = "run behavior-languages/run.py for actual .NET tool artifacts"]
fn csharp_source_tool_application_preserves_review_and_exact_commit_result() {
    let directory = PathBuf::from(
        std::env::var_os("NARA_SOURCE_TOOL_V1_DIR")
            .expect("Runner supplies the actual C# tool output directory"),
    );
    let executable = PathBuf::from(
        std::env::var_os("NARA_SOURCE_TOOL_DOTNET")
            .expect("Runner supplies the resolved dotnet executable"),
    );
    let catalog = ComponentSchemaCatalog::from_json_bytes(
        &fs::read(directory.join("enemy-stats.catalog.json")).unwrap(),
    )
    .unwrap();
    let owner = ComponentSchemaOwnerSnapshot::admit(
        PersistentComponentSchemaOwnerId::try_new("trial.enemy-stats").unwrap(),
        catalog,
        None,
    )
    .unwrap();
    let component = ComponentTypeId::new(STATS);
    let native =
        AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())
            .unwrap();
    assert!(native.owner(owner.owner_id()).is_none());
    let name_owner = native
        .owner_for_component(&ComponentTypeId::new(NAME))
        .unwrap();
    let enemy = SceneEntityId::new("enemy").unwrap();
    let source = SceneDocument::from_ordered_roots([SceneEntityRecord::new(enemy.clone())
        .with_component(
            ComponentTypeId::new(NAME),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("Guard".into()),
            )
            .with_owner(name_owner.owner_id().clone()),
        )
        .with_component(
            component.clone(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                owner
                    .schema(&component)
                    .unwrap()
                    .authoring_initial_value()
                    .unwrap()
                    .clone(),
            )
            .with_owner(owner.owner_id().clone()),
        )]);
    let fixture = PersistenceFixture::new();
    let bundle_path = fixture.root.join("catalogs/authoring.bundle.json");
    fs::write(&bundle_path, native.to_json_string().unwrap()).unwrap();
    let original_bundle = fs::read(&bundle_path).unwrap();
    let scene_path = fixture.root.join("worlds/entry.scene.json");
    fs::write(&scene_path, source.to_json_string().unwrap()).unwrap();
    let original_bytes = fs::read(&scene_path).unwrap();
    let installed = install_tool(&fixture.root, &directory, &executable);
    assert_eq!(fs::read(&bundle_path).unwrap(), original_bundle);
    let mut application =
        fixture.open_with_expected_state(EditorApplicationState::AuthoringDegraded);
    let document = application
        .current_frame()
        .unwrap()
        .workspace()
        .active_document
        .unwrap();
    let old_basis = application.document_query_basis(document).unwrap();
    assert_eq!(
        old_basis
            .schema
            .as_ref()
            .unwrap()
            .component_schema_owner(&ComponentTypeId::new(STATS)),
        None,
    );
    assert_eq!(
        component_readiness(&mut application, document, &enemy),
        AuthoringRecordReadiness::UnknownSchema
    );
    let opened = settle_tool(&mut application);
    assert!(opened.active.is_none() && opened.attempt.is_none() && opened.review.is_none());
    let selection = opened.selection.as_ref().unwrap();
    assert!(selection.desired.is_none() && selection.active_origin.is_none());
    assert_eq!(
        selection.operation.as_ref().unwrap().outcome,
        Some(EditorSourceToolSelectionOutcome::Observed),
    );
    assert!(!fixture.root.join(SELECTION).exists());

    let old_edit = application
        .prepare_authoring_edit(
            &old_basis,
            &ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                entity: SceneEntityRecord::new(
                    SceneEntityId::new("stale-before-adoption").unwrap(),
                ),
                placement: ScenePlacement::last(ScenePlacementOwner::Root),
            }]),
        )
        .unwrap();
    let deadline = Instant::now() + TIMEOUT;
    let adoption = loop {
        match application.prepare_installed_schema_owner(INSTALLATION, installed.receipt) {
            Ok(operation) => break operation,
            Err(EditorSchemaOwnerAdoptionError::Busy) => {
                assert!(
                    Instant::now() < deadline,
                    "Application metadata admission remained Busy"
                );
                application.advance(Duration::ZERO);
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("Application metadata preparation failed: {error:?}"),
        }
    };
    let review = settle_schema_owner(&mut application, EditorSchemaOwnerAdoptionPhase::Review);
    assert_eq!(review.operation, Some(adoption));
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
        component_readiness(&mut application, document, &enemy),
        AuthoringRecordReadiness::UnknownSchema
    );
    assert_eq!(fs::read(&bundle_path).unwrap(), original_bundle);
    assert_eq!(fs::read(&scene_path).unwrap(), original_bytes);
    let before_enable = application.source_tool_view().unwrap();
    assert!(
        before_enable.active.is_none()
            && before_enable.attempt.is_none()
            && before_enable.request.is_none()
            && before_enable.review.is_none()
    );
    assert!(!fixture.root.join(SELECTION).exists());

    application.adopt_schema_owner(adoption).unwrap();
    let adopted = settle_schema_owner(&mut application, EditorSchemaOwnerAdoptionPhase::Terminal);
    assert_eq!(adopted.operation, Some(adoption));
    assert_eq!(
        adopted.outcome,
        Some(EditorSchemaOwnerAdoptionOutcome::Adopted)
    );
    assert_eq!(application.state(), EditorApplicationState::AuthoringReady);
    assert!(application.schema_owner_adoption_publication().is_some());
    let basis = application.document_query_basis(document).unwrap();
    assert_eq!(basis.revision, old_basis.revision);
    assert_ne!(basis.schema, old_basis.schema);
    assert_eq!(
        basis
            .schema
            .as_ref()
            .unwrap()
            .component_schema_owner(&component),
        Some(owner.owner_id())
    );
    assert_eq!(
        read_property_at(&mut application, old_basis.clone(), &enemy, NAME, "value"),
        Err(EditorDocumentQueryRejection::StaleSchema)
    );
    let rejected = application.commit_authoring_edit(old_edit, Duration::ZERO);
    assert!(matches!(
        rejected.actions(),
        [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))]
            if !report.applied
    ));
    assert_eq!(application.document_query_basis(document).unwrap(), basis);
    assert_eq!(
        component_readiness(&mut application, document, &enemy),
        AuthoringRecordReadiness::Available
    );
    assert_eq!(
        read_property(&mut application, document, &enemy, STATS, "health").unwrap(),
        EditorPropertyValue::Present(ComponentValue::I64(100))
    );
    assert_eq!(fs::read(&scene_path).unwrap(), original_bytes);
    let expected_bundle = AuthoringSchemaBundle::from_owner_snapshots(
        native.owners().iter().cloned().chain([owner.clone()]),
    )
    .unwrap();
    assert_eq!(
        AuthoringSchemaBundle::from_json_bytes(&fs::read(&bundle_path).unwrap()).unwrap(),
        expected_bundle
    );
    let frame = application.current_frame().unwrap();
    let tab = frame.workspace().document(document).unwrap();
    assert!(!tab.dirty);
    assert_eq!(tab.history.undo_depth, 0);
    assert_eq!(tab.history.redo_depth, 0);
    let metadata_only = application.source_tool_view().unwrap();
    assert!(
        metadata_only.active.is_none()
            && metadata_only.attempt.is_none()
            && metadata_only.request.is_none()
            && metadata_only.review.is_none()
    );
    assert!(metadata_only.selection.as_ref().unwrap().desired.is_none());
    assert!(!fixture.root.join(SELECTION).exists());
    application
        .acknowledge_schema_owner_adoption(adoption)
        .unwrap();

    let attempt = enable_installed_tool(&mut application, &installed);
    let ready = settle_tool(&mut application);
    let enabled = ready.selection.as_ref().unwrap().desired.clone().unwrap();
    assert!(enabled.enabled);
    assert_eq!(enabled.installed.package.as_ref(), "trial.enemy-stats");
    assert_eq!(enabled.installed.directory.as_ref(), INSTALLATION);
    assert_eq!(enabled.installed.receipt, installed.receipt);
    assert_selection(&ready, &enabled, true);
    let operation = ready
        .selection
        .as_ref()
        .unwrap()
        .operation
        .as_ref()
        .unwrap();
    assert_eq!(operation.attempt, attempt);
    assert_eq!(
        operation.outcome,
        Some(EditorSourceToolSelectionOutcome::Enabled)
    );
    let selection_bytes = fs::read(fixture.root.join(SELECTION)).unwrap();
    let active = ready.active.as_ref().unwrap();
    assert_eq!(active.owner(), owner.owner_id());
    assert_eq!(active.component(), &ComponentTypeId::new(STATS));
    let facet = active.integer_property().unwrap();
    assert_eq!(
        facet.field(),
        &nara::reflect::ComponentFieldId::new("health")
    );
    assert_eq!(facet.parameter(), "bonus");
    assert_eq!(facet.label(), "Health bonus");
    assert_eq!(facet.initial(), 0);
    assert_eq!(ready.attempt.as_ref(), Some(&attempt));
    let activation = active.id().clone();
    let facet_target = EditorPropertyTarget {
        basis: basis.clone(),
        entity: enemy.clone(),
        component: ComponentTypeId::new(STATS),
        property: ComponentPropertyPath::new(ComponentFieldId::new("health")),
    };
    let EditorSourceToolActionOutcome::Requested(Ok(facet_request)) = source_action(
        &mut application,
        EditorSourceToolAction::IntegerProperty {
            activation: activation.clone(),
            target: facet_target.clone(),
            value: 25,
        },
    ) else {
        panic!("The admitted integer facet did not produce a request");
    };
    let ready = settle_tool(&mut application);
    assert!(ready.active.is_some() && ready.review.is_some());
    assert_eq!(ready.attempt.as_ref(), Some(&attempt));
    assert_eq!(ready.request.as_ref(), Some(&facet_request));
    let (facet_patch, facet_inverse) = {
        let review = application.source_tool_review().unwrap().unwrap();
        assert_eq!(review.request(), &facet_request);
        assert_eq!(review.title(), "Edit EnemyStats for ");
        (review.patch().clone(), review.inverse().clone())
    };
    assert_eq!(facet_patch.operations.len(), 2);
    assert_eq!(facet_inverse.operations.len(), 2);
    let mut facet_writes = BTreeMap::new();
    for operation in &facet_patch.operations {
        let ScenePatchOperation::SetField {
            entity,
            component,
            field,
            value,
            ..
        } = operation
        else {
            panic!("Unexpected facet operation: {operation:?}");
        };
        assert_eq!(entity, &enemy);
        assert_eq!(component, &ComponentTypeId::new(STATS));
        assert!(facet_writes.insert(field.as_str(), value.clone()).is_none());
    }
    assert_eq!(
        facet_writes,
        BTreeMap::from([
            ("armor", ComponentValue::I64(10)),
            ("health", ComponentValue::I64(125)),
        ])
    );
    assert_eq!(application.document_query_basis(document).unwrap(), basis);
    let frame = application.current_frame().unwrap();
    assert_eq!(
        frame
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth,
        0
    );
    assert_eq!(fs::read(&scene_path).unwrap(), original_bytes);
    assert!(matches!(
        read_property(&mut application, document, &enemy, STATS, "health").unwrap(),
        EditorPropertyValue::Present(ComponentValue::I64(100)),
    ));
    let EditorSourceToolActionOutcome::Accepted(Ok(committed)) = source_action(
        &mut application,
        EditorSourceToolAction::Accept {
            request: facet_request.clone(),
        },
    ) else {
        panic!("The facet review was not accepted");
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
        Some(&facet_inverse)
    );
    assert!(application.source_tool_review().unwrap().is_none());
    assert_eq!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth,
        1,
    );
    assert_eq!(
        read_property(&mut application, document, &enemy, STATS, "health").unwrap(),
        EditorPropertyValue::Present(ComponentValue::I64(125)),
    );
    replay(&mut application, document, false);
    assert_eq!(
        read_property(&mut application, document, &enemy, STATS, "health").unwrap(),
        EditorPropertyValue::Present(ComponentValue::I64(100)),
    );
    replay(&mut application, document, true);
    assert_eq!(
        read_property(&mut application, document, &enemy, STATS, "health").unwrap(),
        EditorPropertyValue::Present(ComponentValue::I64(125)),
    );
    assert_eq!(fs::read(&scene_path).unwrap(), original_bytes);
    let observers = (
        application.current_frame().unwrap(),
        application.current_frame().unwrap(),
    );
    assert!(Arc::ptr_eq(&observers.0, &observers.1));
    assert_eq!(observers.0.source_tool(), observers.1.source_tool());
    let spawn_basis = application.document_query_basis(document).unwrap();
    let EditorSourceToolActionOutcome::Requested(Ok(spawn_request)) = source_action(
        &mut application,
        EditorSourceToolAction::Request {
            activation: activation.clone(),
            basis: spawn_basis.clone(),
            intent: EditorSourceToolIntent::Spawn {
                name: "Reinforcement".into(),
            },
            parameters: BTreeMap::from([("bonus".into(), ComponentValue::I64(25))]),
        },
    ) else {
        panic!("The actual C# Spawn request was not admitted");
    };
    let ready = settle_tool(&mut application);
    assert!(ready.review.is_some());
    let (spawn_patch, spawn_inverse, created) = {
        let review = application.source_tool_review().unwrap().unwrap();
        assert_eq!(review.request(), &spawn_request);
        assert_eq!(review.title(), "Spawn Reinforcement with EnemyStats");
        assert!(std::ptr::eq(
            review.patch(),
            application.source_tool_review().unwrap().unwrap().patch()
        ));
        (
            review.patch().clone(),
            review.inverse().clone(),
            review.created_entity().unwrap().clone(),
        )
    };
    assert_ne!(created, enemy);
    assert_eq!(spawn_patch.operations.len(), 6);
    assert!(matches!(&spawn_patch.operations[0],
        ScenePatchOperation::AddEntity { entity, placement }
            if entity.id == created && placement == &ScenePlacement::last(ScenePlacementOwner::Root)));
    assert!(!spawn_inverse.operations.is_empty());
    let observers = (
        application.current_frame().unwrap(),
        application.current_frame().unwrap(),
    );
    assert_eq!(observers.0.source_tool(), observers.1.source_tool());
    assert_eq!(
        observers.0.source_tool().review.as_ref().unwrap().request,
        spawn_request
    );
    let EditorSourceToolActionOutcome::Accepted(Ok(committed)) = source_action(
        &mut application,
        EditorSourceToolAction::Accept {
            request: spawn_request.clone(),
        },
    ) else {
        panic!("The actual C# Spawn review was not accepted");
    };
    assert_eq!(committed.created_entity.as_ref(), Some(&created));
    assert_eq!(
        committed
            .report
            .patch_report
            .as_ref()
            .unwrap()
            .inverse
            .as_ref(),
        Some(&spawn_inverse)
    );
    assert_eq!(
        read_property(&mut application, document, &created, STATS, "health").unwrap(),
        EditorPropertyValue::Present(ComponentValue::I64(125))
    );
    replay(&mut application, document, false);
    assert!(matches!(
        read_property(&mut application, document, &created, STATS, "health"),
        Err(EditorDocumentQueryRejection::Source(
            AuthoringReadError::EntityNotFound
        ))
    ));
    replay(&mut application, document, true);
    assert_eq!(
        read_property(&mut application, document, &created, STATS, "health").unwrap(),
        EditorPropertyValue::Present(ComponentValue::I64(125))
    );
    let frame = application.current_frame().unwrap();
    let tab = frame.workspace().document(document).unwrap();
    assert_eq!(tab.history.undo_depth, 2);
    let unchanged = application.document_query_basis(document).unwrap();
    let deferred_request = EditorSourceToolAction::Request {
        activation: activation.clone(),
        basis: unchanged.clone(),
        intent: EditorSourceToolIntent::Edit {
            entity: enemy.clone(),
        },
        parameters: BTreeMap::from([("bonus".into(), ComponentValue::I64(99))]),
    };
    let deferred_property = EditorSourceToolAction::IntegerProperty {
        activation: activation.clone(),
        target: EditorPropertyTarget {
            basis: unchanged.clone(),
            ..facet_target
        },
        value: 99,
    };
    let deferred_disable = EditorSourceToolAction::DisableInstalled {
        basis: application.source_tool_selection_view().unwrap().basis,
    };
    let replacement = enable_installed_tool(&mut application, &installed);
    let ready = settle_tool(&mut application);
    assert_eq!(ready.attempt.as_ref(), Some(&replacement));
    assert_selection(&ready, &enabled, true);
    assert_eq!(
        fs::read(fixture.root.join(SELECTION)).unwrap(),
        selection_bytes
    );
    let next_activation = ready.active.as_ref().unwrap().id().clone();
    assert_ne!(next_activation, activation);
    for deferred in [deferred_request, deferred_property] {
        assert!(matches!(
            source_action(&mut application, deferred),
            EditorSourceToolActionOutcome::Requested(Err(EditorSourceToolError::StaleActivation))
        ));
    }
    assert!(matches!(
        source_action(&mut application, deferred_disable),
        EditorSourceToolActionOutcome::Selection(Err(EditorSourceToolSelectionError::StaleBasis))
    ));
    assert_eq!(
        application.document_query_basis(document).unwrap(),
        unchanged
    );
    let EditorSourceToolActionOutcome::Requested(Ok(revoked)) = source_action(
        &mut application,
        EditorSourceToolAction::IntegerProperty {
            activation: next_activation.clone(),
            target: EditorPropertyTarget {
                basis: unchanged.clone(),
                entity: enemy.clone(),
                component: ComponentTypeId::new(STATS),
                property: ComponentPropertyPath::new(ComponentFieldId::new("health")),
            },
            value: 50,
        },
    ) else {
        panic!("The second facet request was not admitted");
    };
    assert!(settle_tool(&mut application).review.is_some());
    let selection = application.source_tool_selection_view().unwrap();
    application.request_close_project();
    let cancel_close = application
        .current_frame()
        .unwrap()
        .workspace_intent()
        .decision_prompt()
        .expect("the tool-authored dirty source requires a Close decision")
        .resolve(EditorCloseDecision::Cancel);
    let EditorSourceToolActionOutcome::Selection(Ok(disable_attempt)) = source_action(
        &mut application,
        EditorSourceToolAction::DisableInstalled {
            basis: selection.basis,
        },
    ) else {
        panic!("The installed source-tool disable was not admitted");
    };
    assert!(
        application
            .current_frame()
            .unwrap()
            .source_tool()
            .active
            .is_none()
    );
    let frame = application.current_frame().unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::WorkspaceIntent(cancel_close)],
        ),
        Duration::ZERO,
    );
    assert!(
        application
            .current_frame()
            .unwrap()
            .workspace_intent()
            .decision_prompt()
            .is_none()
    );
    assert!(matches!(
        source_action(
            &mut application,
            EditorSourceToolAction::Accept { request: revoked }
        ),
        EditorSourceToolActionOutcome::Accepted(Err(EditorSourceToolError::Disabled))
    ));
    let disabled = settle_tool(&mut application);
    let mut desired_disabled = enabled.clone();
    desired_disabled.enabled = false;
    assert_selection(&disabled, &desired_disabled, false);
    let disable = disabled
        .selection
        .as_ref()
        .unwrap()
        .disable
        .as_ref()
        .unwrap();
    assert_eq!(disable.attempt, disable_attempt);
    assert_eq!(
        disable.outcome,
        Some(EditorSourceToolSelectionOutcome::Disabled)
    );
    assert_eq!(
        disabled.selection.as_ref().unwrap().operation,
        selection.operation
    );
    assert_eq!(
        disabled
            .selection
            .as_ref()
            .unwrap()
            .operation
            .as_ref()
            .unwrap()
            .outcome,
        Some(EditorSourceToolSelectionOutcome::Enabled),
    );
    let disabled_bytes = fs::read(fixture.root.join(SELECTION)).unwrap();
    assert_ne!(disabled_bytes, selection_bytes);
    assert_eq!(
        application.document_query_basis(document).unwrap(),
        unchanged
    );
    assert_eq!(fs::read(&scene_path).unwrap(), original_bytes);
    replay(&mut application, document, false);
    let frame = application.current_frame().unwrap();
    let tab = frame.workspace().document(document).unwrap();
    assert_eq!(tab.history.undo_depth, 1);
    assert_eq!(tab.history.redo_depth, 1);
    replay(&mut application, document, false);
    let frame = application.current_frame().unwrap();
    let tab = frame.workspace().document(document).unwrap();
    assert_eq!(tab.history.undo_depth, 0);
    assert_eq!(tab.history.redo_depth, 2);
    assert!(!tab.dirty);
    assert_eq!(
        read_property(&mut application, document, &enemy, STATS, "health").unwrap(),
        EditorPropertyValue::Present(ComponentValue::I64(100))
    );
    replay(&mut application, document, true);
    replay(&mut application, document, true);
    let frame = application.current_frame().unwrap();
    let basis = application.document_query_basis(document).unwrap();
    let saved_revision = basis.revision;
    let saved = application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision: saved_revision,
            })],
        ),
        Duration::ZERO,
    );
    assert!(matches!(
        saved.actions(),
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
                Some(EditorPersistenceResult::Saved { document: actual, revision, .. })
                    if actual == document && revision == saved_revision
            ));
            assert!(!frame.workspace().document(document).unwrap().dirty);
            assert_eq!(
                frame
                    .workspace()
                    .document(document)
                    .unwrap()
                    .history
                    .undo_depth,
                2
            );
            break;
        }
        assert!(Instant::now() < deadline, "Application Save did not settle");
        std::thread::sleep(Duration::from_millis(1));
    }
    let saved_bytes = fs::read(&scene_path).unwrap();
    assert_ne!(saved_bytes, original_bytes);
    let attempt = enable_installed_tool(&mut application, &installed);
    let reenabled = settle_tool(&mut application);
    assert_selection(&reenabled, &enabled, true);
    assert_eq!(
        reenabled
            .selection
            .as_ref()
            .unwrap()
            .operation
            .as_ref()
            .unwrap()
            .attempt,
        attempt
    );
    assert_eq!(
        reenabled.selection.as_ref().unwrap().disable,
        disabled.selection.as_ref().unwrap().disable,
    );
    assert_eq!(
        fs::read(fixture.root.join(SELECTION)).unwrap(),
        selection_bytes
    );
    close(&mut application);
    drop(application);

    let mut reopened = fixture.open_with_expected_state(EditorApplicationState::AuthoringReady);
    let opened = settle_tool(&mut reopened);
    assert_selection(&opened, &enabled, false);
    assert!(opened.attempt.is_none() && opened.request.is_none() && opened.review.is_none());
    assert_eq!(
        opened
            .selection
            .as_ref()
            .unwrap()
            .operation
            .as_ref()
            .unwrap()
            .outcome,
        Some(EditorSourceToolSelectionOutcome::Observed),
    );
    assert_eq!(
        fs::read(fixture.root.join(SELECTION)).unwrap(),
        selection_bytes
    );
    let document = reopened
        .current_frame()
        .unwrap()
        .workspace()
        .active_document
        .unwrap();
    let frame = reopened.current_frame().unwrap();
    let tab = frame.workspace().document(document).unwrap();
    assert_eq!(tab.history.undo_depth, 0);
    assert_eq!(tab.history.redo_depth, 0);
    assert!(!tab.dirty);
    for entity in [&enemy, &created] {
        assert_eq!(
            read_property(&mut reopened, document, entity, STATS, "health").unwrap(),
            EditorPropertyValue::Present(ComponentValue::I64(125)),
        );
    }
    let selection = reopened.source_tool_selection_view().unwrap();
    let EditorSourceToolActionOutcome::Selection(Ok(disable_attempt)) = source_action(
        &mut reopened,
        EditorSourceToolAction::DisableInstalled {
            basis: selection.basis,
        },
    ) else {
        panic!("The saved installed selection could not be disabled without activation");
    };
    let disabled = settle_tool(&mut reopened);
    assert_selection(&disabled, &desired_disabled, false);
    let disable = disabled
        .selection
        .as_ref()
        .unwrap()
        .disable
        .as_ref()
        .unwrap();
    assert_eq!(disable.attempt, disable_attempt);
    assert_eq!(
        disable.outcome,
        Some(EditorSourceToolSelectionOutcome::Disabled)
    );
    assert_eq!(
        disabled.selection.as_ref().unwrap().operation,
        selection.operation
    );
    assert_eq!(
        disabled
            .selection
            .as_ref()
            .unwrap()
            .operation
            .as_ref()
            .unwrap()
            .outcome,
        Some(EditorSourceToolSelectionOutcome::Observed),
    );
    assert!(disabled.attempt.is_none() && disabled.request.is_none() && disabled.review.is_none());
    assert_eq!(
        fs::read(fixture.root.join(SELECTION)).unwrap(),
        disabled_bytes
    );
    assert_eq!(fs::read(&scene_path).unwrap(), saved_bytes);
    close(&mut reopened);
    drop(reopened);

    let mut reopened = fixture.open_with_expected_state(EditorApplicationState::AuthoringReady);
    let opened = settle_tool(&mut reopened);
    assert_selection(&opened, &desired_disabled, false);
    assert!(opened.attempt.is_none() && opened.request.is_none() && opened.review.is_none());
    assert_eq!(
        fs::read(fixture.root.join(SELECTION)).unwrap(),
        disabled_bytes
    );
    assert_eq!(fs::read(&scene_path).unwrap(), saved_bytes);
    assert!(
        fixture
            .root
            .join(INSTALLATION)
            .join("EnemyStatsTool.dll")
            .is_file()
    );
    close(&mut reopened);
}
