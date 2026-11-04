//! Exercises the production logical-key entry used by Winit event ingestion. Winit's private
//! platform field prevents constructing `KeyEvent`; this covers normalization and native routing,
//! hidden-window presentation and a shared memory clipboard, not OS key or clipboard delivery.

use super::super::clipboard::EditorClipboard;
use super::*;
use winit::{
    dpi::PhysicalPosition,
    event::{DeviceId, ElementState, Ime, MouseButton as NativeMouseButton},
    keyboard::{Key as NativeKey, ModifiersState, NamedKey},
};

const KEYBOARD_LABEL: &str = "keyboard 中文 café";
const SAVED_LABEL: &str = "saved café 中文";

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn native_owned_inspector_keyboard_clipboard_navigation_preserve_drafts_and_source() {
    let project = InteractionProject::new_field_drafts();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = OwnedKeyboardTestApplication {
        project: project.root.clone(),
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    assert!(
        application
            .result
            .expect("the native owned keyboard workflow must run")
            .unwrap()
            > 0
    );
}

struct OwnedKeyboardTestApplication {
    project: PathBuf,
    result: Option<Result<u64, String>>,
}

impl ApplicationHandler for OwnedKeyboardTestApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_native_workflow(event_loop, &self.project, |workflow| {
                workflow.run_owned_keyboard(&self.project)
            }));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

impl AuthoringWorkflow<'_> {
    fn owned_keyboard_flush_platform(&mut self) -> Result<(), String> {
        self.application.advance_project_open();
        self.application
            .drive_editor_frame(Duration::ZERO, false, false);
        clear_controls();
        let pointer = self.pointer;
        let exited = self
            .native
            .redraw_with_input(self.event_loop, self.application, |context| {
                let viewport = context.main_viewport().id();
                context.io_mut().add_focus_event(true);
                context.io_mut().add_mouse_pos_event(pointer);
                context.io_mut().add_mouse_viewport_event(viewport);
            })
            .map_err(|error| error.to_string())?;
        if exited {
            return Err("keyboard input unexpectedly requested application exit".into());
        }
        self.presents += self.native.gpu.frame_transaction_stats().presents();
        Ok(())
    }

    fn owned_keyboard_key(
        &mut self,
        logical: NativeKey<&str>,
        text: Option<&str>,
    ) -> Result<(), String> {
        let window = self.native.window.id();
        for state in [ElementState::Pressed, ElementState::Released] {
            if !self
                .native
                .owned_key(
                    window,
                    logical.clone(),
                    text,
                    state,
                    false,
                    self.application,
                )
                .map_err(|error| error.to_string())?
            {
                return Err(format!(
                    "the owned Inspector did not consume logical key {logical:?}/{state:?}"
                ));
            }
        }
        Ok(())
    }

    fn owned_keyboard_shortcut(&mut self, character: &str) -> Result<(), String> {
        self.owned_test_event(WindowEvent::ModifiersChanged(
            ModifiersState::CONTROL.into(),
        ))?;
        self.owned_keyboard_key(NativeKey::Character(character), None)?;
        self.owned_test_event(WindowEvent::ModifiersChanged(
            ModifiersState::empty().into(),
        ))?;
        Ok(())
    }

    fn owned_keyboard_tab(&mut self, backwards: bool) -> Result<(), String> {
        let modifiers = if backwards {
            ModifiersState::SHIFT
        } else {
            ModifiersState::empty()
        };
        self.owned_test_event(WindowEvent::ModifiersChanged(modifiers.into()))?;
        self.owned_keyboard_key(NativeKey::Named(NamedKey::Tab), None)?;
        self.owned_test_event(WindowEvent::ModifiersChanged(
            ModifiersState::empty().into(),
        ))?;
        Ok(())
    }

    fn owned_keyboard_focus(&self, field: &str, action: Option<&str>) -> Result<(), String> {
        let expected = self.owned_test_control(field, action)?;
        let text_token =
            self.native
                .frontend
                .owned_field_token(expected.panel, &expected.frame, &expected.key);
        let actual = self.native.owned_focused_control_for_test();
        if actual != Some((expected.panel, expected.key)) {
            return Err(format!(
                "native keyboard focus was {actual:?}; expected enabled control {field:?}/{action:?}"
            ));
        }
        if self.native.owned_text_focus_for_test() != action.is_none()
            || text_token.is_some() != action.is_none()
        {
            return Err("native text ownership does not match the focused control's role".into());
        }
        Ok(())
    }

    fn owned_keyboard_custom_field(&mut self) -> Result<(), String> {
        let label = format!("field:{CUSTOM_COMPONENT}:label");
        let rect = control_rect(&label).ok_or("the custom field was not painted")?;
        self.pointer = [
            (rect.min[0] + rect.max[0]) * 0.5,
            (rect.min[1] + rect.max[1]) * 0.5,
        ];
        self.paint(|_| {})?;
        let origin = self.native.context.main_viewport().pos();
        let scale = f64::from(self.native.context.io().display_framebuffer_scale()[0]);
        self.owned_test_event(WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: PhysicalPosition::new(
                f64::from(self.pointer[0] - origin[0]) * scale,
                f64::from(self.pointer[1] - origin[1]) * scale,
            ),
        })?;
        self.owned_keyboard_flush_platform()?;
        for state in [ElementState::Pressed, ElementState::Released] {
            if self.owned_test_event(WindowEvent::MouseInput {
                device_id: DeviceId::dummy(),
                state,
                button: NativeMouseButton::Left,
            })? {
                return Err("the owned image consumed a custom Dear field click".into());
            }
            self.owned_keyboard_flush_platform()?;
        }
        if self.native.owned_text_focus_for_test() || self.native.input.text_owner_item().is_none()
        {
            return Err("native text focus did not hand off to the custom Dear field".into());
        }
        Ok(())
    }

    fn owned_keyboard_flag(
        &self,
        expected: bool,
        history: usize,
        phase: &str,
    ) -> Result<(), String> {
        self.owned_test_document(SAVED_LABEL, 10, history, phase)?;
        let frame = self.frame()?;
        let actual = frame
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
            .and_then(|component| {
                component
                    .fields
                    .iter()
                    .find(|field| field.id.as_str() == "flag")
            })
            .and_then(|field| field.value.as_ref());
        if actual != Some(&ComponentValue::Bool(expected)) {
            return Err(format!(
                "{phase}: document flag was {actual:?}; expected Bool({expected})"
            ));
        }
        let action = if expected { "Set false" } else { "Set true" };
        let probe = self.owned_test_control("flag", Some(action))?;
        let value = probe
            .frame
            .semantics()
            .iter()
            .find(|node| node.key() == &probe.key)
            .and_then(|node| node.value());
        let semantic = if expected { "true" } else { "false" };
        if value != Some(semantic) {
            return Err(format!(
                "{phase}: boolean button semantic value was {value:?}; expected {semantic:?}"
            ));
        }
        Ok(())
    }

    fn run_owned_keyboard(mut self, project: &Path) -> Result<u64, String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            self.paint(|_| {})?;
            if self.application.current_frame().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        self.frame()?;
        let custom = self.install_native_custom_fields(CUSTOM_COMPONENT, &["label"])?;
        self.application
            .panel_workspace_mut()
            .layout_mut()
            .set_visible(custom, false);
        self.owned_test_paint()?;
        let source = project.join("worlds/entry.scene.json");
        let original = read_source(&source)?;
        self.click("+ Root")?;
        self.owned_component_click("Add Component")?;
        self.owned_component_click(&format!("Add {CUSTOM_COMPONENT}"))?;
        let frame = self.frame()?;
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the keyboard fixture has no ordinary Scene")?;
        let document = scene.document;
        let entity = scene
            .selection
            .top_entity()
            .ok_or("the Root widget did not select the keyboard fixture entity")?
            .clone();
        let manifest = self
            .application
            .active_authoring_unavailable_record_manifest()
            .ok_or("the keyboard fixture has no unavailable-record manifest")?;
        if manifest.total_count() == 0 {
            return Err("the keyboard fixture must preserve actual unknown source data".into());
        }
        self.owned_test_document("before", 10, 2, "component creation")?;
        self.owned_test_paint()?;

        let clipboard = EditorClipboard::memory("clipboard sentinel");
        self.native.clipboard = clipboard.clone();
        self.native.context.set_clipboard_backend(clipboard.clone());
        self.native
            .context
            .set_clipboard_text("shared shell clipboard");
        if clipboard.read()? != "shared shell clipboard" {
            return Err(
                "the workspace shell and owned controls use different clipboard owners".into(),
            );
        }
        clipboard.write("shared owned clipboard")?;
        if self.native.context.clipboard_text().as_deref() != Some("shared owned clipboard") {
            return Err("the workspace shell cannot read the owned clipboard".into());
        }
        self.owned_test_event(WindowEvent::ModifiersChanged(
            ModifiersState::CONTROL.into(),
        ))?;
        self.owned_keyboard_flush_platform()?;
        self.owned_test_select("label")?;
        if !self.native.context.io().key_ctrl() {
            return Err("owned text acquisition cleared a physically held Ctrl modifier".into());
        }
        self.application
            .panel_workspace_mut()
            .layout_mut()
            .set_visible(custom, true);
        self.owned_test_paint()?;
        self.owned_keyboard_custom_field()?;
        self.paint(|context| {
            context.io_mut().add_key_event(Key::A, true);
            context.io_mut().add_input_characters_utf8("a");
        })?;
        self.paint(|context| context.io_mut().add_key_event(Key::A, false))?;
        self.owned_test_event(WindowEvent::ModifiersChanged(
            ModifiersState::empty().into(),
        ))?;
        self.owned_keyboard_flush_platform()?;
        self.paint(|context| {
            context
                .io_mut()
                .add_input_characters_utf8("after held Ctrl")
        })?;
        self.owned_test_draft("label", Some("after held Ctrl"))?;
        self.owned_test_document("before", 10, 2, "held Ctrl handoff to custom Dear text")?;
        self.application
            .panel_workspace_mut()
            .layout_mut()
            .set_visible(custom, false);
        self.owned_test_paint()?;
        self.owned_test_select("label")?;
        self.owned_test_text("label", "before", false)?;
        self.owned_test_draft("label", None)?;
        self.owned_keyboard_focus("label", None)?;
        self.owned_keyboard_tab(false)?;
        self.owned_test_paint()?;
        self.owned_keyboard_focus("other", None)?;
        self.owned_keyboard_tab(true)?;
        self.owned_test_paint()?;
        self.owned_keyboard_focus("label", None)?;

        self.owned_keyboard_shortcut("a")?;
        self.owned_keyboard_key(NativeKey::Character("é"), Some("é"))?;
        self.owned_test_paint()?;
        self.owned_test_text("label", "é", false)?;
        self.owned_test_event(WindowEvent::ModifiersChanged(
            (ModifiersState::CONTROL | ModifiersState::ALT).into(),
        ))?;
        self.owned_keyboard_key(NativeKey::Character("q"), Some("@"))?;
        self.owned_test_event(WindowEvent::ModifiersChanged(
            ModifiersState::empty().into(),
        ))?;
        self.owned_test_paint()?;
        self.owned_test_text("label", "é@", false)?;
        self.owned_test_event(WindowEvent::ModifiersChanged(ModifiersState::ALT.into()))?;
        self.owned_keyboard_key(NativeKey::Character("x"), Some("x"))?;
        self.owned_test_event(WindowEvent::ModifiersChanged(
            ModifiersState::empty().into(),
        ))?;
        self.owned_test_paint()?;
        self.owned_test_text("label", "é@", false)?;
        self.owned_test_document("before", 10, 2, "AltGr input and Alt command isolation")?;
        self.owned_keyboard_shortcut("z")?;
        self.owned_test_paint()?;
        self.owned_test_text("label", "é", false)?;
        self.owned_test_document("before", 10, 2, "local Undo after AltGr input")?;
        clipboard.write(KEYBOARD_LABEL)?;
        self.owned_keyboard_shortcut("a")?;
        self.owned_keyboard_shortcut("v")?;
        self.owned_test_paint()?;
        self.owned_test_text("label", KEYBOARD_LABEL, false)?;
        self.owned_test_draft("label", Some(KEYBOARD_LABEL))?;

        self.owned_keyboard_shortcut("a")?;
        clipboard.write("copy sentinel")?;
        self.owned_keyboard_shortcut("C")?;
        if clipboard.read()? != KEYBOARD_LABEL {
            return Err("Unicode Copy did not reach the shared native clipboard".into());
        }
        clipboard.write("cut refusal sentinel")?;
        clipboard.reject_next_write();
        self.owned_keyboard_shortcut("x")?;
        self.owned_test_paint()?;
        self.owned_test_text("label", KEYBOARD_LABEL, false)?;
        self.owned_test_draft("label", Some(KEYBOARD_LABEL))?;
        if clipboard.read()? != "cut refusal sentinel" {
            return Err("a refused Cut changed the clipboard".into());
        }
        self.owned_keyboard_shortcut("c")?;
        if clipboard.read()? != KEYBOARD_LABEL {
            return Err("a refused Cut changed the Unicode selection".into());
        }
        self.owned_keyboard_shortcut("X")?;
        self.owned_test_paint()?;
        self.owned_test_text("label", "", false)?;
        self.owned_test_draft("label", Some(""))?;
        if clipboard.read()? != KEYBOARD_LABEL {
            return Err("Unicode Cut did not retain the complete selected text".into());
        }
        self.owned_keyboard_shortcut("V")?;
        self.owned_test_paint()?;
        self.owned_test_text("label", KEYBOARD_LABEL, false)?;

        self.owned_keyboard_shortcut("a")?;
        for rejected in [
            "line\nbreak".to_owned(),
            "x".repeat(nara_text::TextLimits::default().max_text_bytes + 1),
        ] {
            clipboard.write(&rejected)?;
            self.owned_keyboard_shortcut("v")?;
            self.owned_test_paint()?;
            self.owned_test_text("label", KEYBOARD_LABEL, false)?;
            self.owned_test_draft("label", Some(KEYBOARD_LABEL))?;
            if self
                .native
                .frontend
                .retained_field_error(CUSTOM_COMPONENT, "label")
                .is_none()
            {
                return Err("a rejected paste did not expose its field error".into());
            }
        }
        self.owned_keyboard_shortcut("z")?;
        self.owned_test_paint()?;
        self.owned_test_text("label", "", false)?;
        self.owned_test_document("before", 10, 2, "local Undo after rejected pastes")?;
        self.owned_keyboard_shortcut("y")?;
        self.owned_test_paint()?;
        self.owned_test_text("label", KEYBOARD_LABEL, false)?;
        self.owned_test_document("before", 10, 2, "local Redo after rejected pastes")?;

        self.owned_keyboard_shortcut("a")?;
        self.owned_test_ime(Ime::Enabled)?;
        self.owned_test_ime(Ime::Preedit("pending".into(), Some((0, 7))))?;
        clipboard.write("IME clipboard guard")?;
        for shortcut in ["c", "x", "v"] {
            self.owned_keyboard_shortcut(shortcut)?;
            self.owned_test_paint()?;
            self.owned_test_text("label", KEYBOARD_LABEL, true)?;
            self.owned_test_draft("label", Some(KEYBOARD_LABEL))?;
            self.owned_test_document("before", 10, 2, "clipboard during composition")?;
            if clipboard.read()? != "IME clipboard guard" {
                return Err(format!(
                    "Ctrl+{shortcut} changed the clipboard during composition"
                ));
            }
        }
        self.owned_test_ime(Ime::Disabled)?;
        self.owned_test_text("label", KEYBOARD_LABEL, false)?;
        self.owned_keyboard_shortcut("c")?;
        if clipboard.read()? != KEYBOARD_LABEL {
            return Err("canceling composition did not restore the original selection".into());
        }

        self.owned_keyboard_tab(false)?;
        self.owned_test_paint()?;
        self.owned_keyboard_focus("label", Some("Apply"))?;
        self.owned_keyboard_tab(true)?;
        self.owned_test_paint()?;
        self.owned_keyboard_focus("label", None)?;
        self.owned_keyboard_tab(false)?;
        self.owned_test_paint()?;
        self.owned_keyboard_focus("label", Some("Apply"))?;
        for _ in 0..2 {
            self.owned_test_pointer("label", Some("Apply"), 0.5)?;
            self.owned_test_mouse(ElementState::Pressed)?;
            self.owned_test_event(WindowEvent::CursorLeft {
                device_id: winit::event::DeviceId::dummy(),
            })?;
            self.owned_test_mouse(ElementState::Released)?;
            self.owned_test_paint()?;
            self.owned_test_document(
                "before",
                10,
                2,
                "canceled Apply pointer before keyboard activation",
            )?;
            self.owned_keyboard_focus("label", Some("Apply"))?;
        }
        self.owned_keyboard_key(NativeKey::Named(NamedKey::Enter), None)?;
        self.owned_test_paint()?;
        self.owned_test_document(KEYBOARD_LABEL, 10, 3, "Enter on Apply exact result")?;
        self.owned_test_draft("label", None)?;

        self.owned_test_select("label")?;
        clipboard.write(SAVED_LABEL)?;
        self.owned_keyboard_shortcut("a")?;
        self.owned_keyboard_shortcut("v")?;
        self.owned_test_paint()?;
        self.owned_test_text("label", SAVED_LABEL, false)?;
        self.owned_test_draft("label", Some(SAVED_LABEL))?;
        self.owned_keyboard_tab(false)?;
        self.owned_test_paint()?;
        self.owned_keyboard_focus("label", Some("Apply"))?;
        self.owned_keyboard_key(NativeKey::Named(NamedKey::Space), Some(" "))?;
        self.owned_keyboard_tab(true)?;
        self.owned_keyboard_focus("label", None)?;
        self.owned_keyboard_key(NativeKey::Character("!"), Some("!"))?;
        clipboard.write("queued paste must wait")?;
        self.owned_keyboard_shortcut("v")?;
        let queued = self
            .native
            .frontend
            .retained_field_draft(CUSTOM_COMPONENT, "label");
        if queued != Some((SAVED_LABEL.into(), false, false, true)) {
            return Err(format!(
                "editing before paint replaced the exact queued draft: {queued:?}"
            ));
        }
        self.owned_test_document(KEYBOARD_LABEL, 10, 3, "queued Apply before paint")?;
        self.owned_test_paint()?;
        self.owned_test_document(SAVED_LABEL, 10, 4, "Space on Apply exact result")?;
        self.owned_test_draft("label", None)?;
        self.owned_test_select("label")?;
        self.owned_test_text("label", SAVED_LABEL, false)?;

        self.click("Undo")?;
        self.owned_test_document(KEYBOARD_LABEL, 10, 3, "toolbar document Undo")?;
        self.click("Redo")?;
        self.owned_test_document(SAVED_LABEL, 10, 4, "toolbar document Redo")?;

        self.owned_keyboard_flag(false, 4, "before boolean activation")?;
        self.owned_test_pointer("flag", Some("Set true"), 0.5)?;
        self.owned_test_mouse(ElementState::Pressed)?;
        self.owned_keyboard_focus("flag", Some("Set true"))?;
        self.owned_keyboard_key(NativeKey::Character("!"), Some("!"))?;
        for ime in [
            Ime::Enabled,
            Ime::Preedit("pending".into(), Some((0, 7))),
            Ime::Commit("true".into()),
            Ime::Disabled,
        ] {
            if self.owned_test_event(WindowEvent::Ime(ime))? {
                return Err("the boolean button acquired native IME ownership".into());
            }
        }
        self.owned_keyboard_focus("flag", Some("Set true"))?;
        self.owned_test_draft("flag", None)?;
        self.owned_test_draft("label", None)?;
        self.owned_keyboard_flag(false, 4, "boolean text and IME isolation")?;
        self.owned_test_mouse(ElementState::Released)?;
        for _ in 0..2 {
            self.owned_test_mouse(ElementState::Pressed)?;
            self.owned_test_mouse(ElementState::Released)?;
            self.owned_keyboard_key(NativeKey::Named(NamedKey::Enter), None)?;
            self.owned_keyboard_key(NativeKey::Named(NamedKey::Space), Some(" "))?;
            let queued = self
                .native
                .frontend
                .retained_field_draft(CUSTOM_COMPONENT, "flag");
            if queued != Some(("true".into(), false, false, true)) {
                return Err(format!(
                    "repeated activation before paint replaced the queued boolean: {queued:?}"
                ));
            }
            self.owned_keyboard_focus("flag", Some("Set true"))?;
            self.owned_keyboard_flag(false, 4, "queued pointer toggle before paint")?;
        }
        self.owned_test_paint()?;
        self.owned_keyboard_flag(true, 5, "pointer boolean exact result")?;
        self.owned_test_draft("flag", None)?;

        for (action, key, text, expected, history) in [
            ("Set false", NamedKey::Enter, None, false, 6),
            ("Set true", NamedKey::Space, Some(" "), true, 7),
        ] {
            self.owned_test_select("label")?;
            self.owned_keyboard_tab(true)?;
            self.owned_test_paint()?;
            self.owned_keyboard_focus("flag", Some(action))?;
            self.owned_keyboard_key(NativeKey::Named(key), text)?;
            self.owned_keyboard_key(NativeKey::Named(NamedKey::Enter), None)?;
            self.owned_keyboard_key(NativeKey::Named(NamedKey::Space), Some(" "))?;
            let queued = self
                .native
                .frontend
                .retained_field_draft(CUSTOM_COMPONENT, "flag");
            if queued != Some((expected.to_string(), false, false, true)) {
                return Err(format!(
                    "repeated keyboard activation changed the exact queued boolean: {queued:?}"
                ));
            }
            self.owned_keyboard_focus("flag", Some(action))?;
            self.owned_keyboard_flag(
                !expected,
                history - 1,
                "queued keyboard toggle before paint",
            )?;
            self.owned_test_paint()?;
            self.owned_keyboard_flag(expected, history, "keyboard boolean exact result")?;
            self.owned_test_draft("flag", None)?;
            self.owned_test_draft("label", None)?;
        }

        self.click("Undo")?;
        self.owned_keyboard_flag(false, 6, "toolbar boolean document Undo")?;
        self.click("Redo")?;
        self.owned_keyboard_flag(true, 7, "toolbar boolean document Redo")?;
        if self
            .application
            .active_authoring_unavailable_record_manifest()
            != Some(manifest.clone())
        {
            return Err(
                "keyboard editing or document history changed unavailable source records".into(),
            );
        }
        self.click("Save")?;
        self.settle_persistence()?;
        if !matches!(self.frame()?.persistence().result(), Some(EditorPersistenceResult::Saved { document: actual, .. }) if actual == document)
            || self.dirty()?
        {
            return Err("Save did not persist the exact keyboard Apply result".into());
        }
        let stored = read_source(&source)?;
        let entities = stored["payload"]["entities"]
            .as_array()
            .ok_or("the saved keyboard Scene has no entity records")?;
        let created = entities
            .iter()
            .find(|record| record["id"].as_str() == Some(entity.as_str()))
            .ok_or("the keyboard-edited entity was not saved")?;
        let saved_component: ComponentValue =
            serde_json::from_value(created["components"][CUSTOM_COMPONENT]["value"].clone())
                .map_err(|error| error.to_string())?;
        if saved_component
            != ComponentValue::map([
                ("value", ComponentValue::U64(10)),
                ("label", ComponentValue::String(SAVED_LABEL.into())),
                ("other", ComponentValue::I64(0)),
                ("flag", ComponentValue::Bool(true)),
                ("seconds", ComponentValue::f64(0.5).unwrap()),
            ])
        {
            return Err("the saved component differs from the keyboard Apply result".into());
        }
        let retained_entities = entities
            .iter()
            .filter(|record| record["id"].as_str() != Some(entity.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if original["payload"]["entities"].as_array() != Some(&retained_entities) {
            return Err("the keyboard workflow changed original source entity records".into());
        }
        self.click("Dismiss persistence result")?;
        self.click("Reopen")?;
        self.settle_persistence()?;
        if !matches!(
            self.frame()?.persistence().result(),
            Some(EditorPersistenceResult::Opened { .. })
        ) {
            return Err("Reopen did not load the saved keyboard edits".into());
        }
        self.click(&format!("entity:{}", entity.as_str()))?;
        self.owned_test_document(SAVED_LABEL, 10, 0, "saved Scene Reopen")?;
        self.owned_keyboard_flag(true, 0, "saved boolean Scene Reopen")?;
        if self.dirty()?
            || self
                .application
                .active_authoring_unavailable_record_manifest()
                != Some(manifest)
        {
            return Err("Reopen changed source records or retained unsaved keyboard edits".into());
        }
        self.click("Dismiss persistence result")?;
        self.owned_test_select("label")?;
        self.owned_keyboard_focus("label", None)?;
        self.owned_test_text("label", SAVED_LABEL, false)?;
        self.owned_test_draft("label", None)?;
        Ok(self.presents)
    }
}
