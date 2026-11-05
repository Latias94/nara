//! Native public image-panel consumption with injected events, not OS input automation.
#![forbid(unsafe_code)]

#[path = "panel_surface/observation.rs"]
mod observation;
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
use nara_editor::{
    EditorPanelContext, EditorPanelHost, EditorPanelInstanceToken, EditorPanelSubmission,
    PanelActionDispatch, PanelSubmissionId,
    native::{
        EditorClipboard, EditorClipboardAccess, EditorPanelCancelReason, EditorPanelCapability,
        EditorPanelImage, EditorPanelImageError, EditorPanelImageHost, EditorPanelImageObservation,
        EditorPanelImageStatus, EditorPanelImeArea, EditorPanelInput, EditorPanelInputKind,
        EditorPanelPlatformEvent, EditorPanelServiceError, EditorPanelServices,
        EditorPanelTextInputScope, EditorPanelWheelDelta,
    },
};
use nara_editor_ui_host_consumer::{
    COMPONENT, DraftPanel, Evidence, FIELD, INITIAL, Result,
    project::{
        self, Application, Project, SOURCE, current, entity, open_secondary, persistence, property,
        register,
    },
};
use nara_render::{
    DisplayMapping2d, DisplayRect, DisplayScale, Extent2d, ExtractedView, ExtractedViews,
    RenderFrameSourceGeneration, RenderImage2d, RenderImagePacket, RenderTarget, RenderViewId,
    RenderWindowPacket, ViewLoadOp, ViewMapping2d, ViewportRect, build_image_render_frame_packet,
};
use nara_render_wgpu::{
    WgpuPolicyDefinition, WgpuPolicyInstallation, WgpuRenderHost, WgpuRenderProducer,
    WgpuStockCapture, WgpuStockPolicy, WgpuUiImageSubmission, WgpuUiSubmission,
};
use nara_text::{TextEdit, bundled_font_face};
use nara_ui_kernel::{
    UiAccessibilityInput, UiActivationSource, UiButtonPhase, UiCompiler, UiDispatchBatch,
    UiEnvironment, UiFrame, UiInputEvent, UiInteractionSession, UiModifiers, UiNavigation,
    UiPaintPrimitive, UiPoint, UiPointerButton, UiPointerId, UiPointerInput, UiResolvedBrush,
    UiRouteStamp, UiScrollDelta, UiSeatId, UiSize, UiSurfaceContentTransform, UiSurfaceGenerations,
    UiSurfaceId, UiTheme,
};
use nara_ui_render::{UiPreparedImages, UiSurfacePresentationEvent, UiSurfacePresenter};
use nara_ui_widgets::UiTextInputToken;
use nara_window::{PresentMode, WindowId, WindowResolution, backend::WindowHandleProvider};
use window_gpu::ImagePainter;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition},
    event::{DeviceId, ElementState, Ime, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{CursorIcon, Window, WindowId as NativeWindowId},
};

struct MemoryClipboard(String);

impl EditorClipboardAccess for MemoryClipboard {
    fn read(&mut self) -> std::result::Result<String, String> {
        Ok(self.0.clone())
    }

    fn write(&mut self, text: &str) -> std::result::Result<(), String> {
        self.0 = text.to_owned();
        Ok(())
    }
}

#[derive(Default)]
struct NativeServiceEvidence {
    keys: usize,
    clipboard_reads: usize,
    clipboard_writes: usize,
    ime_enabled: usize,
    ime_areas: usize,
    text_scopes: usize,
    cursors: usize,
    redraws: usize,
    wheels: usize,
    cancellations: Vec<(usize, EditorPanelCancelReason)>,
}

fn reduce_panel(
    panel: &mut DraftPanel,
    context: &mut EditorPanelContext<'_, '_>,
    document: EditorDocumentId,
    batch: &UiDispatchBatch,
) -> Result<Option<PanelSubmissionId>> {
    if !panel.reduce(batch)? {
        return Ok(None);
    }
    let basis = context.document_query_basis(document)?;
    let prepared = panel.prepare_edit(context, &basis, entity("caption"))?;
    Ok(Some(
        context
            .commit_authoring_edit(prepared)
            .map_err(|(error, _)| error)?,
    ))
}

fn field_frame(
    panel: &DraftPanel,
    compiler: &mut UiCompiler,
    accepted: &UiFrame,
) -> Result<Arc<UiFrame>> {
    let frame = compiler.compile(
        &panel.compose(accepted.source_revision().get())?,
        accepted.environment(),
        UiTheme::default(),
    )?;
    assert!(frame.diagnostics().is_empty());
    Ok(frame)
}

fn request_ime_area(
    panel: &DraftPanel,
    frame: &UiFrame,
    services: &mut EditorPanelServices<'_>,
) -> Result<()> {
    let caret = frame
        .layouts()
        .iter()
        .rev()
        .find(|layout| {
            layout.key().is_none()
                && layout.route_parent() == Some(panel.field_key())
                && frame.paint().iter().any(|paint| {
                    matches!(paint, UiPaintPrimitive::Rect { key: None, rect, brush: Some(UiResolvedBrush::Solid(color)), .. }
                        if *rect == layout.rect() && *color == UiTheme::default().palette().focus)
                })
        })
        .ok_or_else(|| io::Error::other("focused field has no composed caret"))?;
    let visible = caret
        .clip_rect()
        .map_or(Some(caret.rect()), |clip| caret.rect().intersect(clip))
        .ok_or_else(|| io::Error::other("composed caret is fully clipped"))?;
    let physical = frame
        .environment()
        .content_transform()
        .authored_rect_to_physical(visible, frame.environment().snap_policy())
        .ok_or_else(|| io::Error::other("composed caret has no target geometry"))?;
    services.set_ime_area(EditorPanelImeArea {
        position: [physical.min_x() as f32, physical.min_y() as f32],
        size: [physical.width() as f32, physical.height() as f32],
    })?;
    Ok(())
}

struct NaraInputAdapter<'a> {
    presenter: &'a UiSurfacePresenter,
    interaction: &'a mut UiInteractionSession,
    compiler: &'a mut UiCompiler,
    route: Option<UiRouteStamp>,
    document: EditorDocumentId,
    text_scope: &'a mut Option<(UiTextInputToken, EditorPanelTextInputScope)>,
    old_token: &'a mut Option<UiTextInputToken>,
    evidence: &'a mut NativeServiceEvidence,
    operation: Option<&'a UiAccessibilityInput>,
}

impl NaraInputAdapter<'_> {
    fn input(
        &mut self,
        panel: &mut DraftPanel,
        context: &mut EditorPanelContext<'_, '_>,
        input: &EditorPanelInput<UiFrame>,
        services: &mut EditorPanelServices<'_>,
    ) -> Result<Option<PanelSubmissionId>> {
        assert!(context.matches_instance(input.endpoint.token()));
        if let EditorPanelInputKind::Cancelled(reason) = &input.kind {
            assert!(matches!(
                services.ime_ready(),
                Err(EditorPanelServiceError::InactiveEndpoint
                    | EditorPanelServiceError::CapabilityNotDeclared(
                        EditorPanelCapability::TextInput
                    ))
            ));
            if let Some(route) = self.route {
                let outcome = self.interaction.dispatch(
                    self.presenter.journal(),
                    route,
                    UiInputEvent::SeatCancelled {
                        seat: UiSeatId::PRIMARY,
                    },
                )?;
                if let Some(batch) = outcome.consumed() {
                    assert!(reduce_panel(panel, context, self.document, batch)?.is_none());
                }
            }
            assert!(panel.input_token().is_none());
            assert_eq!(panel.display_text(), panel.draft());
            *self.text_scope = None;
            self.evidence.cancellations.push((panel.serial(), *reason));
            return Ok(None);
        }

        let route = self
            .route
            .ok_or_else(|| io::Error::other("input has no original UI route"))?;
        let accepted = self
            .presenter
            .journal()
            .latest(input.snapshot.environment().surface_id())?;
        assert!(Arc::ptr_eq(accepted.frame(), &input.snapshot));
        assert_eq!(UiRouteStamp::from_receipt(accepted.receipt()), route);
        if matches!(
            &input.kind,
            EditorPanelInputKind::Text(_)
                | EditorPanelInputKind::ImeEnabled
                | EditorPanelInputKind::ImeDisabled
                | EditorPanelInputKind::ImePreedit { .. }
                | EditorPanelInputKind::ImeCommit(_)
        ) && input.text_scope.as_ref() != self.text_scope.as_ref().map(|(_, scope)| scope)
        {
            return Ok(None);
        }
        let transform = input.snapshot.environment().content_transform();
        let point = input.target_position.map(|[x, y]| {
            transform
                .physical_to_authored(UiPoint::new(x, y))
                .unwrap_or_else(|| {
                    let origin = transform.physical_content_origin();
                    UiPoint::new(
                        (x - origin.x()) / transform.content_scale_factor(),
                        (y - origin.y()) / transform.content_scale_factor(),
                    )
                })
        });
        let modifiers = UiModifiers::new(
            input.modifiers.shift_key(),
            input.modifiers.control_key(),
            input.modifiers.alt_key(),
            input.modifiers.super_key(),
        );
        let pointer = match &input.kind {
            EditorPanelInputKind::PointerMoved => Some(UiPointerInput::moved(
                UiSeatId::PRIMARY,
                UiPointerId::PRIMARY,
                point.ok_or_else(|| io::Error::other("pointer has no authored point"))?,
            )),
            EditorPanelInputKind::PointerLeft => Some(UiPointerInput::cancelled(
                UiSeatId::PRIMARY,
                UiPointerId::PRIMARY,
            )),
            EditorPanelInputKind::PointerButton {
                button: MouseButton::Left,
                state,
            } => {
                if *state == ElementState::Pressed {
                    services.request_pointer_capture(MouseButton::Left)?;
                }
                Some(UiPointerInput::button(
                    UiSeatId::PRIMARY,
                    UiPointerId::PRIMARY,
                    UiPointerButton::Primary,
                    if *state == ElementState::Pressed {
                        UiButtonPhase::Pressed
                    } else {
                        UiButtonPhase::Released
                    },
                    point,
                ))
            }
            EditorPanelInputKind::Wheel { delta, .. } => {
                self.evidence.wheels += 1;
                let delta = match delta {
                    EditorPanelWheelDelta::Lines([x, y]) => {
                        UiScrollDelta::Lines(UiPoint::new(*x, *y))
                    }
                    EditorPanelWheelDelta::Pixels([x, y]) => UiScrollDelta::Pixels(UiPoint::new(
                        *x as f32 / transform.content_scale_factor(),
                        *y as f32 / transform.content_scale_factor(),
                    )),
                };
                Some(UiPointerInput::scroll(
                    UiSeatId::PRIMARY,
                    UiPointerId::PRIMARY,
                    point.ok_or_else(|| io::Error::other("wheel has no authored point"))?,
                    delta,
                ))
            }
            _ => None,
        };
        let event = pointer
            .map(|pointer| UiInputEvent::Pointer(pointer.with_modifiers(modifiers)))
            .or_else(|| match &input.kind {
                EditorPanelInputKind::Key(key) if key.state == ElementState::Pressed => {
                    match &key.logical_key {
                        Key::Named(NamedKey::Tab) => Some(UiInputEvent::Navigate {
                            seat: UiSeatId::PRIMARY,
                            direction: if modifiers.shift() {
                                UiNavigation::Previous
                            } else {
                                UiNavigation::Next
                            },
                        }),
                        Key::Named(NamedKey::Enter) => Some(UiInputEvent::ActivateFocused {
                            seat: UiSeatId::PRIMARY,
                            source: UiActivationSource::Keyboard,
                        }),
                        _ => None,
                    }
                }
                _ => None,
            });
        let mut action = None;
        let outcome = if matches!(input.kind, EditorPanelInputKind::ObservedOperation) {
            let request = self
                .operation
                .ok_or_else(|| io::Error::other("observed operation has no typed UI request"))?;
            assert_eq!(request.route(), route);
            let outcome = self
                .interaction
                .dispatch_accessibility(self.presenter.journal(), request.clone())?;
            if let Some(reason) = outcome.rejection() {
                return Err(observation::ObservedUiOperationRefused(reason.clone()).into());
            }
            Some(outcome)
        } else {
            event
                .map(|event| {
                    self.interaction
                        .dispatch(self.presenter.journal(), route, event)
                })
                .transpose()?
        };
        if let Some(outcome) = outcome {
            if let Some(batch) = outcome.consumed() {
                action = reduce_panel(panel, context, self.document, batch)?;
            }
        }

        if matches!(
            input.kind,
            EditorPanelInputKind::PointerMoved | EditorPanelInputKind::PointerButton { .. }
        ) {
            let field = input
                .snapshot
                .layouts()
                .iter()
                .find(|layout| layout.key() == Some(panel.field_key()))
                .unwrap();
            let over_field = point.is_some_and(|point| field.rect().contains(point));
            services.set_cursor(if over_field {
                CursorIcon::Text
            } else {
                CursorIcon::Default
            })?;
            self.evidence.cursors += 1;
        }

        let token = panel.input_token();
        if let Some(token) = token {
            let focused = self.interaction.semantic_snapshot(
                self.presenter.journal(),
                route,
                UiSeatId::PRIMARY,
            )?;
            assert!(
                focused
                    .nodes()
                    .iter()
                    .any(|node| node.key() == panel.field_key()
                        && node.state().focused()
                        && !node.state().inert())
            );
            let new_scope = self
                .text_scope
                .as_ref()
                .is_none_or(|(current, _)| *current != token);
            if new_scope {
                *self.text_scope = Some((token, EditorPanelTextInputScope::new()));
                self.evidence.text_scopes += 1;
            }
            self.old_token.get_or_insert(token);
            services.set_text_input(Some(self.text_scope.as_ref().unwrap().1.clone()))?;
            if new_scope {
                assert!(!services.ime_ready()?);
            }

            let mut edit = None;
            match &input.kind {
                EditorPanelInputKind::PointerButton {
                    button: MouseButton::Left,
                    state: ElementState::Pressed,
                } => {
                    if panel.display_text() != panel.draft()
                        || input
                            .snapshot
                            .semantics()
                            .iter()
                            .find(|node| node.key() == panel.field_key())
                            .and_then(|node| node.value())
                            != Some(panel.draft())
                    {
                        return Err(io::Error::other(
                            "field Point requires the actual presented text/layout",
                        )
                        .into());
                    }
                    let text = input
                        .snapshot
                        .layouts()
                        .iter()
                        .find(|layout| {
                            layout.key().is_some()
                                && layout.route_parent() == Some(panel.field_key())
                        })
                        .ok_or_else(|| io::Error::other("field has no composed text origin"))?;
                    let point = point.unwrap();
                    edit = Some(TextEdit::Point {
                        x: point.x() - text.rect().origin().x(),
                        y: point.y() - text.rect().origin().y(),
                        extend: modifiers.shift(),
                    });
                }
                EditorPanelInputKind::Key(key) if key.state == ElementState::Pressed => {
                    self.evidence.keys += 1;
                    let command = modifiers.control() || modifiers.meta();
                    edit = match &key.logical_key {
                        Key::Character(key) if command => match key.as_str() {
                            "a" => Some(TextEdit::SelectAll),
                            "c" => {
                                services.write_clipboard(panel.draft())?;
                                self.evidence.clipboard_writes += 1;
                                None
                            }
                            "v" => {
                                let text = services.read_clipboard()?;
                                self.evidence.clipboard_reads += 1;
                                Some(TextEdit::Insert(text))
                            }
                            "z" => Some(if modifiers.shift() {
                                TextEdit::Redo
                            } else {
                                TextEdit::Undo
                            }),
                            "y" => Some(TextEdit::Redo),
                            _ => None,
                        },
                        _ => None,
                    };
                }
                EditorPanelInputKind::Text(text) => edit = Some(TextEdit::Insert(text.clone())),
                EditorPanelInputKind::ImeCommit(text) => {
                    assert!(services.ime_ready()?);
                    edit = Some(TextEdit::Commit(text.clone()));
                }
                EditorPanelInputKind::ImePreedit { text, cursor } => {
                    assert!(services.ime_ready()?);
                    edit = Some(TextEdit::Preedit {
                        text: text.clone(),
                        cursor: *cursor,
                    });
                }
                EditorPanelInputKind::ImeEnabled => {
                    assert_eq!(
                        input.text_scope.as_ref(),
                        Some(&self.text_scope.as_ref().unwrap().1)
                    );
                    assert!(services.ime_ready()?);
                    self.evidence.ime_enabled += 1;
                }
                EditorPanelInputKind::ImeDisabled => edit = Some(TextEdit::CancelComposition),
                _ => {}
            }
            if let Some(edit) = edit {
                panel.edit(token, &edit)?;
            }
            let frame = field_frame(panel, self.compiler, &input.snapshot)?;
            request_ime_area(panel, &frame, services)?;
            self.evidence.ime_areas += 1;
        } else if self.text_scope.take().is_some() {
            services.set_text_input(None)?;
        }
        services.request_redraw()?;
        services.request_redraw_at(Instant::now())?;
        self.evidence.redraws += 1;
        Ok(action)
    }
}

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
    native: EditorPanelImageHost<UiFrame>,
    clipboard: EditorClipboard,
    native_evidence: NativeServiceEvidence,
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
    pending_frame: Option<Arc<UiFrame>>,
    native_token: Option<EditorPanelInstanceToken>,
    retired_native_token: Option<EditorPanelInstanceToken>,
    text_scope: Option<(UiTextInputToken, EditorPanelTextInputScope)>,
    action: Option<EditorPanelSubmission>,
    action_id: Option<PanelSubmissionId>,
    old_token: Option<UiTextInputToken>,
    retired: bool,
    stage: u8,
    presentations: usize,
    commits: usize,
    observed_operations: bool,
    retained_observation: Option<EditorPanelImageObservation<UiFrame>>,
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
        let clipboard = EditorClipboard::with_access(MemoryClipboard("consumer clipboard".into()));
        let native =
            EditorPanelImageHost::new(Arc::clone(&window), WindowId::PRIMARY, clipboard.clone());
        Ok(Self {
            window,
            gpu: Some(gpu),
            native,
            clipboard,
            native_evidence: NativeServiceEvidence::default(),
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
            pending_frame: None,
            native_token: None,
            retired_native_token: None,
            text_scope: None,
            action: None,
            action_id: None,
            old_token: None,
            retired: false,
            stage: 0,
            presentations: 0,
            commits: 0,
            observed_operations: std::env::args().any(|arg| arg == "--observed-operations"),
            retained_observation: None,
            started: Instant::now(),
        })
    }

    fn compile(&mut self) -> Result<Arc<UiFrame>> {
        let revision = self.presentations as u64 + 1;
        let mut snapshot = None;
        let mut token = None;
        drop(
            self.panels
                .prepare(&self.application, |panel, context| {
                    snapshot = Some(panel.compose(revision));
                    token = Some(context.instance_token());
                })
                .unwrap(),
        );
        let token = token.unwrap();
        self.native.attach(
            &self.application,
            token.clone(),
            [
                EditorPanelCapability::Pointer,
                EditorPanelCapability::Wheel,
                EditorPanelCapability::Keyboard,
                EditorPanelCapability::TextInput,
                EditorPanelCapability::Clipboard,
                EditorPanelCapability::Cursor,
                EditorPanelCapability::Refresh,
                EditorPanelCapability::WindowModal,
                EditorPanelCapability::ObservedOperations,
            ],
        )?;
        if let Some(retired) = &self.retired_native_token {
            assert_ne!(&token, retired);
        }
        self.native_token = Some(token);
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
                    id = reduce_panel(panel, context, document, batch)?;
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

    fn dispatch(&mut self, event: Option<EditorPanelPlatformEvent>) -> Result<bool> {
        self.dispatch_input(None, event, None)
    }

    fn dispatch_input(
        &mut self,
        observation: Option<&EditorPanelImageObservation<UiFrame>>,
        event: Option<EditorPanelPlatformEvent>,
        operation: Option<UiAccessibilityInput>,
    ) -> Result<bool> {
        let mut result = Ok(());
        let mut id = None;
        let mut adapter = NaraInputAdapter {
            presenter: &self.presenter,
            interaction: &mut self.interaction,
            compiler: &mut self.compiler,
            route: self.route,
            document: self.document,
            text_scope: &mut self.text_scope,
            old_token: &mut self.old_token,
            evidence: &mut self.native_evidence,
            operation: operation.as_ref(),
        };
        let callback = |panel: &mut DraftPanel,
                        context: &mut EditorPanelContext<'_, '_>,
                        input: &EditorPanelInput<UiFrame>,
                        services: &mut EditorPanelServices<'_>| {
            if result.is_ok() {
                result = adapter
                    .input(panel, context, input, services)
                    .map(|submitted| {
                        if let Some(submitted) = submitted {
                            assert!(id.replace(submitted).is_none());
                        }
                    });
            }
        };
        let outcome = match observation {
            Some(observation) if operation.is_some() => self.native.invoke_observed(
                &self.application,
                &mut self.panels,
                observation,
                callback,
            ),
            Some(observation) => self.native.dispatch_observed(
                &self.application,
                &mut self.panels,
                observation,
                event.unwrap(),
                callback,
            ),
            None => self
                .native
                .dispatch(&self.application, &mut self.panels, event, callback),
        }?;
        result?;
        if let Some(id) = id {
            assert!(self.action.is_none());
            let submission = outcome.submission.ok_or_else(|| {
                io::Error::other("native input lost its original semantic submission")
            })?;
            assert_eq!(submission.submissions().len(), 1);
            assert_eq!(
                submission.submissions().collect::<Vec<_>>(),
                [(self.instance, id)]
            );
            self.action = Some(submission);
            self.action_id = Some(id);
        } else {
            if let Some(submission) = &outcome.submission {
                assert_eq!(submission.submissions().len(), 0);
            }
            drop(outcome.submission);
        }
        Ok(outcome.consumed)
    }

    fn window_event(&mut self, event: WindowEvent) -> Result<()> {
        if let Some(event) = EditorPanelPlatformEvent::from_window_event(&event) {
            self.dispatch(Some(event))?;
        }
        Ok(())
    }

    fn key(&mut self, key: Key, text: Option<&str>) -> Result<()> {
        let event = EditorPanelPlatformEvent::logical_key(
            key,
            ElementState::Pressed,
            false,
            text.map(str::to_owned),
        );
        assert!(if self.observed_operations {
            self.observed_event(event)?
        } else {
            self.dispatch(Some(event))?
        });
        Ok(())
    }

    fn command(&mut self, key: &str) -> Result<()> {
        self.dispatch(Some(EditorPanelPlatformEvent::ModifiersChanged(
            ModifiersState::CONTROL,
        )))?;
        self.key(Key::Character(key.into()), None)?;
        self.dispatch(Some(EditorPanelPlatformEvent::ModifiersChanged(
            ModifiersState::empty(),
        )))?;
        Ok(())
    }

    fn assert_draft(&mut self, expected: &str) {
        drop(
            self.panels
                .prepare(&self.application, |panel, _| {
                    assert_eq!(panel.draft(), expected);
                    assert_eq!(panel.display_text(), expected);
                })
                .unwrap(),
        );
    }

    fn assert_stale_point_rejected(&mut self, draft: &str, display: &str) -> Result<()> {
        let presented = self.presenter.journal().latest(self.surface)?;
        let receipt = presented.receipt();
        let frame = Arc::clone(presented.frame());
        let counts = (self.presentations, self.commits);
        let scope = self.text_scope.clone().unwrap();
        let callbacks = self.evidence.finished.borrow().len();
        let mut snapshot = None;
        drop(
            self.panels
                .prepare(&self.application, |panel, _| {
                    assert_eq!(panel.draft(), draft);
                    assert_eq!(panel.display_text(), display);
                    assert_eq!(panel.input_token(), Some(scope.0));
                    let value = frame
                        .semantics()
                        .iter()
                        .find(|node| node.key() == panel.field_key())
                        .unwrap()
                        .value();
                    if draft == display {
                        assert_ne!(value, Some(draft));
                    } else {
                        assert_eq!(value, Some(draft));
                    }
                    snapshot = Some(panel.compose(frame.source_revision().get()).unwrap());
                })
                .unwrap(),
        );
        let snapshot = snapshot.unwrap();
        let pressed = self.click_control("Caption");
        let released = self.dispatch(Some(EditorPanelPlatformEvent::PointerButton {
            button: MouseButton::Left,
            state: ElementState::Released,
        }));
        let error = pressed.expect_err("the actual adapter must reject stale field Point");
        assert!(error.downcast_ref::<io::Error>().is_some_and(|error| {
            error.kind() == io::ErrorKind::Other
                && error.to_string() == "field Point requires the actual presented text/layout"
        }));
        assert!(released?);
        drop(
            self.panels
                .prepare(&self.application, |panel, _| {
                    assert_eq!(panel.draft(), draft);
                    assert_eq!(panel.display_text(), display);
                    assert_eq!(panel.input_token(), Some(scope.0));
                    assert_eq!(
                        panel.compose(frame.source_revision().get()).unwrap(),
                        snapshot
                    );
                })
                .unwrap(),
        );
        assert_eq!(self.text_scope.as_ref(), Some(&scope));
        let state = self.interaction.semantic_snapshot(
            self.presenter.journal(),
            self.route.unwrap(),
            UiSeatId::PRIMARY,
        )?;
        assert!(state.nodes().iter().all(|node| !node.state().pressed()));
        let presented = self.presenter.journal().latest(self.surface)?;
        assert_eq!(presented.receipt(), receipt);
        assert!(Arc::ptr_eq(presented.frame(), &frame));
        assert_eq!((self.presentations, self.commits), counts);
        assert!(self.action.is_none() && self.action_id.is_none());
        assert_eq!(self.evidence.finished.borrow().len(), callbacks);
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
        let target = presented
            .frame()
            .environment()
            .content_transform()
            .authored_to_physical(UiPoint::new(
                (rect.origin().x() + rect.max().x()) * 0.5,
                (rect.origin().y() + rect.max().y()) * 0.5,
            ))
            .ok_or_else(|| io::Error::other("presented control has no target point"))?;
        let mapping = self.display_mapping()?;
        let logical = mapping
            .uv_to_panel([
                target.x() / mapping.target().width as f32,
                target.y() / mapping.target().height as f32,
            ])
            .ok_or_else(|| io::Error::other("presented control has no native point"))?;
        self.click(f64::from(logical[0]), f64::from(logical[1]))
    }

    fn display_mapping(&self) -> Result<DisplayMapping2d> {
        let size = self
            .window
            .inner_size()
            .to_logical::<f32>(self.window.scale_factor());
        DisplayMapping2d::fit(
            self.gpu.as_ref().unwrap().target.extent(),
            DisplayRect {
                min: [0.0, 0.0],
                max: [size.width, size.height],
            },
            self.window.scale_factor() as f32,
            DisplayScale::Fit,
        )
        .ok_or_else(|| io::Error::other("native panel has no display mapping").into())
    }

    fn retire_panel(&mut self) -> Result<()> {
        let token = self.native_token.clone().unwrap();
        assert!(token.is_current(&self.application));
        assert!(
            self.native_evidence
                .cancellations
                .iter()
                .any(|(serial, _)| *serial == 1)
        );
        assert!(self.text_scope.is_none());
        self.retired_native_token = Some(token.clone());
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
        assert!(!token.is_current(&self.application));
        assert_eq!(self.evidence.created.get(), 1);
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
                .prepare(Arc::clone(&frame), &UiPreparedImages::default())?;
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
            self.pending_frame = Some(frame);
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
        let mapping = self.display_mapping()?;
        let display = self.pending.as_ref().unwrap().display().clone();
        let snapshot = Arc::clone(self.pending_frame.as_ref().unwrap());
        let modal = snapshot.blocks_unhandled_input();
        let candidate = if retire_before_composite && self.retired {
            None
        } else {
            let candidate = self.native.stage(
                &self.application,
                self.gpu.as_ref().unwrap().frame,
                vec![
                    EditorPanelImage::new(
                        self.native_token.clone().unwrap(),
                        display.clone(),
                        snapshot,
                        mapping,
                        mapping.panel_rect(),
                    )
                    .with_window_modal(modal),
                ],
            )?;
            assert_eq!(candidate.images().len(), 1);
            assert_eq!(candidate.images()[0].display(), &display);
            assert_eq!(candidate.images()[0].is_window_modal(), modal);
            Some(candidate)
        };
        if let Some(candidate) = &candidate {
            let observations = self.native.observe(&self.application)?;
            assert!(observations.iter().all(|observation| {
                !Arc::ptr_eq(observation.snapshot(), candidate.images()[0].snapshot())
            }));
        }
        self.dispatch(None)?;
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
        let group = gpu.host.with_image(pending.output(), |device, _, view| {
            gpu.painter.bind(device, view, view)
        })?;
        let placement = mapping.content_rect();
        let clip = mapping.panel_rect();
        let scale = self.window.scale_factor() as f32;
        let destination = [
            placement.min[0] * scale,
            placement.min[1] * scale,
            (placement.max[0] - placement.min[0]) * scale,
            (placement.max[1] - placement.min[1]) * scale,
        ];
        let clip_min = clip
            .min
            .map(|coordinate| (coordinate * scale).floor() as u32);
        let size = self.window.inner_size();
        let clip_max = [
            ((clip.max[0] * scale).ceil() as u32).min(size.width),
            ((clip.max[1] * scale).ceil() as u32).min(size.height),
        ];
        let outcome = gpu.host.present_window(
            window_packet(&self.window),
            gpu.frame,
            std::slice::from_ref(&display),
            |frame| {
                frame.encode(|_, _, encoder, view| {
                    gpu.painter.draw_mapped(
                        encoder,
                        view,
                        &group,
                        destination,
                        [
                            clip_min[0],
                            clip_min[1],
                            clip_max[0] - clip_min[0],
                            clip_max[1] - clip_min[1],
                        ],
                        wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    )
                });
                frame.record_image_sample(&display)
            },
            || self.window.pre_present_notify(),
        )?;
        let Some(window) = outcome.window() else {
            assert!(outcome.composites().is_empty());
            if let Some(candidate) = candidate {
                let status = self
                    .native
                    .presented(&self.application, candidate, &outcome);
                if retire_before_composite {
                    assert!(matches!(
                        status,
                        Err(EditorPanelImageError::RetiredInstance)
                    ));
                } else {
                    assert_eq!(status?, EditorPanelImageStatus::Deferred);
                }
            }
            self.dispatch(None)?;
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
        if let Some(candidate) = candidate {
            let status = self
                .native
                .presented(&self.application, candidate, &outcome);
            if retire_before_composite {
                assert!(matches!(
                    status,
                    Err(EditorPanelImageError::RetiredInstance)
                ));
            } else {
                assert_eq!(status?, EditorPanelImageStatus::Presented);
            }
        }
        if retire_before_composite {
            let callbacks = self.evidence.finished.borrow().len();
            assert!(!self.dispatch(Some(EditorPanelPlatformEvent::PointerMoved {
                physical_position: [20.0, 20.0],
            }))?);
            assert!(matches!(
                self.native.attach(
                    &self.application,
                    self.retired_native_token.clone().unwrap(),
                    [EditorPanelCapability::Pointer],
                ),
                Err(EditorPanelImageError::RetiredInstance)
            ));
            let delivered = self.native_evidence.keys;
            assert!(!self.dispatch(Some(EditorPanelPlatformEvent::logical_key(
                Key::Character("old".into()),
                ElementState::Pressed,
                false,
                Some("stale".into()),
            )))?);
            assert_eq!(self.native_evidence.keys, delivered);
            assert_eq!(self.evidence.finished.borrow().len(), callbacks);
            assert_eq!(self.evidence.created.get(), 1);
        }
        self.dispatch(None)?;
        self.pending_frame = None;
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
        if self.observed_operations {
            self.check_observation_replacement()?;
        }
        match self.stage {
            0 => {
                self.window_event(WindowEvent::Focused(true))?;
                if self.observed_operations {
                    self.check_foreign_observation()?;
                    let before_focus = self.observe_panel()?;
                    assert!(!before_focus.owns_keyboard());
                    assert!(before_focus.text_scope().is_none());
                    self.invoke_control("Caption", false)?;
                    self.refuse_observed_scope(&before_focus)?;
                } else {
                    self.click_control("Caption")?;
                }
                let first_scope = self.text_scope.clone().unwrap();
                let first_observation = if self.observed_operations {
                    Some(self.observe_panel()?)
                } else {
                    None
                };
                self.window_event(WindowEvent::Ime(Ime::Preedit(
                    "not ready".into(),
                    Some((0, 9)),
                )))?;
                self.window_event(WindowEvent::Ime(Ime::Commit("not ready".into())))?;
                self.assert_draft(INITIAL);
                assert_eq!(self.native_evidence.ime_enabled, 0);
                self.window_event(WindowEvent::Ime(Ime::Enabled))?;
                assert_eq!(self.native_evidence.ime_enabled, 1);
                self.key(Key::Character("x".into()), None)?;
                self.assert_draft(INITIAL);
                self.command("a")?;
                self.command("c")?;
                assert_eq!(self.clipboard.read().map_err(io::Error::other)?, INITIAL);
                self.command("v")?;
                self.assert_draft(INITIAL);
                self.command("a")?;
                self.key(Key::Character("x".into()), Some("local draft"))?;
                self.assert_draft("local draft");
                self.assert_stale_point_rejected("local draft", "local draft")?;
                self.command("z")?;
                self.assert_draft(INITIAL);
                self.command("a")?;
                self.window_event(WindowEvent::Ime(Ime::Preedit(
                    "candidate".into(),
                    Some((0, 9)),
                )))?;
                self.assert_stale_point_rejected(INITIAL, "candidate")?;
                self.window_event(WindowEvent::Ime(Ime::Preedit(String::new(), None)))?;
                self.assert_draft(INITIAL);
                self.command("y")?;
                self.assert_draft("local draft");
                assert_eq!(self.text_scope.as_ref(), Some(&first_scope));

                self.key(Key::Named(NamedKey::Tab), None)?;
                assert!(self.text_scope.is_none());
                self.dispatch(Some(EditorPanelPlatformEvent::ModifiersChanged(
                    ModifiersState::SHIFT,
                )))?;
                self.key(Key::Named(NamedKey::Tab), None)?;
                self.dispatch(Some(EditorPanelPlatformEvent::ModifiersChanged(
                    ModifiersState::empty(),
                )))?;
                let second_scope = self.text_scope.as_ref().unwrap();
                assert_ne!(second_scope.0, first_scope.0);
                assert_ne!(second_scope.1, first_scope.1);
                if let Some(first_observation) = first_observation {
                    self.refuse_observed_scope(&first_observation)?;
                }
                self.window_event(WindowEvent::Ime(Ime::Enabled))?;
                self.window_event(WindowEvent::Ime(Ime::Preedit(
                    "old scope".into(),
                    Some((0, 9)),
                )))?;
                assert_eq!(self.native_evidence.ime_enabled, 1);
                self.assert_draft("local draft");
                self.window_event(WindowEvent::Ime(Ime::Disabled))?;
                self.window_event(WindowEvent::Ime(Ime::Enabled))?;
                assert_eq!(self.native_evidence.ime_enabled, 2);
                self.command("a")?;
                if self.observed_operations {
                    self.observed_event(EditorPanelPlatformEvent::Ime(Ime::Preedit(
                        "candidate".into(),
                        Some((0, 9)),
                    )))?;
                } else {
                    self.window_event(WindowEvent::Ime(Ime::Preedit(
                        "candidate".into(),
                        Some((0, 9)),
                    )))?;
                }
                drop(
                    self.panels
                        .prepare(&self.application, |panel, _| {
                            assert_eq!(panel.draft(), "local draft");
                            assert!(panel.display_text().contains("candidate"));
                        })
                        .unwrap(),
                );
                if self.observed_operations {
                    self.observed_event(EditorPanelPlatformEvent::Ime(Ime::Commit(
                        "native draft".into(),
                    )))?;
                } else {
                    self.window_event(WindowEvent::Ime(Ime::Commit("native draft".into())))?;
                }
                self.assert_draft("native draft");
                for delta in [
                    MouseScrollDelta::LineDelta(0.0, 1.0),
                    MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 2.5)),
                ] {
                    self.window_event(WindowEvent::MouseWheel {
                        device_id: DeviceId::dummy(),
                        delta,
                        phase: TouchPhase::Moved,
                    })?;
                }
                assert_eq!(self.native_evidence.wheels, 2);
                assert!(self.native.next_deadline(&self.application).is_some());
                self.native
                    .poll_deadlines(&self.application, Instant::now());
                assert!(self.native.next_deadline(&self.application).is_none());
                self.dispatch(None)?;
                assert!(self.evidence.finished.borrow().is_empty());
            }
            1 => {
                if self.observed_operations {
                    self.invoke_control("Review change", false)?;
                    self.invoke_control("Review change", true)?;
                } else {
                    self.click_control("Review change")?;
                }
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
                if self.observed_operations {
                    let before = current(&self.application);
                    let basis = self.application.document_query_basis(self.document)?;
                    let source = self
                        .application
                        .read_document_property(property(&basis))?
                        .value;
                    let bytes = fs::read(self.project.root().join(SOURCE))?;
                    let frame = Arc::clone(self.presenter.journal().latest(self.surface)?.frame());
                    let token = self.native_token.as_ref().unwrap().clone();
                    let panel_before = self
                        .panels
                        .inspect(&self.application, &token, |panel, _| {
                            panel.compose(frame.source_revision().get()).unwrap()
                        })
                        .unwrap();
                    let interaction_before = self.interaction.semantic_snapshot(
                        self.presenter.journal(),
                        self.route.unwrap(),
                        UiSeatId::PRIMARY,
                    )?;
                    let error = self
                        .invoke_inert_control("Review change")
                        .expect_err("the original kernel must refuse the inert authored action");
                    assert!(matches!(
                        error.downcast_ref::<observation::ObservedUiOperationRefused>(),
                        Some(observation::ObservedUiOperationRefused(
                            nara_ui_kernel::UiNonActivation::TargetOutsideModal { target }
                        )) if frame.semantics().iter().any(|node| node.name() == "Review change" && node.key() == target)
                    ));
                    assert!(self.action.is_none() && self.action_id.is_none());
                    assert!(self.evidence.finished.borrow().is_empty());
                    assert!(Arc::ptr_eq(&before, &current(&self.application)));
                    assert_eq!(current(&self.application).workspace(), before.workspace());
                    assert_eq!(self.application.document_query_basis(self.document)?, basis);
                    assert_eq!(
                        self.application
                            .read_document_property(property(&basis))?
                            .value,
                        source
                    );
                    assert_eq!(fs::read(self.project.root().join(SOURCE))?, bytes);
                    assert_eq!(
                        self.panels.inspect(&self.application, &token, |panel, _| {
                            panel.compose(frame.source_revision().get()).unwrap()
                        }),
                        Some(panel_before)
                    );
                    assert_eq!(
                        self.interaction.semantic_snapshot(
                            self.presenter.journal(),
                            self.route.unwrap(),
                            UiSeatId::PRIMARY,
                        )?,
                        interaction_before
                    );
                }
                drop(
                    self.panels
                        .prepare(&self.application, |panel, _| assert_eq!(panel.reviews(), 1))
                        .unwrap(),
                );
                if self.observed_operations {
                    self.invoke_control("Apply caption", false)?;
                    self.invoke_control("Apply caption", true)?;
                } else {
                    self.click_control("Apply caption")?;
                }
                let action = self
                    .action
                    .take()
                    .ok_or_else(|| io::Error::other("presented modal did not emit Apply"))?;
                let id = self.action_id.take().unwrap();
                assert!(self.evidence.finished.borrow().is_empty());
                self.inspect_pending_apply(&action, id)?;
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
                let replacement = self.native_token.as_ref().unwrap();
                let retired = self.retired_native_token.as_ref().unwrap();
                assert_ne!(replacement, retired);
                assert!(!retired.is_current(&self.application));
                assert!(replacement.is_current(&self.application));
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
                self.window_event(WindowEvent::Ime(Ime::Enabled))?;
                self.window_event(WindowEvent::Ime(Ime::Commit("retired provider".into())))?;
                assert_eq!(self.native_evidence.ime_enabled, 2);
                self.assert_draft(INITIAL);
                self.window_event(WindowEvent::Ime(Ime::Disabled))?;
                self.window_event(WindowEvent::Ime(Ime::Enabled))?;
                assert_eq!(self.native_evidence.ime_enabled, 3);
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
                assert!(self.native_evidence.keys >= 8);
                assert_eq!(self.native_evidence.clipboard_reads, 1);
                assert_eq!(self.native_evidence.clipboard_writes, 1);
                assert_eq!(self.native_evidence.text_scopes, 3);
                assert!(self.native_evidence.ime_areas > 0);
                assert!(self.native_evidence.cursors > 0);
                assert!(self.native_evidence.redraws > 0);
                return Ok(true);
            }
            _ => unreachable!(),
        }
        self.stage += 1;
        Ok(false)
    }

    fn close(&mut self) -> Result<()> {
        let cancellations = self.native_evidence.cancellations.len();
        self.native.close();
        self.dispatch(None)?;
        if self.stage == 5 {
            assert_eq!(self.native_evidence.cancellations.len(), cancellations + 1);
            assert_eq!(
                self.native_evidence.cancellations.last(),
                Some(&(2, EditorPanelCancelReason::WindowClosed))
            );
        }
        assert!(self.text_scope.is_none());
        if self.route.is_some() && self.presenter.journal().latest(self.surface).is_ok() {
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
        assert!(!self.dispatch(Some(EditorPanelPlatformEvent::PointerMoved {
            physical_position: [20.0, 20.0],
        }))?);
        assert!(!self.dispatch(Some(EditorPanelPlatformEvent::logical_key(
            Key::Character("closed".into()),
            ElementState::Pressed,
            false,
            Some("stale".into()),
        )))?);
        assert!(self.native.next_deadline(&self.application).is_none());
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
        self.pending_frame = None;
        self.retained_observation = None;
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
        "EDITOR_UI_HOST_OK native_presentations=6 journal_commits=5 exact_panel_results=7 nonactive_authoring=true undo_redo=true save_reopen=true source_preserved=true modal_underlay_blocked=true hidden_roundtrip_retired=true late_presentation_discarded=true stale_input_rejected=true original_hosts_retired=true input_delivery=injected_pointer_and_ime native_image_host=true keyboard_clipboard=true ime_scope_ready=true mapped_caret=true cursor_refresh=true cancellation_flushed=true"
    );
    if std::env::args().any(|arg| arg == "--observed-operations") {
        println!(
            "EDITOR_UI_OBSERVATION_OK original_journal_reducer=true typed_focus_apply=true observed_key_text_ime=true exact_scope=true stale_output_instance=true foreign_host=true pending_submission_preserved=true exact_result_ids=true"
        );
    }
    Ok(())
}
