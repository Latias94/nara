use super::*;
use crate::{EditorApplicationActionDispatch, workspace::EditorPanelInstanceId};
use nara::{
    reflect::{AuthoringSchemaBundle, ComponentFieldId, ComponentRegistryBuilder, ComponentTypeId},
    render::{DisplayMapping2d, DisplayRect},
    render_wgpu::{WgpuImageDisplay, WgpuWorldColor},
    scene::{
        SceneComponentRecord, SceneDocumentCandidate, SceneEntityId, SceneEntityPlacement,
        SceneEntityRecord,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentMutation,
        EditorNavigationAction, SceneInspectorCommand,
    },
};
use nara_stock_composition_consumer::{oracle::Image, readback};
use nara_ui_kernel::{UiFrame, UiLocalKey, UiPoint, UiSemanticRole};
use std::panic::{AssertUnwindSafe, catch_unwind};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{
        DeviceId, ElementState, MouseButton as NativeMouseButton, MouseScrollDelta, TouchPhase,
    },
};

const SPRITE: &str = "owned-scene-sprite";
const TRANSFORM: &str = "nara.transform.Transform2d";
const TRANSLATION_X: &str = "translation.x";

#[test]
#[ignore = "requires an interactive Windows desktop and an actual DX12 adapter"]
fn native_owned_scene_samples_original_output_and_routes_zoom_pan_reset() {
    run_owned_scene_test(|workflow| workflow.run_owned_scene_input());
}

#[test]
#[ignore = "requires an interactive Windows desktop and an actual DX12 adapter"]
fn native_owned_scene_picks_edits_undoes_resizes_and_retires_immutable_outputs() {
    run_owned_scene_test(|workflow| workflow.run_owned_scene_source());
}

#[test]
#[ignore = "requires an interactive Windows desktop and an actual DX12 adapter"]
fn native_owned_scene_rebind_refuses_queued_pick_until_the_new_sample() {
    run_owned_scene_test(|workflow| workflow.run_owned_scene_rebind());
}

fn run_owned_scene_test(exercise: for<'a> fn(AuthoringWorkflow<'a>) -> Result<(), String>) {
    let project = InteractionProject::new();
    add_scene_sprite(&project.root).unwrap();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = OwnedSceneTestApplication {
        project: project.root.clone(),
        exercise,
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    application
        .result
        .expect("the Nara-owned Scene workflow must execute")
        .unwrap();
}

struct OwnedSceneTestApplication {
    project: PathBuf,
    exercise: for<'a> fn(AuthoringWorkflow<'a>) -> Result<(), String>,
    result: Option<Result<(), String>>,
}

impl ApplicationHandler for OwnedSceneTestApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_native_workflow(event_loop, &self.project, |workflow| {
                catch_unwind(AssertUnwindSafe(|| (self.exercise)(workflow))).unwrap_or_else(
                    |payload| {
                        let message = payload
                            .downcast_ref::<String>()
                            .map(String::as_str)
                            .or_else(|| payload.downcast_ref::<&str>().copied())
                            .unwrap_or("non-string panic");
                        Err(format!(
                            "the native owned Scene workflow panicked: {message}"
                        ))
                    },
                )
            }));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn add_scene_sprite(project: &Path) -> Result<(), String> {
    let bundle_path = project.join("catalogs/authoring.bundle.json");
    let existing = AuthoringSchemaBundle::from_json_bytes(
        &fs::read(&bundle_path).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let builtins = AuthoringSchemaBundle::from_trusted_provider_definitions(
        nara::project_host::advanced::built_in_schema_providers(),
    )
    .map_err(|error| error.to_string())?;
    let bundle = AuthoringSchemaBundle::from_owner_snapshots(
        existing.owners().iter().chain(builtins.owners()).cloned(),
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        bundle_path,
        bundle.to_json_string().map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let mut registry = ComponentRegistryBuilder::new();
    nara::transform::register_transform_components(&mut registry)
        .map_err(|error| error.to_string())?;
    nara::sprite::register_sprite_components(&mut registry).map_err(|error| error.to_string())?;
    let registry = registry.freeze().map_err(|error| error.to_string())?;
    let initial = |name: &str| -> Result<SceneComponentRecord, String> {
        let schema = registry
            .schema(&ComponentTypeId::new(name))
            .ok_or("missing built-in Scene component schema")?;
        Ok(SceneComponentRecord::new(
            schema.version(),
            schema
                .authoring_initial_value()
                .ok_or("missing built-in Scene component defaults")?
                .clone(),
        ))
    };
    let mut sprite = initial("nara.sprite.Sprite")?;
    let ComponentValue::Map(fields) = &mut sprite.value else {
        return Err("Sprite defaults are not a map".into());
    };
    fields.insert(
        "size".into(),
        ComponentValue::map([
            ("x", ComponentValue::f64(160.0).unwrap()),
            ("y", ComponentValue::f64(160.0).unwrap()),
        ]),
    );
    let Some(ComponentValue::Map(material)) = fields.get_mut("material") else {
        return Err("Sprite defaults have no material".into());
    };
    material.insert(
        "tint".into(),
        ComponentValue::map([
            ("r", ComponentValue::f64(1.0).unwrap()),
            ("g", ComponentValue::f64(0.0).unwrap()),
            ("b", ComponentValue::f64(0.0).unwrap()),
            ("a", ComponentValue::f64(1.0).unwrap()),
        ]),
    );
    let source = project.join("worlds/entry.scene.json");
    let mut scene = SceneDocumentCandidate::decode_json_bytes(
        &fs::read(&source).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?
    .into_composition_source()
    .map_err(|error| error.to_string())?;
    let entity =
        SceneEntityRecord::new(SceneEntityId::new(SPRITE).map_err(|error| error.to_string())?)
            .with_component(ComponentTypeId::new(TRANSFORM), initial(TRANSFORM)?)
            .with_component(ComponentTypeId::new("nara.sprite.Sprite"), sprite);
    scene
        .placements
        .push(SceneEntityPlacement::root(entity.id.clone()));
    scene.entities.push(entity);
    fs::write(
        source,
        scene.to_json_string().map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

struct OwnedSceneProbe {
    panel: EditorPanelInstanceId,
    frame: Arc<UiFrame>,
    display: WgpuImageDisplay,
    mapping: DisplayMapping2d,
    clip: DisplayRect,
}

impl OwnedSceneProbe {
    fn center(&self) -> Result<[f32; 2], String> {
        let content = self.mapping.content_rect();
        let min: [f32; 2] = std::array::from_fn(|axis| content.min[axis].max(self.clip.min[axis]));
        let max: [f32; 2] = std::array::from_fn(|axis| content.max[axis].min(self.clip.max[axis]));
        if (0..2).any(|axis| max[axis] - min[axis] <= 2.0) {
            return Err("the sampled Scene has no visible input area".into());
        }
        Ok(std::array::from_fn(|axis| (min[axis] + max[axis]) * 0.5))
    }
}

impl AuthoringWorkflow<'_> {
    fn wait_owned_scene(&mut self) -> Result<OwnedSceneProbe, String> {
        self.wait_owned_scene_matching(|_| true)
    }

    fn wait_owned_scene_matching(
        &mut self,
        ready: impl Fn(&OwnedSceneProbe) -> bool,
    ) -> Result<OwnedSceneProbe, String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        for attempt in 0..240 {
            self.paint(|_| {})?;
            if attempt >= 2
                && self.native.owned_scene_snapshot_for_test().is_some_and(
                    |(_, _, display, _, _)| Some(display.receipt()) == self.native.scene.submitted,
                )
            {
                let probe = self.owned_scene_probe()?;
                if ready(&probe) {
                    return Ok(probe);
                }
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Err(format!(
            "no exact Nara Scene/chrome presentation after bounded paints: submitted={:?}, extent={:?}, diagnostics={:?}",
            self.native.scene.submitted,
            self.native.scene_extent,
            self.application
                .current_frame()
                .map(|frame| frame.diagnostics().clone())
        ))
    }

    fn owned_scene_probe(&self) -> Result<OwnedSceneProbe, String> {
        let (panel, frame, display, mapping, clip) = self
            .native
            .owned_scene_snapshot_for_test()
            .ok_or("the Nara-owned Scene has no confirmed native sample")?;
        let source = self
            .native
            .scene
            .display
            .as_ref()
            .ok_or("the original Scene producer has no display")?;
        let placement = self
            .native
            .frontend
            .owned_scene_placement(panel, &frame)
            .ok_or("the sampled Scene has no retained placement")?;
        let image = placement
            .image
            .ok_or("the Scene placement lost its original image")?;
        let local_mapping = placement
            .mapping
            .ok_or("the Scene placement has no local mapping")?;
        if self.native.scene.submitted != Some(display.receipt())
            || source.output() != display.output()
            || source.occurrence() == display.occurrence()
            || image.occurrence != source.occurrence().get()
            || image.extent != display.output().extent()
            || local_mapping.uv_rect() != mapping.uv_rect()
            || mapping.target() != image.extent
            || !frame.diagnostics().is_empty()
        {
            return Err("the owned Scene did not directly sample its original output through a distinct occurrence".into());
        }
        let viewport = frame
            .semantics()
            .iter()
            .find(|node| node.role() == UiSemanticRole::Button && node.name() == "Scene viewport")
            .ok_or("the Nara Scene frame has no viewport control")?;
        let viewport_key = UiLocalKey::runtime("nara.scene-view", "viewport")
            .map_err(|error| error.to_string())?;
        if viewport.key().local_key() != Some(&viewport_key) || !viewport.enabled() {
            return Err("the sampled Scene viewport is not the enabled Nara control".into());
        }
        let chrome = self
            .native
            .owned
            .get(&panel)
            .and_then(|surface| surface.target.display.as_ref())
            .ok_or("the owned Scene has no chrome image")?;
        let (primary, secondary) = self
            .native
            .lifecycle_frame
            .as_ref()
            .ok_or("the native Scene frame has no actual presentation outcomes")?;
        let receipts = primary
            .composites()
            .iter()
            .chain(secondary.iter().flat_map(|outcome| outcome.composites()))
            .collect::<Vec<_>>();
        let sample = receipts
            .iter()
            .find(|receipt| {
                receipt.image() == display.receipt() && receipt.occurrence() == display.occurrence()
            })
            .ok_or("the exact Nara Scene occurrence has no native sample receipt")?;
        if sample.window().window_id() != super::super::editor_window_packet(&self.native.window).id
            || sample.window().display_frame_index() != self.native.frame_index
            || sample.window().backend_instance_id() != display.receipt().backend_instance_id()
            || sample.window().device_epoch() != display.receipt().device_epoch()
            || !receipts.iter().any(|receipt| {
                receipt.image() == chrome.receipt()
                    && receipt.occurrence() == chrome.occurrence()
                    && receipt.window() == sample.window()
            })
            || chrome.receipt() == display.receipt()
            || self.native.gpu.frame_transaction_stats().presents() == 0
        {
            return Err("the actual Nara chrome and original Scene were not sampled in the same native window frame".into());
        }
        let probe = OwnedSceneProbe {
            panel,
            frame,
            display,
            mapping,
            clip,
        };
        probe.center()?;
        Ok(probe)
    }

    fn owned_scene_start(&mut self) -> Result<OwnedSceneProbe, String> {
        let probe = self.wait_owned_scene()?;
        let info = self
            .native
            .gpu
            .with_device(
                super::super::editor_window_packet(&self.native.window),
                |scope| scope.adapter().get_info(),
            )
            .map_err(|error| error.to_string())?;
        if info.backend != dear_imgui_wgpu::wgpu::Backend::Dx12
            || info.device_type == dear_imgui_wgpu::wgpu::DeviceType::Cpu
        {
            return Err(format!(
                "the owned Scene trajectory requires an actual DX12 adapter, got {info:?}"
            ));
        }
        Ok(probe)
    }

    fn owned_scene_cursor(&mut self, point: [f32; 2]) -> Result<(), String> {
        self.pointer = point;
        let origin = self.native.context.main_viewport().pos();
        let scale = f64::from(self.native.context.io().display_framebuffer_scale()[0]);
        self.owned_test_event(WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: PhysicalPosition::new(
                f64::from(point[0] - origin[0]) * scale,
                f64::from(point[1] - origin[1]) * scale,
            ),
        })?;
        Ok(())
    }

    fn owned_scene_point_at(&mut self, point: [f32; 2]) -> Result<OwnedSceneProbe, String> {
        self.pointer = point;
        self.owned_test_paint()?;
        let probe = self.wait_owned_scene()?;
        self.owned_scene_cursor(point)?;
        Ok(probe)
    }

    fn owned_scene_button_point(
        &self,
        probe: &OwnedSceneProbe,
        name: &str,
    ) -> Result<[f32; 2], String> {
        let node = probe
            .frame
            .semantics()
            .iter()
            .find(|node| {
                node.role() == UiSemanticRole::Button && node.name() == name && node.enabled()
            })
            .ok_or_else(|| format!("the Nara Scene has no enabled {name:?} button"))?;
        let layout = probe
            .frame
            .layouts()
            .iter()
            .find(|layout| layout.key() == Some(node.key()) && layout.visible())
            .ok_or("the Nara Scene button has no visible layout")?;
        let rect = match layout.clip_rect() {
            Some(clip) => layout
                .rect()
                .intersect(clip)
                .ok_or("the Scene button is fully clipped")?,
            None => layout.rect(),
        };
        let point = UiPoint::new(
            rect.origin().x() + rect.size().width() * 0.5,
            rect.origin().y() + rect.size().height() * 0.5,
        );
        if probe
            .frame
            .hit_test(point)
            .is_none_or(|hit| hit.key() != node.key())
        {
            return Err("the actual Nara Scene button does not own its hit target".into());
        }
        let local = self
            .native
            .frontend
            .owned_scene_placement(probe.panel, &probe.frame)
            .and_then(|placement| placement.mapping)
            .ok_or("the Nara Scene button lost its local-to-native mapping")?;
        Ok([
            point.x() + probe.mapping.panel_rect().min[0] - local.panel_rect().min[0],
            point.y() + probe.mapping.panel_rect().min[1] - local.panel_rect().min[1],
        ])
    }

    fn owned_scene_reject_pick(
        &mut self,
        probe: &OwnedSceneProbe,
        phase: &str,
    ) -> Result<(), String> {
        let frame = self.frame()?;
        if self
            .native
            .scene_presenter
            .resolve_pick(
                &frame,
                probe.center()?,
                probe.mapping,
                self.application
                    .current_scene_preview_content_stamp()
                    .as_ref(),
            )
            .is_some()
        {
            return Err(format!(
                "{phase}: an old Scene mapping still authorized source picking"
            ));
        }
        Ok(())
    }

    fn run_owned_scene_input(mut self) -> Result<(), String> {
        let initial = self.owned_scene_start()?;
        let reset = self.owned_scene_button_point(&initial, "Reset View")?;
        for point in [[-1000.0, -1000.0], reset] {
            self.owned_scene_point_at(point)?;
            let consumed = self.owned_test_event(WindowEvent::MouseWheel {
                device_id: DeviceId::dummy(),
                delta: MouseScrollDelta::LineDelta(0.0, 2.0),
                phase: TouchPhase::Moved,
            })?;
            let after = self.wait_owned_scene()?;
            if after.mapping.uv_rect() != initial.mapping.uv_rect() || (point[0] < 0.0 && consumed)
            {
                return Err("an outside-content or toolbar wheel changed the Scene camera".into());
            }
        }
        self.owned_scene_point_at(initial.center()?)?;
        if !self.owned_test_event(WindowEvent::MouseWheel {
            device_id: DeviceId::dummy(),
            delta: MouseScrollDelta::LineDelta(0.0, 2.0),
            phase: TouchPhase::Moved,
        })? {
            return Err("the confirmed Scene did not consume its native zoom wheel".into());
        }
        let zoomed = self.wait_owned_scene_matching(|probe| {
            probe.mapping.uv_rect() != initial.mapping.uv_rect()
        })?;
        let uv = zoomed.mapping.uv_rect();
        if uv.max[0] - uv.min[0] >= 1.0
            || uv.max[1] - uv.min[1] >= 1.0
            || zoomed.display.receipt() != initial.display.receipt()
            || zoomed.display.occurrence() == initial.display.occurrence()
            || self
                .native
                .scene_presenter
                .matches_occurrence(Some(initial.display.occurrence()))
        {
            return Err(
                "native wheel zoom did not replace only the Scene display mapping and occurrence"
                    .into(),
            );
        }
        self.owned_scene_reject_pick(&initial, "after native zoom")?;
        self.owned_scene_point_at(zoomed.center()?)?;
        if !self.owned_test_event(WindowEvent::MouseInput {
            device_id: DeviceId::dummy(),
            state: ElementState::Pressed,
            button: NativeMouseButton::Middle,
        })? {
            return Err("the confirmed Scene did not begin a native middle-button drag".into());
        }
        let center = zoomed.center()?;
        self.owned_scene_cursor([center[0] + 20.0, center[1] + 16.0])?;
        if !self.owned_test_event(WindowEvent::MouseInput {
            device_id: DeviceId::dummy(),
            state: ElementState::Released,
            button: NativeMouseButton::Middle,
        })? {
            return Err("the native Scene drag did not consume its release".into());
        }
        let panned = self.wait_owned_scene_matching(|probe| probe.mapping.uv_rect() != uv)?;
        let panned_uv = panned.mapping.uv_rect();
        if panned_uv == uv
            || panned_uv.min[0] >= uv.min[0]
            || panned_uv.min[1] >= uv.min[1]
            || (0..2).any(|axis| {
                ((panned_uv.max[axis] - panned_uv.min[axis]) - (uv.max[axis] - uv.min[axis])).abs()
                    > 0.0001
            })
            || panned.display.receipt() != initial.display.receipt()
            || panned.display.occurrence() == zoomed.display.occurrence()
        {
            return Err("native middle-button drag did not pan the sampled Scene while preserving its zoom and original pixels".into());
        }
        self.owned_scene_reject_pick(&zoomed, "after native pan")?;
        let reset = self.owned_scene_button_point(&panned, "Reset View")?;
        self.owned_scene_point_at(reset)?;
        self.owned_test_mouse(ElementState::Pressed)?;
        self.owned_test_paint()?;
        self.owned_test_mouse(ElementState::Released)?;
        let restored =
            self.wait_owned_scene_matching(|probe| probe.mapping.uv_rect() == DisplayRect::UNIT)?;
        if restored.mapping.uv_rect() != DisplayRect::UNIT
            || restored.display.receipt() != initial.display.receipt()
            || self.history()? != 0
            || self.dirty()?
        {
            return Err(
                "the actual Nara Reset View button did not restore UVs without changing the source"
                    .into(),
            );
        }
        Ok(())
    }

    fn owned_scene_pixels(&mut self, probe: &OwnedSceneProbe) -> Result<Image, String> {
        let output = probe
            .display
            .output()
            .texture::<WgpuWorldColor>()
            .map_err(|error| error.to_string())?;
        Ok(readback::read(&mut self.native.gpu, &output))
    }

    fn owned_scene_mutate(&mut self, undo: bool) -> Result<(), String> {
        let frame = self.frame()?;
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the owned Scene lost its active source")?;
        let mutation = if undo {
            EditorDocumentMutation::Undo {
                document: scene.document,
                base_revision: scene.revision,
            }
        } else {
            let component = scene
                .editor
                .inspector
                .selected_entity_view
                .as_ref()
                .and_then(|entity| {
                    entity
                        .components
                        .iter()
                        .find(|component| component.component.as_str() == TRANSFORM)
                })
                .ok_or("the picked source Sprite has no Transform Inspector")?;
            EditorDocumentMutation::ApplyInspectorCommand {
                document: scene.document,
                base_revision: scene.revision,
                command: SceneInspectorCommand::SetField {
                    entity: SceneEntityId::new(SPRITE).map_err(|error| error.to_string())?,
                    component: ComponentTypeId::new(TRANSFORM),
                    component_version: component.schema_version,
                    field: ComponentFieldId::new(TRANSLATION_X),
                    value: ComponentValue::f64(240.0).unwrap(),
                },
            }
        };
        let submission = self.application.submit_and_drive(
            EditorActionBatch::new(frame.revision(), [EditorAction::Mutate(mutation)]),
            Duration::ZERO,
        );
        if !matches!(submission.actions(), [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] if report.applied)
        {
            return Err(format!(
                "the public Scene edit/Undo was refused: {:?}",
                submission.actions()
            ));
        }
        if self
            .frame()?
            .workspace()
            .active_scene
            .as_ref()
            .map(|current| current.revision)
            == Some(scene.revision)
        {
            return Err("the applied Scene edit/Undo did not advance the source revision".into());
        }
        Ok(())
    }

    fn run_owned_scene_source(mut self) -> Result<(), String> {
        self.owned_scene_start()?;
        let frame = self.frame()?;
        let document = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("no Scene source to pick")?
            .document;
        self.application.submit_and_drive(
            EditorActionBatch::new(
                frame.revision(),
                [EditorAction::Navigate(
                    EditorNavigationAction::SelectEntity {
                        document,
                        entity: None,
                    },
                )],
            ),
            Duration::ZERO,
        );
        let unselected = self.wait_owned_scene()?;
        if self
            .frame()?
            .workspace()
            .active_scene
            .as_ref()
            .and_then(|scene| scene.selection.top_entity())
            .is_some()
        {
            return Err("the source selection was not cleared before native picking".into());
        }
        let pixels = self.owned_scene_pixels(&unselected)?;
        let center_pixel =
            pixels.pixels[(pixels.height / 2 * pixels.width + pixels.width / 2) as usize];
        if center_pixel != [255, 0, 0, 255] {
            return Err(format!(
                "the actual original Scene output has {center_pixel:?} instead of the red source Sprite at its pick target"
            ));
        }
        self.owned_scene_point_at(unselected.center()?)?;
        self.owned_test_mouse(ElementState::Pressed)?;
        self.owned_test_paint()?;
        self.owned_test_mouse(ElementState::Released)?;
        let original = self.wait_owned_scene()?;
        let frame = self.frame()?;
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("picking lost the Scene source")?;
        if scene.document != document
            || scene.selection.top_entity().map(SceneEntityId::as_str) != Some(SPRITE)
            || self.history()? != 0
        {
            return Err(
                "the native Nara viewport click did not select the exact containing-source Sprite"
                    .into(),
            );
        }
        let original_pixels = self.owned_scene_pixels(&original)?;
        self.owned_scene_mutate(false)?;
        if self.native.scene.submitted != Some(original.display.receipt()) || self.history()? != 1 {
            return Err(
                "source editing did not preserve the old submission until the next paint".into(),
            );
        }
        self.owned_scene_reject_pick(&original, "after source edit before repaint")?;
        let moved = self.wait_owned_scene_matching(|probe| {
            probe.display.receipt() != original.display.receipt()
        })?;
        let moved_pixels = self.owned_scene_pixels(&moved)?;
        if moved.display.receipt() == original.display.receipt()
            || moved.display.occurrence() == original.display.occurrence()
            || moved_pixels.pixels == original_pixels.pixels
            || self
                .native
                .scene_presenter
                .matches_occurrence(Some(original.display.occurrence()))
        {
            return Err("the real source edit did not publish changed pixels and retire the old Scene occurrence".into());
        }
        self.owned_scene_mutate(true)?;
        self.owned_scene_reject_pick(&moved, "after Undo before repaint")?;
        let undone = self.wait_owned_scene_matching(|probe| {
            probe.display.receipt() != moved.display.receipt()
        })?;
        let undone_pixels = self.owned_scene_pixels(&undone)?;
        original_pixels.assert_matches(
            &undone_pixels,
            0,
            "ordinary Undo restores actual original Scene pixels",
        );
        if self.history()? != 0
            || self.dirty()?
            || undone.display.receipt() == moved.display.receipt()
        {
            return Err(
                "Undo did not restore the clean source through a fresh Scene output".into(),
            );
        }
        let before_size = self.native.window.inner_size();
        let requested = PhysicalSize::new(
            before_size.width.saturating_sub(160).max(640),
            before_size.height.saturating_sub(120).max(480),
        );
        let _ = self.native.window.request_inner_size(requested);
        let resized = self.native.window.inner_size();
        if resized == before_size {
            return Err("the actual native window did not resize".into());
        }
        self.owned_test_event(WindowEvent::Resized(resized))?;
        if self.native.owned_scene_snapshot_for_test().is_some() {
            return Err(
                "native resize left the old owned Scene input route confirmed before repaint"
                    .into(),
            );
        }
        let resized = self.wait_owned_scene_matching(|probe| probe.mapping != undone.mapping)?;
        if resized.mapping == undone.mapping
            || resized.display.occurrence() == undone.display.occurrence()
        {
            return Err(
                "native resize did not replace the Scene mapping and display occurrence".into(),
            );
        }
        self.owned_scene_reject_pick(&undone, "after native resize presentation")?;
        self.application
            .panel_workspace_mut()
            .layout_mut()
            .close(resized.panel);
        self.owned_test_paint()?;
        if self.native.owned_scene_snapshot_for_test().is_some()
            || self.native.owned.contains_key(&resized.panel)
            || self.native.scene.submitted.is_some()
            || self
                .native
                .scene_presenter
                .matches_occurrence(Some(resized.display.occurrence()))
        {
            return Err(
                "closing the real Scene panel did not retire its image and pick authority".into(),
            );
        }
        self.owned_scene_reject_pick(&resized, "after Scene retirement")?;
        for (probe, expected) in [(&original, &original_pixels), (&moved, &moved_pixels)] {
            expected.assert_matches(
                &self.owned_scene_pixels(probe)?,
                0,
                "retained Scene output is immutable across edit, Undo, resize and retirement",
            );
        }
        Ok(())
    }

    fn run_owned_scene_rebind(mut self) -> Result<(), String> {
        self.owned_scene_start()?;
        let frame = self.frame()?;
        let document = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the Scene source is absent")?
            .document;
        self.application.submit_and_drive(
            EditorActionBatch::new(
                frame.revision(),
                [EditorAction::Navigate(
                    EditorNavigationAction::SelectEntity {
                        document,
                        entity: None,
                    },
                )],
            ),
            Duration::ZERO,
        );
        let origin = self.native.context.main_viewport().pos();
        let size = [680.0, 360.0];
        let position = [origin[0] + 340.0, origin[1] + 100.0];
        self.native.frontend.place_test_scene(position, size);
        self.wait_owned_scene()?;
        self.owned_test_paint()?;
        let before = self.wait_owned_scene()?;
        if self
            .frame()?
            .workspace()
            .active_scene
            .as_ref()
            .and_then(|scene| scene.selection.top_entity())
            .is_some()
        {
            return Err("the moved Scene fixture must start without selection".into());
        }
        let next_position = [position[0] + 80.0, position[1] + 40.0];
        self.native.frontend.place_test_scene(next_position, size);
        self.paint(|_| {})?;
        if self.native.owned_scene_snapshot_for_test().is_some() {
            return Err(
                "moving a same-sized Scene panel kept its previous sample interactive".into(),
            );
        }
        let (_, frame, image) = self
            .native
            .owned_frames_for_test()
            .into_iter()
            .find(|(panel, _, _)| *panel == before.panel)
            .ok_or("the moved Scene chrome must keep its independent route")?;
        if !Arc::ptr_eq(&frame, &before.frame) {
            return Err("the same-sized move did not retain the exact CPU Scene frame".into());
        }
        let local = self
            .native
            .frontend
            .owned_scene_placement(before.panel, &frame)
            .and_then(|placement| placement.mapping)
            .ok_or("the moved Scene has no retained local mapping")?;
        let content = local.content_rect();
        let point = [
            image.viewport_origin[0] + image.rect.min[0] + (content.min[0] + content.max[0]) * 0.5,
            image.viewport_origin[1] + image.rect.min[1] + (content.min[1] + content.max[1]) * 0.5,
        ];
        self.owned_scene_cursor(point)?;
        self.owned_test_mouse(ElementState::Pressed)?;
        self.paint(|_| {})?;
        self.owned_test_mouse(ElementState::Released)?;
        let after = self.wait_owned_scene()?;
        if after.display.occurrence() == before.display.occurrence()
            || after.mapping == before.mapping
            || self
                .frame()?
                .workspace()
                .active_scene
                .as_ref()
                .and_then(|scene| scene.selection.top_entity())
                .is_some()
        {
            return Err(
                "a refused Scene pick borrowed the new occurrence's later confirmation".into(),
            );
        }
        self.owned_scene_point_at(after.center()?)?;
        self.owned_test_mouse(ElementState::Pressed)?;
        self.owned_test_paint()?;
        self.owned_test_mouse(ElementState::Released)?;
        if self
            .frame()?
            .workspace()
            .active_scene
            .as_ref()
            .and_then(|scene| scene.selection.top_entity())
            .map(SceneEntityId::as_str)
            != Some(SPRITE)
        {
            return Err(
                "the fresh confirmed Scene did not admit a new actual viewport pick".into(),
            );
        }
        Ok(())
    }
}
