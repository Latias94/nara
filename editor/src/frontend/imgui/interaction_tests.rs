use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use dear_imgui_rs::{ConfigFlags, Context, Key, MouseButton, Ui};
use nara::{
    asset::AssetRef,
    reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
        ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
        ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
        PersistentComponentSchemaOwnerId,
    },
    render::DisplayRect,
    scene::{
        PrefabDocument, PrefabInstance, SceneComponentRecord, SceneDocument, SceneEntityId,
        SceneEntityRecord, ScenePatchDocument,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorCloseDecision, EditorDocumentKind, EditorFrameModel,
        EditorPersistenceOperation, EditorPersistenceResult, EditorWorkspaceIntentPhase,
    },
};

use super::ImguiEditorFrontend;
use crate::{EditorApplication, EditorLaunchOptions};

mod entity_references;
mod field_drafts;
mod field_overrides;
mod recursive_shapes;

pub(super) const CUSTOM_COMPONENT: &str = "studio.interaction.Health";
const UNKNOWN_COMPONENT: &str = "studio.unavailable.Payload";

thread_local! {
    static CONTROLS: RefCell<BTreeMap<String, DisplayRect>> = RefCell::default();
}

pub(super) fn record_control(ui: &Ui, name: impl Into<String>) {
    let name = name.into();
    super::ui_interaction_tests::record_control(ui, name.clone());
    let (min, max) = ui.item_rect();
    CONTROLS.with(|controls| controls.borrow_mut().insert(name, DisplayRect { min, max }));
}

pub(crate) struct InteractionProject {
    pub(crate) root: PathBuf,
}

impl InteractionProject {
    pub(crate) fn new_field_drafts() -> Self {
        let project = Self::new();
        field_drafts::configure(&project);
        project
    }
    pub(crate) fn new_field_overrides() -> Self {
        let project = Self::new();
        field_overrides::configure(&project);
        project
    }

    pub(crate) fn new_recursive() -> Self {
        let project = Self::new();
        recursive_shapes::configure(&project);
        project
    }

    pub(crate) fn new_entity_references() -> Self {
        let project = Self::new();
        entity_references::configure(&project);
        project
    }

    pub(crate) fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara-imgui-interaction-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        fs::create_dir(&root).unwrap();
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/generic-editor/known-unbound");
        for directory in [
            "catalogs",
            "worlds",
            "blueprints",
            "media",
            "scripts",
            "src",
        ] {
            fs::create_dir(root.join(directory)).unwrap();
        }
        for member in ["nara.toml", "Cargo.toml", "src/lib.rs"] {
            fs::copy(fixture.join(member), root.join(member)).unwrap();
        }
        let schema = ComponentSchema::new(
            ComponentTypeId::new(CUSTOM_COMPONENT),
            "Health",
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::from_fields(["value"]),
            ComponentValueKind::I64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
        .with_authoring_initial_value(ComponentValue::map([("value", ComponentValue::I64(10))]));
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new("studio.interaction.components").unwrap(),
            ComponentSchemaCatalog {
                components: vec![schema],
                ..Default::default()
            },
            None,
        )])
        .unwrap();
        fs::write(
            root.join("catalogs/authoring.bundle.json"),
            bundle.to_json_string().unwrap(),
        )
        .unwrap();
        let preserved = SceneEntityRecord::new(SceneEntityId::new("preserved").unwrap())
            .with_component(
                ComponentTypeId::new(UNKNOWN_COMPONENT),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([(
                        "payload",
                        ComponentValue::String("keep this source".into()),
                    )]),
                )
                .with_owner(
                    PersistentComponentSchemaOwnerId::try_new("studio.unavailable.components")
                        .unwrap(),
                ),
            );
        let mut anchor = SceneEntityRecord::new(SceneEntityId::new("prefab-anchor").unwrap());
        anchor.prefab = Some(PrefabInstance {
            source: AssetRef::path("ordinary.prefab.json").unwrap(),
            overrides: ScenePatchDocument::default(),
        });
        fs::write(
            root.join("worlds/entry.scene.json"),
            SceneDocument::from_ordered_roots([anchor, preserved.clone()])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("blueprints/ordinary.prefab.json"),
            PrefabDocument::from_ordered_roots([preserved])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        Self { root }
    }

    pub(crate) fn source(&self, kind: EditorDocumentKind) -> PathBuf {
        self.root.join(match kind {
            EditorDocumentKind::Scene => "worlds/entry.scene.json",
            EditorDocumentKind::Prefab => "blueprints/ordinary.prefab.json",
            EditorDocumentKind::Extension(_) => {
                unreachable!("extension documents are not fixture sources")
            }
        })
    }
}

impl Drop for InteractionProject {
    fn drop(&mut self) {
        let temporary = std::env::temp_dir().canonicalize().unwrap();
        let root = self.root.canonicalize().unwrap();
        assert!(root.starts_with(temporary));
        assert!(
            root.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("nara-imgui-interaction-"))
        );
        fs::remove_dir_all(root).unwrap();
    }
}

struct Interaction {
    application: EditorApplication,
    frontend: ImguiEditorFrontend,
    context: Context,
    actions: Vec<EditorAction>,
}

impl Interaction {
    fn new(project: &InteractionProject) -> Self {
        let mut application =
            EditorApplication::new(EditorLaunchOptions::new(&project.root).with_profile("desktop"));
        application.native_shell_ready();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if application.current_frame().is_some() {
                break;
            }
            application.advance_project_open();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            application.current_frame().is_some(),
            "the ordinary fixture opens without project code"
        );
        Self::from_application(application)
    }

    fn from_application(mut application: EditorApplication) -> Self {
        let hidden = application
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .filter(|instance| !matches!(instance.kind(), "nara.hierarchy" | "nara.inspector"))
            .map(|instance| instance.id())
            .collect::<Vec<_>>();
        for instance in hidden {
            application
                .panel_workspace_mut()
                .layout_mut()
                .close(instance);
        }
        let mut context = Context::create();
        context.set_ini_filename::<PathBuf>(None).unwrap();
        context
            .io_mut()
            .set_config_flags(ConfigFlags::DOCKING_ENABLE | ConfigFlags::NAV_ENABLE_KEYBOARD);
        context.io_mut().set_display_size([1800.0, 1100.0]);
        context.io_mut().set_delta_time(1.0 / 60.0);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        let mut interaction = Self {
            application,
            frontend: ImguiEditorFrontend::new(),
            context,
            actions: Vec::new(),
        };
        for _ in 0..4 {
            interaction.render();
        }
        interaction
    }

    fn frame(&self) -> Arc<EditorFrameModel> {
        self.application.current_frame().unwrap()
    }

    fn render(&mut self) {
        CONTROLS.with(|controls| controls.borrow_mut().clear());
        let ui = self.context.frame();
        let batch = self.frontend.show(ui, &mut self.application).unwrap();
        let _ = self.context.render_legacy();
        if let Some(mut batch) = batch {
            let revision = batch.batch.frame_revision();
            let actions = std::mem::replace(&mut batch.batch, EditorActionBatch::new(revision, []))
                .into_actions();
            self.actions
                .extend(actions.iter().map(|action| match action {
                    EditorAction::Navigate(action) => EditorAction::Navigate(action.clone()),
                    EditorAction::Mutate(action) => EditorAction::Mutate(action.clone()),
                    EditorAction::Persistence(action) => EditorAction::Persistence(action.clone()),
                    EditorAction::WorkspaceIntent(action) => {
                        EditorAction::WorkspaceIntent(action.clone())
                    }
                    EditorAction::ReportFieldValidation(action) => {
                        EditorAction::ReportFieldValidation(action.clone())
                    }
                    _ => panic!("unexpected action in the field interaction fixture: {action:?}"),
                }));
            batch.batch = EditorActionBatch::new(revision, actions);
            self.frontend
                .submit_and_drive(&mut self.application, batch, Duration::ZERO);
        }
    }

    fn click(&mut self, label: &str) {
        self.actions.clear();
        let point = CONTROLS.with(|controls| {
            let controls = controls.borrow();
            let rect = controls.get(label).unwrap_or_else(|| {
                panic!(
                    "the real widget {label:?} was not painted; available: {:?}",
                    controls.keys().collect::<Vec<_>>()
                )
            });
            [
                (rect.min[0] + rect.max[0]) * 0.5,
                (rect.min[1] + rect.max[1]) * 0.5,
            ]
        });
        self.context.io_mut().add_mouse_pos_event(point);
        self.render();
        self.context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        self.render();
        self.context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, false);
        self.render();
        // A popup's first frame measures its contents before the following frame draws them.
        for _ in 0..3 {
            self.render();
        }
    }

    fn replace_integer(&mut self, text: &str) {
        self.replace_text(&format!("field:{CUSTOM_COMPONENT}:value"), text);
        self.context.io_mut().add_key_event(Key::Enter, true);
        self.render();
        self.context.io_mut().add_key_event(Key::Enter, false);
        self.render();
    }

    fn replace_text(&mut self, label: &str, text: &str) {
        self.click(label);
        self.context.io_mut().add_key_event(Key::ModCtrl, true);
        self.context.io_mut().add_key_event(Key::A, true);
        self.render();
        self.context.io_mut().add_key_event(Key::A, false);
        self.context.io_mut().add_key_event(Key::ModCtrl, false);
        self.render();
        self.context.io_mut().add_input_characters_utf8(text);
        self.render();
    }

    fn settle_persistence(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.frame().persistence().operation() != EditorPersistenceOperation::Idle
            && Instant::now() < deadline
        {
            self.render();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            self.frame().persistence().operation(),
            EditorPersistenceOperation::Idle
        );
        self.render();
    }

    fn selected_value(&self) -> Option<ComponentValue> {
        self.frame()
            .workspace()
            .active_scene
            .as_ref()?
            .editor
            .inspector
            .selected_entity_view
            .as_ref()?
            .components
            .iter()
            .find(|component| component.component.as_str() == CUSTOM_COMPONENT)?
            .fields
            .iter()
            .find(|field| field.id.as_str() == "value")?
            .value
            .clone()
    }
}

impl Drop for Interaction {
    fn drop(&mut self) {
        if self.application.request_exit().exit_ready() {
            return;
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let Some(frame) = self.application.current_frame() else {
                return;
            };
            let decision = frame.workspace_intent().decision_prompt().map(|prompt| {
                EditorAction::WorkspaceIntent(prompt.resolve(EditorCloseDecision::Discard))
            });
            let update = self.application.submit_and_drive(
                EditorActionBatch::new(frame.revision(), decision),
                Duration::ZERO,
            );
            if update.update().exit_ready() {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            std::thread::panicking(),
            "the interaction fixture did not retire its Session"
        );
    }
}

#[test]
fn ordinary_scene_and_prefab_mouse_keyboard_workflow_saves_and_reopens_real_source() {
    let project = InteractionProject::new();
    let mut interaction = Interaction::new(&project);
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        if kind == EditorDocumentKind::Prefab {
            interaction.click("entity:prefab-anchor");
            interaction.click("Open Prefab Source");
        }
        let before = interaction.frame();
        let model = before.workspace().active_scene.as_ref().unwrap();
        assert_eq!(model.kind, kind);
        assert!(model.editor.inspector.composition.generic_authoring);
        let document = model.document;
        let original_manifest = interaction
            .application
            .active_authoring_unavailable_record_manifest()
            .unwrap();
        interaction.click("+ Root");
        let created = interaction.frame();
        let scene = created.workspace().active_scene.as_ref().unwrap();
        let entity = scene
            .selection
            .top_entity()
            .expect("the create widget selects its accepted entity")
            .clone();
        assert_eq!(scene.editor.inspector.history.undo_depth, 1);
        interaction.click("Add Component");
        interaction.click(CUSTOM_COMPONENT);
        assert_eq!(interaction.selected_value(), Some(ComponentValue::I64(10)));
        let before_edit = interaction
            .frame()
            .workspace()
            .document(document)
            .unwrap()
            .revision;
        interaction.replace_integer("42");
        assert_eq!(interaction.selected_value(), Some(ComponentValue::I64(42)));
        let edited = interaction.frame();
        let tab = edited.workspace().document(document).unwrap();
        assert_ne!(tab.revision, before_edit);
        assert_eq!(tab.history.undo_depth, 3);
        interaction.click("Undo");
        assert_eq!(interaction.selected_value(), Some(ComponentValue::I64(10)));
        interaction.click("Redo");
        assert_eq!(interaction.selected_value(), Some(ComponentValue::I64(42)));
        interaction.click("Remove Component");
        assert_eq!(interaction.selected_value(), None);
        interaction.click("Undo");
        assert_eq!(interaction.selected_value(), Some(ComponentValue::I64(42)));
        interaction.click("Redo");
        assert_eq!(interaction.selected_value(), None);
        interaction.click("Undo");
        assert_eq!(interaction.selected_value(), Some(ComponentValue::I64(42)));
        assert_eq!(
            interaction
                .application
                .active_authoring_unavailable_record_manifest(),
            Some(original_manifest.clone())
        );

        interaction.click("Save");
        interaction.settle_persistence();
        let saved = interaction.frame();
        assert!(
            matches!(saved.persistence().result(), Some(EditorPersistenceResult::Saved { document: actual, .. }) if actual == document)
        );
        assert!(!saved.workspace().document(document).unwrap().dirty);
        let bytes = fs::read(project.source(kind)).unwrap();
        let stored: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let stored = stored["payload"]["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|record| record["id"].as_str() == Some(entity.as_str()))
            .unwrap();
        let value: ComponentValue =
            serde_json::from_value(stored["components"][CUSTOM_COMPONENT]["value"].clone())
                .unwrap();
        assert_eq!(
            value,
            ComponentValue::map([("value", ComponentValue::I64(42))])
        );
        interaction.click("Dismiss persistence result");
        interaction.click("Reopen");
        interaction.settle_persistence();
        assert!(matches!(
            interaction.frame().persistence().result(),
            Some(EditorPersistenceResult::Opened { .. })
        ));
        interaction.click(&format!("entity:{}", entity.as_str()));
        assert_eq!(interaction.selected_value(), Some(ComponentValue::I64(42)));
        assert_eq!(
            interaction
                .application
                .active_authoring_unavailable_record_manifest(),
            Some(original_manifest)
        );
        interaction.click("Dismiss persistence result");
    }
    assert!(
        fs::read_to_string(project.root.join("src/lib.rs"))
            .unwrap()
            .contains("compile_error!")
    );
}

#[test]
fn dirty_close_displays_the_prompted_document_after_advance_and_stale_replay() {
    use std::rc::Rc;

    struct RenderedText(Rc<RefCell<String>>);

    impl dear_imgui_rs::ClipboardBackend for RenderedText {
        fn get(&mut self) -> Option<String> {
            Some(self.0.borrow().clone())
        }

        fn set(&mut self, value: &str) {
            value.clone_into(&mut self.0.borrow_mut());
        }
    }

    fn rendered_close_dialog(interaction: &mut Interaction) -> String {
        let log = Rc::new(RefCell::new(String::new()));
        interaction
            .context
            .set_clipboard_backend(RenderedText(Rc::clone(&log)));
        CONTROLS.with(|controls| controls.borrow_mut().clear());
        let frame = interaction.frame();
        let ui = interaction.context.frame();
        let mut actions = Vec::new();
        ui.window("Close identity proof").build(|| {
            ui.log_to_clipboard(0_u32);
            super::show_dirty_close_dialog(ui, &frame, &mut actions);
            ui.log_finish();
        });
        let _ = interaction.context.render_legacy();
        assert!(actions.is_empty());
        log.take()
    }

    let project = InteractionProject::new();
    let mut interaction = Interaction::new(&project);
    interaction.click("+ Root");
    interaction.click("entity:prefab-anchor");
    interaction.click("Open Prefab Source");
    interaction.click("+ Root");
    assert_eq!(
        interaction
            .frame()
            .workspace()
            .documents
            .iter()
            .filter(|document| document.dirty)
            .count(),
        2
    );

    assert!(!interaction.application.request_exit().exit_ready());
    interaction.render();
    let first = interaction.frame();
    let first_prompt = first.workspace_intent().decision_prompt().unwrap();
    let first_document = first.workspace().document(first_prompt.document()).unwrap();
    let first_title = format!("Document: {}", first_document.title);
    let first_source = format!(
        "Source: {}",
        first_document.source_binding.as_ref().unwrap().locator()
    );
    let first_text = rendered_close_dialog(&mut interaction);
    assert!(first_text.contains(&first_title), "{first_text}");
    assert!(first_text.contains(&first_source), "{first_text}");

    interaction.click("close:Discard and continue");
    let second = interaction.frame();
    let second_prompt = second.workspace_intent().decision_prompt().unwrap();
    assert_ne!(first_prompt.document(), second_prompt.document());
    let second_document = second
        .workspace()
        .document(second_prompt.document())
        .unwrap();
    let second_title = format!("Document: {}", second_document.title);
    let second_source = format!(
        "Source: {}",
        second_document.source_binding.as_ref().unwrap().locator()
    );
    let second_text = rendered_close_dialog(&mut interaction);
    assert!(second_text.contains(&second_title), "{second_text}");
    assert!(second_text.contains(&second_source), "{second_text}");
    assert!(!second_text.contains(&first_title), "{second_text}");
    assert!(!second_text.contains(&first_source), "{second_text}");

    interaction.application.submit_and_drive(
        EditorActionBatch::new(
            second.revision(),
            [EditorAction::WorkspaceIntent(
                first_prompt.resolve(EditorCloseDecision::Discard),
            )],
        ),
        Duration::ZERO,
    );
    let after_replay = interaction.frame();
    assert_eq!(
        after_replay.workspace_intent().decision_prompt(),
        Some(second_prompt)
    );
    assert_eq!(
        after_replay.workspace().documents,
        second.workspace().documents
    );
    let replay_text = rendered_close_dialog(&mut interaction);
    assert!(replay_text.contains(&second_title), "{replay_text}");
    assert!(replay_text.contains(&second_source), "{replay_text}");
    assert!(!replay_text.contains(&first_title), "{replay_text}");
    for label in ["Save and continue", "Discard and continue", "Cancel"] {
        assert!(
            CONTROLS.with(|controls| { controls.borrow().contains_key(&format!("close:{label}")) })
        );
    }
}

#[test]
fn dirty_close_does_not_offer_a_decision_after_save_has_started() {
    use nara::tooling::{EditorWorkspaceIntent, EditorWorkspaceIntentAction};

    let project = InteractionProject::new();
    let mut interaction = Interaction::new(&project);
    interaction.click("+ Root");
    let frame = interaction.frame();
    let document = frame.workspace().active_scene.as_ref().unwrap().document;
    interaction.application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::WorkspaceIntent(
                EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::CloseDocument {
                    document,
                }),
            )],
        ),
        Duration::ZERO,
    );
    interaction.render();
    assert!(CONTROLS.with(|controls| controls.borrow().contains_key("close:Cancel")));
    let frame = interaction.frame();
    // Freeze the public in-progress projection so worker scheduling cannot race the
    // widget-policy observation.
    let saving = nara::tooling::EditorFrameClock::new().publish(
        frame.workspace().clone(),
        nara::tooling::EditorProjectView::new(
            frame.project_readiness(),
            frame.project_browser().clone(),
            frame.play().clone(),
            nara::tooling::EditorPersistenceView::new(
                EditorPersistenceOperation::Saving {
                    document,
                    captured_revision: frame.workspace().document(document).unwrap().revision,
                },
                None,
            ),
            frame.project_content(),
            nara::tooling::EditorWorkspaceIntentView::new(
                Some(EditorWorkspaceIntent::CloseDocument { document }),
                Some(EditorWorkspaceIntentPhase::Saving),
                Some(document),
                None,
                None,
            ),
        ),
        frame.diagnostics().clone(),
        frame.reopen_target(),
        None,
    );
    CONTROLS.with(|controls| controls.borrow_mut().clear());
    let ui = interaction.context.frame();
    let mut actions = Vec::new();
    super::show_dirty_close_dialog(ui, &saving, &mut actions);
    let _ = interaction.context.render_legacy();
    let offers_cancel = CONTROLS.with(|controls| controls.borrow().contains_key("close:Cancel"));
    interaction.application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::WorkspaceIntent(
                frame
                    .workspace_intent()
                    .decision_prompt()
                    .unwrap()
                    .resolve(EditorCloseDecision::Cancel),
            )],
        ),
        Duration::ZERO,
    );
    assert!(interaction.frame().workspace_intent().phase().is_none());
    assert!(
        !offers_cancel,
        "Saving cannot offer a Cancel decision that its owner will reject as Busy"
    );
    assert!(actions.is_empty());
}

#[test]
fn manager_recovery_retry_and_dirty_project_close_use_real_widgets() {
    use crate::EditorApplicationState;

    let project = InteractionProject::new();
    let bundle_path = project.root.join("catalogs/authoring.bundle.json");
    let bundle = fs::read(&bundle_path).unwrap();
    fs::write(&bundle_path, b"invalid authoring bundle").unwrap();
    let mut interaction = Interaction::from_application(EditorApplication::new(
        EditorLaunchOptions::project_manager(),
    ));
    interaction.replace_text("Project path", project.root.to_str().unwrap());
    interaction.replace_text("Profile", "desktop");
    interaction.click("shell:Open Project");
    assert_eq!(
        interaction.application.state(),
        EditorApplicationState::OpeningInitial
    );
    interaction.click("shell:Cancel");
    assert_eq!(
        interaction.application.state(),
        EditorApplicationState::ProjectManager
    );
    interaction.click("shell:Open Project");
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        interaction.application.advance_project_open();
        if interaction.application.state() == EditorApplicationState::Recovery {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        interaction.application.state(),
        EditorApplicationState::Recovery
    );
    interaction.render();
    fs::write(&bundle_path, bundle).unwrap();
    interaction.click("shell:Retry");
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        interaction.application.advance_project_open();
        if interaction.application.current_frame().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    for _ in 0..4 {
        interaction.render();
    }
    assert!(
        interaction.application.current_frame().is_some(),
        "Retry must reopen the repaired source"
    );
    interaction.click("+ Root");
    let dirty = interaction.frame().workspace().clone();
    assert!(dirty.documents.iter().any(|document| document.dirty));

    let replacement = InteractionProject::new();
    interaction.click("shell:Open Project...");
    interaction.replace_text("Project path", replacement.root.to_str().unwrap());
    interaction.click("shell:Open Project");
    assert_eq!(
        interaction.application.state(),
        EditorApplicationState::OpeningReplacement
    );
    interaction.click("shell:Cancel");
    assert_eq!(interaction.frame().workspace(), &dirty);
    interaction.click("shell:Close Project");
    assert_eq!(
        interaction.frame().workspace_intent().phase(),
        Some(EditorWorkspaceIntentPhase::AwaitingDecision)
    );
    interaction.click("close:Cancel");
    assert_eq!(interaction.frame().workspace(), &dirty);
    interaction.click("shell:Close Project");
    interaction.click("close:Discard and continue");
    let deadline = Instant::now() + Duration::from_secs(10);
    while interaction.application.state() != EditorApplicationState::ProjectManager
        && Instant::now() < deadline
    {
        interaction.render();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        interaction.application.state(),
        EditorApplicationState::ProjectManager
    );
    let stored: serde_json::Value =
        serde_json::from_slice(&fs::read(project.source(EditorDocumentKind::Scene)).unwrap())
            .unwrap();
    assert_eq!(
        stored["payload"]["entities"].as_array().unwrap().len(),
        2,
        "Discard must retain the original disk source"
    );
}
