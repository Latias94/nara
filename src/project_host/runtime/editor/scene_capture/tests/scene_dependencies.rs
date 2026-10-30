//! Scene dependency working-copy currentness through the actual Editor capture owner.

use nara_asset::{
    AssetMeta, AssetMetaIdentity, AssetProductRef, AssetSourceKind, ImportedProductId,
    StableAssetId,
};
use nara_reflect::{
    AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldPath, ComponentFieldSchema,
    ComponentSchema, ComponentSchemaCatalog, ComponentValueKind, PersistentComponentSchemaOwnerId,
    encode_asset_reference_value,
};

use super::*;

const DEPENDENT: &str = "scenes/dependent.scene.json";
const UNRELATED: &str = "scenes/unrelated.scene.json";

fn scene_fixture() -> Fixture {
    let fixture = Fixture::new();
    let owner = PersistentComponentSchemaOwnerId::try_new("nara.test.scene-links").unwrap();
    let component = ComponentTypeId::new("nara.test.SceneLink");
    let schema = ComponentSchema::new(component.clone(), "SceneLink", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("scene"),
            "Scene",
            ComponentFieldPath::from_fields(["scene"]),
            ComponentValueKind::AssetRef,
        )
        .with_capability(ComponentCapability::AssetRef)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    let bundle =
        AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())
            .unwrap();
    let mut owners = bundle
        .owners()
        .iter()
        .map(AuthoringSchemaBundleOwner::from)
        .collect::<Vec<_>>();
    owners.push(AuthoringSchemaBundleOwner::new(
        owner.clone(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    ));
    fs::write(
        fixture.root.join("schema/authoring.bundle.json"),
        AuthoringSchemaBundle::new(owners)
            .unwrap()
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let id: StableAssetId = "4ce173cb-ff14-4bbf-9cee-8b72882aed87".parse().unwrap();
    fs::write(
        fixture.root.join(format!("{DEPENDENT}.meta")),
        AssetMeta::new(
            AssetMetaIdentity::new(
                id,
                AssetPath::new("dependent.scene.json").unwrap(),
                AssetSourceKind::Scene,
            ),
            (),
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let mut root = SceneDocument::from_ordered_roots(leaf(INITIAL_ROTATION).entities);
    root.entities[0].components.insert(
        component,
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([(
                "scene",
                encode_asset_reference_value(&AssetRef::Product(
                    AssetProductRef::new(id, ImportedProductId::PRIMARY).unwrap(),
                )),
            )]),
        )
        .with_owner(owner),
    );
    for (path, scene) in [
        (SECOND, root),
        (
            DEPENDENT,
            SceneDocument::from_ordered_roots(leaf(0.5).entities),
        ),
        (
            UNRELATED,
            SceneDocument::from_ordered_roots(leaf(0.6).entities),
        ),
    ] {
        fs::write(fixture.root.join(path), scene.to_json_string().unwrap()).unwrap();
    }
    fixture
}

fn selected_scene_revision(
    editor: &EditorProjectSession,
    path: &str,
) -> Option<SceneAuthoringRevision> {
    editor
        .scene_capture
        .captured
        .as_ref()
        .unwrap()
        .source_capture
        .as_ref()
        .unwrap()
        .scenes()
        .iter()
        .find(|scene| scene.source().project_path().as_str() == path)
        .unwrap()
        .session_revision()
}

#[test]
fn demanded_scene_open_edit_save_reopen_and_close_revoke_the_dependent_stamp() {
    let fixture = scene_fixture();
    fixture.run(|editor| {
        let root = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        let root_revision = editor.workspace.document_revision(root).unwrap();
        editor.configure_editor_preview(Extent2d::new(320, 180));
        let initial = wait_preview(editor, root, None);
        let dependent = open_document(editor, DEPENDENT, AuthoringDocumentKind::Scene);
        assert_eq!(open_document(editor, SECOND, AuthoringDocumentKind::Scene), root);
        assert!(editor.current_editor_preview_content_stamp().is_none());
        let opened = wait_preview(editor, root, Some(&initial.content_stamp()));
        edit_rotation(editor, dependent, 0.75);
        assert!(editor.current_editor_preview_content_stamp().is_none());
        let edited = wait_preview(editor, root, Some(&opened.content_stamp()));
        assert_eq!(selected_scene_revision(editor, DEPENDENT), editor.workspace.document_revision(dependent));
        assert_eq!(editor.current_frame().scene_preview_unsaved_dependencies(), 1);
        assert_eq!(editor.workspace.document_revision(root), Some(root_revision));
        assert_eq!(editor.workspace.document_is_dirty(root), Some(false));
        assert!(matches!(editor.request_persistence(EditorPersistenceCommand::Save { document: Some(dependent) }),
            EditorPersistenceRequestResult::Accepted { .. }));
        assert!(matches!(settle_persistence(editor), EditorPersistenceResult::Saved { document, .. } if document == dependent));
        acknowledge_persistence(editor);
        assert!(editor.current_editor_preview_content_stamp().is_none());
        let saved = wait_preview(editor, root, Some(&edited.content_stamp()));
        assert_eq!(editor.current_frame().scene_preview_unsaved_dependencies(), 0);
        edit_rotation(editor, dependent, 1.25);
        let dirty = wait_preview(editor, root, Some(&saved.content_stamp()));
        let before_reopen = editor.workspace.document_revision(dependent).unwrap();
        assert!(matches!(editor.request_persistence(EditorPersistenceCommand::Reopen { document: Some(dependent) }),
            EditorPersistenceRequestResult::Accepted { .. }));
        assert!(matches!(settle_persistence(editor), EditorPersistenceResult::Opened { document, .. } if document == dependent));
        acknowledge_persistence(editor);
        assert_ne!(editor.workspace.document_revision(dependent).unwrap().source_id(), before_reopen.source_id());
        assert!(editor.current_editor_preview_content_stamp().is_none());
        let reopened = wait_preview(editor, root, Some(&dirty.content_stamp()));
        assert_eq!(editor.request_workspace_intent(EditorWorkspaceIntent::CloseDocument { document: dependent }),
            EditorWorkspaceIntentRequestResult::Accepted);
        let deadline = Instant::now() + WAIT;
        while editor.workspace.document_kind(dependent).is_some() {
            editor.advance_workspace_retirement();
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        assert!(editor.acknowledge_workspace_intent_result());
        assert!(editor.current_editor_preview_content_stamp().is_none());
        wait_preview(editor, root, Some(&reopened.content_stamp()));
        assert_eq!(selected_scene_revision(editor, DEPENDENT), None);
        assert_eq!(editor.workspace.document_revision(root), Some(root_revision));
    });
}

#[test]
fn save_root_persists_only_root_and_unrelated_open_scene_edits_leave_its_stamp_current() {
    let fixture = scene_fixture();
    let disk_dependent = fs::read(fixture.root.join(DEPENDENT)).unwrap();
    fixture.run(|editor| {
        let root = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        let dependent = open_document(editor, DEPENDENT, AuthoringDocumentKind::Scene);
        let unrelated = open_document(editor, UNRELATED, AuthoringDocumentKind::Scene);
        open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        editor.configure_editor_preview(Extent2d::new(320, 180));
        let initial = wait_preview(editor, root, None);
        edit_rotation(editor, unrelated, 1.5);
        assert_eq!(editor.current_editor_preview_content_stamp(), Some(initial.content_stamp()));
        edit_rotation(editor, dependent, 1.25);
        edit_rotation(editor, root, 0.75);
        let changed = wait_preview(editor, root, Some(&initial.content_stamp()));
        assert!(matches!(editor.request_persistence(EditorPersistenceCommand::Save { document: Some(root) }),
            EditorPersistenceRequestResult::Accepted { .. }));
        assert!(matches!(settle_persistence(editor), EditorPersistenceResult::Saved { document, .. } if document == root));
        acknowledge_persistence(editor);
        wait_preview(editor, root, Some(&changed.content_stamp()));
        assert_eq!(editor.workspace.document_is_dirty(root), Some(false));
        assert_eq!(editor.workspace.document_is_dirty(dependent), Some(true));
        assert_eq!(editor.current_frame().scene_preview_unsaved_dependencies(), 1);
        assert_eq!(fs::read(fixture.root.join(DEPENDENT)).unwrap(), disk_dependent);
    });
}

#[test]
fn completed_scene_dependency_capture_rejects_a_newer_working_revision_before_materialization() {
    let fixture = scene_fixture();
    fixture.run(|editor| {
        let root = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        let dependent = open_document(editor, DEPENDENT, AuthoringDocumentKind::Scene);
        open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        editor.configure_editor_preview(Extent2d::new(320, 180));
        let initial = wait_preview(editor, root, None);
        let mut gate = CompletionGate::hold_next(editor);
        edit_rotation(editor, dependent, 0.75);
        editor.drive_editor_frame(Duration::ZERO);
        gate.wait();
        edit_rotation(editor, dependent, 1.25);
        assert!(editor.current_editor_preview_content_stamp().is_none());
        gate.release();
        wait_preview(editor, root, Some(&initial.content_stamp()));
        assert_eq!(
            selected_scene_revision(editor, DEPENDENT),
            editor.workspace.document_revision(dependent)
        );
        assert_eq!(
            editor.current_frame().scene_preview_unsaved_dependencies(),
            1
        );
    });
}

#[test]
fn unrelated_scene_edit_during_pending_capture_preserves_the_original_work() {
    let fixture = scene_fixture();
    fixture.run(|editor| {
        let root = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        let dependent = open_document(editor, DEPENDENT, AuthoringDocumentKind::Scene);
        let unrelated = open_document(editor, UNRELATED, AuthoringDocumentKind::Scene);
        open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        editor.configure_editor_preview(Extent2d::new(320, 180));
        let initial = wait_preview(editor, root, None);
        let mut gate = CompletionGate::hold_next(editor);
        edit_rotation(editor, dependent, 0.75);
        editor.drive_editor_frame(Duration::ZERO);
        gate.wait();
        let generation = editor.scene_capture.generation;
        edit_rotation(editor, unrelated, 1.25);
        for _ in 0..4 {
            editor.drive_editor_frame(Duration::ZERO);
            assert!(!editor.scene_capture.pending_stale);
            assert_eq!(editor.scene_capture.generation, generation);
            assert!(editor.current_editor_preview_content_stamp().is_none());
        }
        gate.release();
        wait_preview(editor, root, Some(&initial.content_stamp()));
        assert_eq!(
            selected_scene_revision(editor, DEPENDENT),
            editor.workspace.document_revision(dependent)
        );
        assert_eq!(
            editor.current_frame().scene_preview_unsaved_dependencies(),
            1
        );
    });
}

#[test]
fn accepted_capture_retires_unrelated_inputs_and_keeps_its_exact_observation_allowance() {
    let fixture = scene_fixture();
    fixture.run(|editor| {
        let root = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        let dependent = open_document(editor, DEPENDENT, AuthoringDocumentKind::Scene);
        let unrelated = open_document(editor, UNRELATED, AuthoringDocumentKind::Scene);
        let prefab = open_document(
            editor,
            "prefabs/unrelated.prefab.json",
            AuthoringDocumentKind::Prefab,
        );
        open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        editor.configure_editor_preview(Extent2d::new(320, 180));
        let initial = wait_preview(editor, root, None);
        let mut gate = CompletionGate::hold_next(editor);
        edit_rotation(editor, dependent, 0.75);
        editor.drive_editor_frame(Duration::ZERO);
        gate.wait();
        let Some(CaptureWork::Compose { selection, .. }) = editor.scene_capture.pending.as_ref()
        else {
            panic!("the single input selection must remain with its physical task");
        };
        assert!(
            selection
                .scenes
                .iter()
                .any(|source| source.document == unrelated)
        );
        assert!(
            selection
                .prefabs
                .iter()
                .any(|source| source.document == prefab)
        );
        gate.release();
        let accepted = wait_preview(editor, root, Some(&initial.content_stamp()));
        let captured = editor.scene_capture.captured.as_ref().unwrap();
        assert_eq!(captured.root.document, root);
        assert_eq!(captured.sources.len(), 1);
        assert_eq!(
            captured.sources[0].selected.as_ref().unwrap().document,
            dependent
        );
        assert_eq!(captured.unsaved_dependencies, 1);
        let source_capture = captured.source_capture.as_ref().unwrap();
        let baseline = source_capture.usage().retained_candidate_bytes();
        let expected = observation_cost(
            &CaptureSelection {
                root: captured.root.clone(),
                prefabs: Vec::new(),
                scenes: vec![captured.sources[0].selected.as_ref().unwrap().clone()],
            },
            demanded_sources(source_capture, &captured.root.source_key),
            source_capture.diagnostics().retained_bytes(),
        )
        .unwrap()
        .retained_bytes;
        assert_eq!(source_capture.host_retained_bytes(), expected);
        editor.scene_capture.validate_at = None;
        assert_eq!(
            editor.request_workspace_intent(EditorWorkspaceIntent::CloseDocument {
                document: unrelated
            }),
            EditorWorkspaceIntentRequestResult::Accepted
        );
        let deadline = Instant::now() + WAIT;
        while editor.workspace.document_kind(unrelated).is_some() {
            editor.advance_workspace_retirement();
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        assert!(editor.acknowledge_workspace_intent_result());
        assert_eq!(
            editor.current_editor_preview_content_stamp(),
            Some(accepted.content_stamp())
        );
        assert_eq!(
            editor
                .scene_capture
                .captured
                .as_ref()
                .unwrap()
                .source_capture
                .as_ref()
                .unwrap()
                .usage()
                .retained_candidate_bytes(),
            baseline
        );
    });
}

#[test]
fn startup_observations_use_complete_project_retention_at_exact_and_one_short_limits() {
    let fixture = Fixture::new();
    let editor = fixture.open_editor_with_limits(AuthoringProjectAdmissionLimits::default());
    let document = editor.scene_preview_document.unwrap();
    let project_retained = editor
        .authoring_project()
        .unwrap()
        .usage()
        .retained_candidate_bytes();
    let root_bytes = editor
        .authoring_source_selection_retained_bytes(document)
        .unwrap();
    let selection = CaptureSelection {
        root: editor.authoring_source_selection(document).unwrap(),
        prefabs: Vec::new(),
        scenes: Vec::new(),
    };
    let output_bytes = observation_cost(&selection, std::iter::empty(), 0)
        .unwrap()
        .retained_bytes;
    drop(selection);
    assert!(editor.scene_capture.captured.is_some());
    fixture.run_editor(editor, |_| {});
    for (maximum, accepted) in [
        (project_retained + root_bytes - 1, false),
        (project_retained + output_bytes, true),
    ] {
        let editor = fixture.open_editor_with_limits(
            AuthoringProjectAdmissionLimits::default()
                .with_retained_candidate_bytes(nara_core::ByteLimit::new(maximum).unwrap()),
        );
        fixture.run_editor(editor, |editor| {
            assert_eq!(
                editor
                    .authoring_project()
                    .unwrap()
                    .usage()
                    .retained_candidate_bytes(),
                project_retained
            );
            assert_eq!(editor.scene_capture.captured.is_some(), accepted);
            if !accepted {
                assert!(
                    editor
                        .scene_capture
                        .failure
                        .as_ref()
                        .unwrap()
                        .iter()
                        .any(|diagnostic| diagnostic.code().as_str()
                            == "project.editor.scene-capture-budget-exceeded")
                );
            }
        });
    }
}

#[test]
fn closing_the_selected_scene_drains_cancelled_capture_after_its_physical_return() {
    for hidden in [false, true] {
        let fixture = scene_fixture();
        fixture.run(|editor| {
            let root = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
            let dependent = open_document(editor, DEPENDENT, AuthoringDocumentKind::Scene);
            open_document(editor, SECOND, AuthoringDocumentKind::Scene);
            editor.configure_editor_preview(Extent2d::new(320, 180));
            wait_preview(editor, root, None);
            let mut gate = CompletionGate::hold_next(editor);
            edit_rotation(editor, dependent, 0.75);
            editor.drive_editor_frame(Duration::ZERO);
            gate.wait();
            if hidden {
                editor.configure_editor_preview(None);
                assert!(editor.scene_capture.pending_stale);
            }
            assert_eq!(
                editor.request_workspace_intent(EditorWorkspaceIntent::CloseDocument {
                    document: root
                }),
                EditorWorkspaceIntentRequestResult::Accepted
            );
            let deadline = Instant::now() + WAIT;
            while editor.workspace.document_kind(root).is_some() {
                editor.advance_workspace_retirement();
                assert!(Instant::now() < deadline);
                thread::yield_now();
            }
            assert!(editor.acknowledge_workspace_intent_result());
            assert!(editor.scene_preview_document.is_none());
            for _ in 0..3 {
                let frame = editor.drive_editor_frame(Duration::ZERO);
                assert!(frame.scene_preview_work_pending());
                assert_eq!(frame.scene_preview_recheck_after(), None);
                assert!(editor.scene_capture.has_pending_work());
                assert!(editor.scene_capture.pending_stale);
                assert!(
                    editor
                        .scene_capture
                        .collect_reclamation_sources(&mut Vec::new())
                        .is_err()
                );
                assert!(editor.current_editor_preview_content_stamp().is_none());
            }
            gate.release();
            let deadline = Instant::now() + WAIT;
            while editor.scene_capture.has_pending_work() {
                editor.drive_editor_frame(Duration::ZERO);
                assert!(
                    Instant::now() < deadline,
                    "the cancelled capture did not return without a selected Scene"
                );
                thread::yield_now();
            }
            assert!(editor.scene_capture.captured.is_none());
            assert!(
                editor
                    .scene_capture
                    .collect_reclamation_sources(&mut Vec::new())
                    .is_ok()
            );
            assert!(!editor.scene_capture.schema_adoption_pending());
            assert_eq!(editor.scene_capture.recheck_after(), None);
            assert!(!editor.current_frame().scene_preview_work_pending());
            assert!(editor.current_editor_preview_content_stamp().is_none());
        });
    }
}

#[test]
fn input_admission_failure_after_an_accepted_root_waits_for_retry_or_a_new_root_revision() {
    let fixture = scene_fixture();
    // Startup also admits the real Image identity store. The open working set, rather than
    // that independent owner, must be the operation that exceeds this limit.
    let maximum = nara_core::ByteLimit::new(32 * 1024 * 1024).unwrap();
    let large = SceneDocument::from_ordered_roots((0..8).map(|index| {
        SceneEntityRecord::new(SceneEntityId::new(format!("large-{index}")).unwrap())
            .with_component(
                ComponentTypeId::new("test.UnavailableLargeInput"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::String("x".repeat(896 * 1024)),
                ),
            )
    }));
    let encoded = large.to_json_string().unwrap();
    for index in 0..5 {
        fs::write(
            fixture
                .root
                .join(format!("scenes/large-{index}.scene.json")),
            &encoded,
        )
        .unwrap();
    }
    drop(encoded);
    drop(large);
    let editor = fixture.editor_with_limits(
        AuthoringProjectAdmissionLimits::default().with_retained_candidate_bytes(maximum),
    );
    fixture.run_editor(editor, |editor| {
        let root = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        editor.configure_editor_preview(Extent2d::new(320, 180));
        wait_preview(editor, root, None);
        assert!(editor.scene_capture.captured.is_some());
        for index in 0..5 {
            open_document(
                editor,
                &format!("scenes/large-{index}.scene.json"),
                AuthoringDocumentKind::Scene,
            );
        }
        open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        edit_rotation(editor, root, 0.75);
        editor.drive_authoring_scene_capture();
        assert!(
            editor
                .scene_capture
                .failure
                .as_ref()
                .unwrap()
                .iter()
                .any(|diagnostic| diagnostic.code().as_str()
                    == "project.editor.scene-capture-budget-exceeded")
        );
        assert!(editor.scene_capture.captured.is_none());
        assert!(editor.scene_capture.failed_attempt.is_some());
        assert!(!editor.scene_capture.has_pending_work());
        let failed_generation = editor.scene_capture.generation;
        let retry_at = editor.scene_capture.retry_at.unwrap();
        assert!(retry_at > Instant::now());
        for _ in 0..4 {
            editor.drive_authoring_scene_capture();
            assert_eq!(editor.scene_capture.generation, failed_generation);
            assert_eq!(editor.scene_capture.retry_at, Some(retry_at));
        }
        editor.scene_capture.retry_at = Some(Instant::now());
        editor.drive_authoring_scene_capture();
        assert_eq!(editor.scene_capture.generation, failed_generation + 1);
        let retried_generation = editor.scene_capture.generation;
        let retry_at = editor.scene_capture.retry_at;
        editor.drive_authoring_scene_capture();
        assert_eq!(editor.scene_capture.generation, retried_generation);
        assert_eq!(editor.scene_capture.retry_at, retry_at);
        edit_rotation(editor, root, 1.25);
        editor.drive_authoring_scene_capture();
        assert_eq!(editor.scene_capture.generation, retried_generation + 1);
    });
}
