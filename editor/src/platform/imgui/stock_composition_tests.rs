//! Actual Scene View qualification of the independent stock-composition consumer.
//!
//! The fixture owns effects, policy wiring, numeric expectations and declared readback. This
//! harness owns only real Editor source/actions, native presentation and their exact joins.

use std::{
    ffi::CString,
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use dear_imgui_rs::{ConfigFlags, Context, MouseButton};
use nara::{
    project_host::TrustedCompiledEditorProjectIntent,
    reflect::{ComponentFieldId, ComponentRegistry, ComponentTypeId, ComponentValue},
    render::{RenderFrameTarget, ViewLoadOp},
    render_wgpu::{
        WgpuCompositeColor, WgpuImageCompositeReceipt, WgpuImageDisplay, WgpuProgramTextureOutput,
        WgpuRenderInput, WgpuRenderProgramError, WgpuSceneColor, WgpuWorldColor,
    },
    scene::{SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord},
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentMutation,
        EditorNavigationAction, SceneInspectorCommand,
    },
};
use nara_stock_composition_consumer::{
    CompositionConfiguration, CompositionPolicy, CompositionPorts, EffectSpace, SelectedPorts,
    oracle::{EFFECT_TOLERANCE, Image},
    readback, recipe_with_inspection,
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::windows::EventLoopBuilderExtWindows,
    window::{Window, WindowId},
};

use super::{
    NativeImgui,
    canvas_tests::retire_application,
    editor_window_packet,
    retirement::{finish_graphics_for_test, initialize_native_for_test},
};
use crate::{
    EditorApplication, EditorApplicationActionDispatch, EditorLaunchOptions,
    frontend::imgui::UiProject,
};

const SPRITE: &str = "stock-composition-sprite";
const TRANSFORM: &str = "nara.transform.Transform2d";
const TRANSLATION_X: &str = "translation.x";
const MOVED_X: f64 = 240.0;
const DEADLINE: Duration = Duration::from_secs(10);

#[test]
#[ignore = "requires an interactive Windows desktop and an actual DX12 adapter"]
fn native_scene_stock_composition_presents_external_effects_picks_edits_undoes_and_retires() {
    let project = UiProject::new();
    write_scene(&project.0).unwrap();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = StockCompositionApplication {
        project: project.0.clone(),
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    // Assert only after native graphics, submission custody and the Session have retired.
    application
        .result
        .expect("the native stock-composition trajectory must execute")
        .unwrap();
}

struct StockCompositionApplication {
    project: PathBuf,
    result: Option<Result<(), String>>,
}

impl ApplicationHandler for StockCompositionApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_native(event_loop, &self.project));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn run_native(event_loop: &ActiveEventLoop, project: &Path) -> Result<(), String> {
    let configuration = CompositionConfiguration {
        space: EffectSpace::World,
        radius: 2,
        ..Default::default()
    };
    let inspection = Arc::new(Mutex::new(Vec::new()));
    // This public fixture recipe calls ProductRecipe::select_wgpu_policy. Its factory creates
    // the original CompositionCapture adapter used by the ordinary Scene preview owner.
    let recipe = recipe_with_inspection(configuration, Arc::clone(&inspection))
        .map_err(|error| error.to_string())?;
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("Nara independent stock composition Scene View qualification")
                    .with_active(false)
                    .with_visible(false)
                    .with_inner_size(LogicalSize::new(1600.0, 1100.0)),
            )
            .map_err(|error| error.to_string())?,
    );
    let mut native = initialize_native_for_test(event_loop, Arc::clone(&window))?;
    native.layout_loaded = true;
    let mut flags = native.context.io().config_flags() | ConfigFlags::NO_MOUSE_CURSOR_CHANGE;
    flags.remove(ConfigFlags::VIEWPORTS_ENABLE);
    native.context.io_mut().set_config_flags(flags);
    let opened = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(project).with_profile("desktop"),
        TrustedCompiledEditorProjectIntent::new()
            .with_profile("desktop")
            .with_recipe(recipe),
    );
    let mut application = match opened {
        Ok(application) => application,
        Err(error) => {
            let cleanup = finish_graphics_for_test(&mut native);
            return Err(format!(
                "trusted composition Open failed: {error}; graphics: {cleanup:?}"
            ));
        }
    };
    application.native_shell_ready();
    let game = application
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .find(|panel| panel.kind() == "nara.game-view")
        .map(|panel| panel.id());
    if let Some(game) = game {
        application.panel_workspace_mut().layout_mut().close(game);
        native.frontend.reset_layout();
    }
    // Fixture readback/oracle assertions may panic. Complete the real owners' shutdown even
    // on that failure, then report it to the test runner after physical retirement.
    let result = catch_unwind(AssertUnwindSafe(|| {
        Workflow {
            native: &mut native,
            application: &mut application,
            event_loop,
            configuration,
            inspection,
        }
        .run()
    }))
    .unwrap_or_else(|payload| {
        let message = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or("non-string panic");
        Err(format!("native composition assertion failed: {message}"))
    });
    let graphics = finish_graphics_for_test(&mut native);
    drop(native);
    drop(window);
    let session = retire_application(&mut application);
    drop(application);
    let failures = [
        result.err(),
        graphics
            .err()
            .map(|error| format!("native cleanup failed: {error}")),
        session
            .err()
            .map(|error| format!("Session cleanup failed: {error}")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

struct Workflow<'a> {
    native: &'a mut NativeImgui,
    application: &'a mut EditorApplication,
    event_loop: &'a ActiveEventLoop,
    configuration: CompositionConfiguration,
    inspection: Arc<Mutex<Vec<CompositionPorts>>>,
}

struct SceneEvidence {
    display: WgpuImageDisplay,
    composite: WgpuImageCompositeReceipt,
    input: WgpuRenderInput,
    original: WgpuProgramTextureOutput<WgpuWorldColor>,
    source: Image,
    expected: Image,
}

impl Workflow<'_> {
    fn paint(&mut self, pointer: [f32; 2], input: impl FnOnce(&mut Context)) -> Result<(), String> {
        self.application.advance_project_open();
        self.application
            .drive_editor_frame(Duration::ZERO, false, false);
        let exited = self
            .native
            .redraw_with_input(self.event_loop, self.application, |context| {
                let viewport = context.main_viewport().id();
                let io = context.io_mut();
                io.clear_events_queue();
                io.add_focus_event(true);
                io.add_mouse_pos_event(pointer);
                io.add_mouse_viewport_event(viewport);
                input(context);
            })
            .map_err(|error| error.to_string())?;
        if exited {
            return Err("the composition workflow unexpectedly requested Exit".into());
        }
        let stats = self.native.gpu.frame_transaction_stats();
        if !self.native.presented || stats.surface_acquires().min(stats.presents()) == 0 {
            return Err("the hidden native Editor frame did not acquire and present".into());
        }
        Ok(())
    }

    fn is_presented(&mut self) -> bool {
        self.application.current_frame().is_some_and(|frame| {
            self.native.scene_presenter.is_presented(
                &self.native.scene,
                &frame,
                self.native.scene_extent,
                self.application
                    .current_scene_preview_content_stamp()
                    .as_ref(),
            )
        })
    }

    fn wait_presented(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + DEADLINE;
        loop {
            self.paint([-1000.0, -1000.0], |_| {})?;
            if self.is_presented() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "the selected recipe produced no presented Scene: extent={:?}, diagnostics={:?}",
                    self.native.scene_extent,
                    self.application
                        .current_frame()
                        .map(|frame| frame.diagnostics().clone()),
                ));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn composite(&self, display: &WgpuImageDisplay) -> Result<WgpuImageCompositeReceipt, String> {
        let (primary, secondary) = self
            .native
            .lifecycle_frame
            .as_ref()
            .ok_or("the actual native frame has no presentation outcomes")?;
        let receipt = primary
            .composites()
            .iter()
            .chain(secondary.iter().flat_map(|outcome| outcome.composites()))
            .copied()
            .find(|receipt| {
                receipt.image() == display.receipt() && receipt.occurrence() == display.occurrence()
            })
            .ok_or(
                "the exact Scene output and designated occurrence were not sampled and presented",
            )?;
        let image = receipt.image();
        if receipt.window().window_id() != editor_window_packet(&self.native.window).id
            || receipt.window().backend_instance_id() != image.backend_instance_id()
            || receipt.window().device_epoch() != image.device_epoch()
            || receipt.window().display_frame_index() != self.native.frame_index
            || !self
                .native
                .scene_presenter
                .matches_occurrence(Some(receipt.occurrence()))
        {
            return Err(
                "the Scene receipt did not join its actual native window/epoch/occurrence".into(),
            );
        }
        Ok(receipt)
    }

    fn observe(&mut self) -> Result<SceneEvidence, String> {
        if !self.is_presented() {
            return Err("cannot inspect an unpresented Scene".into());
        }
        let display = self
            .native
            .scene
            .display
            .as_ref()
            .ok_or("the Scene owner retained no exact display")?
            .clone();
        let composite = self.composite(&display)?;
        // Read the actual Scene View's selected final storage. No replay or new policy can
        // substitute for these pixels; wrong semantic markers must still refuse this lease.
        let final_output = display
            .output()
            .texture::<WgpuWorldColor>()
            .map_err(|error| error.to_string())?;
        assert!(matches!(
            display.output().texture::<WgpuSceneColor>(),
            Err(WgpuRenderProgramError::ResourceSemantic)
        ));
        assert!(matches!(
            display.output().texture::<WgpuCompositeColor>(),
            Err(WgpuRenderProgramError::ResourceSemantic)
        ));
        assert_eq!(final_output.producer_id(), composite.image().producer_id());
        assert_eq!(
            final_output.source_generation(),
            composite.image().source_generation()
        );
        assert_eq!(
            final_output.frame_index(),
            composite.image().render_transaction_frame_index()
        );
        assert_eq!(self.native.scene.submitted, Some(composite.image()));
        assert_eq!(
            self.native.scene.view.as_ref().unwrap().id(),
            final_output.producer_id()
        );

        // A fresh CPU candidate from the same current source is independently publishable
        // through public Root APIs. It exposes before-effects branches without retaining an
        // entire program on the native Scene display owner.
        self.application
            .drive_editor_frame(Duration::ZERO, false, false);
        let candidate = self
            .application
            .take_scene_preview_candidate()
            .ok_or("the real Scene owner supplied no replayable CPU candidate")?;
        assert_eq!(
            Some(candidate.content_stamp()),
            self.application.current_scene_preview_content_stamp()
        );
        if !self.is_presented() {
            return Err("the fresh CPU source no longer matches the actual displayed Scene".into());
        }
        let frame = self
            .application
            .current_frame()
            .ok_or("the Scene lost its Session")?;
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the real Scene is not active")?;
        assert_eq!(candidate.document(), scene.document);
        assert_eq!(candidate.revision(), scene.revision);
        assert_eq!(
            candidate.packet().source_generation(),
            final_output.source_generation()
        );
        assert_eq!(
            candidate.packet().target().extent(),
            display.output().extent()
        );
        assert_eq!(candidate.packet().views().len(), 1);
        assert!(
            candidate
                .packet()
                .source_generation()
                .runtime_generation()
                .is_none()
        );
        assert!(candidate.packet().world_canvas().is_none());
        assert!(self.application.current_ui_authoring_frame().is_none());
        assert_eq!(candidate.hits().len(), 1);
        assert_eq!(candidate.hits()[0].entity().as_str(), SPRITE);
        let sprite_center = candidate.hits()[0].instance().center;
        let input = candidate.render_input().clone();
        let capture = input
            .captured::<CompositionPolicy>()
            .ok_or("the recipe's external policy was not selected")?;
        assert_eq!(capture.configuration, self.configuration);
        assert!(capture.stock.ui_packets().is_empty());
        assert_eq!(candidate.sprite_batches().total_instances(), 1);
        let ViewLoadOp::Clear(clear) = input.packet().views()[0].load else {
            return Err("the ordinary Scene view did not retain its clear".into());
        };
        let (original, source, expected) = self.replay(&input)?;
        // These source checks are independent from both effect passes. An empty/flat output
        // cannot satisfy the trajectory just because its grade was computed consistently.
        let center = [
            ((sprite_center.x + 1.0) * 0.5 * source.width as f32) as u32,
            ((1.0 - sprite_center.y) * 0.5 * source.height as f32) as u32,
        ];
        assert!(center[0] < source.width && center[1] < source.height);
        assert_eq!(
            source.pixels[(center[1] * source.width + center[0]) as usize],
            [255, 0, 0, 255]
        );
        let background =
            Image::solid(1, 1, source.format, [clear.r, clear.g, clear.b, clear.a]).pixels[0];
        for index in [
            0,
            (source.width - 1) as usize,
            ((source.height - 1) * source.width) as usize,
            source.pixels.len() - 1,
        ] {
            assert_eq!(
                source.pixels[index], background,
                "ordinary Scene corner clear"
            );
        }
        let actual = readback::read(&mut self.native.gpu, &final_output);
        expected.assert_matches(
            &actual,
            EFFECT_TOLERANCE,
            "actual natively presented Scene View final",
        );
        assert_ne!(
            source.pixels, actual.pixels,
            "the visible effect must change real Scene pixels"
        );
        Ok(SceneEvidence {
            display,
            composite,
            input,
            original,
            source,
            expected,
        })
    }

    fn replay(
        &mut self,
        input: &WgpuRenderInput,
    ) -> Result<(WgpuProgramTextureOutput<WgpuWorldColor>, Image, Image), String> {
        let producer = self
            .native
            .gpu
            .create_producer()
            .map_err(|error| error.to_string())?;
        let RenderFrameTarget::Image(target) = input.packet().target() else {
            return Err("the original Scene candidate is not an image transaction".into());
        };
        self.inspection
            .lock()
            .map_err(|_| "inspection mutex poisoned")?
            .clear();
        let reservation = self
            .native
            .gpu
            .reserve_image_target(&producer, input.packet().source_generation(), target)
            .map_err(|error| error.to_string())?;
        self.native
            .gpu
            .activate_image_target(&producer, reservation)
            .map_err(|error| error.to_string())?;
        let candidate = self
            .native
            .gpu
            .prepare(&producer, input)
            .map_err(|error| error.to_string())?;
        let ports = {
            let mut inspection = self
                .inspection
                .lock()
                .map_err(|_| "inspection mutex poisoned")?;
            if inspection.len() != 1 {
                return Err(
                    "the original candidate did not publish exactly one set of CPU ports".into(),
                );
            }
            inspection.pop().unwrap()
        };
        let published = self
            .native
            .gpu
            .submit_candidate(&producer, candidate)
            .map_err(|failure| failure.error.to_string())?;
        assert_eq!(
            published.image().receipt().source_generation(),
            input.packet().source_generation()
        );
        assert_eq!(
            published.image().receipt().render_transaction_frame_index(),
            input.packet().frame_index()
        );
        let original = published
            .outputs()
            .texture(&ports.original_world)
            .map_err(|error| error.to_string())?;
        let source = readback::read(&mut self.native.gpu, &original);
        let blurred_expected = source.blur(self.configuration.radius);
        let expected = blurred_expected.grade(self.configuration.grade);
        assert_ne!(
            source.pixels, blurred_expected.pixels,
            "the ordinary Sprite edges must exercise a non-identity blur"
        );
        assert_ne!(
            blurred_expected.pixels, expected.pixels,
            "the independent grade must change the blurred Scene"
        );
        let SelectedPorts::World {
            blurred,
            graded,
            resolved: None,
        } = ports.selected
        else {
            return Err(
                "the no-Canvas Scene did not select the independent world blur/grade branches"
                    .into(),
            );
        };
        assert!(ports.original_scene.is_none());
        let blurred = published
            .outputs()
            .texture(&blurred)
            .map_err(|error| error.to_string())?;
        let graded = published
            .outputs()
            .texture(&graded)
            .map_err(|error| error.to_string())?;
        blurred_expected.assert_matches(
            &readback::read(&mut self.native.gpu, &blurred),
            EFFECT_TOLERANCE,
            "same external blur branch",
        );
        expected.assert_matches(
            &readback::read(&mut self.native.gpu, &graded),
            EFFECT_TOLERANCE,
            "same external grade branch",
        );
        let selected = published
            .final_texture::<WgpuWorldColor>()
            .map_err(|error| error.to_string())?;
        expected.assert_matches(
            &readback::read(&mut self.native.gpu, &selected),
            EFFECT_TOLERANCE,
            "replayed selected final",
        );
        // Effects cannot overwrite the original branch, even after all successor reads.
        source.assert_matches(
            &readback::read(&mut self.native.gpu, &original),
            0,
            "retained before-effects Scene",
        );
        assert!(
            self.native
                .gpu
                .retire_producer(&producer)
                .ok_or("the replay producer did not retain its original submission prefix")?
                .is_complete()
        );
        Ok((original, source, expected))
    }

    fn pick_sprite(&mut self) -> Result<(), String> {
        let frame = self
            .application
            .current_frame()
            .ok_or("the Scene has no frame")?;
        let document = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the Scene is not active")?
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
        self.wait_presented()?;
        assert!(
            self.application
                .current_frame()
                .unwrap()
                .workspace()
                .active_scene
                .as_ref()
                .unwrap()
                .editor
                .inspector
                .selected_entity
                .is_none()
        );
        let point = authoring_center(self.native, self.application)?;
        self.paint(point, |_| {})?;
        self.paint(point, |context| {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, true)
        })?;
        self.paint(point, |context| {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, false)
        })?;
        let frame = self
            .application
            .current_frame()
            .ok_or("picking lost the Session")?;
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("picking lost the source")?;
        if scene.document != document
            || scene
                .editor
                .inspector
                .selected_entity
                .as_ref()
                .map(SceneEntityId::as_str)
                != Some(SPRITE)
        {
            return Err(
                "the real Scene View click did not select the exact containing-source Sprite"
                    .into(),
            );
        }
        Ok(())
    }

    fn mutate(&mut self, undo: bool) -> Result<(), String> {
        let frame = self
            .application
            .current_frame()
            .ok_or("the Scene has no frame")?;
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the Scene is not active")?;
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
                .ok_or("the picked Sprite has no ordinary Transform Inspector")?;
            EditorDocumentMutation::ApplyInspectorCommand {
                document: scene.document,
                base_revision: scene.revision,
                command: SceneInspectorCommand::SetField {
                    entity: SceneEntityId::new(SPRITE).unwrap(),
                    component: ComponentTypeId::new(TRANSFORM),
                    component_version: component.schema_version,
                    field: ComponentFieldId::new(TRANSLATION_X),
                    value: ComponentValue::f64(MOVED_X).unwrap(),
                },
            }
        };
        let revision = scene.revision;
        let submission = self.application.submit_and_drive(
            EditorActionBatch::new(frame.revision(), [EditorAction::Mutate(mutation)]),
            Duration::ZERO,
        );
        if !matches!(submission.actions(), [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))] if report.applied)
        {
            return Err(format!(
                "the normal source edit/Undo was refused: {:?}",
                submission.actions()
            ));
        }
        let frame = self.application.current_frame().unwrap();
        assert_ne!(
            frame.workspace().active_scene.as_ref().unwrap().revision,
            revision
        );
        // No new native submission has run. The old exact receipt must lose source authority
        // before the newly edited document can be presented or picked.
        if self.is_presented() {
            return Err(
                "the old native Scene receipt authorized the changed source revision".into(),
            );
        }
        Ok(())
    }

    fn run(&mut self) -> Result<(), String> {
        let info = self
            .native
            .gpu
            .with_device(editor_window_packet(&self.native.window), |scope| {
                scope.adapter().get_info()
            })
            .map_err(|error| error.to_string())?;
        if info.backend != dear_imgui_wgpu::wgpu::Backend::Dx12
            || info.device_type == dear_imgui_wgpu::wgpu::DeviceType::Cpu
        {
            return Err(format!(
                "the native trajectory requires an actual DX12 adapter, got {info:?}"
            ));
        }
        self.wait_presented()?;
        self.pick_sprite()?;
        let initial = self.observe()?;
        let identity = self.native.scene.identity;
        self.mutate(false)?;
        assert_eq!(self.native.scene.submitted, Some(initial.composite.image()));
        self.wait_presented()?;
        let moved = self.observe()?;
        assert_eq!(self.native.scene.identity, identity);
        assert_ne!(
            initial.composite.image().output_version(),
            moved.composite.image().output_version()
        );
        assert_ne!(initial.display.occurrence(), moved.display.occurrence());
        assert!(
            !self
                .native
                .scene_presenter
                .matches_occurrence(Some(initial.display.occurrence()))
        );
        assert_ne!(
            initial.expected.pixels, moved.expected.pixels,
            "ordinary source editing must move visible graded pixels"
        );
        self.mutate(true)?;
        self.wait_presented()?;
        let undone = self.observe()?;
        assert_eq!(self.native.scene.identity, identity);
        assert_ne!(
            moved.composite.image().output_version(),
            undone.composite.image().output_version()
        );
        initial.expected.assert_matches(
            &undone.expected,
            0,
            "Undo restores the original effect result",
        );
        for evidence in [&initial, &moved] {
            let output = evidence
                .display
                .output()
                .texture::<WgpuWorldColor>()
                .map_err(|error| error.to_string())?;
            evidence.expected.assert_matches(
                &readback::read(&mut self.native.gpu, &output),
                EFFECT_TOLERANCE,
                "old actual Scene pixels survive later native publication",
            );
            evidence.source.assert_matches(
                &readback::read(&mut self.native.gpu, &evidence.original),
                0,
                "old original branch survives later native publication",
            );
        }
        // The exact original public CPU capture remains executable after changed/Undo source
        // generations and retired replay producers. This result is separate from display proof.
        let (_, replayed_source, replayed) = self.replay(&initial.input)?;
        initial
            .source
            .assert_matches(&replayed_source, 0, "exact old capture source replay");
        initial.expected.assert_matches(
            &replayed,
            EFFECT_TOLERANCE,
            "exact old capture effect replay",
        );
        Ok(())
    }
}

fn authoring_center(
    native: &NativeImgui,
    application: &EditorApplication,
) -> Result<[f32; 2], String> {
    let workspace = application.panel_workspace();
    let panel = workspace
        .layout()
        .instances()
        .iter()
        .find(|panel| panel.kind() == "nara.authoring-view" && panel.is_visible())
        .ok_or("the real Authoring pane is not visible")?;
    let title = workspace
        .registry()
        .descriptor(panel.kind())
        .ok_or("the Authoring pane has no descriptor")?
        .title();
    let name = CString::new(format!("{title}###nara-panel-{}", panel.id().0))
        .map_err(|error| error.to_string())?;
    native
        .context
        .binding()
        .try_with_bound_context(|| {
            // SAFETY: only this live, bound test Context's native pane geometry is borrowed. No
            // pointer escapes, no OS input is injected, and no GPU resource is accessed here.
            unsafe {
                let window = dear_imgui_rs::sys::igFindWindowByName(name.as_ptr());
                let Some(window) = window.as_ref() else {
                    return Err("the real Authoring window was not drawn".into());
                };
                if !window.Active || window.Hidden {
                    return Err("the real Authoring window is hidden".into());
                }
                Ok([
                    (window.WorkRect.Min.x + window.WorkRect.Max.x) * 0.5,
                    (window.WorkRect.Min.y + window.WorkRect.Max.y) * 0.5,
                ])
            }
        })
        .map_err(|error| error.to_string())?
}

fn write_scene(project: &Path) -> Result<(), String> {
    // Author a regular source document with real registered component defaults. No Sprite
    // extraction, render packet, stock lowering or alternate preview Runtime is built here.
    let mut registry = ComponentRegistry::new();
    nara::transform::register_transform_components(&mut registry)
        .map_err(|error| error.to_string())?;
    nara::sprite::register_sprite_components(&mut registry).map_err(|error| error.to_string())?;
    registry.freeze().map_err(|error| error.to_string())?;
    let initial = |id: &str| -> Result<SceneComponentRecord, String> {
        let schema = registry
            .schema(&ComponentTypeId::new(id))
            .ok_or("missing source component schema")?;
        Ok(SceneComponentRecord::new(
            schema.version(),
            schema
                .authoring_initial_value()
                .ok_or("missing source component defaults")?
                .clone(),
        ))
    };
    let transform = initial(TRANSFORM)?;
    let mut sprite = initial("nara.sprite.Sprite")?;
    let ComponentValue::Map(fields) = &mut sprite.value else {
        return Err("Sprite defaults are not a map".into());
    };
    fields.insert(
        "size".into(),
        ComponentValue::map([
            ("x", ComponentValue::f64(240.0).unwrap()),
            ("y", ComponentValue::f64(240.0).unwrap()),
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
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new(SPRITE).unwrap(),
    )
    .with_component(ComponentTypeId::new(TRANSFORM), transform)
    .with_component(ComponentTypeId::new("nara.sprite.Sprite"), sprite)]);
    fs::write(
        project.join("scenes/startup.scene.json"),
        document
            .to_json_string()
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}
