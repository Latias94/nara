//! Public Application artifact inventory and project-owned text-template import.

#![cfg(all(windows, target_arch = "x86_64", target_env = "msvc"))]

use std::{
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
        EditorSourceToolInstallationPlan, PreparedEditorSourceToolRelease,
        open_explicit_product_project_root,
    },
    reflect::{
        ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
        ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId,
        ComponentValue, ComponentValueKind,
    },
    scene::{
        SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation, ScenePlacement,
        ScenePlacementOwner,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentMutation,
        EditorPersistenceAction, EditorPersistenceRequestResult, EditorPersistenceResult,
        EditorSourceToolArtifactError as Error, EditorSourceToolArtifactOutcome as Outcome,
        EditorSourceToolArtifactPhase as Phase, EditorSourceToolArtifactView,
        EditorSourceToolAttemptId, EditorSourceToolInstallationRequest,
        EditorSourceToolSelectionPhase, EditorSourceToolTemplateImportOutcome,
        EditorSourceToolTemplateImportView, EditorSourceToolWithdrawalMode,
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

#[test]
fn application_imports_a_project_owned_text_copy_and_withdrawal_preserves_both_owners() {
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
    assert_eq!(inventory.installations.len(), 1);
    assert_eq!(
        inventory.installations[0].installed.directory.as_ref(),
        installation.project_path()
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
    assert!(
        application
            .source_tool_selection_view()
            .unwrap()
            .installed
            .is_empty()
    );
    assert_eq!(fs::read(fixture.root.join(source)).unwrap(), baseline);
    assert_eq!(fs::read(&sample_path).unwrap(), baseline);
    close(&mut application);
    drop(application);

    let mut reopened = fixture.open_with_expected_state(EditorApplicationState::AuthoringDegraded);
    let inventory = settle_artifacts(&mut reopened, None);
    assert_eq!(inventory.phase, Phase::Idle);
    assert_eq!(inventory.installations.len(), 1);
    settle_selection(&mut reopened);
    let selection = reopened.source_tool_selection_view().unwrap();
    assert!(selection.installed.is_empty() && selection.desired.is_none());
    assert!(selection.active_origin.is_none());
    let tool = reopened.source_tool_view().unwrap();
    assert!(tool.active.is_none() && !tool.probing && !tool.working && !tool.retiring);
    assert!(tool.attempt.is_none());
    assert_eq!(fs::read(fixture.root.join(source)).unwrap(), baseline);
    assert_eq!(fs::read(&sample_path).unwrap(), baseline);
    close(&mut reopened);
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
