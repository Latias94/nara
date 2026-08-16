#![cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]

use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use egui::{Key, Modifiers, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable as _};
use nara::{
    ProductRecipe,
    fs::{
        CapabilityRights, ContentDigest, DirectoryCapability, HostCapabilityOptions, RelativePath,
        TrustMode,
    },
    project_workflow::{
        LocalDependencyParent, NaraDependencySource, ProjectCheckRequest, ProjectCreationOutcome,
        ProjectCreationRequest, ProjectSlug, StartupContentMemberKind, check_project,
        prepare_desktop_2d_project,
    },
    reflect::{ComponentFieldId, ComponentTypeId, ComponentValue},
    render::Extent2d,
    scene::SceneEntityId,
    tooling::{
        EditorAction, EditorActionBatch, EditorDocumentMutation, EditorFieldFeedbackReason,
        EditorPersistenceAction, EditorPersistenceResult, EditorWorkspaceIntent,
        EditorWorkspaceIntentAction, EditorWorkspaceIntentResult, SceneInspectorCommand,
    },
};
use nara_editor::{EditorApplication, EditorLaunchOptions};
use serde_json::Value;

const PROJECT_SLUG: &str = "editor-authoring-loop";
const CREATED_ENTITY: &str = "editor_entity_1";
const TRANSFORM_COMPONENT: &str = "nara.transform.Transform2d";
const TRANSLATION_X_FIELD: &str = "translation.x";
const EDITED_TRANSLATION_X: f64 = 128.0;
const STALE_TRANSLATION_X: f64 = 256.0;
const EXPORT_DESTINATION: &str = "editor-authoring-candidate";
const PRODUCT_OBSERVATION_ATTEMPT_ENV: &str = "NARA_STARTER_OBSERVATION_ATTEMPT";
const PRODUCT_OBSERVATION_FILE_ENV: &str = "NARA_STARTER_OBSERVATION_FILE";
const EXPORT_OBSERVATION_ATTEMPT_ENV: &str = "NARA_EXPORT_OBSERVATION_ATTEMPT";
const EXPORT_OBSERVATION_FILE_ENV: &str = "NARA_EXPORT_OBSERVATION_FILE";
static NEXT_TRAJECTORY: AtomicU64 = AtomicU64::new(1);

#[test]
#[ignore = "runs one nested Cargo build, generated product, and real Winit/WGPU export acceptance"]
fn starter_edit_crosses_editor_check_generated_product_and_export() {
    let root = TrajectoryRoot::new();
    let project = create_project(&root);
    let scene_path = project.join("scenes/startup.scene.json");

    let mut application = EditorApplication::open(EditorLaunchOptions::new(&project))
        .expect("the published starter must open in the private Editor application");
    application.set_scene_preview_extent(Extent2d::new(960, 540));
    let opened = application.current_frame();
    let opened_scene = opened
        .workspace()
        .active_scene
        .as_ref()
        .expect("the starter opens its configured startup Scene");
    let document = opened_scene.document;
    let initial_revision = opened_scene.revision;
    assert!(!opened_scene.dirty);

    let mut harness = editor_harness(application);
    harness.get_by_label("+ Root").click();
    harness.run();
    let composed = harness.state().current_frame();
    let composed_scene = composed.workspace().active_scene.as_ref().unwrap();
    let composed_revision = composed_scene.revision;
    let created_entity = SceneEntityId::new(CREATED_ENTITY).unwrap();
    assert_ne!(composed_revision, initial_revision);
    assert_eq!(
        composed_scene.editor.inspector.selected_entity.as_ref(),
        Some(&created_entity)
    );
    assert!(composed_scene.dirty);
    assert_eq!(transform_x_field(&composed).value, 0.0);

    harness
        .get_by_role_and_label(Role::TextInput, TRANSLATION_X_FIELD)
        .focus();
    harness.run();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.run();
    harness
        .get_by_role_and_label(Role::TextInput, TRANSLATION_X_FIELD)
        .type_text("128");
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();

    let edited = harness.state().current_frame();
    let edited_scene = edited.workspace().active_scene.as_ref().unwrap();
    let edited_revision = edited_scene.revision;
    let field = transform_x_field(&edited);
    assert_ne!(edited_revision, composed_revision);
    assert!(edited_scene.dirty);
    assert_eq!(field.value, EDITED_TRANSLATION_X);

    let candidate = harness
        .state_mut()
        .take_scene_preview_candidate()
        .expect("the accepted Transform edit publishes one preview candidate");
    assert_eq!(candidate.document(), document);
    assert_eq!(candidate.revision(), edited_revision);
    let hit = candidate
        .hits()
        .iter()
        .find(|hit| hit.entity() == &created_entity)
        .expect("the composed visible entity contributes one preview hit");
    assert!(hit.contains_clip_point([0.2, 0.0]));

    let mut application = harness.into_state();

    let stale = dispatch(
        &mut application,
        [EditorAction::Mutate(
            EditorDocumentMutation::ApplyInspectorCommand {
                document,
                base_revision: composed_revision,
                command: SceneInspectorCommand::SetField {
                    entity: created_entity.clone(),
                    component: ComponentTypeId::new(TRANSFORM_COMPONENT),
                    component_version: field.component_version,
                    field: ComponentFieldId::new(TRANSLATION_X_FIELD),
                    value: ComponentValue::f64(STALE_TRANSLATION_X).unwrap(),
                },
            },
        )],
    );
    assert_eq!(
        stale.workspace().active_scene.as_ref().unwrap().revision,
        edited_revision
    );
    assert_eq!(transform_x_field(&stale).value, EDITED_TRANSLATION_X);
    assert_eq!(
        stale.field_feedback().unwrap().reason(),
        EditorFieldFeedbackReason::StaleAction
    );
    assert!(has_diagnostic(&stale, "editor.action.stale-document"));

    let mut harness = editor_harness(application);
    harness.get_by_label("Undo").click();
    harness.run();
    let undone = harness.state().current_frame();
    let undone_scene = undone.workspace().active_scene.as_ref().unwrap();
    assert!(undone_scene.dirty);
    assert_eq!(transform_x_field(&undone).value, 0.0);

    harness.get_by_label("Redo").click();
    harness.run();
    let redone = harness.state().current_frame();
    let redone_scene = redone.workspace().active_scene.as_ref().unwrap();
    assert!(redone_scene.dirty);
    assert_eq!(transform_x_field(&redone).value, EDITED_TRANSLATION_X);

    harness.get_by_label("Save").click();
    harness.run();
    let saved = harness.state().current_frame();
    assert!(matches!(
        saved.persistence().result(),
        Some(EditorPersistenceResult::Saved {
            document: saved_document,
            revision,
            ..
        }) if saved_document == document && revision == redone_scene.revision
    ));
    assert!(!saved.workspace().active_scene.as_ref().unwrap().dirty);
    let saved_scene_digest = ContentDigest::of_bytes(&fs::read(&scene_path).unwrap());
    let mut application = harness.into_state();

    let acknowledged_save = dispatch(
        &mut application,
        [EditorAction::Persistence(
            EditorPersistenceAction::AcknowledgeResult,
        )],
    );
    assert!(acknowledged_save.persistence().result().is_none());

    let closed = dispatch(
        &mut application,
        [EditorAction::WorkspaceIntent(
            EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::CloseScene { document }),
        )],
    );
    assert!(closed.workspace().scenes.is_empty());
    assert_eq!(
        closed.workspace_intent().result(),
        Some(EditorWorkspaceIntentResult::Applied {
            intent: EditorWorkspaceIntent::CloseScene { document },
        })
    );

    let ready_to_reopen = dispatch(
        &mut application,
        [EditorAction::WorkspaceIntent(
            EditorWorkspaceIntentAction::AcknowledgeResult,
        )],
    );
    let reopened = dispatch(
        &mut application,
        [EditorAction::Persistence(EditorPersistenceAction::Reopen {
            target: ready_to_reopen.reopen_target(),
        })],
    );
    let reopened_document = match reopened.persistence().result() {
        Some(EditorPersistenceResult::Opened { document, .. }) => document,
        other => panic!("expected a reopened document, got {other:?}"),
    };
    let mut harness = editor_harness(application);
    harness.get_by_label_contains(CREATED_ENTITY).click();
    harness.run();
    let reopened = harness.state().current_frame();
    assert!(!reopened.workspace().active_scene.as_ref().unwrap().dirty);
    assert_eq!(transform_x_field(&reopened).value, EDITED_TRANSLATION_X);
    let reopened_scene = reopened.workspace().active_scene.as_ref().unwrap();
    let created_row = reopened_scene
        .editor
        .inspector
        .entities
        .iter()
        .find(|row| row.id == created_entity)
        .expect("reopen reconstructs the composed entity in the canonical tree");
    assert!(created_row.parent.is_none());

    let mut application = harness.into_state();

    dispatch(
        &mut application,
        [EditorAction::Persistence(
            EditorPersistenceAction::AcknowledgeResult,
        )],
    );
    let finally_closed = dispatch(
        &mut application,
        [EditorAction::WorkspaceIntent(
            EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::CloseScene {
                document: reopened_document,
            }),
        )],
    );
    assert!(finally_closed.workspace().scenes.is_empty());
    drop(application);

    let checked = check_project(ProjectCheckRequest::new(
        readable_directory(&project),
        ProductRecipe::new(),
    ))
    .expect("the ordinary project check must admit the saved Editor source");
    let scene_member = checked
        .members()
        .iter()
        .find(|member| {
            member.kind() == StartupContentMemberKind::Scene
                && member.path().as_str() == "scenes/startup.scene.json"
        })
        .expect("the checked startup closure contains the edited Scene");
    assert_eq!(scene_member.digest(), saved_scene_digest);
    let manifest_digest = checked.manifest_digest();
    let content_digest = checked.content_digest();
    drop(checked);

    let product_observation = run_generated_headless(&root, &project);
    assert_product_observation(
        &product_observation,
        "headless",
        "default",
        manifest_digest,
        content_digest,
    );

    let export_observation = run_export(&root, &project);
    assert_digest_value(
        export_observation
            .get("startup_content")
            .expect("export observation contains startup content"),
        content_digest,
    );
    let products = export_observation
        .get("products")
        .and_then(Value::as_array)
        .expect("export observation contains accepted products");
    assert_eq!(products.len(), 2);
    for (product, (target, profile)) in products
        .iter()
        .zip([("headless", "default"), ("desktop", "desktop")])
    {
        assert_eq!(product.get("target").and_then(Value::as_str), Some(target));
        assert_eq!(
            product.get("profile").and_then(Value::as_str),
            Some(profile)
        );
        assert_digest_value(product.get("manifest").unwrap(), manifest_digest);
        assert_digest_value(product.get("content").unwrap(), content_digest);
    }
    assert_eq!(
        ContentDigest::of_bytes(
            &fs::read(
                root.export_parent()
                    .join(EXPORT_DESTINATION)
                    .join("project/scenes/startup.scene.json"),
            )
            .unwrap(),
        ),
        saved_scene_digest
    );
}

#[derive(Clone)]
struct TransformXField {
    component_version: nara::reflect::ComponentSchemaVersion,
    value: f64,
}

fn create_project(root: &TrajectoryRoot) -> PathBuf {
    let checkout = writable_directory(root.checkout());
    let publication_parent = checkout
        .open_directory(root.project_parent_relation())
        .unwrap();
    let prepared = prepare_desktop_2d_project(ProjectCreationRequest::new(
        publication_parent,
        ProjectSlug::new(PROJECT_SLUG).unwrap(),
        NaraDependencySource::local(
            checkout,
            LocalDependencyParent::Descendant(root.project_parent_relation().clone()),
        ),
    ))
    .unwrap();
    let ProjectCreationOutcome::Created(receipt) = prepared.publish() else {
        panic!("the ordinary starter publication must create an admitted project");
    };
    let project = root.project_parent().join(PROJECT_SLUG);
    assert_eq!(receipt.plan().slug().as_str(), PROJECT_SLUG);
    project
}

fn editor_harness(application: EditorApplication) -> Harness<'static, EditorApplication> {
    Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_ui_state(
            |ui, application| {
                let batch = application.show(ui);
                application.submit_and_drive(batch, Duration::ZERO);
            },
            application,
        )
}

fn dispatch(
    application: &mut EditorApplication,
    actions: impl IntoIterator<Item = EditorAction>,
) -> std::sync::Arc<nara::tooling::EditorFrameModel> {
    let frame = application.current_frame();
    application.submit_and_drive(
        EditorActionBatch::new(frame.revision(), actions),
        Duration::ZERO,
    );
    application.current_frame()
}

fn transform_x_field(frame: &nara::tooling::EditorFrameModel) -> TransformXField {
    let selected = frame
        .workspace()
        .active_scene
        .as_ref()
        .and_then(|scene| scene.editor.inspector.selected_entity_view.as_ref())
        .expect("the composed entity is selected in the Inspector");
    let component = selected
        .components
        .iter()
        .find(|component| component.component.as_str() == TRANSFORM_COMPONENT)
        .expect("the composed entity exposes Transform2d");
    let field = component
        .fields
        .iter()
        .find(|field| field.id.as_str() == TRANSLATION_X_FIELD)
        .expect("the composed entity exposes translation.x");
    let value = field
        .value
        .as_ref()
        .expect("translation.x has a concrete authored value");
    let ComponentValue::F64(value) = value else {
        panic!("translation.x must remain an f64");
    };
    TransformXField {
        component_version: component.schema_version,
        value: (*value).get(),
    }
}

fn has_diagnostic(frame: &nara::tooling::EditorFrameModel, code: &str) -> bool {
    frame
        .diagnostics()
        .iter()
        .any(|entry| entry.code().as_str() == code)
}

fn run_generated_headless(root: &TrajectoryRoot, project: &Path) -> Value {
    let attempt = format!("editor-source-{}", std::process::id());
    let observation = root.path().join("source-headless.json");
    let mut command = Command::new(env!("CARGO"));
    command
        .current_dir(root.unrelated_cwd())
        .args([
            "run",
            "--manifest-path",
            project.join("Cargo.toml").to_str().unwrap(),
            "--bin",
            "headless",
        ])
        .env("CARGO_BUILD_JOBS", "1")
        .env("CARGO_TARGET_DIR", root.product_target())
        .env(PRODUCT_OBSERVATION_ATTEMPT_ENV, &attempt)
        .env(PRODUCT_OBSERVATION_FILE_ENV, &observation);
    expect_success(command, "generated headless product");
    let value = read_json(&observation);
    assert_eq!(
        value.get("attempt").and_then(Value::as_str),
        Some(attempt.as_str())
    );
    value
}

fn run_export(root: &TrajectoryRoot, project: &Path) -> Value {
    let cli = build_nara_cli(root.checkout());
    let attempt = format!("editor-export-{}", std::process::id());
    let observation = root.path().join("export.json");
    let mut command = Command::new(cli);
    command
        .current_dir(project)
        .args([
            "export",
            ".",
            "--parent",
            root.export_parent().to_str().unwrap(),
            "--destination",
            EXPORT_DESTINATION,
        ])
        .env("CARGO_BUILD_JOBS", "1")
        .env("CARGO_TARGET_DIR", root.product_target())
        .env(EXPORT_OBSERVATION_ATTEMPT_ENV, &attempt)
        .env(EXPORT_OBSERVATION_FILE_ENV, &observation);
    expect_success(command, "source-carrying export");
    let value = read_json(&observation);
    assert_eq!(
        value.get("attempt").and_then(Value::as_str),
        Some(attempt.as_str())
    );
    value
}

fn build_nara_cli(checkout: &Path) -> PathBuf {
    let mut command = Command::new(env!("CARGO"));
    command
        .current_dir(checkout)
        .args([
            "build",
            "--locked",
            "--no-default-features",
            "--features",
            "project-workflow-cli",
            "--bin",
            "nara",
        ])
        .env("CARGO_BUILD_JOBS", "1");
    expect_success(command, "public nara CLI build");
    checkout
        .join("target/debug")
        .join(format!("nara{}", std::env::consts::EXE_SUFFIX))
}

fn assert_product_observation(
    value: &Value,
    target: &str,
    profile: &str,
    manifest_digest: ContentDigest,
    content_digest: ContentDigest,
) {
    let manifest_hash = digest_hex(manifest_digest);
    let content_hash = digest_hex(content_digest);
    assert_eq!(value.get("version").and_then(Value::as_u64), Some(1));
    assert_eq!(value.get("target").and_then(Value::as_str), Some(target));
    assert_eq!(value.get("profile").and_then(Value::as_str), Some(profile));
    assert_eq!(
        value.get("manifest_length").and_then(Value::as_u64),
        Some(manifest_digest.length())
    );
    assert_eq!(
        value.get("manifest_blake3").and_then(Value::as_str),
        Some(manifest_hash.as_str())
    );
    assert_eq!(
        value.get("content_length").and_then(Value::as_u64),
        Some(content_digest.length())
    );
    assert_eq!(
        value.get("content_blake3").and_then(Value::as_str),
        Some(content_hash.as_str())
    );
}

fn assert_digest_value(value: &Value, digest: ContentDigest) {
    let hash = digest_hex(digest);
    assert_eq!(
        value.get("length").and_then(Value::as_u64),
        Some(digest.length())
    );
    assert_eq!(
        value.get("blake3").and_then(Value::as_str),
        Some(hash.as_str())
    );
}

fn digest_hex(digest: ContentDigest) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for byte in digest.as_bytes() {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn expect_success(mut command: Command, label: &str) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{label} failed\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

struct TrajectoryRoot {
    checkout: PathBuf,
    target: PathBuf,
    path: PathBuf,
    project_parent: PathBuf,
    project_parent_relation: RelativePath,
    export_parent: PathBuf,
    unrelated_cwd: PathBuf,
    product_target: PathBuf,
}

impl TrajectoryRoot {
    fn new() -> Self {
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap();
        let target = checkout.join("target").canonicalize().unwrap();
        let sequence = NEXT_TRAJECTORY.fetch_add(1, Ordering::Relaxed);
        let name = format!(
            "nara-editor-authoring-loop-{}-{sequence}",
            std::process::id()
        );
        let path = target.join(&name);
        let project_parent = path.join("projects");
        let export_parent = path.join("exports");
        let unrelated_cwd = path.join("unrelated-cwd");
        let product_target = target.join("nara-editor-product-cache");
        fs::create_dir(&path).unwrap();
        fs::create_dir(&project_parent).unwrap();
        fs::create_dir(&export_parent).unwrap();
        fs::create_dir(&unrelated_cwd).unwrap();
        fs::create_dir_all(&product_target).unwrap();
        let project_parent_relation = RelativePath::new(format!("target/{name}/projects")).unwrap();
        Self {
            checkout,
            target,
            path,
            project_parent,
            project_parent_relation,
            export_parent,
            unrelated_cwd,
            product_target,
        }
    }

    fn checkout(&self) -> &Path {
        &self.checkout
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn project_parent(&self) -> &Path {
        &self.project_parent
    }

    const fn project_parent_relation(&self) -> &RelativePath {
        &self.project_parent_relation
    }

    fn export_parent(&self) -> &Path {
        &self.export_parent
    }

    fn unrelated_cwd(&self) -> &Path {
        &self.unrelated_cwd
    }

    fn product_target(&self) -> &Path {
        &self.product_target
    }
}

impl Drop for TrajectoryRoot {
    fn drop(&mut self) {
        if self.path.starts_with(&self.target) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn readable_directory(path: &Path) -> DirectoryCapability {
    DirectoryCapability::from_host_handle(
        host_directory(path, false),
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
    )
    .unwrap()
}

fn writable_directory(path: &Path) -> DirectoryCapability {
    DirectoryCapability::from_host_handle(
        host_directory(path, true),
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .unwrap()
}

#[cfg(windows)]
fn host_directory(path: &Path, writable: bool) -> File {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    OpenOptions::new()
        .read(true)
        .write(writable)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .unwrap()
}

#[cfg(unix)]
fn host_directory(path: &Path, _writable: bool) -> File {
    File::open(path).unwrap()
}
