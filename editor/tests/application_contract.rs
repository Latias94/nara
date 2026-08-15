#![cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]

use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use nara::{
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode},
    project_workflow::{
        NaraDependencySource, ProjectCreationRequest, ProjectDirectory, ProjectMember, ProjectSlug,
        prepare_desktop_2d_project,
    },
    reflect::{ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind},
    scene::{
        SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
        ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorCloseDecision, EditorDocumentMutation,
        EditorFieldFeedbackReason, EditorFieldTarget, EditorFieldValidationFailure,
        EditorNavigationAction, EditorWorkspaceIntent, EditorWorkspaceIntentAction,
        EditorWorkspaceIntentPhase, EditorWorkspaceIntentResult, SceneInspectorCommand,
    },
};
use nara_editor::{EditorApplication, EditorApplicationOpenError, EditorLaunchOptions};

const NARA_GIT_REVISION: &str = "0123456789abcdef0123456789abcdef01234567";
static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(1);

#[test]
fn starter_opens_one_coherent_frame_and_empty_repaint_preserves_document_state() {
    let project = StarterProject::new("open-and-repaint");
    let mut application = EditorApplication::open(EditorLaunchOptions::new(project.root()))
        .expect("the canonical starter should open in the Editor");
    let before = application.current_frame();
    let before_scene = before
        .workspace()
        .active_scene
        .as_ref()
        .expect("the starter opens its configured startup Scene");
    let document = before_scene.document;
    let revision = before_scene.revision;
    let dirty = before_scene.dirty;
    let selection = before_scene.selection.clone();

    let batch = render_once(&mut application);
    let update = application.submit_and_drive(batch, Duration::ZERO);
    let after = application.current_frame();
    let after_scene = after
        .workspace()
        .scene(document)
        .expect("an empty redraw must retain the startup Scene");

    assert!(!update.actions_observed());
    assert!(!update.exit_ready());
    assert_eq!(after_scene.revision, revision);
    assert_eq!(after_scene.dirty, dirty);
    assert_eq!(
        after.workspace().active_scene.as_ref().unwrap().selection,
        selection
    );
    assert_ne!(after.revision(), before.revision());
}

#[test]
fn dirty_exit_cancel_preserves_the_session_and_discard_closes_it() {
    let project = StarterProject::new("dirty-close");
    let mut application = EditorApplication::open(EditorLaunchOptions::new(project.root()))
        .expect("the canonical starter should open in the Editor");
    let frame = application.current_frame();
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    let patch = ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(SceneEntityId::new("dirty-probe").unwrap()),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }]);
    let batch = EditorActionBatch::new(
        frame.revision(),
        [EditorAction::Mutate(
            EditorDocumentMutation::ApplyScenePatch {
                document: scene.document,
                base_revision: scene.revision,
                patch,
            },
        )],
    );
    application.submit_and_drive(batch, Duration::ZERO);
    assert!(
        application
            .current_frame()
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .dirty
    );

    let close_request = application.request_exit();
    let awaiting_cancel = application.current_frame();
    assert!(!close_request.exit_ready());
    assert_eq!(
        awaiting_cancel.workspace_intent().phase(),
        Some(EditorWorkspaceIntentPhase::AwaitingDecision)
    );

    let cancel = EditorActionBatch::new(
        awaiting_cancel.revision(),
        [EditorAction::WorkspaceIntent(
            EditorWorkspaceIntentAction::Resolve(EditorCloseDecision::Cancel),
        )],
    );
    let cancelled = application.submit_and_drive(cancel, Duration::ZERO);
    let cancelled_frame = application.current_frame();
    assert!(!cancelled.exit_ready());
    assert!(cancelled_frame.workspace().active_scene.is_some());
    assert!(matches!(
        cancelled_frame.workspace_intent().result(),
        Some(EditorWorkspaceIntentResult::Cancelled {
            intent: EditorWorkspaceIntent::Exit,
        })
    ));

    let second_request = application.request_exit();
    let awaiting_discard = application.current_frame();
    assert!(!second_request.exit_ready());
    assert_eq!(
        awaiting_discard.workspace_intent().phase(),
        Some(EditorWorkspaceIntentPhase::AwaitingDecision)
    );
    let discard = EditorActionBatch::new(
        awaiting_discard.revision(),
        [EditorAction::WorkspaceIntent(
            EditorWorkspaceIntentAction::Resolve(EditorCloseDecision::Discard),
        )],
    );
    let discarded = application.submit_and_drive(discard, Duration::ZERO);

    assert!(discarded.exit_ready());
    assert!(application.current_frame().workspace().scenes.is_empty());
}

#[test]
fn field_failures_publish_inline_feedback_and_durable_diagnostics() {
    let project = StarterProject::new("field-feedback");
    let mut application = EditorApplication::open(EditorLaunchOptions::new(project.root()))
        .expect("the canonical starter should open in the Editor");
    let frame = application.current_frame();
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    let entity = scene.editor.inspector.entities[0].id.clone();
    let select = EditorActionBatch::new(
        frame.revision(),
        [EditorAction::Navigate(
            EditorNavigationAction::SelectEntity {
                document: scene.document,
                entity: Some(entity.clone()),
            },
        )],
    );
    application.submit_and_drive(select, Duration::ZERO);

    let selected_frame = application.current_frame();
    let selected_scene = selected_frame.workspace().active_scene.as_ref().unwrap();
    let selected = selected_scene
        .editor
        .inspector
        .selected_entity_view
        .as_ref()
        .unwrap();
    let (component, field) = selected
        .components
        .iter()
        .find_map(|component| {
            component
                .fields
                .iter()
                .find(|field| field.value_kind == ComponentValueKind::F64)
                .map(|field| (component, field))
        })
        .expect("the starter camera exposes one editable floating-point field");
    let target = EditorFieldTarget::new(
        selected_scene.document,
        selected_scene.revision,
        entity.clone(),
        component.component.clone(),
        field.id.clone(),
    );
    let failure = EditorFieldValidationFailure::new(target.clone(), ComponentValueKind::F64);
    let invalid = EditorActionBatch::new(
        selected_frame.revision(),
        [EditorAction::ReportFieldValidation(failure.clone())],
    );
    application.submit_and_drive(invalid, Duration::ZERO);

    let invalid_frame = application.current_frame();
    let feedback = invalid_frame.field_feedback().unwrap();
    assert!(feedback.target().same_field(&target));
    assert_eq!(
        feedback.reason(),
        EditorFieldFeedbackReason::InvalidValue(ComponentValueKind::F64)
    );
    assert!(has_diagnostic(&invalid_frame, "editor.field.invalid-value"));

    let current_scene = invalid_frame.workspace().active_scene.as_ref().unwrap();
    let rejected_mutation = EditorActionBatch::new(
        invalid_frame.revision(),
        [EditorAction::Mutate(
            EditorDocumentMutation::ApplyInspectorCommand {
                document: current_scene.document,
                base_revision: current_scene.revision,
                command: SceneInspectorCommand::SetField {
                    entity,
                    component: component.component.clone(),
                    component_version: component.schema_version,
                    field: field.id.clone(),
                    value: ComponentValue::String("not-a-number".to_owned()),
                },
            },
        )],
    );
    application.submit_and_drive(rejected_mutation, Duration::ZERO);
    let rejected_frame = application.current_frame();
    assert_eq!(
        rejected_frame.field_feedback().unwrap().reason(),
        EditorFieldFeedbackReason::MutationRejected
    );
    assert!(rejected_frame.diagnostics().stats().observed_errors() >= 2);

    let stale_source = application.current_frame();
    application.submit_and_drive(
        EditorActionBatch::new(stale_source.revision(), []),
        Duration::ZERO,
    );
    application.submit_and_drive(
        EditorActionBatch::new(
            stale_source.revision(),
            [EditorAction::ReportFieldValidation(failure)],
        ),
        Duration::ZERO,
    );
    let stale_frame = application.current_frame();
    let feedback = stale_frame.field_feedback().unwrap();
    assert!(feedback.target().same_field(&target));
    assert_eq!(feedback.reason(), EditorFieldFeedbackReason::StaleAction);
    assert!(has_diagnostic(&stale_frame, "editor.action.stale-frame"));
}

#[test]
fn invalid_project_inputs_fail_before_an_application_is_created() {
    let missing = unique_test_path("missing-root");
    let error = EditorApplication::open(EditorLaunchOptions::new(&missing)).unwrap_err();
    assert!(matches!(
        error,
        EditorApplicationOpenError::ProjectRootOpen { path, .. } if path == missing
    ));

    let missing_startup = StarterProject::new("missing-startup");
    fs::remove_file(missing_startup.root().join("scenes/startup.scene.json")).unwrap();
    let error = EditorApplication::open(EditorLaunchOptions::new(missing_startup.root()))
        .expect_err("a missing configured startup Scene must fail before presentation");
    assert!(
        error
            .diagnostics()
            .is_some_and(|report| report.has_errors())
    );

    let unavailable_schema = StarterProject::new("unavailable-schema");
    let entity = SceneEntityRecord::new(SceneEntityId::new("unknown-schema").unwrap())
        .with_component(
            ComponentTypeId::new("example.Missing"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null),
        );
    let scene = SceneDocument::from_ordered_roots([entity]);
    fs::write(
        unavailable_schema.root().join("scenes/startup.scene.json"),
        scene.to_json_string().unwrap(),
    )
    .unwrap();
    let error = EditorApplication::open(EditorLaunchOptions::new(unavailable_schema.root()))
        .expect_err("complete-schema-only Editor opening must reject unknown components");
    assert!(
        error
            .diagnostics()
            .is_some_and(|report| report.has_errors())
    );
}

fn render_once(application: &mut EditorApplication) -> EditorActionBatch {
    let context = egui::Context::default();
    let mut input = egui::RawInput::default();
    input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(1200.0, 800.0),
    ));
    let mut batch = None;
    let output = context.run_ui(input, |ui| {
        batch = Some(application.show(ui));
    });
    output.drop_without_applying_deltas();
    batch.expect("the fixed Editor panel returns one action batch")
}

fn has_diagnostic(frame: &nara::tooling::EditorFrameModel, code: &str) -> bool {
    frame
        .diagnostics()
        .iter()
        .any(|entry| entry.code().as_str() == code)
}

struct StarterProject {
    parent: PathBuf,
    root: PathBuf,
}

impl StarterProject {
    fn new(label: &str) -> Self {
        let parent = unique_test_path(label);
        fs::create_dir(&parent).unwrap();
        let slug = ProjectSlug::new("editor-app").unwrap();
        let prepared = prepare_desktop_2d_project(ProjectCreationRequest::new(
            directory_capability(&parent),
            slug,
            NaraDependencySource::immutable_git(
                "https://github.com/nara-engine/nara.git",
                NARA_GIT_REVISION,
            )
            .unwrap(),
        ))
        .unwrap();
        let root = parent.join("editor-app");
        fs::create_dir(&root).unwrap();
        for directory in prepared.plan().directories() {
            fs::create_dir_all(root.join(directory_path(*directory))).unwrap();
        }
        for member in prepared.plan().members() {
            fs::write(root.join(member_path(member.member())), member.bytes()).unwrap();
        }
        Self { parent, root }
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for StarterProject {
    fn drop(&mut self) {
        let temporary_root = std::env::temp_dir().canonicalize().unwrap();
        let parent = self.parent.canonicalize().unwrap();
        assert!(parent.starts_with(&temporary_root));
        assert!(
            parent
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("nara-editor-application-"))
        );
        fs::remove_dir_all(parent).unwrap();
    }
}

fn unique_test_path(label: &str) -> PathBuf {
    let sequence = NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "nara-editor-application-{label}-{}-{sequence}",
        std::process::id()
    ))
}

fn directory_path(directory: ProjectDirectory) -> &'static str {
    match directory {
        ProjectDirectory::Assets => "assets",
        ProjectDirectory::Prefabs => "prefabs",
        ProjectDirectory::Scenes => "scenes",
        ProjectDirectory::Source => "src",
        ProjectDirectory::SourceBinaries => "src/bin",
    }
}

fn member_path(member: ProjectMember) -> &'static str {
    match member {
        ProjectMember::CargoManifest => "Cargo.toml",
        ProjectMember::ProjectManifest => "nara.toml",
        ProjectMember::AuthorGuide => "README.md",
        ProjectMember::LibrarySource => "src/lib.rs",
        ProjectMember::HeadlessSource => "src/bin/headless.rs",
        ProjectMember::DesktopSource => "src/bin/desktop.rs",
        ProjectMember::StartupScene => "scenes/startup.scene.json",
    }
}

fn directory_capability(path: &Path) -> DirectoryCapability {
    DirectoryCapability::from_host_handle(
        open_directory(path),
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .unwrap()
}

#[cfg(windows)]
fn open_directory(path: &Path) -> File {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .unwrap()
}

#[cfg(unix)]
fn open_directory(path: &Path) -> File {
    File::open(path).unwrap()
}
