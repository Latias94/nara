use super::super::clipboard::EditorClipboard;
use super::*;
use nara::{
    reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
        ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
        ComponentSchemaVersion, ComponentTypeId, ComponentValueKind,
        PersistentComponentSchemaOwnerId,
    },
    scene::UnavailableRecordManifest,
    tooling::EditorSceneModel,
};
use nara_ui_kernel::UiSemanticRole;
use winit::{
    dpi::PhysicalPosition,
    event::{DeviceId, ElementState, Ime, MouseScrollDelta, TouchPhase},
    keyboard::{Key as NativeKey, ModifiersState},
};

const COPY_RETAINED: &str = "Copy retained input";
const DISCARD_RETAINED: &str = "Discard retained field drafts";
const RECOVERY_LABEL: &str = "retained 中文 café";
const RETAINED_LIMIT: usize = 64;

#[test]
#[ignore = "requires an interactive Windows desktop and a working DX12 WGPU adapter"]
fn native_owned_inspector_retained_copy_and_capacity_recovery_preserve_source() {
    let copy_project = InteractionProject::new_field_drafts();
    let capacity_project = recovery_capacity_project();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = OwnedRecoveryTestApplication {
        copy_project: copy_project.root.clone(),
        capacity_project: capacity_project.root.clone(),
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    let presents = application
        .result
        .expect("the native retained-input recovery workflow must run")
        .unwrap();
    assert!(presents.into_iter().all(|count| count > 0));
}

fn recovery_capacity_project() -> InteractionProject {
    let project = InteractionProject::new();
    let names = (0..=RETAINED_LIMIT).map(|index| format!("field_{index:02}"));
    let schema = ComponentSchema::new(
        ComponentTypeId::new(CUSTOM_COMPONENT),
        "Retained draft capacity",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields(names.clone().map(|name| {
        ComponentFieldSchema::required(
            ComponentFieldId::new(name.clone()),
            name.clone(),
            ComponentFieldPath::from_fields([name]),
            ComponentValueKind::String,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    }))
    .with_authoring_initial_value(ComponentValue::map(
        names.map(|name| (name, ComponentValue::String("before".into()))),
    ));
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
        project.root.join("catalogs/authoring.bundle.json"),
        bundle.to_json_string().unwrap(),
    )
    .unwrap();
    project
}

struct OwnedRecoveryTestApplication {
    copy_project: PathBuf,
    capacity_project: PathBuf,
    result: Option<Result<[u64; 2], String>>,
}

impl ApplicationHandler for OwnedRecoveryTestApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some((|| {
                let copy = run_native_workflow(event_loop, &self.copy_project, |workflow| {
                    workflow.run_owned_recovery_copy(&self.copy_project)
                })?;
                let capacity =
                    run_native_workflow(event_loop, &self.capacity_project, |workflow| {
                        workflow.run_owned_recovery_capacity(&self.capacity_project)
                    })?;
                Ok([copy, capacity])
            })());
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

struct RecoverySourceSnapshot {
    scene: EditorSceneModel,
    manifest: UnavailableRecordManifest,
    files: Vec<(PathBuf, Vec<u8>)>,
}

impl RecoverySourceSnapshot {
    fn capture(workflow: &AuthoringWorkflow<'_>, project: &Path) -> Result<Self, String> {
        let scene = workflow
            .frame()?
            .workspace()
            .active_scene
            .clone()
            .ok_or("the recovery fixture has no active Scene")?;
        let manifest = workflow
            .application
            .active_authoring_unavailable_record_manifest()
            .ok_or("the recovery fixture has no unavailable-record manifest")?;
        if manifest.total_count() == 0 {
            return Err("recovery must preserve actual unknown source records".into());
        }
        let files = [
            "worlds/entry.scene.json",
            "blueprints/ordinary.prefab.json",
            "src/lib.rs",
        ]
        .into_iter()
        .map(|name| {
            let path = project.join(name);
            fs::read(&path)
                .map(|bytes| (path, bytes))
                .map_err(|error| error.to_string())
        })
        .collect::<Result<_, _>>()?;
        Ok(Self {
            scene,
            manifest,
            files,
        })
    }

    fn unchanged(&self, workflow: &AuthoringWorkflow<'_>, phase: &str) -> Result<(), String> {
        if workflow.frame()?.workspace().active_scene.as_ref() != Some(&self.scene)
            || workflow
                .application
                .active_authoring_unavailable_record_manifest()
                != Some(self.manifest.clone())
        {
            return Err(format!(
                "{phase}: recovery changed source, selection, history or unknown records"
            ));
        }
        for (path, original) in &self.files {
            if fs::read(path).map_err(|error| error.to_string())? != *original {
                return Err(format!("{phase}: recovery wrote {}", path.display()));
            }
        }
        Ok(())
    }
}

impl AuthoringWorkflow<'_> {
    fn owned_recovery_start(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.application.current_frame().is_none() && Instant::now() < deadline {
            self.paint(|_| {})?;
            std::thread::sleep(Duration::from_millis(1));
        }
        self.frame()?;
        self.owned_test_paint()?;
        self.click("+ Root")?;
        self.owned_component_click("Add Component")?;
        self.owned_component_click(&format!("Add {CUSTOM_COMPONENT}"))?;
        self.owned_test_paint()
    }

    fn owned_recovery_feedback(&mut self, field: &str, expected: &str) -> Result<(), String> {
        for _ in 0..3 {
            if self
                .owned_test_named_control(field, UiSemanticRole::Text, expected)
                .is_ok()
            {
                return Ok(());
            }
            self.owned_recovery_scroll(-1.0)?;
        }
        self.owned_test_named_control(field, UiSemanticRole::Text, expected)
            .map(|_| ())
    }

    fn owned_recovery_copy(
        &mut self,
        field: &str,
        clipboard: &EditorClipboard,
        source: &RecoverySourceSnapshot,
    ) -> Result<(), String> {
        let retained = self
            .native
            .frontend
            .retained_field_draft(CUSTOM_COMPONENT, field)
            .ok_or_else(|| format!("field {field:?} has no retained candidate to copy"))?;
        let error = self
            .native
            .frontend
            .retained_field_error(CUSTOM_COMPONENT, field);
        self.owned_test_named_click(field, COPY_RETAINED)?;
        if clipboard.read()? != retained.0
            || self
                .native
                .frontend
                .retained_field_draft(CUSTOM_COMPONENT, field)
                != Some(retained)
            || self
                .native
                .frontend
                .retained_field_error(CUSTOM_COMPONENT, field)
                != error
            || self.native.owned_text_focus_for_test()
        {
            return Err(format!(
                "copying {field:?}: clipboard {:?}, retained {:?}, error {:?}, text focus {}",
                clipboard.read()?,
                self.native
                    .frontend
                    .retained_field_draft(CUSTOM_COMPONENT, field),
                self.native
                    .frontend
                    .retained_field_error(CUSTOM_COMPONENT, field),
                self.native.owned_text_focus_for_test(),
            ));
        }
        self.owned_recovery_feedback(field, "Retained input copied")?;
        source.unchanged(self, "retained input copied")
    }

    fn owned_recovery_shortcut(&mut self, character: &str) -> Result<(), String> {
        self.owned_test_event(WindowEvent::ModifiersChanged(
            ModifiersState::CONTROL.into(),
        ))?;
        for state in [ElementState::Pressed, ElementState::Released] {
            if !self
                .native
                .owned_key(
                    self.native.window.id(),
                    NativeKey::Character(character),
                    None,
                    state,
                    false,
                    self.application,
                )
                .map_err(|error| error.to_string())?
            {
                return Err(format!(
                    "the recovery field did not consume Ctrl+{character}"
                ));
            }
        }
        self.owned_test_event(WindowEvent::ModifiersChanged(
            ModifiersState::empty().into(),
        ))?;
        self.owned_test_paint()
    }

    fn owned_recovery_scroll(&mut self, lines: f32) -> Result<(), String> {
        let inspector = self
            .application
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .find(|instance| instance.kind() == "nara.inspector")
            .ok_or("the recovery fixture has no Inspector panel")?
            .id();
        let (_, _, image) = self
            .native
            .owned_frames_for_test()
            .into_iter()
            .find(|(panel, _, _)| *panel == inspector)
            .ok_or("the recovery fixture has no confirmed Inspector image")?;
        let local = [
            (image.rect.min[0] + image.rect.max[0]) * 0.5,
            (image.rect.min[1] + image.rect.max[1]) * 0.5,
        ];
        self.pointer = [
            image.viewport_origin[0] + local[0],
            image.viewport_origin[1] + local[1],
        ];
        self.paint(|_| {})?;
        if image.occurrence.is_none()
            || !image.owns_native_pointer(&self.native.context, self.pointer)
        {
            return Err("the recovery wheel target has no confirmed native image ownership".into());
        }
        self.owned_test_event(WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: PhysicalPosition::new(
                f64::from(local[0]) * f64::from(image.pixels_per_point),
                f64::from(local[1]) * f64::from(image.pixels_per_point),
            ),
        })?;
        if !self.owned_test_event(WindowEvent::MouseWheel {
            device_id: DeviceId::dummy(),
            delta: MouseScrollDelta::LineDelta(0.0, lines),
            phase: TouchPhase::Moved,
        })? {
            return Err("the recovery Inspector did not consume its real locating wheel".into());
        }
        self.owned_test_paint()
    }

    fn run_owned_recovery_copy(mut self, project: &Path) -> Result<u64, String> {
        self.owned_recovery_start()?;
        let clipboard = EditorClipboard::memory("clipboard sentinel");
        self.native.clipboard = clipboard.clone();
        self.native.context.set_clipboard_backend(clipboard.clone());
        let source = RecoverySourceSnapshot::capture(&self, project)?;
        self.owned_test_document("before", 10, 2, "recovery component creation")?;
        let entity = source
            .scene
            .selection
            .top_entity()
            .ok_or("the recovery entity was not selected")?
            .clone();

        self.owned_test_named_replace("label", "label", RECOVERY_LABEL)?;
        let preedit = "uncommitted IME 中文";
        self.owned_test_ime(Ime::Preedit(preedit.into(), Some((0, preedit.len()))))?;
        self.owned_test_text("label", RECOVERY_LABEL, true)?;
        self.owned_test_draft("label", Some(RECOVERY_LABEL))?;
        self.owned_recovery_copy("label", &clipboard, &source)?;
        self.owned_test_draft("label", Some(RECOVERY_LABEL))?;

        self.owned_test_named_replace("value", "value", "-")?;
        let retained = self
            .native
            .frontend
            .retained_field_draft(CUSTOM_COMPONENT, "value");
        let error = self
            .native
            .frontend
            .retained_field_error(CUSTOM_COMPONENT, "value");
        self.owned_test_draft("value", Some("-"))?;
        if error.is_none() {
            return Err("the invalid numeric candidate has no validation error".into());
        }
        clipboard.write("clipboard sentinel")?;
        clipboard.reject_next_write();
        self.owned_test_named_click("value", COPY_RETAINED)?;
        if clipboard.read()? != "clipboard sentinel"
            || self
                .native
                .frontend
                .retained_field_draft(CUSTOM_COMPONENT, "value")
                != retained
            || self
                .native
                .frontend
                .retained_field_error(CUSTOM_COMPONENT, "value")
                != error
        {
            return Err(
                "a refused retained copy changed the clipboard, candidate or validation error"
                    .into(),
            );
        }
        self.owned_recovery_feedback("value", "Clipboard write failed: clipboard write refused")?;
        source.unchanged(&self, "refused retained copy")?;
        self.owned_recovery_copy("value", &clipboard, &source)?;
        for (field, original, retained) in
            [("value", "10", "-"), ("label", "before", RECOVERY_LABEL)]
        {
            self.owned_test_named_replace(field, field, retained)?;
            self.owned_recovery_shortcut("z")?;
            self.owned_test_text(field, original, false)?;
            self.owned_recovery_shortcut("y")?;
            self.owned_test_text(field, retained, false)?;
            source.unchanged(&self, "local history after retained copy")?;
        }

        self.click("entity:preserved")?;
        self.owned_test_paint()?;
        if self.native.owned_text_focus_for_test() {
            return Err("an unprojected retained field acquired native text editing".into());
        }
        let unprojected = RecoverySourceSnapshot::capture(&self, project)?;
        self.owned_test_draft("label", Some(RECOVERY_LABEL))?;
        self.owned_test_draft("value", Some("-"))?;
        self.owned_recovery_copy("label", &clipboard, &unprojected)?;
        self.owned_recovery_copy("value", &clipboard, &unprojected)?;

        self.click(&format!("entity:{}", entity.as_str()))?;
        let history = self.history()?;
        self.owned_component_click(&format!("Remove {CUSTOM_COMPONENT}"))?;
        if self.history()? != history + 1 {
            return Err("the exact component removal did not create its own history entry".into());
        }
        let removed = RecoverySourceSnapshot::capture(&self, project)?;
        if removed.manifest != source.manifest {
            return Err("component removal changed original unavailable source records".into());
        }
        for (field, expected) in [("label", RECOVERY_LABEL), ("value", "-")] {
            if !self
                .native
                .frontend
                .retained_field_draft(CUSTOM_COMPONENT, field)
                .is_some_and(|draft| draft.0 == expected && draft.2)
            {
                return Err(format!(
                    "removed component field {field:?} was not retained as incompatible"
                ));
            }
            self.owned_recovery_copy(field, &clipboard, &removed)?;
            self.owned_test_named_click(field, "Discard")?;
            self.owned_test_draft(field, None)?;
            removed.unchanged(&self, "incompatible retained draft discarded")?;
        }
        Ok(self.presents)
    }

    fn run_owned_recovery_capacity(mut self, project: &Path) -> Result<u64, String> {
        self.owned_recovery_start()?;
        let clipboard = EditorClipboard::memory("capacity clipboard sentinel");
        self.native.clipboard = clipboard.clone();
        self.native.context.set_clipboard_backend(clipboard.clone());
        let source = RecoverySourceSnapshot::capture(&self, project)?;
        if source.scene.editor.inspector.history.undo_depth != 2 {
            return Err(
                "capacity setup must contain only entity and component creation history".into(),
            );
        }
        self.owned_recovery_scroll(-10_000.0)?;
        let clean_key = self.owned_test_control("field_64", None)?.key;
        self.owned_recovery_scroll(10_000.0)?;
        for index in 0..RETAINED_LIMIT {
            let field = format!("field_{index:02}");
            for _ in 0..8 {
                if self.owned_test_control(&field, None).is_ok_and(|probe| {
                    probe.rect.size().width() > 2.0
                        && probe.rect.size().height() > 2.0
                        && probe.rect.origin().y()
                            < probe.frame.environment().logical_size().height() * 0.5
                }) {
                    break;
                }
                self.owned_recovery_scroll(-2.0)?;
            }
            let retained = format!("retained {index:02} 中文");
            self.owned_test_named_replace(&field, &field, &retained)?;
            self.owned_test_draft(&field, Some(&retained))?;
        }
        source.unchanged(&self, "sixty-four native pointer and IME drafts")?;
        self.owned_recovery_scroll(-10_000.0)?;
        if self.owned_test_named_enabled("field_64", UiSemanticRole::TextInput, "field_64")? {
            return Err("the sixty-fifth field stayed editable at retained draft capacity".into());
        }
        self.owned_test_draft("field_64", None)?;
        self.owned_recovery_scroll(10_000.0)?;
        self.owned_test_select("field_00")?;
        let previous = self.owned_test_control("field_00", None)?;
        let token = self
            .native
            .frontend
            .owned_field_token(previous.panel, &previous.frame, &previous.key)
            .ok_or("the retained capacity field has no native text lifetime")?;
        self.owned_test_named_click("Retained drafts", DISCARD_RETAINED)?;
        for index in 0..RETAINED_LIMIT {
            self.owned_test_draft(&format!("field_{index:02}"), None)?;
        }
        source.unchanged(&self, "native global retained-draft recovery")?;
        let current = self.owned_test_control("field_00", None)?;
        if current.key == previous.key
            || current
                .frame
                .layouts()
                .iter()
                .any(|layout| layout.key() == Some(&previous.key))
            || !current
                .frame
                .layouts()
                .iter()
                .any(|layout| layout.key() == Some(&clean_key))
            || current.frame.semantics().iter().any(|node| {
                node.role() == UiSemanticRole::Group && node.name() == "Retained drafts"
            })
            || self.native.owned_text_focus_for_test()
        {
            return Err(
                "global recovery retained old controls or retired an unrelated clean field".into(),
            );
        }
        if self
            .native
            .frontend
            .owned_edit(
                previous.panel,
                &previous.frame,
                &previous.key,
                token,
                &nara_text::TextEdit::Commit("stale recovered input".into()),
            )
            .is_ok()
        {
            return Err("a recovered field accepted its retired native text token".into());
        }
        self.owned_test_event(WindowEvent::Ime(Ime::Commit("late recovered IME".into())))?;
        self.owned_test_paint()?;
        self.owned_test_draft("field_00", None)?;
        self.owned_recovery_scroll(-10_000.0)?;
        self.owned_test_draft("field_64", None)?;
        self.owned_test_named_text("field_64", "field_64", "before")?;
        self.owned_test_named_replace("field_64", "field_64", "capacity freed 中文")?;
        self.owned_test_draft("field_64", Some("capacity freed 中文"))?;
        self.owned_test_text("field_64", "capacity freed 中文", false)?;
        if clipboard.read()? != "capacity clipboard sentinel" {
            return Err("capacity recovery changed the shared clipboard".into());
        }
        source.unchanged(&self, "native editing after capacity recovery")?;
        Ok(self.presents)
    }
}
