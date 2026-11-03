use super::*;
use crate::{frontend::imgui::ImguiImageInteraction, workspace::EditorPanelInstanceId};
use nara_ui_kernel::{UiFrame, UiKey, UiPoint, UiRect, UiSemanticRole};
use winit::{
    dpi::PhysicalPosition,
    event::{
        DeviceId, ElementState, Ime, MouseButton as NativeMouseButton, MouseScrollDelta, TouchPhase,
    },
};

const OWNED_LABEL: &str = "draft 中文 café";

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn native_owned_inspector_pointer_and_ime_drafts_apply_undo_redo_save_and_reopen() {
    let project = InteractionProject::new_field_drafts();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = OwnedFieldsTestApplication {
        project: project.root.clone(),
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    let observed = application
        .result
        .expect("the Nara-owned Inspector workflow must run")
        .unwrap();
    assert!(observed.presents > 0);
    assert!(observed.unavailable_preserved);
    assert!(observed.original_entities_preserved);
    assert_eq!(
        observed.saved_component,
        ComponentValue::map([
            ("value", ComponentValue::U64(42)),
            ("label", ComponentValue::String(OWNED_LABEL.into())),
            ("other", ComponentValue::I64(0)),
            ("flag", ComponentValue::Bool(false)),
            ("seconds", ComponentValue::f64(0.5).unwrap()),
        ])
    );
}

struct OwnedFieldsObservation {
    presents: u64,
    saved_component: ComponentValue,
    unavailable_preserved: bool,
    original_entities_preserved: bool,
}

struct OwnedFieldsTestApplication {
    project: PathBuf,
    result: Option<Result<OwnedFieldsObservation, String>>,
}

impl ApplicationHandler for OwnedFieldsTestApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_native_workflow(event_loop, &self.project, |workflow| {
                workflow.run_owned_fields(&self.project)
            }));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

pub(super) struct OwnedFieldProbe {
    pub(super) panel: EditorPanelInstanceId,
    pub(super) frame: Arc<UiFrame>,
    pub(super) key: UiKey,
    pub(super) image: ImguiImageInteraction,
    pub(super) rect: UiRect,
}

impl OwnedFieldProbe {
    fn position(&self, horizontal: f32) -> Result<([f32; 2], PhysicalPosition<f64>), String> {
        if self.rect.size().width() <= 2.0 || self.rect.size().height() <= 2.0 {
            return Err("the presented Inspector control is too small for pointer input".into());
        }
        let point = UiPoint::new(
            self.rect.origin().x() + 1.0 + (self.rect.size().width() - 2.0) * horizontal,
            self.rect.origin().y() + self.rect.size().height() * 0.5,
        );
        if self
            .frame
            .hit_test(point)
            .is_none_or(|hit| hit.key() != &self.key)
        {
            return Err("the presented Inspector control does not own its pointer target".into());
        }
        let logical = self.frame.environment().logical_size();
        let local = [
            self.image.rect.min[0]
                + point.x() / logical.width() * (self.image.rect.max[0] - self.image.rect.min[0]),
            self.image.rect.min[1]
                + point.y() / logical.height() * (self.image.rect.max[1] - self.image.rect.min[1]),
        ];
        Ok((
            [
                self.image.viewport_origin[0] + local[0],
                self.image.viewport_origin[1] + local[1],
            ],
            PhysicalPosition::new(
                f64::from(local[0]) * f64::from(self.image.pixels_per_point),
                f64::from(local[1]) * f64::from(self.image.pixels_per_point),
            ),
        ))
    }
}

impl AuthoringWorkflow<'_> {
    pub(super) fn owned_test_control(
        &self,
        field: &str,
        action: Option<&str>,
    ) -> Result<OwnedFieldProbe, String> {
        let (role, name) = match action {
            Some(action) => (UiSemanticRole::Button, action),
            None => (UiSemanticRole::TextInput, field),
        };
        self.owned_test_named_control(field, role, name)
    }

    fn owned_test_named_probe(
        &self,
        field: &str,
        role: UiSemanticRole,
        name: &str,
    ) -> Result<OwnedFieldProbe, String> {
        for (panel, frame, image) in self.native.owned_frames_for_test() {
            let Some(group) = frame
                .semantics()
                .iter()
                .find(|node| node.role() == UiSemanticRole::Group && node.name() == field)
            else {
                continue;
            };
            let mut nodes = frame.semantics().iter().filter(|node| {
                node.role() == role && node.name() == name && node.parent() == Some(group.key())
            });
            let node = nodes
                .next()
                .ok_or_else(|| format!("the presented field {field:?} has no {role:?} {name:?}"))?;
            if nodes.next().is_some() {
                return Err(format!(
                    "the presented field {field:?} has ambiguous {role:?} {name:?} controls"
                ));
            }
            let layout = frame
                .layouts()
                .iter()
                .find(|layout| layout.key() == Some(node.key()))
                .ok_or("the Inspector semantic control has no compiled layout")?;
            if image.occurrence.is_none() {
                return Err(format!(
                    "the Inspector control {field:?}/{name:?} has no confirmed image occurrence"
                ));
            }
            if !frame.diagnostics().is_empty() {
                return Err(format!(
                    "the presented Inspector has diagnostics: {:?}",
                    frame.diagnostics()
                ));
            }
            let main_viewport = self
                .native
                .platform
                .window_snapshot(self.native.window.id())
                .map_err(|error| error.to_string())?
                .ok_or("the native primary window has no platform snapshot")?
                .target
                .viewport_id;
            if image.viewport != main_viewport {
                return Err("the hidden-window Inspector escaped the primary viewport".into());
            }
            let rect = layout.rect();
            return Ok(OwnedFieldProbe {
                panel,
                key: node.key().clone(),
                frame,
                image,
                rect,
            });
        }
        Err(format!(
            "no confirmed Nara Inspector image contains field Group {field:?}"
        ))
    }

    pub(super) fn owned_test_named_control(
        &self,
        field: &str,
        role: UiSemanticRole,
        name: &str,
    ) -> Result<OwnedFieldProbe, String> {
        let mut probe = self.owned_test_named_probe(field, role, name)?;
        let node = probe
            .frame
            .semantics()
            .iter()
            .find(|node| node.key() == &probe.key)
            .ok_or("the Inspector control lost its semantic node")?;
        let layout = probe
            .frame
            .layouts()
            .iter()
            .find(|layout| layout.key() == Some(&probe.key))
            .ok_or("the Inspector control lost its compiled layout")?;
        if !node.enabled() || !layout.visible() {
            return Err(format!(
                "the Inspector control {field:?}/{name:?} is not enabled and presented"
            ));
        }
        probe.rect = match layout.clip_rect() {
            Some(clip) => layout
                .rect()
                .intersect(clip)
                .ok_or("the Inspector control is fully clipped")?,
            None => layout.rect(),
        };
        Ok(probe)
    }

    pub(super) fn owned_test_named_enabled(
        &self,
        field: &str,
        role: UiSemanticRole,
        name: &str,
    ) -> Result<bool, String> {
        let probe = self.owned_test_named_probe(field, role, name)?;
        probe
            .frame
            .semantics()
            .iter()
            .find(|node| node.key() == &probe.key)
            .map(|node| node.enabled())
            .ok_or_else(|| "the Inspector control lost its semantic node".into())
    }

    fn owned_test_reveal_named(
        &mut self,
        field: &str,
        role: UiSemanticRole,
        name: &str,
    ) -> Result<(), String> {
        for vertical in [1.0, -1.0] {
            for _ in 0..48 {
                if let Ok(visible) = self.owned_test_named_control(field, role, name)
                    && visible.rect.size().width() > 2.0
                    && visible.rect.size().height() > 2.0
                {
                    return Ok(());
                }
                let (_, image) = self.owned_test_inspector_image()?;
                self.pointer = [
                    image.viewport_origin[0] + (image.rect.min[0] + image.rect.max[0]) * 0.5,
                    image.viewport_origin[1] + (image.rect.min[1] + image.rect.max[1]) * 0.5,
                ];
                self.paint(|_| {})?;
                let (before, image) = self.owned_test_inspector_image()?;
                let local = [
                    (image.rect.min[0] + image.rect.max[0]) * 0.5,
                    (image.rect.min[1] + image.rect.max[1]) * 0.5,
                ];
                self.pointer = [
                    image.viewport_origin[0] + local[0],
                    image.viewport_origin[1] + local[1],
                ];
                if !image.owns_native_pointer(&self.native.context, self.pointer) {
                    return Err("the actual Inspector image is occluded at the wheel target".into());
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
                    delta: MouseScrollDelta::LineDelta(0.0, vertical),
                    phase: TouchPhase::Moved,
                })? {
                    return Err(
                        "the confirmed Inspector image did not consume its locating wheel".into(),
                    );
                }
                self.owned_test_paint()?;
                let (after, _) = self.owned_test_inspector_image()?;
                if Arc::ptr_eq(&before, &after) {
                    break;
                }
            }
        }
        self.owned_test_named_control(field, role, name)
            .map(|_| ())
            .map_err(|error| format!("after a bounded Inspector wheel search: {error}"))
    }

    fn owned_test_inspector_image(&self) -> Result<(Arc<UiFrame>, ImguiImageInteraction), String> {
        let inspector = self
            .application
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .find(|instance| instance.kind() == "nara.inspector")
            .ok_or("the native fixture has no Inspector panel")?
            .id();
        let (_, frame, image) = self
            .native
            .owned_frames_for_test()
            .into_iter()
            .find(|(panel, _, _)| *panel == inspector)
            .ok_or("the Inspector has no confirmed native image")?;
        if image.occurrence.is_none() || !frame.diagnostics().is_empty() {
            return Err("the Inspector image is unconfirmed or contains diagnostics".into());
        }
        Ok((frame, image))
    }

    pub(super) fn owned_test_named_click(&mut self, field: &str, name: &str) -> Result<(), String> {
        self.owned_test_named_pointer(field, UiSemanticRole::Button, name, 0.5)?;
        for state in [ElementState::Pressed, ElementState::Released] {
            self.owned_test_mouse(state)?;
            self.owned_test_paint()?;
        }
        Ok(())
    }

    pub(super) fn owned_test_named_replace(
        &mut self,
        field: &str,
        name: &str,
        text: &str,
    ) -> Result<(), String> {
        self.owned_test_named_pointer(field, UiSemanticRole::TextInput, name, 1.0)?;
        self.owned_test_mouse(ElementState::Pressed)?;
        let probe = self.owned_test_named_control(field, UiSemanticRole::TextInput, name)?;
        if self.native.owned_focused_control_for_test() != Some((probe.panel, probe.key.clone()))
            || !self.native.owned_text_focus_for_test()
        {
            return Err(format!(
                "the native pointer did not focus TextInput {field:?}/{name:?}"
            ));
        }
        self.owned_test_named_pointer(field, UiSemanticRole::TextInput, name, 0.0)?;
        self.owned_test_mouse(ElementState::Released)?;
        self.owned_test_paint()?;
        self.owned_test_ime(Ime::Enabled)?;
        self.owned_test_ime(Ime::Commit(text.into()))?;
        self.owned_test_named_text(field, name, text)
    }

    pub(super) fn owned_test_named_text(
        &self,
        field: &str,
        name: &str,
        expected: &str,
    ) -> Result<(), String> {
        let probe = self.owned_test_named_probe(field, UiSemanticRole::TextInput, name)?;
        let actual = probe
            .frame
            .semantics()
            .iter()
            .find(|node| node.key() == &probe.key)
            .and_then(|node| node.value());
        if actual != Some(expected) {
            return Err(format!(
                "TextInput {field:?}/{name:?}: expected retained text {expected:?}, observed {actual:?}"
            ));
        }
        Ok(())
    }

    pub(super) fn owned_test_paint(&mut self) -> Result<(), String> {
        for _ in 0..3 {
            self.paint(|_| {})?;
        }
        Ok(())
    }

    fn owned_test_idle(&mut self, field: &str) -> Result<(), String> {
        self.owned_test_paint()?;
        let before = self.owned_test_control(field, None)?;
        let receipt = self
            .native
            .owned
            .get(&before.panel)
            .and_then(|surface| surface.target.display.as_ref())
            .ok_or("the idle Inspector has no retained image")?
            .receipt();
        self.paint(|_| {})?;
        let after = self.owned_test_control(field, None)?;
        let next_receipt = self
            .native
            .owned
            .get(&after.panel)
            .and_then(|surface| surface.target.display.as_ref())
            .ok_or("the unchanged Inspector lost its retained image")?
            .receipt();
        if !Arc::ptr_eq(&before.frame, &after.frame)
            || receipt != next_receipt
            || self.native.ui_followup
        {
            return Err(
                "an unchanged Inspector rebuilt or continuously requested presentation".into(),
            );
        }
        Ok(())
    }

    pub(super) fn owned_test_event(&mut self, event: WindowEvent) -> Result<bool, String> {
        let window = self.native.window.id();
        let consumed = self
            .native
            .handle_owned_event(window, &event, self.application)
            .map_err(|error| error.to_string())?;
        if !consumed {
            self.native
                .platform
                .handle_window_event(&mut self.native.context, window, &event)
                .map_err(|error| error.to_string())?;
        }
        Ok(consumed)
    }

    pub(super) fn owned_test_pointer(
        &mut self,
        field: &str,
        action: Option<&str>,
        horizontal: f32,
    ) -> Result<(), String> {
        let (role, name) = match action {
            Some(action) => (UiSemanticRole::Button, action),
            None => (UiSemanticRole::TextInput, field),
        };
        self.owned_test_point_at_named(field, role, name, horizontal)
    }

    fn owned_test_named_pointer(
        &mut self,
        field: &str,
        role: UiSemanticRole,
        name: &str,
        horizontal: f32,
    ) -> Result<(), String> {
        self.owned_test_reveal_named(field, role, name)?;
        self.owned_test_point_at_named(field, role, name, horizontal)
    }

    fn owned_test_point_at_named(
        &mut self,
        field: &str,
        role: UiSemanticRole,
        name: &str,
        horizontal: f32,
    ) -> Result<(), String> {
        self.pointer = self
            .owned_test_named_control(field, role, name)?
            .position(horizontal)?
            .0;
        self.paint(|_| {})?;
        let probe = self.owned_test_named_control(field, role, name)?;
        let (global, position) = probe.position(horizontal)?;
        self.pointer = global;
        if !probe
            .image
            .owns_native_pointer(&self.native.context, global)
        {
            return Err("the actual Inspector image is occluded at the synthetic pointer".into());
        }
        self.owned_test_event(WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position,
        })?;
        Ok(())
    }

    pub(super) fn owned_test_mouse(&mut self, state: ElementState) -> Result<(), String> {
        self.owned_test_event(WindowEvent::MouseInput {
            device_id: DeviceId::dummy(),
            state,
            button: NativeMouseButton::Left,
        })?;
        Ok(())
    }

    pub(super) fn owned_test_select(&mut self, field: &str) -> Result<(), String> {
        self.owned_test_pointer(field, None, 1.0)?;
        self.owned_test_mouse(ElementState::Pressed)?;
        if !self.native.owned_text_focus_for_test() {
            return Err(format!(
                "the native pointer did not focus TextInput {field:?}"
            ));
        }
        self.owned_test_pointer(field, None, 0.0)?;
        self.owned_test_mouse(ElementState::Released)?;
        self.owned_test_paint()?;
        if !self.native.owned_text_focus_for_test() {
            return Err("redrawing the selected field revoked its native text lifetime".into());
        }
        Ok(())
    }

    pub(super) fn owned_test_ime(&mut self, ime: Ime) -> Result<(), String> {
        if !self.owned_test_event(WindowEvent::Ime(ime))? {
            return Err("the owned Inspector did not consume its native IME event".into());
        }
        self.owned_test_paint()
    }

    pub(super) fn owned_test_apply(&mut self, field: &str) -> Result<(), String> {
        self.owned_test_pointer(field, Some("Apply"), 0.5)?;
        self.owned_test_mouse(ElementState::Pressed)?;
        self.owned_test_paint()?;
        self.owned_test_mouse(ElementState::Released)?;
        self.owned_test_paint()?;
        self.owned_test_draft(field, None)
    }

    pub(super) fn owned_test_text(
        &self,
        field: &str,
        expected: &str,
        composing: bool,
    ) -> Result<(), String> {
        let probe = self.owned_test_control(field, None)?;
        let value = probe
            .frame
            .semantics()
            .iter()
            .find(|node| node.key() == &probe.key)
            .and_then(|node| node.value());
        let token = self
            .native
            .frontend
            .owned_field_token(probe.panel, &probe.frame, &probe.key)
            .ok_or("the presented TextInput has no live native text token")?;
        let actual_composing =
            self.native
                .frontend
                .owned_composing(probe.panel, &probe.frame, &probe.key, token);
        if value != Some(expected) || actual_composing != composing {
            return Err(format!(
                "TextInput {field:?}: committed semantic value {value:?}, composing {actual_composing}; expected {expected:?}, {composing}"
            ));
        }
        Ok(())
    }

    pub(super) fn owned_test_draft(
        &self,
        field: &str,
        expected: Option<&str>,
    ) -> Result<(), String> {
        let actual = self
            .native
            .frontend
            .retained_field_draft(CUSTOM_COMPONENT, field);
        let expected = expected.map(|text| (text.to_owned(), false, false, false));
        if actual != expected {
            return Err(format!(
                "the owned field {field:?} retained draft {actual:?}; expected {expected:?}"
            ));
        }
        Ok(())
    }

    pub(super) fn owned_test_document(
        &self,
        label: &str,
        value: u64,
        history: usize,
        phase: &str,
    ) -> Result<(), String> {
        let frame = self.frame()?;
        let component = frame
            .workspace()
            .active_scene
            .as_ref()
            .and_then(|scene| scene.editor.inspector.selected_entity_view.as_ref())
            .and_then(|entity| {
                entity
                    .components
                    .iter()
                    .find(|component| component.component.as_str() == CUSTOM_COMPONENT)
            })
            .ok_or("the ordinary selected entity lost its custom data component")?;
        for (field, expected) in [
            ("label", ComponentValue::String(label.into())),
            ("value", ComponentValue::U64(value)),
        ] {
            let actual = component
                .fields
                .iter()
                .find(|candidate| candidate.id.as_str() == field)
                .and_then(|field| field.value.as_ref());
            if actual != Some(&expected) {
                return Err(format!(
                    "{phase}: document field {field:?} was {actual:?}; expected {expected:?}"
                ));
            }
        }
        let actual = self.history()?;
        if actual != history {
            return Err(format!(
                "{phase}: document Undo depth was {actual}; expected {history}"
            ));
        }
        Ok(())
    }

    fn run_owned_fields(mut self, project: &Path) -> Result<OwnedFieldsObservation, String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            self.paint(|_| {})?;
            if self.application.current_frame().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        self.frame()?;
        self.owned_test_paint()?;
        let source = project.join("worlds/entry.scene.json");
        let original = read_source(&source)?;
        let initial_manifest = self
            .application
            .active_authoring_unavailable_record_manifest()
            .ok_or("the field fixture has no unavailable-record manifest")?;
        if initial_manifest.total_count() == 0 {
            return Err("the owned field fixture must retain actual unknown source data".into());
        }
        self.click("+ Root")?;
        self.click("Add Component")?;
        self.click(CUSTOM_COMPONENT)?;
        let frame = self.frame()?;
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the ordinary source has no active Scene")?;
        let document = scene.document;
        let entity = scene
            .selection
            .top_entity()
            .ok_or("the real Root widget did not select its entity")?
            .clone();
        let manifest = self
            .application
            .active_authoring_unavailable_record_manifest()
            .ok_or("the created entity lost the unavailable-record manifest")?;
        self.owned_test_document("before", 10, 2, "component creation")?;
        self.click("Inspector tab: Nara properties")?;
        self.owned_test_paint()?;
        self.owned_test_idle("label")?;

        self.owned_test_select("label")?;
        self.owned_test_ime(Ime::Enabled)?;
        let canceled = "discard café";
        self.owned_test_ime(Ime::Preedit(canceled.into(), Some((0, canceled.len()))))?;
        self.owned_test_text("label", "before", true)?;
        self.owned_test_draft("label", None)?;
        self.owned_test_document("before", 10, 2, "string preedit")?;
        self.owned_test_ime(Ime::Disabled)?;
        self.owned_test_text("label", "before", false)?;
        self.owned_test_draft("label", None)?;
        self.owned_test_document("before", 10, 2, "composition cancellation")?;

        let original_focus = self.owned_test_control("label", None)?;
        let original_token = self
            .native
            .frontend
            .owned_field_token(
                original_focus.panel,
                &original_focus.frame,
                &original_focus.key,
            )
            .ok_or("the original field has no text focus lifetime")?;
        self.pointer = [-1000.0, -1000.0];
        self.paint(|_| {})?;
        self.owned_test_event(WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: PhysicalPosition::new(-1000.0, -1000.0),
        })?;
        self.owned_test_mouse(ElementState::Pressed)?;
        self.owned_test_mouse(ElementState::Released)?;
        if self.native.owned_text_focus_for_test() {
            return Err("clicking outside the Inspector retained native text focus".into());
        }
        self.owned_test_select("label")?;
        let replacement = self.owned_test_control("label", None)?;
        if self.native.frontend.owned_field_token(
            replacement.panel,
            &replacement.frame,
            &replacement.key,
        ) == Some(original_token)
        {
            return Err("refocusing the same field reused its retired native text lifetime".into());
        }
        self.owned_test_ime(Ime::Commit("late".into()))?;
        self.owned_test_text("label", "before", false)?;

        self.owned_test_ime(Ime::Enabled)?;
        self.owned_test_ime(Ime::Preedit(
            OWNED_LABEL.into(),
            Some((OWNED_LABEL.len(), OWNED_LABEL.len())),
        ))?;
        self.owned_test_text("label", "before", true)?;
        self.owned_test_document("before", 10, 2, "replacement preedit")?;
        self.owned_test_ime(Ime::Commit(OWNED_LABEL.into()))?;
        self.owned_test_text("label", OWNED_LABEL, false)?;
        self.owned_test_draft("label", Some(OWNED_LABEL))?;
        self.owned_test_document("before", 10, 2, "string commit without Apply")?;
        self.click("Inspector tab: Source asset")?;
        self.owned_test_draft("label", Some(OWNED_LABEL))?;
        self.owned_test_document("before", 10, 2, "Inspector tab change without Apply")?;
        self.click("Inspector tab: Nara properties")?;
        self.owned_test_paint()?;
        self.owned_test_select("value")?;
        self.owned_test_draft("label", Some(OWNED_LABEL))?;
        self.owned_test_document("before", 10, 2, "string focus loss without Apply")?;
        self.owned_test_apply("label")?;
        self.owned_test_document(OWNED_LABEL, 10, 3, "explicit string Apply")?;

        self.owned_test_select("value")?;
        self.owned_test_ime(Ime::Enabled)?;
        self.owned_test_ime(Ime::Preedit("42".into(), Some((0, 2))))?;
        self.owned_test_text("value", "10", true)?;
        self.owned_test_draft("value", None)?;
        self.owned_test_document(OWNED_LABEL, 10, 3, "numeric preedit")?;
        self.owned_test_ime(Ime::Commit("42".into()))?;
        self.owned_test_text("value", "42", false)?;
        self.owned_test_draft("value", Some("42"))?;
        self.owned_test_document(OWNED_LABEL, 10, 3, "numeric commit without Apply")?;
        self.owned_test_apply("value")?;
        self.owned_test_document(OWNED_LABEL, 42, 4, "explicit numeric Apply")?;
        self.click("Undo")?;
        self.owned_test_document(OWNED_LABEL, 10, 3, "toolbar document Undo")?;
        self.click("Redo")?;
        self.owned_test_document(OWNED_LABEL, 42, 4, "toolbar document Redo")?;

        let manifest_after_edit = self
            .application
            .active_authoring_unavailable_record_manifest()
            == Some(manifest.clone());
        self.click("Save")?;
        self.settle_persistence()?;
        if !matches!(self.frame()?.persistence().result(), Some(EditorPersistenceResult::Saved { document: actual, .. }) if actual == document)
            || self.dirty()?
        {
            return Err("the real Save widget did not persist the owned Inspector edits".into());
        }
        let stored = read_source(&source)?;
        let entities = stored["payload"]["entities"]
            .as_array()
            .ok_or("the saved Scene has no entity records")?;
        let created = entities
            .iter()
            .find(|record| record["id"].as_str() == Some(entity.as_str()))
            .ok_or("the owned Inspector's entity was not saved")?;
        let saved_component =
            serde_json::from_value(created["components"][CUSTOM_COMPONENT]["value"].clone())
                .map_err(|error| error.to_string())?;
        let retained_entities = entities
            .iter()
            .filter(|record| record["id"].as_str() != Some(entity.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let original_entities_preserved =
            original["payload"]["entities"].as_array() == Some(&retained_entities);
        self.click("Dismiss persistence result")?;
        self.click("Reopen")?;
        self.settle_persistence()?;
        if !matches!(
            self.frame()?.persistence().result(),
            Some(EditorPersistenceResult::Opened { .. })
        ) {
            return Err("the real Reopen widget did not reopen the owned Inspector edits".into());
        }
        self.click(&format!("entity:{}", entity.as_str()))?;
        self.owned_test_document(OWNED_LABEL, 42, 0, "saved Scene Reopen")?;
        if self.dirty()? {
            return Err("the reopened owned Inspector source is still dirty".into());
        }
        self.click("Dismiss persistence result")?;
        self.owned_test_select("label")?;
        self.owned_test_text("label", OWNED_LABEL, false)?;
        self.owned_test_draft("label", None)?;
        self.owned_test_document(OWNED_LABEL, 42, 0, "reopened native field selection")?;
        Ok(OwnedFieldsObservation {
            presents: self.presents,
            saved_component,
            unavailable_preserved: manifest_after_edit
                && self
                    .application
                    .active_authoring_unavailable_record_manifest()
                    == Some(manifest),
            original_entities_preserved,
        })
    }
}
