//! Public Application template import and explicitly reviewed artifact reclamation.

#![cfg(all(windows, target_arch = "x86_64", target_env = "msvc"))]

use std::{
    collections::BTreeMap,
    fs,
    os::windows::fs::OpenOptionsExt,
    sync::Arc,
    time::{Duration, Instant},
};

use nara::{
    fs::{
        CapabilityRights, ContentDigest, DirectoryCapability, HostCapabilityOptions,
        RelativeComponent, TrustMode,
    },
    project_host::{
        EditorSourceToolInstallation, EditorSourceToolInstallationOutcome,
        EditorSourceToolInstallationPlan, EditorSourceToolPrefabTemplateImportRequest,
        PreparedEditorSourceToolRelease, open_explicit_product_project_root,
    },
    reflect::{
        ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
        ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId,
        ComponentValue, ComponentValueKind,
    },
    scene::{
        PrefabDocument, SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
        ScenePlacement, ScenePlacementOwner,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentMutation,
        EditorPersistenceAction, EditorPersistenceRequestResult, EditorPersistenceResult,
        EditorSourceToolArtifactError as Error, EditorSourceToolArtifactOutcome as Outcome,
        EditorSourceToolArtifactPhase as Phase, EditorSourceToolArtifactPublication,
        EditorSourceToolArtifactView, EditorSourceToolAttemptId,
        EditorSourceToolInstallationRequest, EditorSourceToolReclamationError as ReclamationError,
        EditorSourceToolReclamationMemberState as ReclamationMemberState,
        EditorSourceToolReclamationOutcome as ReclamationOutcome,
        EditorSourceToolReclamationPhase as ReclamationPhase,
        EditorSourceToolReclamationRecordPhase as ReclamationRecordPhase,
        EditorSourceToolReclamationView, EditorSourceToolSelectionPhase,
        EditorSourceToolTemplateImportOutcome, EditorSourceToolTemplateImportView,
        EditorSourceToolWithdrawalMode,
    },
};
use serde_json::json;

use crate::app::{
    EditorApplication, EditorApplicationActionDispatch, EditorApplicationState,
    EditorProjectAction,
    persistence_tests::{PersistenceFixture, close},
};

const DIRECTORY: &str = "installed-template-tool";
const SAMPLE: &str = "samples/readme.txt";
const RECORD: &str = ".nara/source-tools/artifacts.json";
const TIMEOUT: Duration = Duration::from_secs(10);

fn install_release(fixture: &PersistenceFixture) -> Arc<EditorSourceToolInstallation> {
    let source = fixture.root.join("template-tool-release");
    fs::create_dir_all(source.join("samples")).unwrap();
    let catalog = ComponentSchemaCatalog {
        components: vec![
            ComponentSchema::new(
                ComponentTypeId::new("test.artifact.Stats"),
                "Stats",
                ComponentSchemaVersion::ONE,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([ComponentFieldSchema::required(
                ComponentFieldId::new("health"),
                "Health",
                ComponentFieldPath::from_fields(["health"]),
                ComponentValueKind::I64,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
            .with_authoring_initial_value(ComponentValue::map([(
                "health",
                ComponentValue::I64(100),
            )])),
        ],
        ..Default::default()
    };
    let mut members = Vec::new();
    for (path, bytes, kind) in [
        (
            "catalog.json",
            catalog.to_json_string().unwrap().into_bytes(),
            "tool",
        ),
        (
            "Tool.dll",
            b"opaque non-executable release member".to_vec(),
            "tool",
        ),
        (SAMPLE, b"EnemyStats text template".to_vec(), "content"),
        ("samples/empty.prefab.json", PrefabDocument::default().to_json_string().unwrap().into_bytes(), "content"),
        ("samples/native-template.json", serde_json::to_vec(&json!({
            "format_version":1, "kind":"prefab_template",
            "source_universe":"cc4db9bc-388c-44be-91c5-c74fa2c08147",
            "native_document_identity":"project_location", "prefabs_root":"library/blueprints",
            "entry":"library/blueprints/empty.prefab.json", "documents":[
                {"member":"samples/empty.prefab.json","path":"library/blueprints/empty.prefab.json"}
            ],
        })).unwrap(), "content"),
    ] {
        fs::write(source.join(path), &bytes).unwrap();
        let digest = ContentDigest::of_bytes(&bytes);
        let hash: String = digest
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        members.push(json!({
            "path": path,
            "kind": kind,
            "length": digest.length(),
            "blake3": hash,
        }));
    }
    let manifest = serde_json::to_vec(&json!({
        "format_version": 1,
        "package": "test.artifact",
        "release": "1",
        "host_target": "x86_64-pc-windows-msvc",
        "authority": "trusted_process",
        "protocol_version": 1,
        "owner": "test.artifact-schema",
        "component": "test.artifact.Stats",
        "catalog": "catalog.json",
        "entry": "Tool.dll",
        "requires": [],
        "files": members,
    }))
    .unwrap();
    fs::write(source.join("nara-tool.json"), &manifest).unwrap();
    let captured = PreparedEditorSourceToolRelease::capture(
        &open_explicit_product_project_root(&source).unwrap(),
        ContentDigest::of_bytes(&manifest),
    )
    .unwrap();
    let plan = EditorSourceToolInstallationPlan::new(
        DirectoryCapability::from_host_handle(
            fs::OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(&fixture.root)
                .unwrap(),
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::Untrusted),
        )
        .unwrap(),
        RelativeComponent::new(DIRECTORY).unwrap(),
        captured,
    )
    .unwrap();
    match plan.publish() {
        EditorSourceToolInstallationOutcome::Published(installation) => {
            installation.verify().unwrap();
            Arc::new(installation)
        }
        other => panic!("Actual artifact publication failed: {other:?}"),
    }
}

fn project_root(fixture: &PersistenceFixture) -> DirectoryCapability {
    DirectoryCapability::from_host_handle(
        fs::OpenOptions::new()
            .read(true)
            .share_mode(7)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(&fixture.root)
            .unwrap(),
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::Untrusted),
    )
    .unwrap()
}

fn settle_artifacts(
    application: &mut EditorApplication,
    attempt: Option<&EditorSourceToolAttemptId>,
) -> EditorSourceToolArtifactView {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        application.advance(Duration::ZERO);
        let view = application.source_tool_artifact_view().unwrap();
        if !view.physical_work_pending
            && view.operation.as_ref().is_some_and(|operation| {
                operation.outcome.is_some()
                    && attempt.is_none_or(|attempt| &operation.attempt == attempt)
            })
        {
            return view;
        }
        assert!(
            Instant::now() < deadline,
            "Artifact inventory did not return: {view:?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn settle_selection(application: &mut EditorApplication) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        application.advance(Duration::ZERO);
        let view = application.source_tool_selection_view().unwrap();
        if view.phase == EditorSourceToolSelectionPhase::Idle
            && !application.source_tool_view().unwrap().has_pending_work()
            && view
                .operation
                .as_ref()
                .is_some_and(|operation| operation.outcome.is_some())
        {
            assert!(view.error.is_none(), "{view:?}");
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Installed graph did not return: {view:?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn settle_import(
    application: &mut EditorApplication,
    attempt: &EditorSourceToolAttemptId,
) -> EditorSourceToolTemplateImportView {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        application.advance(Duration::ZERO);
        let view = application.source_tool_template_import_view().unwrap();
        if view.attempt.as_ref() == Some(attempt)
            && view.outcome.is_some()
            && !view.physical_work_pending
        {
            return view;
        }
        assert!(
            Instant::now() < deadline,
            "Template import did not return: {view:?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn settle_reclamation(
    application: &mut EditorApplication,
    attempt: &EditorSourceToolAttemptId,
) -> EditorSourceToolReclamationView {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        application.advance(Duration::ZERO);
        let view = application.source_tool_reclamation_view().unwrap();
        if view.attempt.as_ref() == Some(attempt)
            && view.phase == ReclamationPhase::Terminal
            && !view.physical_work_pending
            && !view.record_publication_pending
        {
            return view;
        }
        assert!(
            Instant::now() < deadline,
            "Reviewed artifact reclamation did not return: {view:?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn application_prepares_and_publishes_native_template_with_the_same_attempt_and_owner() {
    let fixture = PersistenceFixture::new();
    let installation = install_release(&fixture);
    let root = project_root(&fixture);
    let mut application =
        fixture.open_with_expected_state(EditorApplicationState::AuthoringDegraded);
    settle_selection(&mut application);
    let observed = application.source_tool_selection_view().unwrap();
    let graph = application
        .prepare_source_tool_installations(
            &observed.basis,
            root.share(),
            &[EditorSourceToolInstallationRequest {
                directory: installation.project_path().into(),
                receipt: installation.receipt_digest(),
                requirements: Vec::new(),
            }],
        )
        .unwrap();
    application
        .register_source_tool_installations(graph)
        .unwrap();
    settle_selection(&mut application);
    let deadline = Instant::now() + TIMEOUT;
    while application
        .current_frame()
        .unwrap()
        .project_browser()
        .has_pending_work()
    {
        assert!(Instant::now() < deadline);
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    let documents = application
        .current_frame()
        .unwrap()
        .workspace()
        .documents
        .clone();
    let prepared = application
        .prepare_source_tool_prefab_template_import(
            &application.source_tool_selection_view().unwrap().basis,
            root.share(),
            EditorSourceToolPrefabTemplateImportRequest::new(
                *installation.receipt().installation_id(),
                "samples/native-template.json",
                "blueprints/copied",
            ),
        )
        .unwrap();
    assert!(!fixture.root.join(prepared.destination()).exists());
    assert_eq!(prepared.entry(), "blueprints/copied/empty.prefab.json");
    let count = prepared.files().len();
    let attempt = application
        .commit_source_tool_prefab_template_import(prepared)
        .unwrap();
    let view = settle_import(&mut application, &attempt);
    assert!(
        matches!(
            view.outcome,
            Some(EditorSourceToolTemplateImportOutcome::PrefabImported { .. })
        ),
        "{view:?}"
    );
    assert_eq!(
        application.current_frame().unwrap().workspace().documents,
        documents
    );
    assert_eq!(
        application
            .source_tool_prefab_template_publication()
            .unwrap()
            .retained_files(),
        count
    );
    let publication = application
        .take_source_tool_prefab_template_publication(&attempt)
        .unwrap();
    assert_eq!(publication.created_members().len(), count);
    drop(publication);
    assert!(
        fixture
            .root
            .join("blueprints/copied/empty.prefab.json")
            .is_file()
    );
    close(&mut application);
}

#[test]
fn application_reclaims_reviewed_artifacts_and_reopens_results_without_deleting_the_import() {
    let fixture = PersistenceFixture::new();
    let installation = install_release(&fixture);
    let root = project_root(&fixture);
    assert_eq!(
        installation.project_path(),
        ".nara/source-tools/artifacts/installed-template-tool"
    );
    assert!(installation.same_project(&root).unwrap());
    let id = *installation.receipt().installation_id();
    let sample_path = fixture.root.join(installation.project_path()).join(SAMPLE);
    let baseline = fs::read(&sample_path).unwrap();
    let installed_bytes: BTreeMap<_, _> = installation
        .members()
        .iter()
        .map(|member| {
            let path = fixture
                .root
                .join(installation.project_path())
                .join(member.path());
            let bytes = fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
    let mut application =
        fixture.open_with_expected_state(EditorApplicationState::AuthoringDegraded);
    let loaded = settle_artifacts(&mut application, None);
    assert_eq!(loaded.phase, Phase::Idle);
    let registration = application
        .prepare_source_tool_artifact_registration(&loaded.basis, Arc::clone(&installation))
        .unwrap();
    let attempt = application
        .commit_source_tool_artifacts(registration)
        .unwrap();
    let inventory = settle_artifacts(&mut application, Some(&attempt));
    assert_eq!(
        inventory.operation.as_ref().unwrap().outcome,
        Some(Outcome::Registered)
    );
    assert_eq!(inventory.artifacts.len(), 1);
    assert_eq!(
        inventory.artifacts[0].directory.as_ref(),
        installation.project_path()
    );
    assert_eq!(inventory.artifacts[0].id, id);
    assert_eq!(
        inventory.artifacts[0].publication,
        EditorSourceToolArtifactPublication::Ready {
            receipt: installation.receipt_digest(),
        }
    );
    let inventory_bytes = fs::read(fixture.root.join(RECORD)).unwrap();
    settle_selection(&mut application);
    let observed = application.source_tool_selection_view().unwrap();
    let prepared = application
        .prepare_source_tool_installations(
            &observed.basis,
            root.share(),
            &[EditorSourceToolInstallationRequest {
                directory: installation.project_path().into(),
                receipt: installation.receipt_digest(),
                requirements: Vec::new(),
            }],
        )
        .unwrap();
    application
        .register_source_tool_installations(prepared)
        .unwrap();
    settle_selection(&mut application);
    let selection = application.source_tool_selection_view().unwrap();
    let initial_reclamation = application.source_tool_reclamation_view().unwrap();
    assert_eq!(initial_reclamation.phase, ReclamationPhase::Idle);
    assert!(inventory.reclamations.is_empty());
    assert!(matches!(
        application.prepare_source_tool_reclamation(
            &inventory.basis,
            &selection.basis,
            root.share(),
            &id,
        ),
        Err(ReclamationError::StillInstalled)
    ));
    let initial_import = application.source_tool_template_import_view().unwrap();
    let documents = application
        .current_frame()
        .unwrap()
        .workspace()
        .documents
        .clone();
    let source = "worlds/imported-enemy-stats.txt";
    let prepared = application
        .prepare_source_tool_template_import(&selection.basis, root.share(), &id, SAMPLE, source)
        .unwrap();
    assert!(!fixture.root.join(source).exists());
    drop(prepared);
    assert!(!fixture.root.join(source).exists());
    assert_eq!(
        application.source_tool_template_import_view().unwrap(),
        initial_import
    );
    assert_eq!(
        application.current_frame().unwrap().workspace().documents,
        documents
    );
    assert_eq!(application.source_tool_artifact_view().unwrap(), inventory);

    let prepared = application
        .prepare_source_tool_template_import(&selection.basis, root.share(), &id, SAMPLE, source)
        .unwrap();
    let attempt = application
        .commit_source_tool_template_import(prepared)
        .unwrap();
    let imported = settle_import(&mut application, &attempt);
    assert!(matches!(
        imported.outcome,
        Some(EditorSourceToolTemplateImportOutcome::Imported { .. })
    ));
    assert_eq!(fs::read(fixture.root.join(source)).unwrap(), baseline);
    assert_eq!(fs::read(&sample_path).unwrap(), baseline);
    assert_eq!(
        fs::read(fixture.root.join(RECORD)).unwrap(),
        inventory_bytes
    );
    assert_eq!(
        application.current_frame().unwrap().workspace().documents,
        documents
    );
    assert!(
        application
            .prepare_source_tool_template_import(
                &selection.basis,
                root.share(),
                &id,
                SAMPLE,
                source,
            )
            .is_err()
    );
    let tool = application.source_tool_view().unwrap();
    assert!(tool.active.is_none() && !tool.probing && !tool.working && !tool.retiring);
    assert!(tool.attempt.is_none());
    let unknown = fixture
        .root
        .join(installation.project_path())
        .join("samples/unlisted.txt");
    fs::write(&unknown, b"unknown artifact sibling").unwrap();
    let prepared = application
        .prepare_source_tool_withdrawal(
            &selection.basis,
            &id,
            EditorSourceToolWithdrawalMode::RejectDependents,
        )
        .unwrap();
    application
        .withdraw_source_tool_installations(prepared)
        .unwrap();
    settle_selection(&mut application);
    let withdrawn = application.source_tool_selection_view().unwrap();
    assert!(withdrawn.installed.is_empty());
    assert_eq!(fs::read(fixture.root.join(source)).unwrap(), baseline);
    assert_eq!(fs::read(&sample_path).unwrap(), baseline);
    let graph_path = fixture.root.join(".nara/source-tools/selection.json");
    let graph_bytes = fs::read(&graph_path).unwrap();
    let prepared = application
        .prepare_source_tool_reclamation(&inventory.basis, &withdrawn.basis, root.share(), &id)
        .unwrap();
    assert!(prepared.creation_custody());
    let review = prepared.record().clone();
    assert_ne!(review.id, [0; 32]);
    assert_eq!(review.artifact, id);
    assert_eq!(review.phase, ReclamationRecordPhase::Intent);
    assert_eq!(review.members.len(), installation.members().len());
    assert_eq!(
        review
            .members
            .iter()
            .map(|member| (member.path.to_string(), member.state))
            .collect::<BTreeMap<_, _>>(),
        installation
            .members()
            .iter()
            .map(|member| (member.path().to_owned(), ReclamationMemberState::Pending))
            .collect::<BTreeMap<_, _>>()
    );
    drop(prepared);
    assert_eq!(
        application.source_tool_reclamation_view().unwrap(),
        initial_reclamation
    );
    assert_eq!(application.source_tool_artifact_view().unwrap(), inventory);
    assert_eq!(application.source_tool_selection_view().unwrap(), withdrawn);
    assert_eq!(
        application.current_frame().unwrap().workspace().documents,
        documents
    );
    assert_eq!(
        fs::read(fixture.root.join(RECORD)).unwrap(),
        inventory_bytes
    );
    assert_eq!(fs::read(&graph_path).unwrap(), graph_bytes);
    assert_eq!(fs::read(&unknown).unwrap(), b"unknown artifact sibling");
    assert_eq!(fs::read(fixture.root.join(source)).unwrap(), baseline);
    for (path, bytes) in &installed_bytes {
        assert_eq!(&fs::read(path).unwrap(), bytes, "{}", path.display());
    }

    let stale = application
        .prepare_source_tool_reclamation(&inventory.basis, &withdrawn.basis, root.share(), &id)
        .unwrap();
    let prepared = application
        .prepare_source_tool_artifact_observation(&inventory.basis, &id)
        .unwrap();
    let original_artifact_basis = inventory.basis.clone();
    let attempt = application.commit_source_tool_artifacts(prepared).unwrap();
    let inventory = settle_artifacts(&mut application, Some(&attempt));
    assert_ne!(inventory.basis, original_artifact_basis);
    assert!(matches!(
        application.confirm_source_tool_reclamation(stale),
        Err(ReclamationError::StaleBasis)
    ));
    assert_eq!(
        application.source_tool_reclamation_view().unwrap(),
        initial_reclamation
    );
    assert_eq!(application.source_tool_artifact_view().unwrap(), inventory);
    assert_eq!(
        fs::read(fixture.root.join(RECORD)).unwrap(),
        inventory_bytes
    );
    for (path, bytes) in &installed_bytes {
        assert_eq!(&fs::read(path).unwrap(), bytes, "{}", path.display());
    }

    let prepared = application
        .prepare_source_tool_reclamation(&inventory.basis, &withdrawn.basis, root.share(), &id)
        .unwrap();
    let mut expected_results = prepared.record().clone();
    assert_eq!(expected_results.members, review.members);
    let attempt = application
        .confirm_source_tool_reclamation(prepared)
        .unwrap();
    let reclaimed = settle_reclamation(&mut application, &attempt);
    assert_eq!(reclaimed.outcome, Some(ReclamationOutcome::Complete));
    assert!(reclaimed.error.is_none(), "{reclaimed:?}");
    expected_results.phase = ReclamationRecordPhase::Results;
    for member in &mut expected_results.members {
        member.state = ReclamationMemberState::Removed;
    }
    assert_eq!(reclaimed.record.as_ref(), Some(&expected_results));
    let results = expected_results;
    let inventory = settle_artifacts(&mut application, None);
    assert_eq!(inventory.reclamations, [results.clone()]);
    let reclaimed_bytes = fs::read(fixture.root.join(RECORD)).unwrap();
    assert_ne!(reclaimed_bytes, inventory_bytes);
    for path in installed_bytes.keys() {
        assert!(
            !path.exists(),
            "Selected artifact survived: {}",
            path.display()
        );
    }
    assert_eq!(fs::read(&unknown).unwrap(), b"unknown artifact sibling");
    assert_eq!(fs::read(&graph_path).unwrap(), graph_bytes);
    assert_eq!(fs::read(fixture.root.join(source)).unwrap(), baseline);
    assert_eq!(application.source_tool_selection_view().unwrap(), withdrawn);
    assert_eq!(
        application.current_frame().unwrap().workspace().documents,
        documents
    );
    let tool = application.source_tool_view().unwrap();
    assert!(tool.active.is_none() && !tool.probing && !tool.working && !tool.retiring);
    assert!(tool.attempt.is_none());
    close(&mut application);
    drop(application);

    let mut reopened = fixture.open_with_expected_state(EditorApplicationState::AuthoringDegraded);
    let inventory = settle_artifacts(&mut reopened, None);
    assert_eq!(inventory.phase, Phase::Idle);
    assert_eq!(inventory.artifacts.len(), 1);
    assert_eq!(inventory.reclamations, [results.clone()]);
    settle_selection(&mut reopened);
    let selection = reopened.source_tool_selection_view().unwrap();
    assert!(selection.installed.is_empty() && selection.desired.is_none());
    assert!(selection.active_origin.is_none());
    let tool = reopened.source_tool_view().unwrap();
    assert!(tool.active.is_none() && !tool.probing && !tool.working && !tool.retiring);
    assert!(tool.attempt.is_none());
    let fresh_reclamation = reopened.source_tool_reclamation_view().unwrap();
    assert_eq!(fresh_reclamation.phase, ReclamationPhase::Idle);
    assert!(fresh_reclamation.attempt.is_none() && fresh_reclamation.outcome.is_none());
    assert!(!fresh_reclamation.physical_work_pending);
    assert!(!fresh_reclamation.record_publication_pending);
    assert_eq!(fs::read(fixture.root.join(source)).unwrap(), baseline);
    assert_eq!(
        fs::read(fixture.root.join(RECORD)).unwrap(),
        reclaimed_bytes
    );
    assert_eq!(fs::read(&graph_path).unwrap(), graph_bytes);
    for path in installed_bytes.keys() {
        assert!(
            !path.exists(),
            "Deleted artifact reappeared: {}",
            path.display()
        );
    }
    assert_eq!(fs::read(&unknown).unwrap(), b"unknown artifact sibling");

    // A new explicit review observes absence; persisted Removed facts grant no live authority.
    let deadline = Instant::now() + TIMEOUT;
    let prepared = loop {
        match reopened.prepare_source_tool_reclamation(
            &inventory.basis,
            &selection.basis,
            root.share(),
            &id,
        ) {
            Ok(prepared) => break prepared,
            Err(ReclamationError::Busy) => {}
            Err(error) => panic!("fresh original-basis review was rejected: {error:?}"),
        }
        assert!(
            Instant::now() < deadline,
            "fresh Open sources did not physically return"
        );
        reopened.advance(Duration::ZERO);
        std::thread::yield_now();
    };
    assert!(!prepared.creation_custody());
    let missing = prepared.record();
    assert_ne!(missing.id, results.id);
    assert_eq!(missing.artifact, id);
    assert_eq!(missing.phase, ReclamationRecordPhase::Intent);
    let mut expected_missing = results;
    expected_missing.id = missing.id;
    expected_missing.phase = ReclamationRecordPhase::Intent;
    for member in &mut expected_missing.members {
        member.state = ReclamationMemberState::Missing;
    }
    assert_eq!(missing, &expected_missing);
    drop(prepared);
    assert_eq!(
        reopened.source_tool_reclamation_view().unwrap(),
        fresh_reclamation
    );
    assert_eq!(reopened.source_tool_artifact_view().unwrap(), inventory);
    assert_eq!(
        fs::read(fixture.root.join(RECORD)).unwrap(),
        reclaimed_bytes
    );
    assert_eq!(fs::read(&unknown).unwrap(), b"unknown artifact sibling");
    close(&mut reopened);
    assert_eq!(fs::read(fixture.root.join(source)).unwrap(), baseline);
    assert_eq!(
        fs::read(fixture.root.join(RECORD)).unwrap(),
        reclaimed_bytes
    );
    assert_eq!(fs::read(&unknown).unwrap(), b"unknown artifact sibling");
}

#[test]
fn unavailable_artifact_inventory_does_not_gate_unrelated_source_save_and_close() {
    let fixture = PersistenceFixture::new();
    fs::create_dir_all(fixture.root.join(".nara/source-tools")).unwrap();
    fs::write(fixture.root.join(RECORD), b"not-json").unwrap();
    let source = fixture.root.join("worlds/entry.scene.json");
    let original = fs::read(&source).unwrap();
    let mut application =
        fixture.open_with_expected_state(EditorApplicationState::AuthoringDegraded);
    let unavailable = settle_artifacts(&mut application, None);
    assert_eq!(unavailable.phase, Phase::Unavailable);
    assert_eq!(unavailable.error, Some(Error::InvalidRecord));
    let frame = application.current_frame().unwrap();
    let document = frame.workspace().active_document.unwrap();
    let basis = application.document_query_basis(document).unwrap();
    let edited = application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyScenePatch {
                    document,
                    base_revision: basis.revision,
                    patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                        entity: SceneEntityRecord::new(
                            SceneEntityId::new("ordinary-saved-source").unwrap(),
                        ),
                        placement: ScenePlacement::last(ScenePlacementOwner::Root),
                    }]),
                },
            )],
        ),
        Duration::ZERO,
    );
    assert!(
        matches!(edited.actions(), [EditorApplicationActionDispatch::Dispatched(
        EditorActionOutcome::Workspace(report)
    )] if report.applied && report.document == Some(document))
    );
    let frame = application.current_frame().unwrap();
    let tab = frame.workspace().document(document).unwrap();
    assert!(tab.dirty);
    let revision = tab.revision;
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
    let [
        EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Persistence(
            EditorPersistenceRequestResult::Accepted { attempt },
        )),
    ] = submitted.actions()
    else {
        panic!("Unavailable artifact inventory blocked ordinary Save: {submitted:?}");
    };
    let attempt = *attempt;
    let deadline = Instant::now() + TIMEOUT;
    loop {
        application.advance(Duration::ZERO);
        let frame = application.current_frame().unwrap();
        if frame.persistence().attempt() == Some(attempt)
            && let Some(result) = frame.persistence().result()
        {
            assert!(
                matches!(result, EditorPersistenceResult::Saved {
                document: saved, revision: saved_revision, ..
            } if saved == document && saved_revision == revision),
                "{result:?}"
            );
            assert!(!frame.workspace().document(document).unwrap().dirty);
            break;
        }
        assert!(Instant::now() < deadline, "Ordinary Save did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
    let saved = fs::read(&source).unwrap();
    assert_ne!(saved, original);
    assert_eq!(
        application.source_tool_artifact_view().unwrap(),
        unavailable
    );
    assert_eq!(fs::read(fixture.root.join(RECORD)).unwrap(), b"not-json");
    application.submit_project_action(EditorProjectAction::CloseProject);
    let deadline = Instant::now() + TIMEOUT;
    while application.state() != EditorApplicationState::ProjectManager {
        application.advance(Duration::ZERO);
        assert!(
            Instant::now() < deadline,
            "Unavailable artifact inventory blocked ordinary Close"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(application.current_frame().is_none());
    assert_eq!(fs::read(&source).unwrap(), saved);
    drop(application);

    let mut reopened = fixture.open_with_expected_state(EditorApplicationState::AuthoringDegraded);
    let unavailable = settle_artifacts(&mut reopened, None);
    assert_eq!(unavailable.phase, Phase::Unavailable);
    assert_eq!(unavailable.error, Some(Error::InvalidRecord));
    let tool = reopened.source_tool_view().unwrap();
    assert!(tool.active.is_none() && !tool.probing && !tool.working && !tool.retiring);
    assert!(tool.attempt.is_none());
    assert_eq!(fs::read(&source).unwrap(), saved);
    close(&mut reopened);
}
