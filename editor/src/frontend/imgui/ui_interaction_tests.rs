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
    asset::{AssetMeta, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, StableAssetId},
    image::ImageImportSettings,
    project_host::{EditorUiAuthoringFrame, TrustedCompiledEditorProjectIntent},
    reflect::{AuthoringSchemaBundle, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    render::DisplayRect,
    scene::{
        PrefabInstance, SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
        ScenePatchDocument,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorCloseDecision, EditorFrameModel,
        EditorPersistenceOperation, EditorPersistenceResult,
    },
    ui::__private::{UiAuthoringFieldValue, UiAuthoringHierarchyKind},
};

use super::ImguiEditorFrontend;
use crate::{EditorApplication, EditorLaunchOptions};

const IMAGE: &str = "textures/player.png";

thread_local! {
    static CONTROLS: RefCell<BTreeMap<String, DisplayRect>> = RefCell::default();
    static CONTROL_CLIPS: RefCell<BTreeMap<String, DisplayRect>> = RefCell::default();
    static VISIBLE_CONTROLS: RefCell<BTreeMap<String, bool>> = RefCell::default();
}

pub(super) fn record_control(ui: &Ui, name: impl Into<String>) {
    let (min, max) = ui.item_rect();
    let name = name.into();
    let draw = ui.get_window_draw_list();
    CONTROL_CLIPS.with(|clips| {
        clips.borrow_mut().insert(
            name.clone(),
            DisplayRect {
                min: draw.clip_rect_min(),
                max: draw.clip_rect_max(),
            },
        )
    });
    VISIBLE_CONTROLS.with(|visible| {
        visible
            .borrow_mut()
            .insert(name.clone(), ui.is_item_visible())
    });
    record_rect(name, DisplayRect { min, max });
}

pub(super) fn record_rect(name: impl Into<String>, rect: DisplayRect) {
    CONTROLS.with(|controls| controls.borrow_mut().insert(name.into(), rect));
}

pub(crate) fn clear_controls() {
    CONTROLS.with(|controls| controls.borrow_mut().clear());
    CONTROL_CLIPS.with(|clips| clips.borrow_mut().clear());
    VISIBLE_CONTROLS.with(|visible| visible.borrow_mut().clear());
}

pub(crate) fn control_rect(name: &str) -> Option<DisplayRect> {
    CONTROLS.with(|controls| controls.borrow().get(name).copied())
}

pub(crate) fn control_clip(name: &str) -> Option<DisplayRect> {
    CONTROL_CLIPS.with(|clips| clips.borrow().get(name).copied())
}

pub(crate) fn control_visible(name: &str) -> bool {
    VISIBLE_CONTROLS.with(|visible| visible.borrow().get(name).copied().unwrap_or(false))
}

pub(crate) struct UiProject(pub(crate) PathBuf);

impl UiProject {
    pub(crate) fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara-imgui-ui-interaction-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        fs::create_dir(&root).unwrap();
        for directory in ["assets", "scenes", "prefabs", "scripts", "schema"] {
            fs::create_dir(root.join(directory)).unwrap();
        }
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/generic-editor/image-import");
        for member in ["nara.toml", "prefabs/ui.prefab.json"] {
            fs::copy(fixture.join(member), root.join(member)).unwrap();
        }
        let bundle = AuthoringSchemaBundle::from_trusted_provider_definitions(
            nara::project_host::advanced::built_in_schema_providers(),
        )
        .unwrap();
        fs::write(
            root.join("schema/authoring.bundle.json"),
            bundle.to_json_string().unwrap(),
        )
        .unwrap();
        fs::create_dir(root.join("assets/textures")).unwrap();
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../tests/fixtures/images/valid-rgba-1x1.png"),
            root.join("assets").join(IMAGE),
        )
        .unwrap();
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str("3c7c5be4-fd4e-4b65-b8d4-c671f5982186").unwrap(),
                AssetPath::new(IMAGE).unwrap(),
                AssetSourceKind::Image,
            ),
            ImageImportSettings::default(),
        );
        fs::write(
            root.join("assets/textures/player.png.meta"),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
        let mut anchor = SceneEntityRecord::new(SceneEntityId::new("ui-anchor").unwrap());
        anchor.prefab = Some(PrefabInstance {
            source: AssetRef::path("ui.prefab.json").unwrap(),
            overrides: ScenePatchDocument::default(),
        });
        // A real startup Sprite loads this catalog image through the trusted project's normal bootstrap.
        let sprite = SceneEntityRecord::new(SceneEntityId::new("image-source").unwrap())
            .with_component(
                ComponentTypeId::new("nara.transform.Transform2d"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([
                        ("translation", vector(0.0, 0.0)),
                        ("rotation", ComponentValue::f64(0.0).unwrap()),
                        ("scale", vector(1.0, 1.0)),
                    ]),
                ),
            )
            .with_component(
                ComponentTypeId::new("nara.sprite.Sprite"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([
                        ("size", vector(16.0, 16.0)),
                        (
                            "material",
                            ComponentValue::map([
                                (
                                    "image",
                                    ComponentValue::map([
                                        ("kind", ComponentValue::String("path".into())),
                                        ("value", ComponentValue::String(IMAGE.into())),
                                    ]),
                                ),
                                (
                                    "tint",
                                    ComponentValue::map(["r", "g", "b", "a"].map(|channel| {
                                        (channel, ComponentValue::f64(1.0).unwrap())
                                    })),
                                ),
                            ]),
                        ),
                        ("layer", ComponentValue::I64(0)),
                        ("sort_key", ComponentValue::I64(0)),
                    ]),
                ),
            );
        fs::write(
            root.join("scenes/startup.scene.json"),
            SceneDocument::from_ordered_roots([anchor, sprite])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        Self(root)
    }
}

fn vector(x: f64, y: f64) -> ComponentValue {
    ComponentValue::map([
        ("x", ComponentValue::f64(x).unwrap()),
        ("y", ComponentValue::f64(y).unwrap()),
    ])
}

impl Drop for UiProject {
    fn drop(&mut self) {
        let temporary = std::env::temp_dir().canonicalize().unwrap();
        let root = self.0.canonicalize().unwrap();
        assert!(root.starts_with(temporary));
        assert!(
            root.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("nara-imgui-ui-interaction-"))
        );
        fs::remove_dir_all(root).unwrap();
    }
}

struct UiInteraction {
    application: EditorApplication,
    frontend: ImguiEditorFrontend,
    context: Context,
}

impl UiInteraction {
    fn new(project: &UiProject) -> Self {
        let mut application = EditorApplication::open_trusted_compiled(
            EditorLaunchOptions::new(&project.0).with_profile("desktop"),
            TrustedCompiledEditorProjectIntent::new(),
        )
        .expect("the current built-in schema bundle must open through its trusted lane");
        application.native_shell_ready();
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
        // Keep every specialized Inspector property visible without replacing its real controls.
        context.io_mut().set_display_size([1800.0, 6000.0]);
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
        };
        for _ in 0..4 {
            interaction.render();
        }
        interaction.click("entity:ui-anchor");
        interaction.click("Open Prefab Source");
        interaction
    }

    fn frame(&self) -> Arc<EditorFrameModel> {
        self.application.current_frame().unwrap()
    }

    fn authoring(&self) -> Arc<EditorUiAuthoringFrame> {
        self.application
            .current_ui_authoring_frame()
            .expect("the UI source is active")
    }

    fn render(&mut self) {
        CONTROLS.with(|controls| controls.borrow_mut().clear());
        let ui = self.context.frame();
        let batch = self.frontend.show(ui, &mut self.application).unwrap();
        let _ = self.context.render_legacy();
        if let Some(batch) = batch {
            self.frontend
                .submit_and_drive(&mut self.application, batch, Duration::ZERO);
        }
    }

    fn point(&self, label: &str) -> [f32; 2] {
        CONTROLS.with(|controls| {
            let controls = controls.borrow();
            let rect = controls.get(label).unwrap_or_else(|| {
                panic!(
                    "the real UI widget {label:?} was not painted; available: {:?}",
                    controls.keys().collect::<Vec<_>>()
                )
            });
            [
                (rect.min[0] + rect.max[0]) * 0.5,
                (rect.min[1] + rect.max[1]) * 0.5,
            ]
        })
    }

    fn click(&mut self, label: &str) {
        let point = self.point(label);
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
        for _ in 0..3 {
            self.render();
        }
    }

    fn press(&mut self, key: Key) {
        self.context.io_mut().add_key_event(key, true);
        self.render();
        self.context.io_mut().add_key_event(key, false);
        self.render();
    }

    fn value(&self, entity: &str, field: &str) -> UiAuthoringFieldValue {
        self.authoring()
            .projection()
            .unwrap()
            .fields()
            .iter()
            .find(|descriptor| {
                descriptor.key().entity().as_str() == entity
                    && descriptor.key().component().as_str() == "nara.ui.UiNode"
                    && descriptor.key().primary_field().as_str() == field
            })
            .expect("the selected UI node has its typed source field")
            .value()
            .clone()
    }

    fn undo_depth(&self) -> usize {
        let frame = self.frame();
        frame
            .workspace()
            .document(self.authoring().document())
            .unwrap()
            .history
            .undo_depth
    }

    fn cancel_scalar_drag(&mut self) {
        let baseline = self.value("image-node", "style.flex_grow");
        let history = self.undo_depth();
        let center = self.point("ui-field:image-node:style.flex_grow");
        self.context.io_mut().add_mouse_pos_event(center);
        self.render();
        self.context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        self.render();
        self.context
            .io_mut()
            .add_mouse_pos_event([center[0] + 32.0, center[1]]);
        self.render();
        self.render();
        assert!(
            self.authoring().gesture().is_some(),
            "the real drag must begin a document gesture"
        );
        assert_ne!(
            self.value("image-node", "style.flex_grow"),
            baseline,
            "the accepted preview must reach the real document owner"
        );
        self.press(Key::Escape);
        self.context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, false);
        self.render();
        assert!(self.authoring().gesture().is_none());
        assert_eq!(self.value("image-node", "style.flex_grow"), baseline);
        assert_eq!(
            self.undo_depth(),
            history,
            "cancelling must not add history"
        );
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
}

impl Drop for UiInteraction {
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
            "the UI interaction fixture did not retire its Session"
        );
    }
}

#[test]
fn specialized_ui_prefab_widgets_cancel_edit_select_image_and_save_reopen_source() {
    let project = UiProject::new();
    let source = project.0.join("prefabs/ui.prefab.json");
    let before = fs::read(&source).unwrap();
    let mut interaction = UiInteraction::new(&project);
    assert!(interaction.authoring().structure_editing_enabled());
    assert_eq!(
        interaction
            .authoring()
            .image_choices()
            .unwrap()
            .choices()
            .len(),
        1
    );
    interaction.click("ui-entity:image-node");
    interaction.cancel_scalar_drag();
    assert_eq!(
        fs::read(&source).unwrap(),
        before,
        "a cancelled preview cannot write the source"
    );
    interaction.click("ui-field:image-node:style.visible");
    assert_eq!(
        interaction.value("image-node", "style.visible"),
        UiAuthoringFieldValue::Bool(false)
    );
    assert_eq!(interaction.undo_depth(), 1);
    interaction.click("Undo");
    assert_eq!(
        interaction.value("image-node", "style.visible"),
        UiAuthoringFieldValue::Bool(true)
    );
    interaction.click("Redo");
    assert_eq!(
        interaction.value("image-node", "style.visible"),
        UiAuthoringFieldValue::Bool(false)
    );

    interaction.click("Add UI node");
    interaction.click("Node kind");
    interaction.press(Key::DownArrow);
    interaction.press(Key::DownArrow);
    interaction.press(Key::Enter);
    interaction.click("Create image");
    interaction.click(&format!("create-image:{IMAGE}"));
    interaction.click("Apply structure change");
    let authoring = interaction.authoring();
    let created = authoring
        .selection()
        .top_entity()
        .expect("the accepted create selects its node")
        .clone();
    assert_ne!(created.as_str(), "image-node");
    assert!(authoring.projection().unwrap().hierarchy().iter()
        .any(|node| node.entity() == &created && node.kind() == UiAuthoringHierarchyKind::Image));
    assert_eq!(interaction.undo_depth(), 2);
    interaction.click("Delete subtree");
    interaction.click("Apply structure change");
    assert!(
        !interaction
            .authoring()
            .projection()
            .unwrap()
            .hierarchy()
            .iter()
            .any(|node| node.entity() == &created)
    );
    interaction.click("Undo");
    assert!(
        interaction
            .authoring()
            .projection()
            .unwrap()
            .hierarchy()
            .iter()
            .any(|node| node.entity() == &created)
    );
    interaction.click("Redo");
    assert!(
        !interaction
            .authoring()
            .projection()
            .unwrap()
            .hierarchy()
            .iter()
            .any(|node| node.entity() == &created)
    );
    interaction.click("Undo");
    assert_eq!(
        fs::read(&source).unwrap(),
        before,
        "accepted edits remain unsaved until Save"
    );
    let document = interaction.authoring().document();
    interaction.click("Save");
    interaction.settle_persistence();
    assert!(matches!(interaction.frame().persistence().result(),
        Some(EditorPersistenceResult::Saved { document: actual, .. }) if actual == document));
    assert!(
        !interaction
            .frame()
            .workspace()
            .document(document)
            .unwrap()
            .dirty
    );
    let stored: serde_json::Value = serde_json::from_slice(&fs::read(&source).unwrap()).unwrap();
    let entities = stored["payload"]["entities"].as_array().unwrap();
    let original = entities
        .iter()
        .find(|record| record["id"] == "image-node")
        .unwrap();
    let original_node: ComponentValue =
        serde_json::from_value(original["components"]["nara.ui.UiNode"]["value"].clone()).unwrap();
    assert_eq!(
        original_node
            .field("style")
            .unwrap()
            .field("visible")
            .unwrap()
            .as_bool(),
        Some(false)
    );
    let created_source = entities
        .iter()
        .find(|record| record["id"].as_str() == Some(created.as_str()))
        .unwrap();
    let visual: ComponentValue =
        serde_json::from_value(created_source["components"]["nara.ui.UiVisual"]["value"].clone())
            .unwrap();
    let brush = visual.field("background").unwrap();
    assert_eq!(brush.field("kind").unwrap().as_str(), Some("image"));
    let image = brush.field("image").unwrap();
    assert_eq!(image.field("kind").unwrap().as_str(), Some("path"));
    assert_eq!(image.field("value").unwrap().as_str(), Some(IMAGE));
    interaction.click("Dismiss persistence result");
    interaction.click("Reopen");
    interaction.settle_persistence();
    assert!(matches!(
        interaction.frame().persistence().result(),
        Some(EditorPersistenceResult::Opened { .. })
    ));
    assert_eq!(
        interaction.value("image-node", "style.visible"),
        UiAuthoringFieldValue::Bool(false)
    );
    assert!(interaction.authoring().projection().unwrap().hierarchy().iter()
        .any(|node| node.entity() == &created && node.kind() == UiAuthoringHierarchyKind::Image));
    assert_eq!(interaction.undo_depth(), 0);
}
