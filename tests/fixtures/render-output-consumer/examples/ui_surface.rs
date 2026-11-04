//! Native presentation with injected pointer/IME events and logical keys, not OS input delivery.
#![forbid(unsafe_code)]

#[allow(dead_code)]
#[path = "../../render-composition-consumer/tests/support/window_gpu.rs"]
mod window_gpu;

use nara_core::{Color, Mat3};
use nara_ecs::Entity;
use nara_render::{
    Extent2d, ExtractedView, ExtractedViews, RenderFrameSourceGeneration, RenderImage2d,
    RenderImagePacket, RenderTarget, RenderViewId, RenderWindowPacket, ViewLoadOp, ViewMapping2d,
    ViewportRect, build_image_render_frame_packet,
};
use nara_render_wgpu::{
    WgpuImageOutput, WgpuPolicyDefinition, WgpuPolicyInstallation, WgpuRenderHost,
    WgpuRenderHostError, WgpuRenderProducer, WgpuStockCapture, WgpuStockPolicy,
    WgpuUiImageSubmission, WgpuUiSubmission,
};
use nara_text::{TextEdit, TextEngine, TextRect, TextSelection, bundled_font_face};
use nara_ui_kernel::{
    UiActionId, UiAuthoringSnapshotBuilder, UiBrush, UiButtonPhase, UiColorRole, UiCompiler,
    UiDispatchBatch, UiEnvironment, UiFrame, UiInputEvent, UiInteractionSession, UiKey, UiLayer,
    UiLocalKey, UiPoint, UiPointerButton, UiPointerId, UiRect, UiRoot, UiRootKey, UiRouteStamp,
    UiSeatId, UiSize, UiStyle, UiSurfaceContentTransform, UiSurfaceGenerations, UiSurfaceId,
    UiTheme, UiVisual,
};
use nara_ui_module_consumer::keyed_editors::{EditorRow, KeyedEditors};
use nara_ui_render::{UiPreparedImages, UiSurfacePresentationEvent, UiSurfacePresenter};
use nara_ui_widgets::{
    UiButton, UiButtonState, UiIntent, UiIntentBatch, UiStateHandle, UiStateTextInput, UiTextField,
    UiTextFieldError, UiTextInputToken,
};
use nara_window::{PresentMode, WindowId, WindowResolution, backend::WindowHandleProvider};
use std::{
    io,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use window_gpu::{ImagePainter, Result};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition},
    event::{DeviceId, ElementState, Ime, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId as NativeWindowId},
};

// No Runtime, document replica, alternative device request, or independent Queue::submit.
struct Graphics {
    host: WgpuRenderHost,
    producer: WgpuRenderProducer,
    policy: WgpuPolicyInstallation<WgpuStockPolicy>,
    target: RenderImagePacket,
    source: RenderFrameSourceGeneration,
    view: RenderViewId,
    painter: ImagePainter,
    frame: u64,
}

fn window_packet(window: &Window) -> RenderWindowPacket {
    let size = window.inner_size();
    RenderWindowPacket {
        id: WindowId::PRIMARY,
        resolution: WindowResolution::new(size.width, size.height)
            .with_scale_factor(window.scale_factor()),
        present_mode: PresentMode::AutoNoVsync,
    }
}

impl Graphics {
    fn new(window: &Arc<Window>) -> Result<Self> {
        let mut host = WgpuRenderHost::new(
            WindowId::PRIMARY,
            WindowHandleProvider::new(Arc::clone(window)),
        )?;
        let packet = window_packet(window);
        let painter = host.with_device(packet, |scope| {
            ImagePainter::new(scope.device(), scope.surface_format(), false)
        })?;
        let source = RenderFrameSourceGeneration::try_acquire().unwrap();
        let target = RenderImagePacket::new(
            RenderImage2d::allocate_handle()?,
            Extent2d::new(
                packet.resolution.physical_width,
                packet.resolution.physical_height,
            )
            .unwrap(),
        )?;
        let producer = host.create_producer()?;
        let reservation = host.reserve_image_target(&producer, source, target)?;
        host.activate_image_target(&producer, reservation)?;
        Ok(Self {
            host,
            producer,
            policy: WgpuPolicyDefinition::new(WgpuStockPolicy::new()).install()?,
            target,
            source,
            view: RenderViewId::try_acquire().unwrap(),
            painter,
            frame: 0,
        })
    }

    fn close(mut self) -> Result<()> {
        self.policy.close();
        let prefix = self.host.retire_producer(&self.producer);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.host.poll_retirement()? {
            if Instant::now() >= deadline {
                return Err(io::Error::other("original Host retirement timed out").into());
            }
            thread::sleep(Duration::from_millis(1));
        }
        if let Some(prefix) = prefix {
            assert!(prefix.poll()?);
        }
        assert_eq!(self.host.cleanup_fault_count(), 0);
        assert_eq!(self.host.abandoned_submission_count(), 0);
        Ok(())
    }
}

struct Controls {
    root: UiRootKey,
    popup_root: UiRootKey,
    field: UiTextField,
    buttons: [(UiButton, UiButtonState); 2],
    popup: bool,
}

impl Controls {
    fn new(incarnation: u64, draft: &str, engine: &mut TextEngine) -> Result<Self> {
        let root = UiRootKey::runtime("independent-native-ui", incarnation.to_string())?;
        let popup_root = UiRootKey::runtime("independent-native-popup", incarnation.to_string())?;
        let button = |root: &UiRootKey, action: &str, label: &str| -> Result<_> {
            Ok((
                UiButton::new(
                    UiKey::scoped(root.clone(), UiLocalKey::runtime("button", action)?)?,
                    label,
                    UiActionId::new(action)?,
                )?,
                UiButtonState::default(),
            ))
        };
        Ok(Self {
            field: UiTextField::new(root.clone(), "Draft", draft, 18.0, engine)?,
            buttons: [
                button(&root, "open", "Open popup")?,
                button(&popup_root, "apply", "Apply once")?,
            ],
            root,
            popup_root,
            popup: false,
        })
    }

    fn button_node(
        &self,
        index: usize,
        root: &UiRootKey,
        style: UiStyle,
    ) -> Result<nara_ui_kernel::UiNode> {
        Ok(self.buttons[index].0.authored_node(
            root,
            style,
            UiVisual::role(UiColorRole::Accent),
            UiBrush::Role(UiColorRole::PrimaryText),
            16.0,
        )?)
    }
}

struct Probe {
    window: Arc<Window>,
    gpu: Option<Graphics>,
    presenter: UiSurfacePresenter,
    surface: UiSurfaceId,
    font: nara_text::FontFace,
    compiler: UiCompiler,
    text: TextEngine,
    controls: Controls,
    interaction: UiInteractionSession,
    route: Option<UiRouteStamp>,
    pending: Option<WgpuUiImageSubmission>,
    cursor: UiPoint,
    modifiers: ModifiersState,
    old_route: Option<UiRouteStamp>,
    old_token: Option<UiTextInputToken>,
    old_pixels: Option<WgpuImageOutput>,
    keyed: Option<KeyedEditors>,
    keyed_fields: Vec<UiStateHandle<UiTextField>>,
    keyed_previous: Vec<UiStateHandle<UiTextField>>,
    keyed_alpha: Option<UiStateHandle<UiTextField>>,
    keyed_token: Option<UiStateTextInput>,
    keyed_reset_input: Option<UiStateTextInput>,
    keyed_geometry: Option<(TextSelection, Option<TextRect>)>,
    keyed_slot: Option<UiRect>,
    stage: u8,
    retired: bool,
    presents: usize,
    commits: usize,
    opens: usize,
    actions: usize,
    started: Instant,
}

impl Probe {
    fn new(window: Arc<Window>) -> Result<Self> {
        let font = bundled_font_face()?;
        let mut text = TextEngine::new(font.clone(), Default::default())?;
        let controls = Controls::new(1, "12", &mut text)?;
        let gpu = Graphics::new(&window)?;
        let surface = UiSurfaceId::new(91).unwrap();
        let presenter = UiSurfacePresenter::with_fonts(
            surface,
            1,
            RenderTarget::Image(gpu.target.image()),
            vec![font.clone()],
            Default::default(),
            Default::default(),
        )?;
        Ok(Self {
            window,
            gpu: Some(gpu),
            presenter,
            surface,
            compiler: UiCompiler::new(font.clone(), Default::default())?,
            font,
            text,
            controls,
            interaction: UiInteractionSession::default(),
            route: None,
            pending: None,
            cursor: UiPoint::new(0.0, 0.0),
            modifiers: ModifiersState::empty(),
            old_route: None,
            old_token: None,
            old_pixels: None,
            keyed: None,
            keyed_fields: Vec::new(),
            keyed_previous: Vec::new(),
            keyed_alpha: None,
            keyed_token: None,
            keyed_reset_input: None,
            keyed_geometry: None,
            keyed_slot: None,
            stage: 0,
            retired: false,
            presents: 0,
            commits: 0,
            opens: 0,
            actions: 0,
            started: Instant::now(),
        })
    }

    fn compile(&mut self) -> Result<Arc<UiFrame>> {
        let base = if let Some(keyed) = &mut self.keyed {
            let ids: &[&str] = match self.stage {
                5 => &["alpha", "beta"],
                6 => &["beta", "inserted", "alpha"],
                7 => &["beta", "inserted"],
                _ => &["beta", "inserted", "gamma"],
            };
            let rows: Vec<_> = ids
                .iter()
                .map(|&id| {
                    let reset = id == "gamma" && self.stage >= 9;
                    EditorRow {
                        id,
                        document: if reset {
                            "document-two"
                        } else {
                            "document-one"
                        },
                        schema: if reset { 2 } else { 1 },
                        initial: match id {
                            "alpha" => "Alpha",
                            "beta" => "Beta",
                            "inserted" => "Inserted",
                            _ if reset => "Reset",
                            _ => "Gamma",
                        },
                        replace: id == "gamma" && self.stage == 9,
                    }
                })
                .collect();
            let (root, fields) = keyed
                .compose(&rows, &mut self.text)
                .map_err(|error| -> Box<dyn std::error::Error> { error })?;
            self.keyed_previous = std::mem::replace(&mut self.keyed_fields, fields);
            root
        } else {
            UiRoot::new(self.controls.root.clone(), UiLayer::Base, UiStyle::fill())
                .with_visual(UiVisual::role(UiColorRole::Surface))
                .with_child(self.controls.field.authored_node(
                    &self.controls.root,
                    UiStyle::absolute(12.0, 12.0, 150.0, 40.0),
                    150.0,
                )?)
                .with_child(self.controls.button_node(
                    0,
                    &self.controls.root,
                    UiStyle::absolute(12.0, 72.0, 150.0, 40.0),
                )?)
        };
        let mut builder = UiAuthoringSnapshotBuilder::new(self.presents as u64 + 1).with_root(base);
        if self.keyed.is_none() && self.controls.popup {
            builder = builder.with_root(
                UiRoot::new(
                    self.controls.popup_root.clone(),
                    UiLayer::Modal,
                    UiStyle::absolute(180.0, 12.0, 168.0, 140.0),
                )
                .with_visual(UiVisual::role(UiColorRole::Panel))
                .with_child(self.controls.button_node(
                    1,
                    &self.controls.popup_root,
                    UiStyle::absolute(8.0, 24.0, 150.0, 40.0),
                )?),
            );
        }
        let size = self
            .window
            .inner_size()
            .to_logical::<f32>(self.window.scale_factor());
        let generation = self.presents as u64 + 1;
        let environment = UiEnvironment::for_surface(
            self.surface,
            UiSurfaceContentTransform::direct(
                UiSize::new(size.width, size.height).unwrap(),
                self.window.scale_factor() as f32,
            )?,
            UiSurfaceGenerations::new(1, generation, generation, generation)?,
            1,
            1,
        )?;
        let frame = self
            .compiler
            .compile(&builder.build()?, environment, UiTheme::default())?;
        assert!(frame.diagnostics().is_empty());
        assert!(
            frame
                .paint()
                .iter()
                .any(|p| matches!(p, nara_ui_kernel::UiPaintPrimitive::Text { .. }))
        );
        Ok(frame)
    }

    fn draw(&mut self, retire_before_composite: bool) -> Result<bool> {
        let previous = self
            .presenter
            .journal()
            .latest(self.surface)
            .ok()
            .map(|pair| pair.receipt());
        let candidate = if self.pending.is_none() {
            Some(self.compile()?)
        } else {
            None
        };
        let gpu = self.gpu.as_mut().unwrap();
        gpu.frame += 1;
        let window = window_packet(&self.window);
        gpu.host.begin_window_frame(window, gpu.frame)?;
        if let Some(frame) = candidate {
            let backend = self.presenter.prepare(frame, &UiPreparedImages::new())?;
            let (packets, sidecar) = WgpuUiSubmission::capture(backend);
            let viewport = ViewportRect::from_extent(gpu.target.extent()).unwrap();
            let mut views = ExtractedViews::default();
            views.push(ExtractedView {
                id: gpu.view,
                camera_entity: Entity::PLACEHOLDER,
                target: RenderTarget::Image(gpu.target.image()),
                viewport,
                mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, 200.0, viewport).unwrap(),
                order: 0,
                load: ViewLoadOp::Clear(Color::BLACK),
            });
            let input = gpu.policy.capture(
                build_image_render_frame_packet(gpu.source, gpu.frame, &views, gpu.target)?
                    .unwrap(),
                Arc::new(WgpuStockCapture::new().with_ui(packets, None, None)),
            )?;
            self.pending = Some(
                match gpu.host.submit_for_display(&gpu.producer, &input, sidecar) {
                    Ok(submitted) => submitted,
                    Err(failure) => {
                        let (error, outcomes) = failure.complete();
                        for outcome in outcomes {
                            self.presenter.return_outcome(outcome).unwrap();
                        }
                        let _ = self.presenter.drain()?;
                        return Err(error.into());
                    }
                },
            );
            assert_eq!(
                self.presenter
                    .journal()
                    .latest(self.surface)
                    .ok()
                    .map(|pair| pair.receipt()),
                previous,
                "preparation and image submission cannot publish input authority"
            );
            if self.stage == 0 {
                match std::env::var("NARA_UI_SURFACE_FAILURE_AFTER_SUBMIT").as_deref() {
                    Ok("error") => {
                        return Err(
                            io::Error::other("injected native UI failure after submit").into()
                        );
                    }
                    Ok("panic") => panic!("injected native UI failure after submit"),
                    _ => {}
                }
            }
        }
        if retire_before_composite && !self.retired {
            self.old_route = self.route.take();
            assert!(self.presenter.retire(gpu.frame + 1)?.is_some());
            assert!(self.presenter.journal().latest(self.surface).is_err());
            self.retired = true;
        }
        let submitted = self.pending.as_ref().unwrap();
        let display = submitted.display().clone();
        let group = gpu.host.with_image(submitted.output(), |device, _, view| {
            gpu.painter.bind(device, view, view)
        })?;
        let extent = gpu.target.extent();
        assert_eq!(
            (
                window.resolution.physical_width,
                window.resolution.physical_height
            ),
            (extent.width, extent.height)
        );
        let outcome = gpu.host.present_window(
            window,
            gpu.frame,
            std::slice::from_ref(&display),
            |frame| {
                frame.encode(|_, _, encoder, view| {
                    gpu.painter.draw_placed(
                        encoder,
                        view,
                        &group,
                        [0, 0, extent.width, extent.height],
                        [0, 0, extent.width, extent.height],
                    )
                });
                frame.record_image_sample(&display)
            },
            || self.window.pre_present_notify(),
        )?;
        let Some(window_receipt) = outcome.window() else {
            assert!(outcome.composites().is_empty());
            return Ok(false);
        };
        assert_eq!(outcome.composites().len(), 1);
        let composite = outcome.composites()[0];
        assert_eq!(composite.window(), window_receipt);
        assert_eq!(composite.image(), display.receipt());
        assert_eq!(composite.occurrence(), display.occurrence());
        assert_eq!(
            window_receipt.backend_instance_id(),
            gpu.host.backend_instance_id()
        );
        assert_eq!(window_receipt.device_epoch(), gpu.host.device_epoch());
        if !retire_before_composite {
            assert_eq!(
                self.presenter
                    .journal()
                    .latest(self.surface)
                    .ok()
                    .map(|pair| pair.receipt()),
                previous,
                "native presentation still needs its original one-shot outcome returned and drained"
            );
        }
        self.presents += 1;
        if retire_before_composite {
            self.old_pixels = Some(display.output().clone());
        }
        let completed = self.pending.take().unwrap().presented(composite).unwrap();
        for outcome in completed {
            self.presenter.return_outcome(outcome).unwrap();
        }
        let events = self.presenter.drain()?;
        if retire_before_composite {
            assert!(matches!(
                events.as_slice(),
                [UiSurfacePresentationEvent::Discarded { attempt: Some(_) }]
            ));
            assert!(self.presenter.journal().latest(self.surface).is_err());
            assert!(self.presenter.is_quiescent());
        } else {
            let [UiSurfacePresentationEvent::Presented(receipt)] = events.as_slice() else {
                return Err(io::Error::other(format!(
                    "expected one original presentation commit: {events:?}"
                ))
                .into());
            };
            self.route = Some(UiRouteStamp::from_receipt(*receipt));
            self.commits += 1;
            if let Some(batch) = self.interaction.reconcile_latest_presentation(
                self.presenter.journal(),
                self.surface,
                &[],
            )? {
                self.reduce(&batch)?;
            }
        }
        Ok(true)
    }

    fn reduce(&mut self, batch: &UiDispatchBatch) -> Result<()> {
        if let Some(keyed) = &mut self.keyed {
            assert!(self.controls.field.input_token().is_none());
            keyed.state.reduce_text_focus(batch, &mut self.text)?;
            return Ok(());
        }
        self.controls.field.reduce_focus(batch, &mut self.text)?;
        let mut intents = UiIntentBatch::default();
        for (button, state) in &mut self.controls.buttons {
            state.reduce_batch(button, batch, &mut intents)?;
        }
        for UiIntent::Activate { action, .. } in intents.intents() {
            match action.as_str() {
                "open" => {
                    self.opens += 1;
                    self.controls.popup = true;
                }
                "apply" => {
                    self.actions += 1;
                    self.controls.popup = false;
                }
                other => return Err(io::Error::other(format!("unexpected action {other}")).into()),
            }
        }
        Ok(())
    }

    fn route_input(&mut self, input: UiInputEvent) -> Result<()> {
        let outcome =
            self.interaction
                .dispatch(self.presenter.journal(), self.route.unwrap(), input)?;
        if let Some(batch) = outcome.consumed() {
            self.reduce(batch)?;
        }
        Ok(())
    }

    fn edit(&mut self, action: TextEdit) -> Result<()> {
        let keyed_input = self
            .keyed
            .as_ref()
            .map(|_| self.keyed_input())
            .transpose()?;
        if let (Some(keyed), Some(admitted)) = (&mut self.keyed, keyed_input) {
            let input = self
                .keyed_token
                .as_ref()
                .filter(|saved| *saved == &admitted)
                .unwrap_or(&admitted);
            keyed.state.edit_text(input, &action, &mut self.text)?;
            return Ok(());
        }
        let snapshot = self.interaction.semantic_snapshot(
            self.presenter.journal(),
            self.route.unwrap(),
            UiSeatId::PRIMARY,
        )?;
        assert!(
            snapshot
                .nodes()
                .iter()
                .any(|node| node.key() == self.controls.field.key()
                    && node.state().focused()
                    && !node.state().inert())
        );
        self.controls.field.edit(
            self.controls.field.input_token().unwrap(),
            &action,
            &mut self.text,
        )?;
        Ok(())
    }

    // Winit's KeyEvent has private platform fields. Inject logical keys through this same
    // adapter used by real KeyboardInput; never manufacture a platform KeyEvent or move the OS pointer.
    fn key(&mut self, key: &Key) -> Result<()> {
        let edit = match key {
            Key::Character(value) if self.modifiers.control_key() => match value.as_str() {
                "a" => TextEdit::SelectAll,
                "z" => TextEdit::Undo,
                "y" => TextEdit::Redo,
                _ => return Ok(()),
            },
            Key::Named(NamedKey::End) => TextEdit::End { extend: false },
            Key::Character(value) => TextEdit::Insert(value.to_string()),
            _ => return Ok(()),
        };
        self.edit(edit)
    }

    fn window_event(&mut self, event: WindowEvent) -> Result<()> {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let point = position.to_logical::<f32>(self.window.scale_factor());
                self.cursor = UiPoint::new(point.x, point.y);
                self.route_input(UiInputEvent::PointerMoved {
                    seat: UiSeatId::PRIMARY,
                    pointer: UiPointerId::PRIMARY,
                    position: self.cursor,
                })?;
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.route_input(UiInputEvent::PointerButton {
                    seat: UiSeatId::PRIMARY,
                    pointer: UiPointerId::PRIMARY,
                    button: UiPointerButton::Primary,
                    phase: if state == ElementState::Pressed {
                        UiButtonPhase::Pressed
                    } else {
                        UiButtonPhase::Released
                    },
                    position: Some(self.cursor),
                })?;
            }
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::Focused(false) => {
                // Hidden qualification needs no OS focus. Losing it cancels existing input;
                // subsequent injected commands still require the exact presented route.
                self.modifiers = ModifiersState::empty();
                if self.route.is_some() {
                    self.route_input(UiInputEvent::SeatCancelled {
                        seat: UiSeatId::PRIMARY,
                    })?;
                }
                self.controls.field.blur(&mut self.text)?;
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.key(&event.logical_key)?
            }
            WindowEvent::Ime(Ime::Preedit(text, cursor)) => {
                self.edit(TextEdit::Preedit { text, cursor })?
            }
            WindowEvent::Ime(Ime::Commit(text)) => self.edit(TextEdit::Commit(text))?,
            _ => {}
        }
        Ok(())
    }

    fn click(&mut self, x: f64, y: f64) -> Result<()> {
        self.window_event(WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: PhysicalPosition::new(
                x * self.window.scale_factor(),
                y * self.window.scale_factor(),
            ),
        })?;
        for state in [ElementState::Pressed, ElementState::Released] {
            self.window_event(WindowEvent::MouseInput {
                device_id: DeviceId::dummy(),
                state,
                button: MouseButton::Left,
            })?;
        }
        Ok(())
    }

    fn keyed_input(&self) -> Result<UiStateTextInput> {
        let snapshot = self.interaction.semantic_snapshot(
            self.presenter.journal(),
            self.route.unwrap(),
            UiSeatId::PRIMARY,
        )?;
        let state = &self.keyed.as_ref().unwrap().state;
        for handle in &self.keyed_fields {
            let key = state.get(handle)?.key();
            if snapshot
                .nodes()
                .iter()
                .any(|node| node.key() == key && node.state().focused() && !node.state().inert())
            {
                return Ok(state
                    .text_input(handle)?
                    .expect("admitted current field focus"));
            }
        }
        Err(io::Error::other("no currently presented keyed field has admitted focus").into())
    }

    fn keyed_rect(&self, index: usize) -> Result<UiRect> {
        let field = self
            .keyed
            .as_ref()
            .unwrap()
            .state
            .get(&self.keyed_fields[index])?;
        let snapshot = self.interaction.semantic_snapshot(
            self.presenter.journal(),
            self.route.unwrap(),
            UiSeatId::PRIMARY,
        )?;
        Ok(snapshot
            .nodes()
            .iter()
            .find(|node| node.key() == field.key())
            .expect("current field must have visible presented semantics")
            .rect())
    }

    fn click_rect(&mut self, rect: UiRect) -> Result<()> {
        self.click(
            f64::from((rect.origin().x() + rect.max().x()) * 0.5),
            f64::from((rect.origin().y() + rect.max().y()) * 0.5),
        )
    }

    fn step(&mut self) -> Result<bool> {
        if self.started.elapsed() > Duration::from_secs(40) {
            return Err(io::Error::other("native presentation deadline exceeded").into());
        }
        if !self.draw(self.stage == 2)? {
            return Ok(false);
        }
        match self.stage {
            0 => {
                self.click(30.0, 30.0)?;
                self.old_token = self.controls.field.input_token();
                assert!(self.old_token.is_some());
                self.window_event(WindowEvent::ModifiersChanged(
                    ModifiersState::CONTROL.into(),
                ))?;
                self.key(&Key::Character("a".into()))?;
                self.window_event(WindowEvent::Ime(Ime::Preedit("34".into(), Some((0, 2)))))?;
                assert_eq!(self.controls.field.editor().text(), "12");
                self.window_event(WindowEvent::Ime(Ime::Commit("34".into())))?;
                assert_eq!(self.controls.field.editor().text(), "34");
                self.key(&Key::Character("z".into()))?;
                assert_eq!(self.controls.field.editor().text(), "12");
                self.key(&Key::Character("y".into()))?;
                assert_eq!(self.controls.field.editor().text(), "34");
                self.window_event(WindowEvent::ModifiersChanged(
                    ModifiersState::empty().into(),
                ))?;
                self.click(30.0, 90.0)?;
                assert_eq!(self.opens, 1);
                assert_eq!(self.actions, 0);
            }
            1 => {
                assert!(
                    self.presenter
                        .journal()
                        .latest(self.surface)?
                        .frame()
                        .blocks_unhandled_input()
                );
                self.click(30.0, 90.0)?;
                assert_eq!(
                    self.opens, 1,
                    "the actually presented modal blocks the underlay"
                );
                let popup_rect = self
                    .presenter
                    .journal()
                    .latest(self.surface)?
                    .frame()
                    .semantics()
                    .iter()
                    .find(|node| node.key() == self.controls.buttons[1].0.key())
                    .expect("the actual presented popup action must expose its geometry")
                    .rect();
                self.click(
                    f64::from((popup_rect.origin().x() + popup_rect.max().x()) * 0.5),
                    f64::from((popup_rect.origin().y() + popup_rect.max().y()) * 0.5),
                )?;
                assert_eq!(self.actions, 1);
                self.window_event(WindowEvent::MouseInput {
                    device_id: DeviceId::dummy(),
                    state: ElementState::Released,
                    button: MouseButton::Left,
                })?;
                assert_eq!(
                    self.actions, 1,
                    "an unpaired release cannot repeat the popup action"
                );
            }
            2 => {
                let old_route = self.old_route.unwrap();
                assert!(
                    self.interaction
                        .dispatch(
                            self.presenter.journal(),
                            old_route,
                            UiInputEvent::ActivateFocused {
                                seat: UiSeatId::PRIMARY,
                                source: nara_ui_kernel::UiActivationSource::Keyboard
                            }
                        )
                        .is_err()
                );
                let old_key = self.controls.field.key().clone();
                let draft = self.controls.field.editor().text().to_owned();
                self.controls.field.blur(&mut self.text)?;
                self.gpu.take().unwrap().close()?;
                let mut gpu = Graphics::new(&self.window)?;
                assert_ne!(
                    gpu.host.backend_instance_id(),
                    self.old_pixels
                        .as_ref()
                        .unwrap()
                        .receipt()
                        .backend_instance_id()
                );
                assert!(matches!(
                    gpu.host
                        .display_image(self.old_pixels.as_ref().unwrap().clone()),
                    Err(WgpuRenderHostError::StaleImageSubmission)
                ));
                assert!(matches!(
                    gpu.host
                        .with_image(self.old_pixels.as_ref().unwrap(), |_, _, _| ()),
                    Err(WgpuRenderHostError::StaleImageSubmission)
                ));
                self.controls = Controls::new(2, &draft, &mut self.text)?;
                assert_ne!(*self.controls.field.key(), old_key);
                self.surface = UiSurfaceId::new(92).unwrap();
                self.presenter = UiSurfacePresenter::with_fonts(
                    self.surface,
                    1,
                    RenderTarget::Image(gpu.target.image()),
                    vec![self.font.clone()],
                    Default::default(),
                    Default::default(),
                )?;
                assert!(self.presenter.journal().resolve(old_route).is_err());
                assert!(matches!(
                    self.controls.field.edit(
                        self.old_token.unwrap(),
                        &TextEdit::Commit("late".into()),
                        &mut self.text
                    ),
                    Err(UiTextFieldError::StaleInput)
                ));
                assert_eq!(self.controls.field.editor().text(), "34");
                self.gpu = Some(gpu);
                self.interaction = UiInteractionSession::default();
                self.route = None;
            }
            3 => {
                self.click(30.0, 30.0)?;
                assert!(matches!(
                    self.controls.field.edit(
                        self.old_token.unwrap(),
                        &TextEdit::Commit("late".into()),
                        &mut self.text
                    ),
                    Err(UiTextFieldError::StaleInput)
                ));
                assert!(
                    self.interaction
                        .dispatch(
                            self.presenter.journal(),
                            self.old_route.unwrap(),
                            UiInputEvent::PointerButton {
                                seat: UiSeatId::PRIMARY,
                                pointer: UiPointerId::PRIMARY,
                                button: UiPointerButton::Primary,
                                phase: UiButtonPhase::Released,
                                position: Some(UiPoint::new(30.0, 30.0))
                            }
                        )
                        .is_err()
                );
                assert_eq!(self.controls.field.editor().text(), "34");
                self.key(&Key::Named(NamedKey::End))?;
                self.window_event(WindowEvent::Ime(Ime::Commit("!".into())))?;
                assert_eq!(self.controls.field.editor().text(), "34!");
            }
            4 => {
                let latest = self.presenter.journal().latest(self.surface)?;
                assert!(
                    latest
                        .frame()
                        .semantics()
                        .iter()
                        .any(|node| node.key() == self.controls.field.key()
                            && node.value() == Some("34!"))
                );
                assert_eq!(
                    (self.presents, self.commits, self.opens, self.actions),
                    (5, 4, 1, 1)
                );
                self.controls.field.blur(&mut self.text)?;
                self.keyed = Some(
                    KeyedEditors::new(UiRootKey::runtime("native-keyed", "panel")?)
                        .map_err(|error| -> Box<dyn std::error::Error> { error })?,
                );
            }
            5..=10 => {
                if self.step_keyed()? {
                    return Ok(true);
                }
            }
            _ => unreachable!(),
        }
        self.stage += 1;
        Ok(false)
    }

    fn step_keyed(&mut self) -> Result<bool> {
        assert_eq!(self.controls.field.editor().text(), "34!");
        assert!(self.controls.field.input_token().is_none());
        let latest = self.presenter.journal().latest(self.surface)?;
        let inputs: Vec<_> = latest
            .frame()
            .semantics()
            .iter()
            .filter(|node| node.role() == nara_ui_kernel::UiSemanticRole::TextInput)
            .collect();
        assert_eq!(inputs.len(), self.keyed_fields.len());
        assert!(inputs.iter().all(|node| node.name() == "Value"));
        for handle in &self.keyed_fields {
            let field = self.keyed.as_ref().unwrap().state.get(handle)?;
            let source = field.editor().layout();
            assert!(inputs.iter().any(
                |node| node.key() == field.key() && node.value() == Some(field.editor().text())
            ));
            assert!(latest.frame().paint().iter().any(|paint| matches!(paint,
                nara_ui_kernel::UiPaintPrimitive::Text { layout, .. }
                    if layout.width() == source.width() && layout.height() == source.height()
                        && layout.glyphs().len() == source.glyphs().len()
                        && layout.glyphs().iter().zip(source.glyphs()).all(|(paint, original)|
                            paint.x() == original.x() && paint.y() == original.y()
                                && paint.advance() == original.advance()
                                && paint.key().face_id() == original.key().face_id()
                                && paint.key().glyph_id() == original.key().glyph_id()))));
        }
        match self.stage {
            5 => {
                assert_ne!(self.keyed_fields[0], self.keyed_fields[1]);
                self.keyed_alpha = Some(self.keyed_fields[0].clone());
                self.click_rect(self.keyed_rect(0)?)?;
                let first_focus = self.keyed_input()?;
                self.modifiers = ModifiersState::CONTROL;
                self.key(&Key::Character("a".into()))?;
                self.window_event(WindowEvent::Ime(Ime::Commit("Draft alpha".into())))?;
                self.click_rect(self.keyed_rect(1)?)?;
                self.click_rect(self.keyed_rect(0)?)?;
                self.keyed_token = Some(self.keyed_input()?);
                assert_ne!(self.keyed_token.as_ref(), Some(&first_focus));
                assert!(
                    self.keyed
                        .as_mut()
                        .unwrap()
                        .state
                        .edit_text(
                            &first_focus,
                            &TextEdit::Commit("late".into()),
                            &mut self.text,
                        )
                        .is_err()
                );
                self.key(&Key::Character("a".into()))?;
                self.window_event(WindowEvent::Ime(Ime::Preedit(
                    "e\u{301}".into(),
                    Some((0, 3)),
                )))?;
                let editor = self
                    .keyed
                    .as_ref()
                    .unwrap()
                    .state
                    .get(&self.keyed_fields[0])?
                    .editor();
                self.keyed_geometry = Some((editor.selection(), editor.caret()));
            }
            6 => {
                assert_eq!(self.keyed_fields[2], self.keyed_previous[0]);
                assert_eq!(self.keyed_fields[0], self.keyed_previous[1]);
                assert_eq!(Some(self.keyed_input()?), self.keyed_token);
                let editor = self
                    .keyed
                    .as_ref()
                    .unwrap()
                    .state
                    .get(&self.keyed_fields[2])?
                    .editor();
                assert_eq!(editor.text(), "Draft alpha");
                assert_eq!(editor.display_text(), "e\u{301}");
                assert!(editor.is_composing());
                assert_eq!(
                    Some((editor.selection(), editor.caret())),
                    self.keyed_geometry
                );
                self.keyed_slot = Some(self.keyed_rect(2)?);
                self.window_event(WindowEvent::Ime(Ime::Preedit(String::new(), None)))?;
                self.key(&Key::Character("z".into()))?;
                assert_eq!(
                    self.keyed
                        .as_ref()
                        .unwrap()
                        .state
                        .get(&self.keyed_fields[2])?
                        .editor()
                        .text(),
                    "Alpha"
                );
                self.key(&Key::Character("y".into()))?;
                assert_eq!(
                    self.keyed
                        .as_ref()
                        .unwrap()
                        .state
                        .get(&self.keyed_fields[2])?
                        .editor()
                        .text(),
                    "Draft alpha"
                );
                self.key(&Key::Character("a".into()))?;
                self.window_event(WindowEvent::Ime(Ime::Preedit(
                    "e\u{301}".into(),
                    Some((0, 3)),
                )))?;
                self.modifiers = ModifiersState::empty();
            }
            7 => {
                assert_eq!(self.keyed_fields, self.keyed_previous[..2]);
                assert!(
                    self.keyed
                        .as_ref()
                        .unwrap()
                        .state
                        .get(self.keyed_alpha.as_ref().unwrap())
                        .is_err()
                );
                assert_ne!(Some(self.keyed_input()?), self.keyed_token);
            }
            8 => {
                assert_eq!(Some(self.keyed_rect(2)?), self.keyed_slot);
                assert_ne!(self.keyed_fields[2], *self.keyed_alpha.as_ref().unwrap());
                let editor = self
                    .keyed
                    .as_ref()
                    .unwrap()
                    .state
                    .get(&self.keyed_fields[2])?
                    .editor();
                assert_eq!(editor.display_text(), "Gamma");
                assert!(!editor.is_composing() && !editor.can_undo() && !editor.can_redo());
                self.click_rect(self.keyed_rect(2)?)?;
                self.keyed_reset_input = Some(self.keyed_input()?);
                self.modifiers = ModifiersState::CONTROL;
                self.key(&Key::Character("a".into()))?;
                self.window_event(WindowEvent::Ime(Ime::Commit("Gamma draft".into())))?;
                self.key(&Key::Character("a".into()))?;
                self.window_event(WindowEvent::Ime(Ime::Preedit(
                    "e\u{301}".into(),
                    Some((0, 3)),
                )))?;
                self.modifiers = ModifiersState::empty();
            }
            9 => {
                assert_eq!(self.keyed_fields[..2], self.keyed_previous[..2]);
                assert_ne!(self.keyed_fields[2], self.keyed_previous[2]);
                let retired_input = self.keyed_reset_input.as_ref().unwrap().clone();
                let state = &mut self.keyed.as_mut().unwrap().state;
                assert!(state.get(&self.keyed_previous[2]).is_err());
                assert!(state.get(&self.keyed_fields[2])?.input_token().is_none());
                assert!(
                    state
                        .edit_text(
                            &retired_input,
                            &TextEdit::Commit("late".into()),
                            &mut self.text
                        )
                        .is_err()
                );
                self.click_rect(self.keyed_rect(2)?)?;
                self.keyed_reset_input = Some(self.keyed_input()?);
                assert_ne!(self.keyed_reset_input.as_ref(), Some(&retired_input));
                assert!(
                    self.keyed
                        .as_mut()
                        .unwrap()
                        .state
                        .edit_text(
                            &retired_input,
                            &TextEdit::Commit("late after refocus".into()),
                            &mut self.text,
                        )
                        .is_err()
                );
            }
            10 => {
                assert_eq!(self.keyed_fields, self.keyed_previous);
                assert_eq!(Some(self.keyed_input()?), self.keyed_reset_input);
            }
            _ => unreachable!(),
        }
        if self.stage >= 7 {
            let state = &mut self.keyed.as_mut().unwrap().state;
            let before: Vec<_> = self
                .keyed_fields
                .iter()
                .map(|handle| {
                    let editor = state.get(handle).unwrap().editor();
                    (
                        editor.text().to_owned(),
                        editor.display_text().to_owned(),
                        editor.selection(),
                    )
                })
                .collect();
            assert!(
                state
                    .edit_text(
                        self.keyed_token.as_ref().unwrap(),
                        &TextEdit::Commit("late".into()),
                        &mut self.text
                    )
                    .is_err()
            );
            for (handle, unchanged) in self.keyed_fields.iter().zip(before) {
                let editor = state.get(handle)?.editor();
                assert_eq!(
                    (
                        editor.text().to_owned(),
                        editor.display_text().to_owned(),
                        editor.selection()
                    ),
                    unchanged
                );
            }
            assert_eq!(state.get(&self.keyed_fields[0])?.editor().text(), "Beta");
        }
        if self.stage >= 9 {
            let editor = self
                .keyed
                .as_ref()
                .unwrap()
                .state
                .get(&self.keyed_fields[2])?
                .editor();
            assert_eq!(editor.text(), "Reset");
            assert_eq!(editor.display_text(), "Reset");
            assert!(!editor.is_composing() && !editor.can_undo() && !editor.can_redo());
        }
        assert_eq!((self.opens, self.actions), (1, 1));
        if self.stage == 10 {
            assert_eq!((self.presents, self.commits), (11, 10));
            return Ok(true);
        }
        Ok(false)
    }

    fn close(&mut self) -> Result<()> {
        if let Some(route) = self.route.take() {
            let cancelled = self.interaction.dispatch(
                self.presenter.journal(),
                route,
                UiInputEvent::SeatCancelled {
                    seat: UiSeatId::PRIMARY,
                },
            )?;
            if let Some(batch) = cancelled.consumed() {
                self.reduce(batch)?;
            }
        }
        self.controls.field.blur(&mut self.text)?;
        if let Some(keyed) = &mut self.keyed {
            keyed.state.begin().finish()?;
            assert!(keyed.state.is_empty());
            for input in [&self.keyed_token, &self.keyed_reset_input]
                .into_iter()
                .flatten()
            {
                assert!(
                    keyed
                        .state
                        .edit_text(
                            input,
                            &TextEdit::Commit("late close".into()),
                            &mut self.text
                        )
                        .is_err()
                );
            }
            self.keyed_fields.clear();
        }
        if let Some(pending) = self.pending.take() {
            for outcome in pending.deferred("native fixture is closing".into()) {
                self.presenter.return_outcome(outcome).unwrap();
            }
            let _ = self.presenter.drain()?;
        }
        let _ = self.presenter.retire(1000)?;
        let _ = self.presenter.drain()?;
        assert!(self.presenter.is_quiescent());
        if let Some(gpu) = self.gpu.take() {
            gpu.close()?;
        }
        Ok(())
    }
}

#[derive(Default)]
struct NativeApp {
    probe: Option<Probe>,
    result: Option<std::result::Result<(), String>>,
}

impl NativeApp {
    fn finish(
        &mut self,
        event_loop: &ActiveEventLoop,
        mut result: std::result::Result<(), String>,
    ) {
        if let Some(mut probe) = self.probe.take()
            && let Err(error) = probe.close()
        {
            result = Err(format!("{result:?}; cleanup: {error}"));
        }
        self.result = Some(result);
        event_loop.exit();
    }
}

impl ApplicationHandler for NativeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.probe.is_some() || self.result.is_some() {
            return;
        }
        let initialized = (|| -> Result<Probe> {
            let window = Arc::new(
                event_loop.create_window(
                    Window::default_attributes()
                        .with_title("Nara independent UI: injected input conformance")
                        .with_inner_size(LogicalSize::new(360.0, 200.0))
                        .with_resizable(false)
                        .with_active(false)
                        .with_visible(false),
                )?,
            );
            Probe::new(window)
        })();
        match initialized {
            Ok(probe) => self.probe = Some(probe),
            Err(error) => self.finish(event_loop, Err(error.to_string())),
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: NativeWindowId,
        event: WindowEvent,
    ) {
        let Some(probe) = &mut self.probe else {
            return;
        };
        if probe.window.id() != id {
            return;
        }
        if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed) {
            self.finish(
                event_loop,
                Err("native window closed before conformance completed".into()),
            );
            return;
        }
        if probe.route.is_some()
            && let Err(error) = probe.window_event(event)
        {
            self.finish(event_loop, Err(error.to_string()));
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(probe) = &mut self.probe else {
            return;
        };
        let result = catch_unwind(AssertUnwindSafe(|| probe.step())).unwrap_or_else(|panic| {
            let message = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|text| (*text).to_owned()))
                .unwrap_or_else(|| "native conformance assertion panicked".into());
            Err(io::Error::other(message).into())
        });
        match result {
            Ok(true) => self.finish(event_loop, Ok(())),
            Ok(false) => event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(1),
            )),
            Err(error) => self.finish(event_loop, Err(error.to_string())),
        }
    }
}

fn main() -> Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = NativeApp::default();
    let result = event_loop.run_app(&mut app);
    if let Some(mut probe) = app.probe.take() {
        probe.close()?;
    }
    result?;
    let terminal = app
        .result
        .ok_or_else(|| io::Error::other("native event loop has no terminal result"))?;
    if let Ok(mode) = std::env::var("NARA_UI_SURFACE_FAILURE_AFTER_SUBMIT") {
        if !matches!(mode.as_str(), "error" | "panic") {
            return Err(io::Error::other("unknown native UI failure control").into());
        }
        // NativeApp::finish appends any cleanup failure. Exact equality therefore requires the
        // injected failure AND successful original presenter/Host cleanup; an arbitrary failure
        // or a failure during initialization cannot satisfy this control.
        assert_eq!(
            terminal,
            Err("injected native UI failure after submit".into())
        );
        println!(
            "UI_SURFACE_FAILURE_OK mode={mode} original_permits_settled=true presenter_quiescent=true original_host_retired=true"
        );
        return Ok(());
    }
    terminal.map_err(io::Error::other)?;
    println!(
        "UI_SURFACE_OK native_presentations=11 journal_commits=10 popup_actions=1 underlay_blocked=true stale_input_rejected=true late_acceptance_discarded=true reconstructed=true original_hosts_retired=true keyed_component_presented=true keyed_reorder_preserved=true filtered_preedit_discarded=true keyed_binding_reset=true compatible_compose=true input_delivery=injected_pointer_ime_and_logical_keys"
    );
    Ok(())
}
