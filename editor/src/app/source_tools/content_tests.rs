//! Public Application repair remains available while installed-content protection blocks Close.

#![cfg(all(windows, target_arch = "x86_64", target_env = "msvc"))]

use std::{
    fs,
    os::windows::fs::OpenOptionsExt,
    path::Path,
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
    tooling::{
        EditorSourceToolAttemptId, EditorSourceToolContentError as Error,
        EditorSourceToolContentMember, EditorSourceToolContentOutcome as Outcome,
        EditorSourceToolContentPhase as Phase, EditorSourceToolContentView, EditorWorkspaceIntent,
        EditorWorkspaceIntentResult,
    },
};
use serde_json::json;

use crate::app::{
    EditorApplication, EditorApplicationState, EditorProjectAction,
    persistence_tests::{PersistenceFixture, close},
};

const DIRECTORY: &str = "installed-content-tool";
const SAMPLE: &str = "samples/readme.txt";
const RECORD: &str = ".nara/source-tools/content.json";
const TIMEOUT: Duration = Duration::from_secs(10);

fn install_release(fixture: &PersistenceFixture) -> Arc<EditorSourceToolInstallation> {
    let source = fixture.root.join("content-tool-release");
    fs::create_dir_all(source.join("samples")).unwrap();
    let catalog = ComponentSchemaCatalog {
        components: vec![
            ComponentSchema::new(
                ComponentTypeId::new("test.content.Stats"),
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
        (SAMPLE, b"project-owned sample".to_vec(), "content"),
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
        "package": "test.content",
        "release": "1",
        "host_target": "x86_64-pc-windows-msvc",
        "authority": "trusted_process",
        "protocol_version": 1,
        "owner": "test.content-schema",
        "component": "test.content.Stats",
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
        other => panic!("Actual installed-content publication failed: {other:?}"),
    }
}

fn settle_content(
    application: &mut EditorApplication,
    attempt: Option<&EditorSourceToolAttemptId>,
) -> EditorSourceToolContentView {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        application.advance(Duration::ZERO);
        let view = application.source_tool_content_view().unwrap();
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
            "Content publication did not physically return: {view:?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn sample(view: &EditorSourceToolContentView) -> &EditorSourceToolContentMember {
    assert_eq!(view.installations.len(), 1);
    view.installations[0]
        .members
        .iter()
        .find(|member| member.path.as_ref() == SAMPLE)
        .unwrap()
}

struct RestorePermissions<'a> {
    path: &'a Path,
    permissions: fs::Permissions,
}

impl Drop for RestorePermissions<'_> {
    fn drop(&mut self) {
        fs::set_permissions(self.path, self.permissions.clone()).unwrap();
    }
}

#[test]
fn failed_adoption_blocks_application_close_until_public_content_repair_is_persisted() {
    let fixture = PersistenceFixture::new();
    let mut application =
        fixture.open_with_expected_state(EditorApplicationState::AuthoringDegraded);
    let loaded = settle_content(&mut application, None);
    assert_eq!(loaded.phase, Phase::Idle);
    let installation = install_release(&fixture);
    let id = *installation.receipt().installation_id();
    let management = application
        .prepare_source_tool_content_management(&loaded.basis, DIRECTORY, Arc::clone(&installation))
        .unwrap();
    let attempt = application.commit_source_tool_content(management).unwrap();
    let managed = settle_content(&mut application, Some(&attempt));
    assert_eq!(
        managed.operation.as_ref().unwrap().outcome,
        Some(Outcome::Managed)
    );
    assert!(managed.installations[0].creation_custody);
    assert!(!sample(&managed).adopted);
    let record = fixture.root.join(RECORD);
    let baseline = fs::read(&record).unwrap();
    let documents = application
        .current_frame()
        .unwrap()
        .workspace()
        .documents
        .clone();
    let adoption = application
        .prepare_source_tool_content_adoption(&managed.basis, &id, &[SAMPLE])
        .unwrap();
    let confirmed = adoption.installations();
    let failed = {
        let restore = RestorePermissions {
            path: &record,
            permissions: fs::metadata(&record).unwrap().permissions(),
        };
        let mut permissions = restore.permissions.clone();
        permissions.set_readonly(true);
        fs::set_permissions(&record, permissions).unwrap();
        let attempt = application.commit_source_tool_content(adoption).unwrap();
        let failed = settle_content(&mut application, Some(&attempt));
        drop(restore);
        failed
    };
    assert_eq!(failed.phase, Phase::RequiresReconcile);
    assert_eq!(failed.error, Some(Error::Write));
    assert_eq!(failed.installations, confirmed);
    assert!(sample(&failed).adopted);
    assert!(failed.protection_publication_pending);
    assert_eq!(fs::read(&record).unwrap(), baseline);

    application.submit_project_action(EditorProjectAction::CloseProject);
    let deadline = Instant::now() + TIMEOUT;
    loop {
        application.advance(Duration::ZERO);
        let frame = application
            .current_frame()
            .expect("Protection failure must retain the published source session");
        assert_eq!(frame.workspace().documents, documents);
        let blocked = application.source_tool_content_view().unwrap();
        assert_eq!(blocked.installations, confirmed);
        assert!(blocked.protection_publication_pending);
        if matches!(
            frame.workspace_intent().result(),
            Some(EditorWorkspaceIntentResult::Applied {
                intent: EditorWorkspaceIntent::Exit,
            })
        ) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Application Close did not reach retirement"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        application.state(),
        EditorApplicationState::AuthoringDegraded
    );
    let blocked = application.source_tool_content_view().unwrap();
    assert!(matches!(
        application.prepare_source_tool_content_protection(&blocked.basis),
        Err(Error::RequiresReconcile)
    ));
    let attempt = application
        .reconcile_source_tool_content(&blocked.basis)
        .unwrap();
    let reconciled = settle_content(&mut application, Some(&attempt));
    assert_eq!(
        reconciled.operation.as_ref().unwrap().outcome,
        Some(Outcome::NeedsProtectionPublication)
    );
    assert!(sample(&reconciled).adopted);
    assert!(reconciled.protection_publication_pending);
    assert_eq!(fs::read(&record).unwrap(), baseline);
    let protection = application
        .prepare_source_tool_content_protection(&reconciled.basis)
        .unwrap();
    assert_eq!(protection.installations(), confirmed);
    application.commit_source_tool_content(protection).unwrap();
    let deadline = Instant::now() + TIMEOUT;
    while application.state() != EditorApplicationState::ProjectManager {
        assert!(
            Instant::now() < deadline,
            "Repaired Application Close did not finish"
        );
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(application.current_frame().is_none());
    drop(application);

    let mut reopened = fixture.open_with_expected_state(EditorApplicationState::AuthoringDegraded);
    let restored = settle_content(&mut reopened, None);
    assert_eq!(restored.phase, Phase::Idle);
    assert!(restored.error.is_none());
    assert!(sample(&restored).adopted);
    assert!(!sample(&restored).modified);
    assert!(!restored.protection_publication_pending);
    assert!(!restored.installations[0].creation_custody);
    let mut expected = confirmed;
    expected[0].creation_custody = false;
    assert_eq!(restored.installations, expected);
    assert_eq!(
        fs::read(fixture.root.join(DIRECTORY).join(SAMPLE)).unwrap(),
        b"project-owned sample"
    );
    close(&mut reopened);
}
