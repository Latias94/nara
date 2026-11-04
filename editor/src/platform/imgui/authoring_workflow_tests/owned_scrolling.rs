use super::owned_fields::OwnedFieldProbe;
use super::*;
use nara::reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValueKind, PersistentComponentSchemaOwnerId,
};
use nara_ui_kernel::{UiKey, UiPoint};
use winit::{
    dpi::PhysicalPosition,
    event::{DeviceId, ElementState, Ime, MouseScrollDelta, TouchPhase},
};

const SCROLL_ROWS: usize = 16;
const SCROLL_LABEL: &str = "retained scrolling draft";
const SCROLL_TO_END: f32 = 10_000.0;

#[test]
#[ignore = "requires an interactive Windows desktop and a working DX12 WGPU adapter"]
fn native_owned_inspector_wheel_clipping_preserves_drafts_history_and_source() {
    let project = scrolling_project();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = OwnedScrollingTestApplication {
        project: project.root.clone(),
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    assert!(
        application
            .result
            .expect("the native owned scrolling workflow must run")
            .unwrap()
            > 0
    );
}

fn scrolling_component(label: &str, value: u64) -> ComponentValue {
    let mut fields = vec![
        ("label".to_owned(), ComponentValue::String(label.into())),
        ("value".to_owned(), ComponentValue::U64(value)),
    ];
    fields.extend((0..SCROLL_ROWS).map(|index| {
        (
            format!("row_{index:02}"),
            ComponentValue::U64(100 + index as u64),
        )
    }));
    ComponentValue::map(fields)
}

fn scrolling_project() -> InteractionProject {
    let project = InteractionProject::new();
    let names = std::iter::once("label".to_owned())
        .chain((0..SCROLL_ROWS).map(|index| format!("row_{index:02}")))
        .chain(std::iter::once("value".to_owned()));
    let schema = ComponentSchema::new(
        ComponentTypeId::new(CUSTOM_COMPONENT),
        "Scrolling scalars",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields(names.map(|name| {
        let kind = if name == "label" {
            ComponentValueKind::String
        } else {
            ComponentValueKind::U64
        };
        ComponentFieldSchema::required(
            ComponentFieldId::new(name.clone()),
            name.clone(),
            ComponentFieldPath::from_fields([name]),
            kind,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    }))
    .with_authoring_initial_value(scrolling_component("before", 10));
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

struct OwnedScrollingTestApplication {
    project: PathBuf,
    result: Option<Result<u64, String>>,
}

impl ApplicationHandler for OwnedScrollingTestApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_native_workflow(event_loop, &self.project, |workflow| {
                workflow.run_owned_scrolling(&self.project)
            }));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn scrolling_y(probe: &OwnedFieldProbe) -> Result<f32, String> {
    probe
        .frame
        .layouts()
        .iter()
        .find(|layout| layout.key() == Some(&probe.key))
        .map(|layout| layout.rect().origin().y())
        .ok_or_else(|| "the scrolling control lost its actual layout".into())
}

fn expect_scrolling_y(probe: &OwnedFieldProbe, expected: f32, phase: &str) -> Result<(), String> {
    let actual = scrolling_y(probe)?;
    if (actual - expected).abs() > 0.02 {
        return Err(format!(
            "{phase}: presented field y was {actual}; expected {expected}"
        ));
    }
    Ok(())
}

impl AuthoringWorkflow<'_> {
    fn owned_scroll_wheel(
        &mut self,
        delta: MouseScrollDelta,
        phase: TouchPhase,
    ) -> Result<(), String> {
        if !self.owned_test_event(WindowEvent::MouseWheel {
            device_id: DeviceId::dummy(),
            delta,
            phase,
        })? {
            return Err("the confirmed Inspector image did not consume its Winit wheel".into());
        }
        self.owned_test_paint()
    }

    fn owned_scroll_document(
        &self,
        label: &str,
        value: u64,
        history: usize,
        phase: &str,
    ) -> Result<(), String> {
        self.owned_test_document(label, value, history, phase)?;
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
            .ok_or("the scrolling fixture lost its selected component")?;
        for index in 0..SCROLL_ROWS {
            let name = format!("row_{index:02}");
            let actual = component
                .fields
                .iter()
                .find(|field| field.id.as_str() == name.as_str())
                .and_then(|field| field.value.as_ref());
            if actual != Some(&ComponentValue::U64(100 + index as u64)) {
                return Err(format!(
                    "{phase}: scrolling edited unrelated {name}: {actual:?}"
                ));
            }
        }
        Ok(())
    }

    fn owned_scroll_clipped_click(&mut self, key: &UiKey) -> Result<(), String> {
        let probe = self.owned_test_control("value", None)?;
        let layout = probe
            .frame
            .layouts()
            .iter()
            .find(|layout| layout.key() == Some(key))
            .ok_or("the clipped field lost its original layout identity")?;
        let rect = layout.rect();
        let point = UiPoint::new(
            rect.origin().x() + rect.size().width() * 0.5,
            rect.origin().y() + rect.size().height() * 0.5,
        );
        if layout.visible()
            || layout
                .clip_rect()
                .is_none_or(|clip| rect.intersect(clip).is_some())
            || probe.frame.hit_test(point).is_some()
        {
            return Err("a scrolled-away control retained a clipped hit target".into());
        }
        let logical = probe.frame.environment().logical_size();
        let local = [
            probe.image.rect.min[0]
                + point.x() / logical.width() * (probe.image.rect.max[0] - probe.image.rect.min[0]),
            probe.image.rect.min[1]
                + point.y() / logical.height()
                    * (probe.image.rect.max[1] - probe.image.rect.min[1]),
        ];
        self.pointer = [
            probe.image.viewport_origin[0] + local[0],
            probe.image.viewport_origin[1] + local[1],
        ];
        if probe
            .image
            .owns_native_pointer(&self.native.context, self.pointer)
        {
            return Err(
                "the fully clipped field still lies inside the actual Inspector image".into(),
            );
        }
        self.owned_test_event(WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: PhysicalPosition::new(
                f64::from(local[0]) * f64::from(probe.image.pixels_per_point),
                f64::from(local[1]) * f64::from(probe.image.pixels_per_point),
            ),
        })?;
        self.owned_test_mouse(ElementState::Pressed)?;
        self.owned_test_mouse(ElementState::Released)?;
        if self.native.owned_text_focus_for_test() {
            return Err("a clipped hit point acquired native text ownership".into());
        }
        self.owned_test_event(WindowEvent::Ime(Ime::Enabled))?;
        self.owned_test_event(WindowEvent::Ime(Ime::Commit("777".into())))?;
        self.owned_test_paint()?;
        self.owned_test_draft("label", Some(SCROLL_LABEL))?;
        self.owned_test_draft("value", Some("42"))?;
        self.owned_scroll_document("before", 10, 2, "clipped input/Apply hit")
    }

    fn run_owned_scrolling(mut self, project: &Path) -> Result<u64, String> {
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
        let project_code =
            fs::read(project.join("src/lib.rs")).map_err(|error| error.to_string())?;
        let manifest = self
            .application
            .active_authoring_unavailable_record_manifest()
            .ok_or("the scrolling fixture has no unavailable-record manifest")?;
        if manifest.total_count() == 0 {
            return Err("the scrolling fixture must contain actual unknown source records".into());
        }
        self.click("+ Root")?;
        self.owned_component_click("Add Component")?;
        self.owned_component_click(&format!("Add {CUSTOM_COMPONENT}"))?;
        let frame = self.frame()?;
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("no scrolling Scene")?;
        let document = scene.document;
        let entity = scene
            .selection
            .top_entity()
            .ok_or("no scrolling entity")?
            .clone();
        self.owned_scroll_document("before", 10, 2, "component creation")?;
        let manifest = self
            .application
            .active_authoring_unavailable_record_manifest()
            .ok_or("the created scrolling fixture has no unavailable-record manifest")?;
        self.owned_test_paint()?;
        if self.owned_test_control("value", None).is_ok() {
            return Err(
                "the finite scalar fixture did not overflow the actual Inspector viewport".into(),
            );
        }
        self.owned_test_select("label")?;
        self.owned_test_ime(Ime::Enabled)?;
        self.owned_test_ime(Ime::Commit(SCROLL_LABEL.into()))?;
        self.owned_test_text("label", SCROLL_LABEL, false)?;
        let top = self.owned_test_control("label", None)?;
        let top_apply = self.owned_test_control("label", Some("Apply"))?;
        let top_y = scrolling_y(&top)?;

        self.owned_scroll_wheel(MouseScrollDelta::LineDelta(0.0, -1.0), TouchPhase::Started)?;
        let line = self.owned_test_control("label", None)?;
        expect_scrolling_y(&line, top_y - 48.0, "one downward line")?;
        let physical_scale = f64::from(line.image.pixels_per_point)
            * f64::from(line.image.rect.max[1] - line.image.rect.min[1])
            / f64::from(line.frame.environment().logical_size().height());
        for (delta, phase) in [
            (MouseScrollDelta::LineDelta(3.0, 0.0), TouchPhase::Moved),
            (
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(20.0, 0.0)),
                TouchPhase::Moved,
            ),
            (
                MouseScrollDelta::LineDelta(0.0, -1.0),
                TouchPhase::Cancelled,
            ),
            (
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, -48.0 * physical_scale)),
                TouchPhase::Cancelled,
            ),
            (
                MouseScrollDelta::LineDelta(0.0, f32::NAN),
                TouchPhase::Moved,
            ),
            (
                MouseScrollDelta::LineDelta(0.0, f32::INFINITY),
                TouchPhase::Moved,
            ),
            (
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, f64::NAN)),
                TouchPhase::Moved,
            ),
            (
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, f64::NEG_INFINITY)),
                TouchPhase::Moved,
            ),
        ] {
            self.owned_scroll_wheel(delta, phase)?;
            expect_scrolling_y(
                &self.owned_test_control("label", None)?,
                top_y - 48.0,
                "ignored wheel input",
            )?;
            self.owned_test_draft("label", Some(SCROLL_LABEL))?;
            self.owned_scroll_document("before", 10, 2, "ignored wheel input")?;
        }
        self.owned_scroll_wheel(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 48.0 * physical_scale)),
            TouchPhase::Ended,
        )?;
        expect_scrolling_y(
            &self.owned_test_control("label", None)?,
            top_y,
            "pixel return",
        )?;
        for (sign, expected) in [(-1.0, top_y - 24.5), (1.0, top_y)] {
            self.owned_scroll_wheel(
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(
                    0.0,
                    sign * 24.5 * physical_scale,
                )),
                TouchPhase::Moved,
            )?;
            expect_scrolling_y(
                &self.owned_test_control("label", None)?,
                expected,
                "fractional pixels",
            )?;
        }

        self.owned_scroll_wheel(
            MouseScrollDelta::LineDelta(0.0, -SCROLL_TO_END),
            TouchPhase::Moved,
        )?;
        let bottom = self.owned_test_control("value", None)?;
        let bottom_y = scrolling_y(&bottom)?;
        if self.owned_test_control("label", None).is_ok() {
            return Err("scrolling to the revealed last field did not clip the old field".into());
        }
        self.owned_scroll_wheel(
            MouseScrollDelta::LineDelta(0.0, -SCROLL_TO_END),
            TouchPhase::Moved,
        )?;
        expect_scrolling_y(
            &self.owned_test_control("value", None)?,
            bottom_y,
            "bottom clamp",
        )?;
        self.owned_test_select("value")?;
        self.owned_test_ime(Ime::Enabled)?;
        self.owned_test_ime(Ime::Commit("42".into()))?;
        self.owned_test_text("value", "42", false)?;
        for key in [&top.key, &top_apply.key] {
            self.owned_scroll_clipped_click(key)?;
        }
        self.owned_test_pointer("value", None, 0.5)?;
        self.owned_scroll_wheel(
            MouseScrollDelta::LineDelta(0.0, SCROLL_TO_END),
            TouchPhase::Moved,
        )?;
        let returned = self.owned_test_control("label", None)?;
        expect_scrolling_y(&returned, top_y, "top clamp and retained field")?;
        if returned.key != top.key {
            return Err("scrolling away and back replaced the original dirty field".into());
        }
        if self.native.owned_text_focus_for_test() {
            return Err("scrolling retained a text lifetime after its field was clipped".into());
        }
        self.owned_test_select("label")?;
        self.owned_test_text("label", SCROLL_LABEL, false)?;

        for release_field in ["value", "label"] {
            self.owned_test_pointer("label", Some("Apply"), 0.5)?;
            self.owned_test_mouse(ElementState::Pressed)?;
            self.owned_test_paint()?;
            if release_field == "value" {
                self.owned_scroll_wheel(
                    MouseScrollDelta::LineDelta(0.0, -SCROLL_TO_END),
                    TouchPhase::Moved,
                )?;
                self.owned_test_pointer("value", Some("Apply"), 0.5)?;
            } else {
                for vertical in [-1.0, 1.0] {
                    if !self.owned_test_event(WindowEvent::MouseWheel {
                        device_id: DeviceId::dummy(),
                        delta: MouseScrollDelta::LineDelta(0.0, vertical),
                        phase: TouchPhase::Moved,
                    })? {
                        return Err(
                            "the pending Apply did not consume its wheel before repaint".into()
                        );
                    }
                }
            }
            self.owned_test_mouse(ElementState::Released)?;
            self.owned_test_paint()?;
            self.owned_scroll_document("before", 10, 2, "wheel-cancelled Apply release")?;
            self.owned_test_draft("label", Some(SCROLL_LABEL))?;
            self.owned_test_draft("value", Some("42"))?;
            if release_field == "value" {
                self.owned_scroll_wheel(
                    MouseScrollDelta::LineDelta(0.0, SCROLL_TO_END),
                    TouchPhase::Moved,
                )?;
            }
        }

        self.owned_scroll_wheel(
            MouseScrollDelta::LineDelta(0.0, -SCROLL_TO_END),
            TouchPhase::Moved,
        )?;
        self.owned_test_pointer("value", Some("Discard"), 0.5)?;
        self.owned_test_mouse(ElementState::Pressed)?;
        self.owned_test_paint()?;
        self.owned_test_mouse(ElementState::Released)?;
        self.owned_test_paint()?;
        self.owned_test_draft("value", None)?;
        self.owned_scroll_wheel(
            MouseScrollDelta::LineDelta(0.0, SCROLL_TO_END),
            TouchPhase::Moved,
        )?;
        self.owned_test_apply("label")?;
        self.owned_scroll_document(SCROLL_LABEL, 10, 3, "explicit retained string Apply")?;
        self.owned_scroll_wheel(
            MouseScrollDelta::LineDelta(0.0, -SCROLL_TO_END),
            TouchPhase::Moved,
        )?;
        self.owned_test_select("value")?;
        self.owned_test_ime(Ime::Enabled)?;
        self.owned_test_ime(Ime::Commit("42".into()))?;
        self.owned_test_apply("value")?;
        self.owned_scroll_document(SCROLL_LABEL, 42, 4, "explicit revealed numeric Apply")?;
        self.click("Undo")?;
        self.owned_scroll_document(SCROLL_LABEL, 10, 3, "document Undo")?;
        self.click("Redo")?;
        self.owned_scroll_document(SCROLL_LABEL, 42, 4, "document Redo")?;
        self.click("entity:preserved")?;
        self.owned_test_paint()?;
        self.click(&format!("entity:{}", entity.as_str()))?;
        self.owned_test_paint()?;
        expect_scrolling_y(
            &self.owned_test_control("label", None)?,
            top_y,
            "shorter selected content clamps the offset",
        )?;
        self.owned_scroll_document(SCROLL_LABEL, 42, 4, "selection after content shrink")?;
        if !self.dirty()?
            || read_source(&source)? != original
            || self
                .application
                .active_authoring_unavailable_record_manifest()
                != Some(manifest.clone())
        {
            return Err(
                "scrolling/history changed original disk data or unavailable records".into(),
            );
        }

        self.click("Save")?;
        self.settle_persistence()?;
        if !matches!(self.frame()?.persistence().result(), Some(EditorPersistenceResult::Saved { document: actual, .. }) if actual == document)
            || self.dirty()?
        {
            return Err("Save did not persist the explicit scrolling edits".into());
        }
        let stored = read_source(&source)?;
        let entities = stored["payload"]["entities"]
            .as_array()
            .ok_or("no saved entities")?;
        let created = entities
            .iter()
            .find(|record| record["id"].as_str() == Some(entity.as_str()))
            .ok_or("the scrolling entity was not saved")?;
        let saved_component: ComponentValue =
            serde_json::from_value(created["components"][CUSTOM_COMPONENT]["value"].clone())
                .map_err(|error| error.to_string())?;
        let retained = entities
            .iter()
            .filter(|record| record["id"].as_str() != Some(entity.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if saved_component != scrolling_component(SCROLL_LABEL, 42)
            || original["payload"]["entities"].as_array() != Some(&retained)
        {
            return Err(
                "Save changed an unrelated scalar or original unknown source record".into(),
            );
        }
        self.click("Dismiss persistence result")?;
        self.click("Reopen")?;
        self.settle_persistence()?;
        if !matches!(
            self.frame()?.persistence().result(),
            Some(EditorPersistenceResult::Opened { .. })
        ) {
            return Err("Reopen did not load the saved scrolling edits".into());
        }
        self.click(&format!("entity:{}", entity.as_str()))?;
        self.owned_scroll_document(SCROLL_LABEL, 42, 0, "saved Scene Reopen")?;
        if self.dirty()?
            || self
                .application
                .active_authoring_unavailable_record_manifest()
                != Some(manifest)
            || fs::read(project.join("src/lib.rs")).map_err(|error| error.to_string())?
                != project_code
        {
            return Err(
                "Reopen changed unknown records, project code or the clean checkpoint".into(),
            );
        }
        self.click("Dismiss persistence result")?;
        self.owned_test_paint()?;
        self.owned_test_select("label")?;
        self.owned_test_text("label", SCROLL_LABEL, false)?;
        self.owned_test_draft("label", None)?;
        self.owned_scroll_wheel(
            MouseScrollDelta::LineDelta(0.0, -SCROLL_TO_END),
            TouchPhase::Moved,
        )?;
        self.owned_test_select("value")?;
        self.owned_test_text("value", "42", false)?;
        self.owned_test_draft("value", None)?;
        self.owned_scroll_document(SCROLL_LABEL, 42, 0, "reopened native fields")?;
        Ok(self.presents)
    }
}
