//! Native public panel consumption with injected pointer/IME events, not OS input automation.
#![forbid(unsafe_code)]

#[allow(dead_code)]
#[path = "../../render-composition-consumer/tests/support/window_gpu.rs"]
mod window_gpu;

use std::{
    fs, io,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use nara::{
    reflect::{
        ComponentFieldId, ComponentPropertyPath, ComponentSchemaVersion, ComponentTypeId,
        ComponentValue,
    },
    scene::{AuthoringPropertyEdit, SceneDocumentCandidate},
    tooling::{
        EditorAction, EditorActionOutcome, EditorDocumentId, EditorDocumentMutation,
        EditorPersistenceAction, EditorPersistenceRequestResult, EditorPersistenceResult,
        EditorPersistenceView, EditorPropertyValue, EditorReopenTarget,
    },
};
use nara_core::{Color, Mat3};
use nara_ecs::Entity;
use nara_editor::{EditorPanelHost, EditorPanelSubmission, PanelActionDispatch, PanelSubmissionId};
use nara_editor_ui_host_consumer::{
    COMPONENT, DraftPanel, Evidence, FIELD, INITIAL, Result,
    project::{
        self, Application, Project, SOURCE, current, entity, open_secondary, persistence, property,
        register,
    },
};
use nara_render::{
    Extent2d, ExtractedView, ExtractedViews, RenderFrameSourceGeneration, RenderImage2d,
    RenderImagePacket, RenderTarget, RenderViewId, RenderWindowPacket, ViewLoadOp, ViewMapping2d,
    ViewportRect, build_image_render_frame_packet,
};
use nara_render_wgpu::{
    WgpuPolicyDefinition, WgpuPolicyInstallation, WgpuRenderHost, WgpuRenderProducer,
    WgpuStockCapture, WgpuStockPolicy, WgpuUiImageSubmission, WgpuUiSubmission,
};
use nara_text::{TextEdit, bundled_font_face};
use nara_ui_kernel::{
    UiButtonPhase, UiCompiler, UiDispatchBatch, UiEnvironment, UiFrame, UiInputEvent,
    UiInteractionSession, UiPoint, UiPointerButton, UiPointerId, UiPointerInput, UiRouteStamp,
    UiSeatId, UiSize, UiSurfaceContentTransform, UiSurfaceGenerations, UiSurfaceId, UiTheme,
};
use nara_ui_render::{UiPreparedImages, UiSurfacePresentationEvent, UiSurfacePresenter};
use nara_ui_widgets::UiTextInputToken;
use nara_window::{PresentMode, WindowId, WindowResolution, backend::WindowHandleProvider};
use window_gpu::ImagePainter;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition},
    event::{DeviceId, ElementState, Ime, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId as NativeWindowId},
};

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
                return Err(io::Error::other("original GPU Host retirement timed out").into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        if let Some(prefix) = prefix {
            assert!(prefix.poll()?);
        }
        assert_eq!(self.host.cleanup_fault_count(), 0);
        assert_eq!(self.host.abandoned_submission_count(), 0);
        Ok(())
    }
}

struct Probe {
    window: Arc<Window>,
    gpu: Option<Graphics>,
    presenter: UiSurfacePresenter,
    surface: UiSurfaceId,
    compiler: UiCompiler,
    application: Application,
    project: Project,
    panels: EditorPanelHost<DraftPanel>,
    instance: nara_editor::workspace::EditorPanelInstanceId,
    evidence: Rc<Evidence>,
    document: EditorDocumentId,
    active: EditorDocumentId,
    interaction: UiInteractionSession,
    route: Option<UiRouteStamp>,
    pending: Option<WgpuUiImageSubmission>,
    action: Option<EditorPanelSubmission>,
    action_id: Option<PanelSubmissionId>,
    cursor: UiPoint,
    old_token: Option<UiTextInputToken>,
    retired: bool,
    stage: u8,
    presentations: usize,
    commits: usize,
    started: Instant,
}

impl Probe {
    fn new(window: Arc<Window>) -> Result<Self> {
        let project = Project::new();
        let mut application = Application::open(&project);
        let document = open_secondary(&mut application);
        let active = current(&application).workspace().active_document.unwrap();
        let mut panels = EditorPanelHost::new();
        let evidence = Rc::new(Evidence::default());
        let instance = register(
            &mut panels,
            &mut application,
            &evidence,
            "consumer.native-caption",
        );
        let font = bundled_font_face()?;
        let compiler = UiCompiler::new(font.clone(), Default::default())?;
        let gpu = Graphics::new(&window)?;
        let surface = UiSurfaceId::new(901).unwrap();
        let presenter = UiSurfacePresenter::with_fonts(
            surface,
            1,
            RenderTarget::Image(gpu.target.image()),
            vec![font],
            Default::default(),
            Default::default(),
        )?;
        Ok(Self {
            window,
            gpu: Some(gpu),
            presenter,
            surface,
            compiler,
            application,
            project,
            panels,
            instance,
            evidence,
            document,
            active,
            interaction: UiInteractionSession::default(),
            route: None,
            pending: None,
            action: None,
            action_id: None,
            cursor: UiPoint::new(0.0, 0.0),
            old_token: None,
            retired: false,
            stage: 0,
            presentations: 0,
            commits: 0,
            started: Instant::now(),
        })
    }

    fn compile(&mut self) -> Result<Arc<UiFrame>> {
        let revision = self.presentations as u64 + 1;
        let mut snapshot = None;
        drop(
            self.panels
                .prepare(&self.application, |panel, _| {
                    snapshot = Some(panel.compose(revision));
                })
                .unwrap(),
        );
        let size = self
            .window
            .inner_size()
            .to_logical::<f32>(self.window.scale_factor());
        let environment = UiEnvironment::for_surface(
            self.surface,
            UiSurfaceContentTransform::direct(
                UiSize::new(size.width, size.height).unwrap(),
                self.window.scale_factor() as f32,
            )?,
            UiSurfaceGenerations::new(1, revision, revision, revision)?,
            1,
            1,
        )?;
        let frame = self
            .compiler
            .compile(&snapshot.unwrap()?, environment, UiTheme::default())?;
        assert!(frame.diagnostics().is_empty());
        let expected = if self.stage == 0 || self.stage == 5 {
            INITIAL
        } else {
            "native draft"
        };
        assert!(
            frame
                .semantics()
                .iter()
                .any(|node| node.value() == Some(expected))
        );
        Ok(frame)
    }

    fn reduce(&mut self, batch: &UiDispatchBatch) -> Result<()> {
        let mut result = Ok(());
        let mut id = None;
        let document = self.document;
        let submission = self
            .panels
            .prepare(&self.application, |panel, context| {
                result = (|| -> Result<()> {
                    if panel.reduce(batch)? {
                        let basis = context.document_query_basis(document)?;
                        let prepared = panel.prepare_edit(context, &basis, entity("caption"))?;
                        id = Some(
                            context
                                .commit_authoring_edit(prepared)
                                .map_err(|(error, _)| error)?,
                        );
                    }
                    Ok(())
                })();
            })
            .unwrap();
        result?;
        if let Some(id) = id {
            assert!(self.action.is_none());
            self.action = Some(submission);
            self.action_id = Some(id);
        }
        Ok(())
    }

    fn route_input(&mut self, event: UiInputEvent) -> Result<()> {
        let outcome =
            self.interaction
                .dispatch(self.presenter.journal(), self.route.unwrap(), event)?;
        if let Some(batch) = outcome.consumed() {
            self.reduce(batch)?;
        }
        Ok(())
    }

    fn edit(&mut self, edit: TextEdit) -> Result<()> {
        let snapshot = self.interaction.semantic_snapshot(
            self.presenter.journal(),
            self.route.unwrap(),
            UiSeatId::PRIMARY,
        )?;
        let mut result = Ok(());
        drop(
            self.panels
                .prepare(&self.application, |panel, _| {
                    assert!(
                        snapshot
                            .nodes()
                            .iter()
                            .any(|node| node.key() == panel.field_key()
                                && node.state().focused()
                                && !node.state().inert())
                    );
                    let token = panel.input_token().unwrap();
                    self.old_token = Some(token);
                    result = panel.edit(token, &edit);
                })
                .unwrap(),
        );
        result
    }

    fn window_event(&mut self, event: WindowEvent) -> Result<()> {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let point = position.to_logical::<f32>(self.window.scale_factor());
                self.cursor = UiPoint::new(point.x, point.y);
                self.route_input(UiInputEvent::Pointer(UiPointerInput::moved(
                    UiSeatId::PRIMARY,
                    UiPointerId::PRIMARY,
                    self.cursor,
                )))?;
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.route_input(UiInputEvent::Pointer(UiPointerInput::button(
                    UiSeatId::PRIMARY,
                    UiPointerId::PRIMARY,
                    UiPointerButton::Primary,
                    if state == ElementState::Pressed {
                        UiButtonPhase::Pressed
                    } else {
                        UiButtonPhase::Released
                    },
                    Some(self.cursor),
                )))?;
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

    fn click_control(&mut self, name: &str) -> Result<()> {
        let presented = self.presenter.journal().latest(self.surface)?;
        let mut nodes = presented
            .frame()
            .semantics()
            .iter()
            .filter(|node| node.name() == name);
        let node = nodes
            .next()
            .ok_or_else(|| io::Error::other(format!("missing presented control: {name}")))?;
        assert!(
            nodes.next().is_none(),
            "the consumer uses exact unique control names"
        );
        let rect = node
            .clip_rect()
            .map_or(Some(node.rect()), |clip| node.rect().intersect(clip))
            .ok_or_else(|| {
                io::Error::other(format!("presented control is fully clipped: {name}"))
            })?;
        self.click(
            f64::from((rect.origin().x() + rect.max().x()) * 0.5),
            f64::from((rect.origin().y() + rect.max().y()) * 0.5),
        )
    }

    fn retire_panel(&mut self) -> Result<()> {
        self.route_input(UiInputEvent::SeatCancelled {
            seat: UiSeatId::PRIMARY,
        })?;
        let basis = self.application.document_query_basis(self.document)?;
        let before = current(&self.application);
        let bytes = fs::read(self.project.root().join(SOURCE))?;
        let candidate = self
            .panels
            .prepare(&self.application, |_, context| {
                let prepared = context
                    .prepare_authoring_property_edit(
                        &basis,
                        &[AuthoringPropertyEdit {
                            entity: entity("caption"),
                            component: ComponentTypeId::new(COMPONENT),
                            component_version: ComponentSchemaVersion::ONE,
                            property: ComponentPropertyPath::new(ComponentFieldId::new(FIELD)),
                            value: ComponentValue::String("retired action must not publish".into()),
                        }],
                    )
                    .unwrap();
                context.commit_authoring_edit(prepared).unwrap();
            })
            .unwrap();
        let callbacks = self.evidence.finished.borrow().len();
        let layout = self.application.panel_workspace_mut().layout_mut();
        assert!(layout.set_visible(self.instance, false));
        assert!(layout.reopen(self.instance));
        self.panels
            .submit_and_drive(&mut self.application, candidate, Duration::ZERO);
        assert_eq!(self.evidence.finished.borrow().len(), callbacks);
        assert_eq!(*self.evidence.dropped.borrow(), [1]);
        assert_eq!(self.application.document_query_basis(self.document)?, basis);
        assert_eq!(current(&self.application).workspace(), before.workspace());
        assert_eq!(
            self.application
                .read_document_property(property(&basis))?
                .value,
            EditorPropertyValue::Present(ComponentValue::String("native draft".into()))
        );
        assert_eq!(fs::read(self.project.root().join(SOURCE))?, bytes);
        Ok(())
    }

    fn draw(&mut self, retire_before_composite: bool) -> Result<bool> {
        let previous = self
            .presenter
            .journal()
            .latest(self.surface)
            .ok()
            .map(|pair| pair.receipt());
        if self.pending.is_none() {
            let frame = self.compile()?;
            let gpu = self.gpu.as_mut().unwrap();
            gpu.frame += 1;
            gpu.host
                .begin_window_frame(window_packet(&self.window), gpu.frame)?;
            let backend = self
                .presenter
                .prepare(frame, &UiPreparedImages::default())?;
            let (packets, sidecar) = WgpuUiSubmission::capture(backend);
            assert_eq!(packets.len(), 1);
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
            let capture = gpu.policy.capture(
                build_image_render_frame_packet(gpu.source, gpu.frame, &views, gpu.target)?
                    .unwrap(),
                Arc::new(WgpuStockCapture::new().with_ui(packets, None, None)),
            )?;
            self.pending = Some(
                match gpu
                    .host
                    .submit_for_display(&gpu.producer, &capture, sidecar)
                {
                    Ok(submitted) => submitted,
                    Err(failure) => {
                        let (error, outcomes) = failure.complete();
                        for outcome in outcomes {
                            self.presenter.return_outcome(outcome).unwrap();
                        }
                        self.presenter.drain()?;
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
                previous
            );
        } else {
            let gpu = self.gpu.as_mut().unwrap();
            gpu.frame += 1;
            gpu.host
                .begin_window_frame(window_packet(&self.window), gpu.frame)?;
        }
        if retire_before_composite && !self.retired {
            self.retire_panel()?;
            let old = self.route.take().unwrap();
            assert!(
                self.presenter
                    .retire(self.gpu.as_ref().unwrap().frame + 1)?
                    .is_some()
            );
            assert!(self.presenter.journal().latest(self.surface).is_err());
            assert!(
                self.interaction
                    .dispatch(
                        self.presenter.journal(),
                        old,
                        UiInputEvent::SeatCancelled {
                            seat: UiSeatId::PRIMARY
                        }
                    )
                    .is_err()
            );
            self.interaction.clear_surface_state(self.surface);
            assert!(!self.interaction.has_pointer_facts(self.surface));
            self.retired = true;
        }
        let gpu = self.gpu.as_mut().unwrap();
        let pending = self.pending.as_ref().unwrap();
        let display = pending.display().clone();
        let group = gpu.host.with_image(pending.output(), |device, _, view| {
            gpu.painter.bind(device, view, view)
        })?;
        let extent = gpu.target.extent();
        let outcome = gpu.host.present_window(
            window_packet(&self.window),
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
        let Some(window) = outcome.window() else {
            assert!(outcome.composites().is_empty());
            return Ok(false);
        };
        assert_eq!(outcome.composites().len(), 1);
        let composite = outcome.composites()[0];
        assert_eq!(composite.window(), window);
        assert_eq!(composite.image(), display.receipt());
        assert_eq!(composite.occurrence(), display.occurrence());
        self.presentations += 1;
        for outcome in self.pending.take().unwrap().presented(composite).unwrap() {
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
                    "original presentation was not accepted: {events:?}"
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

    fn queue(&mut self, action: EditorAction) -> PanelActionDispatch {
        let mut action = Some(action);
        let mut id = None;
        let candidate = self
            .panels
            .prepare(&self.application, |_, context| {
                id = Some(context.submit(action.take().unwrap()).unwrap());
            })
            .unwrap();
        self.panels
            .submit_and_drive(&mut self.application, candidate, Duration::ZERO);
        let id = id.unwrap();
        let finished = self.evidence.finished.borrow();
        let (_, actual, result) = finished.last().unwrap();
        assert_eq!(*actual, id);
        result.clone()
    }

    fn history_and_persistence(&mut self) -> Result<()> {
        for (undo, expected) in [(true, INITIAL), (false, "native draft")] {
            let basis = self.application.document_query_basis(self.document)?;
            let mutation = if undo {
                EditorDocumentMutation::Undo {
                    document: self.document,
                    base_revision: basis.revision,
                }
            } else {
                EditorDocumentMutation::Redo {
                    document: self.document,
                    base_revision: basis.revision,
                }
            };
            let result = self.queue(EditorAction::Mutate(mutation));
            assert!(
                matches!(result, PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)) if report.applied)
            );
            let basis = self.application.document_query_basis(self.document)?;
            assert_eq!(
                self.application
                    .read_document_property(property(&basis))?
                    .value,
                EditorPropertyValue::Present(ComponentValue::String(expected.into()))
            );
        }
        let basis = self.application.document_query_basis(self.document)?;
        let saved = self.queue(EditorAction::Persistence(EditorPersistenceAction::Save {
            document: self.document,
            base_revision: basis.revision,
        }));
        let PanelActionDispatch::Dispatched(EditorActionOutcome::Persistence(
            EditorPersistenceRequestResult::Accepted {
                attempt: save_attempt,
            },
        )) = saved
        else {
            return Err(io::Error::other(format!("Save was not accepted: {saved:?}")).into());
        };
        assert!(
            matches!(persistence(&mut self.application), EditorPersistenceResult::Saved { document, .. } if document == self.document)
        );
        assert_eq!(
            current(&self.application).persistence().attempt(),
            Some(save_attempt)
        );
        let bytes = fs::read(self.project.root().join(SOURCE))?;
        let mut expected = project::source();
        expected
            .entities
            .iter_mut()
            .find(|record| record.id == entity("caption"))
            .unwrap()
            .components
            .get_mut(&ComponentTypeId::new(COMPONENT))
            .unwrap()
            .value = ComponentValue::map([(FIELD, ComponentValue::String("native draft".into()))]);
        assert_eq!(
            SceneDocumentCandidate::decode_json_bytes(&bytes)?.into_composition_source()?,
            expected
        );
        let attempt = current(&self.application).persistence().attempt().unwrap();
        assert_eq!(
            self.queue(EditorAction::Persistence(
                EditorPersistenceAction::AcknowledgeResult { attempt },
            )),
            PanelActionDispatch::Dispatched(EditorActionOutcome::Persistence(
                EditorPersistenceRequestResult::Acknowledged {
                    attempt: save_attempt
                }
            ))
        );
        assert_eq!(
            current(&self.application).persistence(),
            EditorPersistenceView::Idle
        );
        let reopened = self.queue(EditorAction::Persistence(EditorPersistenceAction::Reopen {
            target: EditorReopenTarget::OpenDocument {
                document: self.document,
                base_revision: basis.revision,
            },
        }));
        let PanelActionDispatch::Dispatched(EditorActionOutcome::Persistence(
            EditorPersistenceRequestResult::Accepted {
                attempt: reopen_attempt,
            },
        )) = reopened
        else {
            return Err(io::Error::other(format!("Reopen was not accepted: {reopened:?}")).into());
        };
        let EditorPersistenceResult::Opened { document, .. } = persistence(&mut self.application)
        else {
            return Err(io::Error::other("native public Reopen failed").into());
        };
        self.document = document;
        assert_eq!(
            current(&self.application).persistence().attempt(),
            Some(reopen_attempt)
        );
        assert_ne!(save_attempt, reopen_attempt);
        assert!(
            self.application
                .read_document_property(property(&basis))
                .is_err()
        );
        let current_basis = self.application.document_query_basis(document)?;
        assert_eq!(
            self.application
                .read_document_property(property(&current_basis))?
                .value,
            EditorPropertyValue::Present(ComponentValue::String("native draft".into()))
        );
        assert_eq!(
            current(&self.application).workspace().active_document,
            Some(self.active)
        );
        assert_eq!(
            current(&self.application)
                .workspace()
                .document(document)
                .unwrap()
                .history
                .undo_depth,
            0
        );
        assert_eq!(fs::read(self.project.root().join(SOURCE))?, bytes);
        let attempt = current(&self.application).persistence().attempt().unwrap();
        assert_eq!(
            self.queue(EditorAction::Persistence(
                EditorPersistenceAction::AcknowledgeResult { attempt },
            )),
            PanelActionDispatch::Dispatched(EditorActionOutcome::Persistence(
                EditorPersistenceRequestResult::Acknowledged {
                    attempt: reopen_attempt
                }
            ))
        );
        assert_eq!(
            current(&self.application).persistence(),
            EditorPersistenceView::Idle
        );
        assert_eq!(self.evidence.finished.borrow().len(), 7);
        Ok(())
    }

    fn step(&mut self) -> Result<bool> {
        if self.started.elapsed() > Duration::from_secs(60) {
            return Err(io::Error::other("native panel trajectory timed out").into());
        }
        if !self.draw(self.stage == 4)? {
            return Ok(false);
        }
        match self.stage {
            0 => {
                self.click_control("Caption")?;
                self.edit(TextEdit::SelectAll)?;
                self.window_event(WindowEvent::Ime(Ime::Preedit(
                    "candidate".into(),
                    Some((0, 9)),
                )))?;
                self.window_event(WindowEvent::Ime(Ime::Commit("native draft".into())))?;
                assert!(self.evidence.finished.borrow().is_empty());
            }
            1 => {
                self.click_control("Review change")?;
                drop(
                    self.panels
                        .prepare(&self.application, |panel, _| {
                            assert_eq!(panel.reviews(), 1);
                            assert!(panel.popup_open());
                        })
                        .unwrap(),
                );
            }
            2 => {
                assert!(
                    self.presenter
                        .journal()
                        .latest(self.surface)?
                        .frame()
                        .blocks_unhandled_input()
                );
                self.click_control("Review change")?;
                assert!(self.action.is_none());
                drop(
                    self.panels
                        .prepare(&self.application, |panel, _| assert_eq!(panel.reviews(), 1))
                        .unwrap(),
                );
                self.click_control("Apply caption")?;
                let action = self
                    .action
                    .take()
                    .ok_or_else(|| io::Error::other("presented modal did not emit Apply"))?;
                let id = self.action_id.take().unwrap();
                assert!(self.evidence.finished.borrow().is_empty());
                self.panels
                    .submit_and_drive(&mut self.application, action, Duration::ZERO);
                let evidence = self.evidence.finished.borrow();
                assert_eq!(evidence.len(), 1);
                assert_eq!(evidence[0].1, id);
                assert!(
                    matches!(&evidence[0].2, PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)) if report.applied)
                );
                assert_eq!(
                    current(&self.application)
                        .workspace()
                        .document(self.document)
                        .unwrap()
                        .history
                        .undo_depth,
                    1
                );
                assert_eq!(
                    current(&self.application).workspace().active_document,
                    Some(self.active)
                );
                assert_eq!(
                    fs::read(self.project.root().join(SOURCE))?,
                    project::source().to_json_string()?.into_bytes()
                );
            }
            3 => {
                self.history_and_persistence()?;
                self.click_control("Review change")?;
            }
            4 => {
                assert!(self.retired);
                self.surface = UiSurfaceId::new(902).unwrap();
                self.presenter = UiSurfacePresenter::with_fonts(
                    self.surface,
                    1,
                    RenderTarget::Image(self.gpu.as_ref().unwrap().target.image()),
                    vec![bundled_font_face()?],
                    Default::default(),
                    Default::default(),
                )?;
                self.interaction = UiInteractionSession::default();
            }
            5 => {
                let old = self.old_token.unwrap();
                self.click_control("Caption")?;
                drop(
                    self.panels
                        .prepare(&self.application, |panel, _| {
                            assert_eq!(panel.serial(), 2);
                            assert_eq!(panel.draft(), INITIAL);
                            assert!(panel.input_token().is_some());
                            assert_ne!(panel.input_token(), Some(old));
                            assert!(panel.edit(old, &TextEdit::Commit("stale".into())).is_err());
                            assert_eq!(panel.draft(), INITIAL);
                        })
                        .unwrap(),
                );
                self.window_event(WindowEvent::Ime(Ime::Preedit(
                    "pending at close".into(),
                    Some((0, 16)),
                )))?;
                drop(
                    self.panels
                        .prepare(&self.application, |panel, _| {
                            assert!(panel.display_text().contains("pending at close"));
                            assert_eq!(panel.draft(), INITIAL);
                        })
                        .unwrap(),
                );
                assert_eq!(self.evidence.created.get(), 2);
                assert_eq!(self.evidence.finished.borrow().len(), 7);
                assert_eq!((self.presentations, self.commits), (6, 5));
                return Ok(true);
            }
            _ => unreachable!(),
        }
        self.stage += 1;
        Ok(false)
    }

    fn close(&mut self) -> Result<()> {
        if self.route.is_some() && self.presenter.journal().latest(self.surface).is_ok() {
            self.route_input(UiInputEvent::SeatCancelled {
                seat: UiSeatId::PRIMARY,
            })?;
            let route = self.route.unwrap();
            let state = self.interaction.semantic_snapshot(
                self.presenter.journal(),
                route,
                UiSeatId::PRIMARY,
            )?;
            assert!(state.nodes().iter().all(|node| !node.state().focused()));
            drop(
                self.panels
                    .prepare(&self.application, |panel, _| {
                        assert!(panel.input_token().is_none());
                        assert_eq!(panel.display_text(), panel.draft());
                    })
                    .unwrap(),
            );
        }
        self.interaction.clear_surface_state(self.surface);
        assert!(!self.interaction.has_pointer_facts(self.surface));
        self.route = None;
        assert!(
            self.application
                .panel_workspace_mut()
                .layout_mut()
                .set_visible(self.instance, false)
        );
        drop(self.panels.prepare(&self.application, |_, _| {
            panic!("hidden panel must not be visited")
        }));
        assert_eq!(
            self.evidence.dropped.borrow().len(),
            self.evidence.created.get()
        );
        if let Some(pending) = self.pending.take() {
            for outcome in pending.deferred("native panel consumer is closing".into()) {
                self.presenter.return_outcome(outcome).unwrap();
            }
            self.presenter.drain()?;
        }
        self.presenter.retire(1000)?;
        self.presenter.drain()?;
        assert!(self.presenter.is_quiescent());
        if let Some(gpu) = self.gpu.take() {
            gpu.close()?;
        }
        assert!(self.application.close());
        Ok(())
    }
}

#[derive(Default)]
struct NativeApplication {
    probe: Option<Probe>,
    result: Option<std::result::Result<(), String>>,
}

impl NativeApplication {
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

impl ApplicationHandler for NativeApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.probe.is_some() || self.result.is_some() {
            return;
        }
        let initialized = (|| -> Result<Probe> {
            let window = Arc::new(
                event_loop.create_window(
                    Window::default_attributes()
                        .with_title("Independent Editor panel Host")
                        .with_inner_size(LogicalSize::new(400.0, 180.0))
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
        if self
            .probe
            .as_ref()
            .is_some_and(|probe| probe.window.id() == id)
            && matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed)
        {
            self.finish(
                event_loop,
                Err("native window closed before qualification completed".into()),
            );
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
                .unwrap_or_else(|| "native panel qualification panicked".into());
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
    let mut application = NativeApplication::default();
    let result = event_loop.run_app(&mut application);
    if let Some(mut probe) = application.probe.take() {
        probe.close()?;
    }
    result?;
    application
        .result
        .ok_or_else(|| io::Error::other("native panel event loop has no result"))?
        .map_err(io::Error::other)?;
    println!(
        "EDITOR_UI_HOST_OK native_presentations=6 journal_commits=5 exact_panel_results=7 nonactive_authoring=true undo_redo=true save_reopen=true source_preserved=true modal_underlay_blocked=true hidden_roundtrip_retired=true late_presentation_discarded=true stale_input_rejected=true original_hosts_retired=true input_delivery=injected_pointer_and_ime"
    );
    Ok(())
}
