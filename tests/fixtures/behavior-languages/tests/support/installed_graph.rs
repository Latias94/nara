//! Exact installed relationships consumed through the actual managed Host and Project Session.

#![cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]

#[path = "installed_graph/creation_metadata.rs"]
mod creation_metadata;
#[path = "installed_graph/template_import.rs"]
mod template_import;

use std::{
    collections::BTreeMap,
    fs,
    os::windows::fs::OpenOptionsExt,
    sync::Arc,
    time::{Duration, Instant},
};

use nara_engine::{
    fs::{
        CapabilityRights, ContentDigest, DirectoryCapability, HostCapabilityOptions,
        RelativeComponent, TrustMode,
    },
    project_host::{
        EditorProjectSession, EditorSourceToolInstallation, EditorSourceToolInstallationInspection,
        EditorSourceToolInstallationOutcome, EditorSourceToolInstallationPlan,
        EditorSourceToolReleaseMemberKind, PreparedEditorSourceToolRelease,
        open_explicit_product_project_root,
    },
};
use nara_reflect::{ComponentSchemaVersion, ComponentTypeId, ComponentValue};
use nara_scene::{SceneComponentRecord, SceneDocument, SceneEntityRecord};
use nara_tooling::{
    EditorDocumentId, EditorSourceToolArtifact, EditorSourceToolArtifactError,
    EditorSourceToolArtifactMember, EditorSourceToolArtifactObservation,
    EditorSourceToolArtifactOutcome, EditorSourceToolArtifactPhase,
    EditorSourceToolArtifactPublication, EditorSourceToolArtifactRole,
    EditorSourceToolArtifactView, EditorSourceToolAttemptId, EditorSourceToolError,
    EditorSourceToolInstallationRequest, EditorSourceToolInstalledEntry,
    EditorSourceToolInstalledSelection, EditorSourceToolIntent, EditorSourceToolRequestId,
    EditorSourceToolRequirementBinding, EditorSourceToolSelectionError,
    EditorSourceToolSelectionOutcome, EditorSourceToolSelectionPhase,
    EditorSourceToolSelectionView, EditorSourceToolWithdrawalMode,
};
use serde_json::json;

use crate::{
    authoring_project::{
        Project, assert_history, assert_value, close, replay, save, schema_bundle,
    },
    enemy_stats::approved_managed_host,
    local_release::installed_variant,
    scene_runtime::id,
};

const PACKAGE_A: &str = "trial.enemy-stats";
const PACKAGE_B: &str = "trial.enemy-stats-dependent";
const PACKAGE_C: &str = "trial.enemy-stats-independent";
const STATS: &str = "trial.enemy.EnemyStats";
const NAME: &str = "nara.scene.Name";
const RECORD_PATH: &str = ".nara/source-tools/selection.json";
const ARTIFACT_PATH: &str = ".nara/source-tools/artifacts.json";
const SAMPLE: &str = "samples/readme.txt";
const TIMEOUT: Duration = Duration::from_secs(30);

#[test]
fn csharp_artifact_inventory_observes_changes_without_mutating_managed_tool_history() {
    let (_template_directory, template) = installed_variant("V1");
    let project = Project::new(
        &schema_bundle(template.release().schema_owner()),
        &original(),
        "json",
    );
    let root = project_root(&project);
    let (primary, primary_owner) = install_owned(
        &root,
        template.release(),
        "installed-primary",
        PACKAGE_A,
        None,
    );
    let (secondary, secondary_owner) = install_owned(
        &root,
        template.release(),
        "installed-secondary",
        PACKAGE_A,
        None,
    );
    assert_eq!(primary.installed.manifest, secondary.installed.manifest);
    assert_ne!(
        primary.installed.installation,
        secondary.installed.installation
    );
    let expected_primary = artifact_installation(&primary, &primary_owner);
    let mut expected_secondary = artifact_installation(&secondary, &secondary_owner);
    let mut installed_bytes = BTreeMap::new();
    for (entry, owner) in [(&primary, &primary_owner), (&secondary, &secondary_owner)] {
        for member in owner.members() {
            let path = format!("{}/{}", entry.installed.directory, member.path());
            let bytes = fs::read(project.root().join(&path)).unwrap();
            assert_eq!(ContentDigest::of_bytes(&bytes), member.digest());
            installed_bytes.insert(path, bytes);
        }
    }
    let source_bytes = fs::read(project.scene_path()).unwrap();
    let mut editor = project.open();
    let document = editor.current_frame().workspace().active_document.unwrap();
    let source_basis = editor.document_query_basis(document).unwrap();
    let selection = settle_selection(&mut editor);
    let content = settle_artifacts(&mut editor);
    assert_eq!(content.phase, EditorSourceToolArtifactPhase::Idle);
    assert!(content.error.is_none());
    assert!(matches!(
        content.operation.as_ref().unwrap().outcome,
        Some(EditorSourceToolArtifactOutcome::Observed)
    ));
    assert!(content.artifacts.is_empty());
    assert!(selection.installed.is_empty());
    assert!(!project.root().join(ARTIFACT_PATH).exists());
    assert_no_execution(&editor);

    let cancelled = editor
        .prepare_source_tool_artifact_registration(&content.basis, Arc::clone(&primary_owner))
        .unwrap();
    assert_artifact_installations(
        &cancelled.artifacts(),
        std::slice::from_ref(&expected_primary),
    );
    drop(cancelled);
    assert_eq!(editor.source_tool_artifact_view(), content);
    assert_eq!(editor.source_tool_selection_view(), selection);
    assert_eq!(editor.document_query_basis(document).unwrap(), source_basis);
    assert_history(&editor, document, 0, 0);
    assert_no_execution(&editor);
    assert!(!project.root().join(ARTIFACT_PATH).exists());
    assert!(!project.root().join(RECORD_PATH).exists());
    assert_eq!(fs::read(project.scene_path()).unwrap(), source_bytes);
    assert_installed_bytes(&project, &installed_bytes);

    let prepared = editor
        .prepare_source_tool_artifact_registration(&content.basis, Arc::clone(&primary_owner))
        .unwrap();
    let attempt = editor.commit_source_tool_artifacts(prepared).unwrap();
    let managed = settle_artifacts(&mut editor);
    assert_artifact_outcome(
        &managed,
        &attempt,
        EditorSourceToolArtifactOutcome::Registered,
    );
    assert_artifact_installations(&managed.artifacts, std::slice::from_ref(&expected_primary));
    let prepared = editor
        .prepare_source_tool_artifact_registration(&managed.basis, Arc::clone(&secondary_owner))
        .unwrap();
    let attempt = editor.commit_source_tool_artifacts(prepared).unwrap();
    let managed = settle_artifacts(&mut editor);
    assert_artifact_outcome(
        &managed,
        &attempt,
        EditorSourceToolArtifactOutcome::Registered,
    );
    assert_artifact_installations(
        &managed.artifacts,
        &[expected_primary.clone(), expected_secondary.clone()],
    );
    let managed_bytes = fs::read(project.root().join(ARTIFACT_PATH)).unwrap();
    assert_eq!(editor.source_tool_selection_view(), selection);
    assert_eq!(editor.document_query_basis(document).unwrap(), source_basis);
    assert_history(&editor, document, 0, 0);
    assert_no_execution(&editor);
    assert!(editor.source_tool_view().attempt.is_none());
    assert!(!project.root().join(RECORD_PATH).exists());
    assert_eq!(fs::read(project.scene_path()).unwrap(), source_bytes);
    assert_installed_bytes(&project, &installed_bytes);

    let prepared = editor
        .prepare_source_tool_installations(
            &selection.basis,
            root.share(),
            &[request(&primary), request(&secondary)],
        )
        .unwrap();
    let attempt = editor.register_source_tool_installations(prepared).unwrap();
    let registered = settle_selection(&mut editor);
    assert!(matches!(
        registered.operation.as_ref(),
        Some(operation) if operation.attempt == attempt
            && operation.outcome == Some(EditorSourceToolSelectionOutcome::Registered)
    ));
    assert_installations(&registered.installed, &[primary.clone(), secondary.clone()]);
    assert_eq!(editor.source_tool_artifact_view(), managed);
    assert_no_execution(&editor);
    let host = approved_managed_host();
    let prepared = editor
        .prepare_installed_source_tool_selection(
            root.share(),
            primary.installed.directory.as_ref(),
            primary.installed.receipt,
            &host,
        )
        .unwrap();
    let attempt = editor
        .enable_installed_source_tool(&registered.basis, prepared)
        .unwrap();
    let enabled = settle_selection(&mut editor);
    assert!(matches!(
        enabled.operation.as_ref(),
        Some(operation) if operation.attempt == attempt
            && operation.outcome == Some(EditorSourceToolSelectionOutcome::Enabled)
    ));
    assert_eq!(enabled.active_origin.as_ref(), Some(&primary.installed));
    let active = editor.source_tool_view();
    assert_eq!(active.attempt.as_ref(), Some(&attempt));
    assert!(active.attempt_error.is_none(), "{active:?}");
    assert_eq!(
        active.active.as_ref().unwrap().owner(),
        template.release().schema_owner().owner_id()
    );
    assert_eq!(
        active.active.as_ref().unwrap().component(),
        &ComponentTypeId::new(STATS)
    );

    let attached = review(
        &mut editor,
        document,
        EditorSourceToolIntent::Attach {
            entity: id("enemy"),
        },
        0,
    );
    let reviewed_tool = editor.source_tool_view();
    let reviewed_patch = editor.source_tool_review().unwrap().patch().clone();
    let reviewed_inverse = editor.source_tool_review().unwrap().inverse().clone();
    let selection_bytes = fs::read(project.root().join(RECORD_PATH)).unwrap();
    // Retain the original candidate; a later inventory observation must not rebase it.
    let stale = editor
        .prepare_source_tool_artifact_observation(&managed.basis, &primary.installed.installation)
        .unwrap();
    let stale_artifacts = stale.artifacts();
    let prepared = editor
        .prepare_source_tool_artifact_observation(&managed.basis, &primary.installed.installation)
        .unwrap();
    assert_eq!(editor.source_tool_artifact_view(), managed);
    assert_eq!(
        fs::read(project.root().join(ARTIFACT_PATH)).unwrap(),
        managed_bytes
    );
    assert_installed_bytes(&project, &installed_bytes);
    let attempt = editor.commit_source_tool_artifacts(prepared).unwrap();
    let inspected = settle_artifacts(&mut editor);
    assert_artifact_outcome(
        &inspected,
        &attempt,
        EditorSourceToolArtifactOutcome::Observed,
    );
    assert_artifact_installations(
        &inspected.artifacts,
        &[expected_primary.clone(), expected_secondary.clone()],
    );
    assert_ne!(inspected.basis, managed.basis);
    assert_eq!(stale.artifacts(), stale_artifacts);
    assert_eq!(stale_artifacts.len(), 2);
    assert!(
        stale_artifacts
            .iter()
            .flat_map(|artifact| &artifact.members)
            .all(|member| member.observation == EditorSourceToolArtifactObservation::Present)
    );
    let observed_bytes = fs::read(project.root().join(ARTIFACT_PATH)).unwrap();
    assert_eq!(observed_bytes, managed_bytes);
    assert_eq!(
        editor
            .commit_source_tool_artifacts(stale)
            .unwrap_err()
            .error(),
        &EditorSourceToolArtifactError::StaleBasis
    );
    assert_eq!(editor.source_tool_artifact_view(), inspected);
    assert_eq!(editor.source_tool_selection_view(), enabled);
    let mut expected_tool = reviewed_tool;
    expected_tool.artifacts = Some(inspected.clone());
    assert_eq!(editor.source_tool_view(), expected_tool);
    assert_eq!(editor.source_tool_review().unwrap().request(), &attached);
    assert_eq!(
        editor.source_tool_review().unwrap().patch(),
        &reviewed_patch
    );
    assert_eq!(
        editor.source_tool_review().unwrap().inverse(),
        &reviewed_inverse
    );
    assert_eq!(editor.document_query_basis(document).unwrap(), source_basis);
    assert_history(&editor, document, 0, 0);
    assert_eq!(fs::read(project.scene_path()).unwrap(), source_bytes);
    assert_eq!(
        fs::read(project.root().join(ARTIFACT_PATH)).unwrap(),
        observed_bytes
    );
    assert_eq!(
        fs::read(project.root().join(RECORD_PATH)).unwrap(),
        selection_bytes
    );
    assert_installed_bytes(&project, &installed_bytes);
    accept(&mut editor, document, &attached);
    assert_stats(&editor, document, 100);
    assert_history(&editor, document, 1, 0);
    let edited = review(
        &mut editor,
        document,
        EditorSourceToolIntent::Edit {
            entity: id("enemy"),
        },
        25,
    );
    accept(&mut editor, document, &edited);
    assert_stats(&editor, document, 125);
    assert_history(&editor, document, 2, 0);
    let edited_basis = editor.document_query_basis(document).unwrap();
    let edited_tool = editor.source_tool_view();

    // Add user files only after the actual command's exact release checks have completed.
    let unknown = format!("{}/samples/user-notes.txt", secondary.installed.directory);
    let user_bytes = b"Unlisted notes outside the declared artifact inventory.\n".to_vec();
    fs::write(project.root().join(&unknown), &user_bytes).unwrap();
    installed_bytes.insert(unknown.into(), user_bytes);
    let sample = format!("{}/{}", secondary.installed.directory, SAMPLE);
    let baseline = installed_bytes[&sample].clone();
    let changed = b"Externally changed EnemyStats artifact template.\n".to_vec();
    assert_ne!(
        ContentDigest::of_bytes(&changed),
        ContentDigest::of_bytes(&baseline)
    );
    fs::write(project.root().join(&sample), &changed).unwrap();
    installed_bytes.insert(sample.clone(), changed);
    let prepared = editor
        .prepare_source_tool_artifact_observation(
            &inspected.basis,
            &secondary.installed.installation,
        )
        .unwrap();
    assert_eq!(editor.source_tool_artifact_view(), inspected);
    assert_eq!(
        fs::read(project.root().join(ARTIFACT_PATH)).unwrap(),
        observed_bytes
    );
    assert_installed_bytes(&project, &installed_bytes);
    let attempt = editor.commit_source_tool_artifacts(prepared).unwrap();
    let observed = settle_artifacts(&mut editor);
    assert_artifact_outcome(
        &observed,
        &attempt,
        EditorSourceToolArtifactOutcome::Observed,
    );
    expected_secondary
        .members
        .iter_mut()
        .find(|member| member.path.as_ref() == SAMPLE)
        .unwrap()
        .observation = EditorSourceToolArtifactObservation::Changed;
    assert_artifact_installations(
        &observed.artifacts,
        &[expected_primary.clone(), expected_secondary.clone()],
    );
    let changed_bytes = fs::read(project.root().join(ARTIFACT_PATH)).unwrap();
    assert_ne!(changed_bytes, observed_bytes);
    assert_installed_bytes(&project, &installed_bytes);
    fs::write(project.root().join(&sample), &baseline).unwrap();
    installed_bytes.insert(sample, baseline);
    let prepared = editor
        .prepare_source_tool_artifact_observation(
            &observed.basis,
            &secondary.installed.installation,
        )
        .unwrap();
    let attempt = editor.commit_source_tool_artifacts(prepared).unwrap();
    let restored = settle_artifacts(&mut editor);
    expected_secondary
        .members
        .iter_mut()
        .find(|member| member.path.as_ref() == SAMPLE)
        .unwrap()
        .observation = EditorSourceToolArtifactObservation::Present;
    assert_artifact_outcome(
        &restored,
        &attempt,
        EditorSourceToolArtifactOutcome::Observed,
    );
    assert_artifact_installations(
        &restored.artifacts,
        &[expected_primary.clone(), expected_secondary.clone()],
    );
    assert_eq!(
        fs::read(project.root().join(ARTIFACT_PATH)).unwrap(),
        observed_bytes
    );
    assert_eq!(editor.source_tool_selection_view(), enabled);
    let mut expected_tool = edited_tool;
    expected_tool.artifacts = Some(restored.clone());
    assert_eq!(editor.source_tool_view(), expected_tool);
    assert_eq!(editor.document_query_basis(document).unwrap(), edited_basis);
    assert_stats(&editor, document, 125);
    assert_history(&editor, document, 2, 0);
    assert_eq!(fs::read(project.scene_path()).unwrap(), source_bytes);
    assert_installed_bytes(&project, &installed_bytes);

    let mut withdrawn = enabled;
    for entry in [&primary, &secondary] {
        let prepared = editor
            .prepare_source_tool_withdrawal(
                &withdrawn.basis,
                &entry.installed.installation,
                EditorSourceToolWithdrawalMode::RejectDependents,
            )
            .unwrap();
        assert_installations(prepared.installations(), std::slice::from_ref(entry));
        let attempt = editor.withdraw_source_tool_installations(prepared).unwrap();
        withdrawn = settle_selection(&mut editor);
        assert!(matches!(
            withdrawn.operation.as_ref(),
            Some(operation) if operation.attempt == attempt
                && operation.outcome == Some(EditorSourceToolSelectionOutcome::Withdrawn)
        ));
        settle_tool(&mut editor);
        assert_eq!(editor.source_tool_artifact_view(), restored);
        assert_eq!(editor.document_query_basis(document).unwrap(), edited_basis);
        assert_stats(&editor, document, 125);
        assert_history(&editor, document, 2, 0);
        assert_eq!(fs::read(project.scene_path()).unwrap(), source_bytes);
        assert_eq!(
            fs::read(project.root().join(ARTIFACT_PATH)).unwrap(),
            observed_bytes
        );
        assert_installed_bytes(&project, &installed_bytes);
        assert_no_execution(&editor);
    }
    assert!(withdrawn.installed.is_empty());
    assert!(withdrawn.desired.is_none());
    assert!(withdrawn.active_origin.is_none());
    replay(&mut editor, document, false);
    assert_stats(&editor, document, 100);
    assert_history(&editor, document, 1, 1);
    replay(&mut editor, document, true);
    assert_stats(&editor, document, 125);
    assert_history(&editor, document, 2, 0);
    for (entity, name) in [("enemy", "Guard"), ("guide", "Guide")] {
        assert_value(
            &editor,
            document,
            entity,
            NAME,
            "value",
            ComponentValue::String(name.into()),
        );
    }
    save(&mut editor, document);
    assert_history(&editor, document, 2, 0);
    let saved = project.read();
    assert_ne!(saved, original());
    let saved_bytes = fs::read(project.scene_path()).unwrap();
    let graph_bytes = fs::read(project.root().join(RECORD_PATH)).unwrap();
    assert_eq!(editor.source_tool_artifact_view(), restored);
    assert_eq!(
        fs::read(project.root().join(ARTIFACT_PATH)).unwrap(),
        observed_bytes
    );
    assert_installed_bytes(&project, &installed_bytes);
    close(&mut editor);
    assert_no_execution(&editor);
    drop(editor);
    drop(primary_owner);
    drop(secondary_owner);

    let mut reopened = project.open();
    let document = reopened
        .current_frame()
        .workspace()
        .active_document
        .unwrap();
    let selection = settle_selection(&mut reopened);
    let content = settle_artifacts(&mut reopened);
    assert!(selection.installed.is_empty());
    assert!(selection.desired.is_none());
    assert!(selection.active_origin.is_none());
    assert_eq!(content.phase, EditorSourceToolArtifactPhase::Idle);
    assert!(content.error.is_none());
    assert!(matches!(
        content.operation.as_ref().unwrap().outcome,
        Some(EditorSourceToolArtifactOutcome::Observed)
    ));
    assert_artifact_installations(&content.artifacts, &[expected_primary, expected_secondary]);
    assert_no_execution(&reopened);
    assert!(reopened.source_tool_view().attempt.is_none());
    assert!(reopened.source_tool_view().request.is_none());
    assert_stats(&reopened, document, 125);
    assert_history(&reopened, document, 0, 0);
    assert_eq!(project.read(), saved);
    assert_eq!(fs::read(project.scene_path()).unwrap(), saved_bytes);
    assert_eq!(
        fs::read(project.root().join(RECORD_PATH)).unwrap(),
        graph_bytes
    );
    assert_eq!(
        fs::read(project.root().join(ARTIFACT_PATH)).unwrap(),
        observed_bytes
    );
    assert_installed_bytes(&project, &installed_bytes);
    close(&mut reopened);
    assert_no_execution(&reopened);
    assert_eq!(fs::read(project.scene_path()).unwrap(), saved_bytes);
    assert_eq!(
        fs::read(project.root().join(ARTIFACT_PATH)).unwrap(),
        observed_bytes
    );
    assert_installed_bytes(&project, &installed_bytes);
}

#[test]
fn csharp_installed_graph_keeps_exact_disabled_dependents_and_confirms_fresh_cascade() {
    let (_template_directory, template) = installed_variant("V1");
    let project = Project::new(
        &schema_bundle(template.release().schema_owner()),
        &original(),
        "json",
    );
    let root = project_root(&project);
    let a1 = install(&root, template.release(), "installed-a1", PACKAGE_A, None);
    let a2 = install(&root, template.release(), "installed-a2", PACKAGE_A, None);
    assert_eq!(a1.installed.manifest, a2.installed.manifest);
    assert_ne!(a1.installed.installation, a2.installed.installation);
    assert_ne!(a1.installed.receipt, a2.installed.receipt);
    assert_eq!(
        fs::read(
            project
                .root()
                .join(a1.installed.directory.as_ref())
                .join("EnemyStatsTool.dll")
        )
        .unwrap(),
        fs::read(
            project
                .root()
                .join(a2.installed.directory.as_ref())
                .join("EnemyStatsTool.dll")
        )
        .unwrap(),
    );
    let mut b = install(
        &root,
        template.release(),
        "installed-b",
        PACKAGE_B,
        Some((PACKAGE_A, a1.installed.manifest)),
    );
    b.requirements.push(binding(&a1));
    let c = install(&root, template.release(), "installed-c", PACKAGE_C, None);
    let host = approved_managed_host();

    // Standalone approval cannot resolve B's requirement or select between equal releases.
    let b_directory =
        open_explicit_product_project_root(&project.root().join(b.installed.directory.as_ref()))
            .unwrap();
    let b_inspection =
        EditorSourceToolInstallationInspection::read(&b_directory, b.installed.receipt).unwrap();
    assert!(host.approve_installation(b_inspection).is_err());

    let original_disk = fs::read(project.scene_path()).unwrap();
    let mut editor = project.open();
    let document = editor.current_frame().workspace().active_document.unwrap();
    let observed = settle_selection(&mut editor);
    assert!(observed.installed.is_empty());
    assert_no_execution(&editor);
    let document_basis = editor.document_query_basis(document).unwrap();
    let prepared = editor
        .prepare_source_tool_installations(
            &observed.basis,
            root.share(),
            &[request(&a1), request(&a2), request(&b)],
        )
        .unwrap();
    assert_eq!(editor.source_tool_selection_view(), observed);
    assert_no_execution(&editor);
    assert!(!project.root().join(RECORD_PATH).exists());
    let attempt = editor.register_source_tool_installations(prepared).unwrap();
    let registered = settle_selection(&mut editor);
    assert!(matches!(
        registered.operation.as_ref(),
        Some(operation) if operation.attempt == attempt
            && operation.outcome == Some(EditorSourceToolSelectionOutcome::Registered)
    ));
    assert_installations(&registered.installed, &[a1.clone(), a2.clone(), b.clone()]);
    assert!(registered.desired.is_none());
    assert!(registered.active_origin.is_none());
    assert_no_execution(&editor);
    assert!(editor.source_tool_view().attempt.is_none());
    assert_eq!(
        editor.document_query_basis(document).unwrap(),
        document_basis
    );
    assert_history(&editor, document, 0, 0);
    assert_eq!(fs::read(project.scene_path()).unwrap(), original_disk);

    let prepared = editor
        .prepare_installed_source_tool_selection(
            root.share(),
            b.installed.directory.as_ref(),
            b.installed.receipt,
            &host,
        )
        .unwrap();
    assert_eq!(prepared.selection(), &b.installed);
    assert_no_execution(&editor);
    let attempt = editor
        .enable_installed_source_tool(&registered.basis, prepared)
        .unwrap();
    let enabled = settle_selection(&mut editor);
    assert!(matches!(
        enabled.operation.as_ref(),
        Some(operation) if operation.attempt == attempt
            && operation.outcome == Some(EditorSourceToolSelectionOutcome::Enabled)
    ));
    assert_installations(&enabled.installed, &[a1.clone(), a2.clone(), b.clone()]);
    assert_eq!(enabled.active_origin.as_ref(), Some(&b.installed));
    let desired = enabled.desired.as_ref().unwrap();
    assert!(desired.enabled);
    assert_eq!(desired.installed, b.installed);
    let active = editor.source_tool_view().active.unwrap();
    assert_eq!(active.owner(), template.release().schema_owner().owner_id());
    assert_eq!(active.component(), &ComponentTypeId::new(STATS));

    let attached = review(
        &mut editor,
        document,
        EditorSourceToolIntent::Attach {
            entity: id("enemy"),
        },
        0,
    );
    assert_eq!(
        editor.document_query_basis(document).unwrap(),
        document_basis
    );
    assert_history(&editor, document, 0, 0);
    accept(&mut editor, document, &attached);
    assert_stats(&editor, document, 100);
    let edited = review(
        &mut editor,
        document,
        EditorSourceToolIntent::Edit {
            entity: id("enemy"),
        },
        25,
    );
    assert_stats(&editor, document, 100);
    assert_history(&editor, document, 1, 0);
    accept(&mut editor, document, &edited);
    assert_stats(&editor, document, 125);
    assert_history(&editor, document, 2, 0);

    let abandoned = review(
        &mut editor,
        document,
        EditorSourceToolIntent::Edit {
            entity: id("enemy"),
        },
        500,
    );
    let retained_basis = editor.document_query_basis(document).unwrap();
    let activation = editor.source_tool_view().active.unwrap().id().clone();
    let pending = editor
        .request_source_tool(
            &activation,
            &retained_basis,
            EditorSourceToolIntent::Edit {
                entity: id("enemy"),
            },
            BTreeMap::from([("bonus".into(), ComponentValue::I64(900))]),
        )
        .unwrap();
    assert!(editor.source_tool_view().working);
    let attempt = editor
        .disable_installed_source_tool(&enabled.basis)
        .unwrap();
    assert!(editor.source_tool_view().active.is_none());
    assert!(editor.source_tool_review().is_none());
    assert!(matches!(
        editor.accept_source_tool_review(&abandoned),
        Err(EditorSourceToolError::Disabled)
    ));
    assert!(matches!(
        editor.accept_source_tool_review(&pending),
        Err(EditorSourceToolError::Disabled)
    ));
    let disabled = settle_selection(&mut editor);
    assert!(matches!(
        disabled.disable.as_ref(),
        Some(operation) if operation.attempt == attempt
            && operation.outcome == Some(EditorSourceToolSelectionOutcome::Disabled)
    ));
    // Durable selection completion and the actual child/IO return are separate observations.
    settle_tool(&mut editor);
    assert_no_execution(&editor);
    assert_installations(&disabled.installed, &[a1.clone(), a2.clone(), b.clone()]);
    let desired = disabled.desired.as_ref().unwrap();
    assert!(!desired.enabled);
    assert_eq!(desired.installed, b.installed);
    assert!(disabled.active_origin.is_none());
    assert_eq!(
        editor.document_query_basis(document).unwrap(),
        retained_basis
    );
    assert_stats(&editor, document, 125);
    assert_history(&editor, document, 2, 0);
    assert_eq!(fs::read(project.scene_path()).unwrap(), original_disk);

    let disabled_disk = fs::read(project.root().join(RECORD_PATH)).unwrap();
    assert!(matches!(
        editor.prepare_source_tool_withdrawal(
            &disabled.basis,
            &a1.installed.installation,
            EditorSourceToolWithdrawalMode::RejectDependents,
        ),
        Err(EditorSourceToolSelectionError::RequiredDependents)
    ));
    assert_eq!(editor.source_tool_selection_view(), disabled);
    assert_eq!(
        fs::read(project.root().join(RECORD_PATH)).unwrap(),
        disabled_disk
    );

    let cancelled = editor
        .prepare_source_tool_withdrawal(
            &disabled.basis,
            &a1.installed.installation,
            EditorSourceToolWithdrawalMode::Cascade,
        )
        .unwrap();
    assert_installations(cancelled.installations(), &[a1.clone(), b.clone()]);
    drop(cancelled);
    assert_eq!(editor.source_tool_selection_view(), disabled);
    assert_eq!(
        fs::read(project.root().join(RECORD_PATH)).unwrap(),
        disabled_disk
    );

    let stale = editor
        .prepare_source_tool_withdrawal(
            &disabled.basis,
            &a1.installed.installation,
            EditorSourceToolWithdrawalMode::Cascade,
        )
        .unwrap();
    let prepared = editor
        .prepare_source_tool_installations(&disabled.basis, root.share(), &[request(&c)])
        .unwrap();
    let attempt = editor.register_source_tool_installations(prepared).unwrap();
    let extended = settle_selection(&mut editor);
    assert!(matches!(
        extended.operation.as_ref(),
        Some(operation) if operation.attempt == attempt
            && operation.outcome == Some(EditorSourceToolSelectionOutcome::Registered)
    ));
    assert_installations(
        &extended.installed,
        &[a1.clone(), a2.clone(), b.clone(), c.clone()],
    );
    let extended_disk = fs::read(project.root().join(RECORD_PATH)).unwrap();
    assert!(matches!(
        editor.withdraw_source_tool_installations(stale),
        Err(EditorSourceToolSelectionError::StaleBasis)
    ));
    assert_eq!(editor.source_tool_selection_view(), extended);
    assert_eq!(
        fs::read(project.root().join(RECORD_PATH)).unwrap(),
        extended_disk
    );

    let prepared = editor
        .prepare_source_tool_withdrawal(
            &extended.basis,
            &a1.installed.installation,
            EditorSourceToolWithdrawalMode::Cascade,
        )
        .unwrap();
    assert_installations(prepared.installations(), &[a1.clone(), b.clone()]);
    let attempt = editor.withdraw_source_tool_installations(prepared).unwrap();
    let withdrawn = settle_selection(&mut editor);
    assert!(matches!(
        withdrawn.operation.as_ref(),
        Some(operation) if operation.attempt == attempt
            && operation.outcome == Some(EditorSourceToolSelectionOutcome::Withdrawn)
    ));
    settle_tool(&mut editor);
    assert_installations(&withdrawn.installed, &[a2.clone(), c.clone()]);
    assert!(withdrawn.desired.is_none());
    assert!(withdrawn.active_origin.is_none());
    assert_no_execution(&editor);
    assert_eq!(
        editor.document_query_basis(document).unwrap(),
        retained_basis
    );
    assert_stats(&editor, document, 125);
    assert_history(&editor, document, 2, 0);
    replay(&mut editor, document, false);
    assert_stats(&editor, document, 100);
    assert_history(&editor, document, 1, 1);
    replay(&mut editor, document, true);
    assert_stats(&editor, document, 125);
    assert_history(&editor, document, 2, 0);
    assert_value(
        &editor,
        document,
        "guide",
        NAME,
        "value",
        ComponentValue::String("Guide".into()),
    );
    assert_eq!(fs::read(project.scene_path()).unwrap(), original_disk);
    // Withdrawal changes selection only, including for the now-withdrawn A1 and B.
    for entry in [&a1, &a2, &b, &c] {
        let directory = open_explicit_product_project_root(
            &project.root().join(entry.installed.directory.as_ref()),
        )
        .unwrap();
        let current =
            EditorSourceToolInstallationInspection::read(&directory, entry.installed.receipt)
                .unwrap();
        assert_eq!(
            current.receipt().installation_id(),
            &entry.installed.installation
        );
        assert_eq!(
            current.release().manifest_digest(),
            entry.installed.manifest
        );
    }

    save(&mut editor, document);
    let saved = project.read();
    let saved_bytes = fs::read(project.scene_path()).unwrap();
    let graph_bytes = fs::read(project.root().join(RECORD_PATH)).unwrap();
    close(&mut editor);
    assert_no_execution(&editor);
    drop(editor);

    let mut reopened = project.open();
    let reopened_document = reopened
        .current_frame()
        .workspace()
        .active_document
        .unwrap();
    let observed = settle_selection(&mut reopened);
    assert_installations(&observed.installed, &[a2, c]);
    assert!(observed.desired.is_none());
    assert!(observed.active_origin.is_none());
    assert_no_execution(&reopened);
    assert!(reopened.source_tool_view().attempt.is_none());
    assert_stats(&reopened, reopened_document, 125);
    assert_history(&reopened, reopened_document, 0, 0);
    assert_eq!(project.read(), saved);
    assert_eq!(fs::read(project.scene_path()).unwrap(), saved_bytes);
    assert_eq!(
        fs::read(project.root().join(RECORD_PATH)).unwrap(),
        graph_bytes
    );
    close(&mut reopened);
    assert_no_execution(&reopened);
}

#[test]
fn csharp_installed_graph_rejects_missing_mismatched_and_silently_rebound_requirements() {
    let (_template_directory, template) = installed_variant("V1");
    let (_v2_directory, v2) = installed_variant("V2");
    let project = Project::new(
        &schema_bundle(template.release().schema_owner()),
        &original(),
        "json",
    );
    let root = project_root(&project);
    let a1 = install(&root, template.release(), "installed-a1", PACKAGE_A, None);
    let a2 = install(&root, template.release(), "installed-a2", PACKAGE_A, None);
    let different = install(&root, v2.release(), "installed-different", PACKAGE_A, None);
    assert_ne!(different.installed.manifest, a1.installed.manifest);
    let mut b = install(
        &root,
        template.release(),
        "installed-b",
        PACKAGE_B,
        Some((PACKAGE_A, a1.installed.manifest)),
    );
    b.requirements.push(binding(&a1));
    let mut editor = project.open();
    let document = editor.current_frame().workspace().active_document.unwrap();
    let observed = settle_selection(&mut editor);
    let document_basis = editor.document_query_basis(document).unwrap();
    let scene_bytes = fs::read(project.scene_path()).unwrap();
    let mut missing_binding = request(&b);
    missing_binding.requirements.clear();
    let mut missing_target = request(&b);
    missing_target.requirements = vec![binding(&a2)];
    let mut wrong_package = request(&b);
    wrong_package.requirements[0].package = PACKAGE_C.into();
    let mut wrong_manifest = request(&b);
    wrong_manifest.requirements = vec![binding(&different)];
    let mut missing_directory = request(&a1);
    missing_directory.directory = ".nara/source-tools/artifacts/missing-installation".into();
    let mut wrong_receipt = request(&a1);
    wrong_receipt.receipt = ContentDigest::of_bytes(b"another installation receipt");
    for (case, requests, expected) in [
        (
            "missing binding",
            vec![request(&a1), missing_binding],
            EditorSourceToolSelectionError::Graph,
        ),
        (
            "missing target",
            vec![request(&a1), missing_target],
            EditorSourceToolSelectionError::Graph,
        ),
        (
            "wrong package",
            vec![request(&a1), wrong_package],
            EditorSourceToolSelectionError::Graph,
        ),
        (
            "wrong manifest",
            vec![request(&different), wrong_manifest],
            EditorSourceToolSelectionError::Graph,
        ),
        (
            "missing installation directory",
            vec![missing_directory],
            EditorSourceToolSelectionError::Installation,
        ),
        (
            "wrong receipt",
            vec![wrong_receipt],
            EditorSourceToolSelectionError::Installation,
        ),
    ] {
        assert_eq!(
            editor
                .prepare_source_tool_installations(&observed.basis, root.share(), &requests,)
                .err(),
            Some(expected),
            "{case}",
        );
        assert_eq!(editor.source_tool_selection_view(), observed, "{case}");
        assert_no_execution(&editor);
        assert_eq!(
            editor.document_query_basis(document).unwrap(),
            document_basis
        );
        assert_history(&editor, document, 0, 0);
        assert_eq!(fs::read(project.scene_path()).unwrap(), scene_bytes);
        assert!(!project.root().join(RECORD_PATH).exists());
    }

    let prepared = editor
        .prepare_source_tool_installations(
            &observed.basis,
            root.share(),
            &[request(&a1), request(&a2), request(&b)],
        )
        .unwrap();
    editor.register_source_tool_installations(prepared).unwrap();
    let registered = settle_selection(&mut editor);
    assert_installations(&registered.installed, &[a1.clone(), a2.clone(), b.clone()]);
    let record_bytes = fs::read(project.root().join(RECORD_PATH)).unwrap();
    let mut rebound = request(&b);
    rebound.requirements = vec![binding(&a2)];
    assert!(matches!(
        editor.prepare_source_tool_installations(&registered.basis, root.share(), &[rebound]),
        Err(EditorSourceToolSelectionError::Graph)
    ));
    assert_eq!(editor.source_tool_selection_view(), registered);
    assert_eq!(
        fs::read(project.root().join(RECORD_PATH)).unwrap(),
        record_bytes
    );
    assert_no_execution(&editor);

    // An exact repeat is admitted; equal release bytes do not make changed bindings a repeat.
    let prepared = editor
        .prepare_source_tool_installations(&registered.basis, root.share(), &[request(&b)])
        .unwrap();
    let attempt = editor.register_source_tool_installations(prepared).unwrap();
    let repeated = settle_selection(&mut editor);
    assert!(matches!(
        repeated.operation.as_ref(),
        Some(operation) if operation.attempt == attempt
            && operation.outcome == Some(EditorSourceToolSelectionOutcome::Registered)
    ));
    assert_installations(&repeated.installed, &[a1, a2, b]);
    assert_eq!(
        fs::read(project.root().join(RECORD_PATH)).unwrap(),
        record_bytes
    );
    assert_eq!(
        editor.document_query_basis(document).unwrap(),
        document_basis
    );
    assert_eq!(fs::read(project.scene_path()).unwrap(), scene_bytes);
    assert_history(&editor, document, 0, 0);
    assert_no_execution(&editor);
    assert!(editor.source_tool_view().attempt.is_none());
    close(&mut editor);
}

#[test]
fn csharp_installed_graph_rechecks_required_payloads_before_describe_and_each_proposal() {
    let (_template_directory, template) = installed_variant("V1");
    let project = Project::new(
        &schema_bundle(template.release().schema_owner()),
        &original(),
        "json",
    );
    let root = project_root(&project);
    let a1 = install(&root, template.release(), "installed-a1", PACKAGE_A, None);
    let a2 = install(&root, template.release(), "installed-a2", PACKAGE_A, None);
    let mut b = install(
        &root,
        template.release(),
        "installed-b",
        PACKAGE_B,
        Some((PACKAGE_A, a1.installed.manifest)),
    );
    b.requirements.push(binding(&a1));
    let host = approved_managed_host();
    let mut editor = project.open();
    let document = editor.current_frame().workspace().active_document.unwrap();
    let observed = settle_selection(&mut editor);
    let prepared = editor
        .prepare_source_tool_installations(
            &observed.basis,
            root.share(),
            &[request(&a1), request(&a2), request(&b)],
        )
        .unwrap();
    editor.register_source_tool_installations(prepared).unwrap();
    let registered = settle_selection(&mut editor);
    let registered_bytes = fs::read(project.root().join(RECORD_PATH)).unwrap();
    let scene_bytes = fs::read(project.scene_path()).unwrap();
    let original_basis = editor.document_query_basis(document).unwrap();

    let prepared = editor
        .prepare_installed_source_tool_selection(
            root.share(),
            b.installed.directory.as_ref(),
            b.installed.receipt,
            &host,
        )
        .unwrap();
    let a1_payload = project
        .root()
        .join(a1.installed.directory.as_ref())
        .join("EnemyStatsTool.dll");
    let original_payload = fs::read(&a1_payload).unwrap();
    let mut changed_payload = original_payload.clone();
    changed_payload[0] ^= 1;
    // Approval captured an intact closure; the actual describe worker must inspect A1 again.
    fs::write(&a1_payload, &changed_payload).unwrap();
    let attempt = editor
        .enable_installed_source_tool(&registered.basis, prepared)
        .unwrap();
    settle_tool(&mut editor);
    let failed_probe = editor.source_tool_selection_view();
    assert!(matches!(
        failed_probe.operation.as_ref(),
        Some(operation) if operation.attempt == attempt
            && operation.outcome == Some(EditorSourceToolSelectionOutcome::Failed(
                EditorSourceToolSelectionError::ProbeFailed,
            ))
    ));
    assert_eq!(
        editor.source_tool_view().attempt_error,
        Some(EditorSourceToolError::Process("required-installation")),
    );
    assert_no_execution(&editor);
    assert_installations(
        &failed_probe.installed,
        &[a1.clone(), a2.clone(), b.clone()],
    );
    assert!(failed_probe.desired.is_none());
    assert!(failed_probe.active_origin.is_none());
    assert_eq!(
        fs::read(project.root().join(RECORD_PATH)).unwrap(),
        registered_bytes
    );
    assert_eq!(
        editor.document_query_basis(document).unwrap(),
        original_basis
    );
    assert_eq!(fs::read(project.scene_path()).unwrap(), scene_bytes);
    assert_history(&editor, document, 0, 0);
    assert_eq!(
        fs::read(
            project
                .root()
                .join(a2.installed.directory.as_ref())
                .join("EnemyStatsTool.dll")
        )
        .unwrap(),
        original_payload,
    );

    fs::write(&a1_payload, &original_payload).unwrap();
    let prepared = editor
        .prepare_installed_source_tool_selection(
            root.share(),
            b.installed.directory.as_ref(),
            b.installed.receipt,
            &host,
        )
        .unwrap();
    let attempt = editor
        .enable_installed_source_tool(&failed_probe.basis, prepared)
        .unwrap();
    let enabled = settle_selection(&mut editor);
    assert!(matches!(
        enabled.operation.as_ref(),
        Some(operation) if operation.attempt == attempt
            && operation.outcome == Some(EditorSourceToolSelectionOutcome::Enabled)
    ));
    assert_eq!(enabled.active_origin.as_ref(), Some(&b.installed));
    let attached = review(
        &mut editor,
        document,
        EditorSourceToolIntent::Attach {
            entity: id("enemy"),
        },
        0,
    );
    accept(&mut editor, document, &attached);
    assert_stats(&editor, document, 100);
    assert_history(&editor, document, 1, 0);
    let attached_basis = editor.document_query_basis(document).unwrap();
    let enabled_bytes = fs::read(project.root().join(RECORD_PATH)).unwrap();
    let activation = editor.source_tool_view().active.unwrap().id().clone();

    // A2 remains byte-identical and intact, but B is bound to the now-modified A1.
    fs::write(&a1_payload, &changed_payload).unwrap();
    let rejected = editor
        .request_source_tool(
            &activation,
            &attached_basis,
            EditorSourceToolIntent::Edit {
                entity: id("enemy"),
            },
            BTreeMap::from([("bonus".into(), ComponentValue::I64(25))]),
        )
        .unwrap();
    settle_tool(&mut editor);
    let rejected_view = editor.source_tool_view();
    assert_eq!(rejected_view.request.as_ref(), Some(&rejected));
    assert_eq!(
        rejected_view.request_error,
        Some(EditorSourceToolError::Process("required-installation")),
    );
    assert!(rejected_view.attempt_error.is_none());
    assert_eq!(rejected_view.active.as_ref().unwrap().id(), &activation);
    assert!(editor.source_tool_review().is_none());
    assert_eq!(editor.source_tool_selection_view(), enabled);
    assert_eq!(
        editor.document_query_basis(document).unwrap(),
        attached_basis
    );
    assert_history(&editor, document, 1, 0);
    assert_stats(&editor, document, 100);
    assert_value(
        &editor,
        document,
        "enemy",
        NAME,
        "value",
        ComponentValue::String("Guard".into()),
    );
    assert_eq!(fs::read(project.scene_path()).unwrap(), scene_bytes);
    assert_eq!(
        fs::read(project.root().join(RECORD_PATH)).unwrap(),
        enabled_bytes
    );

    fs::write(&a1_payload, &original_payload).unwrap();
    let recovered = review(
        &mut editor,
        document,
        EditorSourceToolIntent::Edit {
            entity: id("enemy"),
        },
        25,
    );
    assert_stats(&editor, document, 100);
    accept(&mut editor, document, &recovered);
    assert_stats(&editor, document, 125);
    assert_history(&editor, document, 2, 0);
    save(&mut editor, document);
    let saved = project.read();
    close(&mut editor);
    assert_no_execution(&editor);
    drop(editor);

    // Fresh Open retains enabled intent and the complete graph, without restoring approval.
    let mut reopened = project.open();
    let document = reopened
        .current_frame()
        .workspace()
        .active_document
        .unwrap();
    let observed = settle_selection(&mut reopened);
    assert_installations(&observed.installed, &[a1, a2, b]);
    assert_eq!(observed.desired, enabled.desired);
    assert!(observed.desired.as_ref().unwrap().enabled);
    assert!(observed.active_origin.is_none());
    assert_no_execution(&reopened);
    assert!(reopened.source_tool_view().attempt.is_none());
    assert_stats(&reopened, document, 125);
    assert_history(&reopened, document, 0, 0);
    assert_eq!(project.read(), saved);
    assert_eq!(
        fs::read(project.root().join(RECORD_PATH)).unwrap(),
        enabled_bytes
    );
    close(&mut reopened);
    assert_no_execution(&reopened);
}

#[test]
fn csharp_installed_graph_session_only_routes_follow_exact_approved_installation_provenance() {
    let (_template_directory, template) = installed_variant("V1");
    let managed = approved_managed_host();
    for (corrupt_selection, external_route) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        eprintln!(
            "Session-only installation provenance: corrupt={corrupt_selection}, \
             external={external_route}",
        );
        let project = Project::new(
            &schema_bundle(template.release().schema_owner()),
            &original(),
            "json",
        );
        let root = project_root(&project);
        let a1 = install(&root, template.release(), "installed-a1", PACKAGE_A, None);
        let installation_path = project.root().join(a1.installed.directory.as_ref());
        let installation_bytes: BTreeMap<_, _> = template
            .release()
            .members()
            .iter()
            .map(|member| member.path())
            .chain(["nara-tool.json", ".nara-installation.json"])
            .map(|path| (path, fs::read(installation_path.join(path)).unwrap()))
            .collect();
        let scene_bytes = fs::read(project.scene_path()).unwrap();
        let mut editor = project.open();
        let document = editor.current_frame().workspace().active_document.unwrap();
        let observed = settle_selection(&mut editor);
        let prepared = editor
            .prepare_source_tool_installations(&observed.basis, root.share(), &[request(&a1)])
            .unwrap();
        editor.register_source_tool_installations(prepared).unwrap();
        let registered = settle_selection(&mut editor);
        assert_installations(&registered.installed, &[a1.clone()]);
        assert!(registered.desired.is_none());
        assert_no_execution(&editor);
        let graph_bytes = fs::read(project.root().join(RECORD_PATH)).unwrap();

        let installed_directory = open_explicit_product_project_root(&installation_path).unwrap();
        let external_directory = tempfile::tempdir().unwrap();
        let route_directory = if external_route {
            // Keep every logical installation fact identical; only the actual directory differs.
            for (path, bytes) in &installation_bytes {
                let destination = external_directory.path().join(path);
                fs::create_dir_all(destination.parent().unwrap()).unwrap();
                fs::write(destination, bytes).unwrap();
            }
            open_explicit_product_project_root(external_directory.path()).unwrap()
        } else {
            installed_directory.share()
        };
        assert_eq!(
            route_directory.same_object(&installed_directory).unwrap(),
            !external_route,
        );
        let inspected =
            EditorSourceToolInstallationInspection::read(&route_directory, a1.installed.receipt)
                .unwrap();
        assert_eq!(
            inspected.receipt().installation_id(),
            &a1.installed.installation
        );
        assert_eq!(inspected.release().manifest_digest(), a1.installed.manifest);
        let definition = managed.approve_installation(inspected).unwrap();
        let probe = editor.enable_source_tool(definition).unwrap();
        settle_tool(&mut editor);
        let tool = editor.source_tool_view();
        assert_eq!(tool.attempt.as_ref(), Some(&probe));
        assert!(tool.attempt_error.is_none(), "{tool:?}");
        let activation = tool.active.as_ref().unwrap().id().clone();
        let mut active_selection = editor.source_tool_selection_view();
        assert_installations(&active_selection.installed, &[a1.clone()]);
        assert!(active_selection.desired.is_none());
        assert!(active_selection.active_origin.is_none());
        assert_eq!(
            fs::read(project.root().join(RECORD_PATH)).unwrap(),
            graph_bytes
        );

        if corrupt_selection && !external_route {
            // Keep a positively observed link even if equal receipt bytes later appear at the
            // old path. Namespace replacement cannot erase the route's proven relationship.
            editor
                .reconcile_source_tool_selection(&active_selection.basis)
                .unwrap();
            active_selection = settle_selection(&mut editor);
            assert_eq!(
                editor.source_tool_view().active.as_ref().unwrap().id(),
                &activation
            );
            fs::rename(
                &installation_path,
                project.root().join("displaced-installed-a1"),
            )
            .unwrap();
            for (path, bytes) in &installation_bytes {
                let destination = installation_path.join(path);
                fs::create_dir_all(destination.parent().unwrap()).unwrap();
                fs::write(destination, bytes).unwrap();
            }
            let replacement = open_explicit_product_project_root(&installation_path).unwrap();
            assert!(!replacement.same_object(&route_directory).unwrap());
        }

        let source_basis = editor.document_query_basis(document).unwrap();
        let reviewed = review(
            &mut editor,
            document,
            EditorSourceToolIntent::Attach {
                entity: id("enemy"),
            },
            25,
        );
        let proposal = editor.source_tool_review().unwrap().patch().clone();
        assert_eq!(editor.document_query_basis(document).unwrap(), source_basis);
        assert_history(&editor, document, 0, 0);
        assert_eq!(fs::read(project.scene_path()).unwrap(), scene_bytes);

        let attempt = if corrupt_selection {
            fs::write(project.root().join(RECORD_PATH), b"{").unwrap();
            editor
                .reconcile_source_tool_selection(&active_selection.basis)
                .unwrap()
        } else {
            let prepared = editor
                .prepare_source_tool_withdrawal(
                    &active_selection.basis,
                    &a1.installed.installation,
                    EditorSourceToolWithdrawalMode::RejectDependents,
                )
                .unwrap();
            assert_installations(prepared.installations(), &[a1.clone()]);
            // Read-only preparation must preserve the active route and exact reviewed proposal.
            assert_eq!(editor.source_tool_selection_view(), active_selection);
            assert_eq!(editor.source_tool_view().active.unwrap().id(), &activation);
            assert_eq!(editor.source_tool_review().unwrap().request(), &reviewed);
            assert_eq!(
                fs::read(project.root().join(RECORD_PATH)).unwrap(),
                graph_bytes
            );
            let attempt = editor.withdraw_source_tool_installations(prepared).unwrap();
            if !external_route {
                assert!(editor.source_tool_view().active.is_none());
                assert!(editor.source_tool_review().is_none());
            }
            attempt
        };
        let deadline = Instant::now() + TIMEOUT;
        let settled = loop {
            editor.drive_editor_frame(Duration::ZERO);
            let view = editor.source_tool_selection_view();
            if view.operation.as_ref().is_some_and(|operation| {
                operation.attempt == attempt && operation.outcome.is_some()
            }) {
                break view;
            }
            assert!(
                Instant::now() < deadline,
                "Provenance operation did not settle: {view:?}"
            );
            std::thread::sleep(Duration::from_millis(1));
        };
        settle_tool(&mut editor);
        assert!(settled.desired.is_none());
        assert!(settled.active_origin.is_none());
        if corrupt_selection {
            assert_eq!(settled.phase, EditorSourceToolSelectionPhase::Unavailable);
            assert_eq!(
                settled.error,
                Some(EditorSourceToolSelectionError::InvalidRecord)
            );
            assert_eq!(
                settled.operation.as_ref().unwrap().outcome,
                Some(EditorSourceToolSelectionOutcome::Failed(
                    EditorSourceToolSelectionError::InvalidRecord,
                )),
            );
            assert_installations(&settled.installed, &[a1.clone()]);
            assert_eq!(fs::read(project.root().join(RECORD_PATH)).unwrap(), b"{");
        } else {
            assert_eq!(settled.phase, EditorSourceToolSelectionPhase::Idle);
            assert!(settled.error.is_none());
            assert_eq!(
                settled.operation.as_ref().unwrap().outcome,
                Some(EditorSourceToolSelectionOutcome::Withdrawn),
            );
            assert!(settled.installed.is_empty());
        }
        assert_eq!(editor.source_tool_view().attempt.as_ref(), Some(&probe));
        assert_eq!(editor.document_query_basis(document).unwrap(), source_basis);
        assert_history(&editor, document, 0, 0);
        assert_eq!(fs::read(project.scene_path()).unwrap(), scene_bytes);
        for (path, expected) in &installation_bytes {
            assert_eq!(
                &fs::read(installation_path.join(path)).unwrap(),
                expected,
                "{path}"
            );
        }
        assert_value(
            &editor,
            document,
            "enemy",
            NAME,
            "value",
            ComponentValue::String("Guard".into()),
        );
        assert_value(
            &editor,
            document,
            "guide",
            NAME,
            "value",
            ComponentValue::String("Guide".into()),
        );
        if external_route {
            assert_eq!(editor.source_tool_view().active.unwrap().id(), &activation);
            assert_eq!(editor.source_tool_review().unwrap().request(), &reviewed);
            assert_eq!(editor.source_tool_review().unwrap().patch(), &proposal);
            accept(&mut editor, document, &reviewed);
            assert_stats(&editor, document, 125);
            assert_history(&editor, document, 1, 0);
            save(&mut editor, document);
            for (path, expected) in &installation_bytes {
                assert_eq!(
                    &fs::read(external_directory.path().join(path)).unwrap(),
                    expected,
                    "{path}",
                );
            }
        } else {
            assert_no_execution(&editor);
            assert!(matches!(
                editor.accept_source_tool_review(&reviewed),
                Err(EditorSourceToolError::Disabled)
            ));
            assert!(matches!(
                editor.request_source_tool(
                    &activation,
                    &source_basis,
                    EditorSourceToolIntent::Attach {
                        entity: id("enemy")
                    },
                    BTreeMap::new(),
                ),
                Err(EditorSourceToolError::Disabled)
            ));
            assert_eq!(editor.document_query_basis(document).unwrap(), source_basis);
            assert_history(&editor, document, 0, 0);
            assert_eq!(project.read(), original());
        }
        close(&mut editor);
        assert_no_execution(&editor);
    }
}

fn original() -> SceneDocument {
    SceneDocument::from_ordered_roots([("enemy", "Guard"), ("guide", "Guide")].map(
        |(entity, name)| {
            SceneEntityRecord::new(id(entity)).with_component(
                ComponentTypeId::new(NAME),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::String(name.into()),
                ),
            )
        },
    ))
}

fn project_root(project: &Project) -> DirectoryCapability {
    let directory = fs::OpenOptions::new()
        .read(true)
        .share_mode(7)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(project.root())
        .unwrap();
    DirectoryCapability::from_host_handle(
        directory,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::Untrusted),
    )
    .unwrap()
}

fn install(
    project: &DirectoryCapability,
    template: &PreparedEditorSourceToolRelease,
    destination: &str,
    package: &str,
    required: Option<(&str, ContentDigest)>,
) -> EditorSourceToolInstalledEntry {
    install_owned(project, template, destination, package, required).0
}

fn install_owned(
    project: &DirectoryCapability,
    template: &PreparedEditorSourceToolRelease,
    destination: &str,
    package: &str,
    required: Option<(&str, ContentDigest)>,
) -> (
    EditorSourceToolInstalledEntry,
    Arc<EditorSourceToolInstallation>,
) {
    let source = tempfile::tempdir().unwrap();
    for member in template.members() {
        let path = source.path().join(member.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, member.bytes()).unwrap();
    }
    let mut manifest: serde_json::Value =
        serde_json::from_slice(template.manifest_bytes()).unwrap();
    manifest["package"] = json!(package);
    manifest["requires"] = json!(
        required
            .into_iter()
            .map(|(package, digest)| json!({
                "package": package,
                "manifest": {
                    "length": digest.length(),
                    "blake3": digest.as_bytes().iter()
                        .map(|byte| format!("{byte:02x}")).collect::<String>(),
                },
            }))
            .collect::<Vec<_>>()
    );
    let manifest = serde_json::to_vec(&manifest).unwrap();
    fs::write(source.path().join("nara-tool.json"), &manifest).unwrap();
    let captured = PreparedEditorSourceToolRelease::capture(
        &open_explicit_product_project_root(source.path()).unwrap(),
        ContentDigest::of_bytes(&manifest),
    )
    .unwrap();
    let plan = EditorSourceToolInstallationPlan::new(
        project.share(),
        RelativeComponent::new(destination).unwrap(),
        captured,
    )
    .unwrap();
    let installation = match plan.publish() {
        EditorSourceToolInstallationOutcome::Published(installation) => installation,
        other => panic!("Actual managed graph installation failed: {other:?}"),
    };
    let inspected = EditorSourceToolInstallationInspection::read(
        installation.directory(),
        installation.receipt_digest(),
    )
    .unwrap();
    let entry = EditorSourceToolInstalledEntry {
        installed: EditorSourceToolInstalledSelection {
            package: inspected.release().package().into(),
            directory: installation.project_path().into(),
            installation: *inspected.receipt().installation_id(),
            receipt: inspected.receipt_digest(),
            manifest: inspected.release().manifest_digest(),
        },
        requirements: Vec::new(),
    };
    (entry, Arc::new(installation))
}

fn artifact_installation(
    entry: &EditorSourceToolInstalledEntry,
    owner: &EditorSourceToolInstallation,
) -> EditorSourceToolArtifact {
    EditorSourceToolArtifact {
        id: entry.installed.installation,
        package: entry.installed.package.clone(),
        directory: entry.installed.directory.clone(),
        manifest: entry.installed.manifest,
        publication: EditorSourceToolArtifactPublication::Ready {
            receipt: entry.installed.receipt,
        },
        members: owner
            .members()
            .iter()
            .map(|member| EditorSourceToolArtifactMember {
                path: member.path().into(),
                role: match member.kind() {
                    Some(EditorSourceToolReleaseMemberKind::Tool) => {
                        EditorSourceToolArtifactRole::Tool
                    }
                    Some(EditorSourceToolReleaseMemberKind::Content) => {
                        EditorSourceToolArtifactRole::Content
                    }
                    None => EditorSourceToolArtifactRole::Metadata,
                },
                baseline: member.digest(),
                observation: EditorSourceToolArtifactObservation::Present,
            })
            .collect(),
    }
}

fn assert_artifact_installations(
    actual: &[EditorSourceToolArtifact],
    expected: &[EditorSourceToolArtifact],
) {
    assert_eq!(actual.len(), expected.len(), "{actual:?}");
    for expected in expected {
        let artifact = actual
            .iter()
            .find(|artifact| artifact.id == expected.id)
            .unwrap_or_else(|| panic!("Missing exact artifact entry {expected:?}: {actual:?}"));
        assert_eq!(artifact.package, expected.package);
        assert_eq!(artifact.directory, expected.directory);
        assert_eq!(artifact.manifest, expected.manifest);
        assert_eq!(artifact.publication, expected.publication);
        assert_eq!(
            artifact.members.len(),
            expected.members.len(),
            "{artifact:?}"
        );
        for member in &expected.members {
            assert!(
                artifact.members.contains(member),
                "Missing complete artifact member {member:?}: {artifact:?}"
            );
        }
    }
}

fn assert_installed_bytes(project: &Project, expected: &BTreeMap<String, Vec<u8>>) {
    for (path, bytes) in expected {
        assert_eq!(
            &fs::read(project.root().join(path)).unwrap(),
            bytes,
            "{path}"
        );
    }
}

fn assert_artifact_outcome(
    view: &EditorSourceToolArtifactView,
    attempt: &EditorSourceToolAttemptId,
    outcome: EditorSourceToolArtifactOutcome,
) {
    assert_eq!(view.phase, EditorSourceToolArtifactPhase::Idle, "{view:?}");
    assert!(view.error.is_none(), "{view:?}");
    assert!(!view.physical_work_pending, "{view:?}");
    let operation = view.operation.as_ref().unwrap();
    assert_eq!(&operation.attempt, attempt);
    assert_eq!(operation.outcome.as_ref(), Some(&outcome));
}

fn binding(target: &EditorSourceToolInstalledEntry) -> EditorSourceToolRequirementBinding {
    EditorSourceToolRequirementBinding {
        package: target.installed.package.clone(),
        installation: target.installed.installation,
    }
}

fn request(entry: &EditorSourceToolInstalledEntry) -> EditorSourceToolInstallationRequest {
    EditorSourceToolInstallationRequest {
        directory: entry.installed.directory.clone(),
        receipt: entry.installed.receipt,
        requirements: entry.requirements.clone(),
    }
}

fn assert_installations(
    actual: &[EditorSourceToolInstalledEntry],
    expected: &[EditorSourceToolInstalledEntry],
) {
    assert_eq!(actual.len(), expected.len(), "{actual:?}");
    for entry in expected {
        assert!(
            actual.contains(entry),
            "Missing exact entry {entry:?}: {actual:?}"
        );
    }
}

fn assert_no_execution(editor: &EditorProjectSession) {
    let view = editor.source_tool_view();
    assert!(
        view.active.is_none() && !view.probing && !view.working && !view.retiring,
        "{view:?}",
    );
    assert!(editor.source_tool_review().is_none());
}

fn settle_selection(editor: &mut EditorProjectSession) -> EditorSourceToolSelectionView {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let view = editor.source_tool_selection_view();
        if view
            .operation
            .as_ref()
            .is_some_and(|operation| operation.outcome.is_some())
            && view
                .disable
                .as_ref()
                .is_none_or(|operation| operation.outcome.is_some())
            && matches!(
                view.phase,
                EditorSourceToolSelectionPhase::Idle
                    | EditorSourceToolSelectionPhase::Unavailable
                    | EditorSourceToolSelectionPhase::RequiresReconcile
            )
        {
            assert_eq!(view.phase, EditorSourceToolSelectionPhase::Idle, "{view:?}");
            assert!(view.error.is_none(), "{view:?}");
            return view;
        }
        assert!(
            Instant::now() < deadline,
            "Installed graph did not settle: {view:?}"
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn settle_artifacts(editor: &mut EditorProjectSession) -> EditorSourceToolArtifactView {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let view = editor.source_tool_artifact_view();
        if !view.physical_work_pending
            && view
                .operation
                .as_ref()
                .is_some_and(|operation| operation.outcome.is_some())
            && matches!(
                view.phase,
                EditorSourceToolArtifactPhase::Idle
                    | EditorSourceToolArtifactPhase::Unavailable
                    | EditorSourceToolArtifactPhase::RequiresReconcile
            )
        {
            return view;
        }
        assert!(
            Instant::now() < deadline,
            "Artifact inventory did not settle: {view:?}"
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn settle_tool(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        editor.drive_editor_frame(Duration::ZERO);
        let view = editor.source_tool_view();
        if !view.probing && !view.working && !view.retiring {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Actual managed child did not return: {view:?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn review(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
    intent: EditorSourceToolIntent,
    bonus: i64,
) -> EditorSourceToolRequestId {
    let activation = editor.source_tool_view().active.unwrap().id().clone();
    let basis = editor.document_query_basis(document).unwrap();
    let request = editor
        .request_source_tool(
            &activation,
            &basis,
            intent,
            BTreeMap::from([("bonus".into(), ComponentValue::I64(bonus))]),
        )
        .unwrap();
    settle_tool(editor);
    let view = editor.source_tool_view();
    assert_eq!(view.request.as_ref(), Some(&request));
    assert!(
        view.attempt_error.is_none() && view.request_error.is_none(),
        "{view:?}"
    );
    let review = editor.source_tool_review().unwrap();
    assert_eq!(review.request(), &request);
    assert!(!review.patch().operations.is_empty());
    assert!(!review.inverse().operations.is_empty());
    request
}

fn accept(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
    request: &EditorSourceToolRequestId,
) {
    let committed = editor.accept_source_tool_review(request).unwrap();
    assert!(committed.report.applied, "{:?}", committed.report);
    assert_eq!(committed.report.document, Some(document));
    assert!(committed.created_entity.is_none());
    editor.drive_editor_frame(Duration::ZERO);
}

fn assert_stats(editor: &EditorProjectSession, document: EditorDocumentId, health: i64) {
    for (field, value) in [("health", health), ("armor", 10)] {
        assert_value(
            editor,
            document,
            "enemy",
            STATS,
            field,
            ComponentValue::I64(value),
        );
    }
}
