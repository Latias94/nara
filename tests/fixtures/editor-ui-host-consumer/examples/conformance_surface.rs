//! A user-owned frontend with no Nara widget tree, using the public native image panel ports.
#![forbid(unsafe_code)]

#[path = "conformance_surface/graphics.rs"]
mod graphics;
#[path = "conformance_surface/input.rs"]
mod input;

use std::{
    cell::RefCell,
    fs, io,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use graphics::Graphics;
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
use nara_editor::{
    EditorPanelHost, EditorPanelInstanceToken, PanelActionDispatch,
    native::{
        EditorClipboard, EditorClipboardAccess, EditorPanelCapability, EditorPanelImage,
        EditorPanelImageError, EditorPanelImageFrame, EditorPanelImageHost, EditorPanelImageStatus,
        EditorPanelPlatformEvent, EditorPanelTextInputScope,
    },
    workspace::{
        EditorPanelDescriptor, EditorPanelInstanceId, EditorPanelMultiplicity,
        EditorPanelPlacement, EditorPanelTarget,
    },
};
use nara_editor_ui_host_consumer::{
    COMPONENT, Evidence, FIELD, INITIAL, Result,
    conformance::{ConformancePanel, InputToken, Snapshot, SoftwarePainter},
    project::{
        self, Application, Project, SOURCE, current, entity, open_secondary, persistence, property,
    },
};
use nara_render::{DisplayMapping2d, DisplayRect, DisplayScale, Extent2d};
use nara_render_wgpu::WgpuWindowFrameOutcome;
use nara_text::TextEdit;
use nara_window::WindowId;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, Ime, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId as NativeWindowId},
};

const CAPABILITIES: [EditorPanelCapability; 8] = [
    EditorPanelCapability::Pointer,
    EditorPanelCapability::Wheel,
    EditorPanelCapability::Keyboard,
    EditorPanelCapability::TextInput,
    EditorPanelCapability::Clipboard,
    EditorPanelCapability::Cursor,
    EditorPanelCapability::Refresh,
    EditorPanelCapability::WindowModal,
];

#[derive(Default)]
struct ClipboardState {
    text: String,
    fail_write: bool,
    writes: usize,
}
struct Clipboard(Rc<RefCell<ClipboardState>>);
impl EditorClipboardAccess for Clipboard {
    fn read(&mut self) -> std::result::Result<String, String> {
        Ok(self.0.borrow().text.clone())
    }
    fn write(&mut self, text: &str) -> std::result::Result<(), String> {
        let mut state = self.0.borrow_mut();
        if std::mem::take(&mut state.fail_write) {
            return Err("conformance clipboard refuses this write".into());
        }
        state.text = text.into();
        state.writes += 1;
        Ok(())
    }
}

#[derive(Default)]
struct InputEvidence {
    scope: Option<(InputToken, EditorPanelTextInputScope)>,
    clipboard_errors: usize,
    clipped_ime_refusals: usize,
    deadline: Option<Instant>,
    mapped_point: Option<[f32; 2]>,
    cancellations: usize,
    wheel: Vec<nara_editor::native::EditorPanelWheelDelta>,
}

struct Probe {
    window: Arc<Window>,
    gpu: Option<Graphics>,
    native: EditorPanelImageHost<Snapshot>,
    application: Application,
    project: Project,
    panels: EditorPanelHost<ConformancePanel>,
    evidence: Rc<Evidence>,
    input: InputEvidence,
    clipboard: Rc<RefCell<ClipboardState>>,
    painter: SoftwarePainter,
    instance: EditorPanelInstanceId,
    document: EditorDocumentId,
    active: EditorDocumentId,
    old_token: Option<InputToken>,
    last_image: Option<EditorPanelImage<Snapshot>>,
    last_outcome: Option<WgpuWindowFrameOutcome>,
    pending: Option<EditorPanelImageFrame<Snapshot>>,
    retired: bool,
    retired_absence_checked: bool,
    fractional: bool,
    stage: u8,
    started: Instant,
}

impl Probe {
    fn new(window: Arc<Window>) -> Result<Self> {
        let project = Project::new();
        let mut application = Application::open(&project);
        let document = open_secondary(&mut application);
        let active = current(&application).workspace().active_document.unwrap();
        let evidence = Rc::new(Evidence::default());
        let observed = Rc::clone(&evidence);
        let mut panels = EditorPanelHost::new();
        panels.register_panel(
            &mut application,
            EditorPanelDescriptor::new(
                "consumer.conformance",
                "User frontend",
                EditorPanelMultiplicity::Multiple,
                EditorPanelPlacement::Floating,
                true,
            ),
            move || ConformancePanel::new(Rc::clone(&observed)).unwrap(),
        )?;
        let instance = application
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .find(|panel| panel.kind() == "consumer.conformance")
            .unwrap()
            .id();
        application
            .panel_workspace_mut()
            .layout_mut()
            .set_target(instance, EditorPanelTarget::Workspace);
        let clipboard = Rc::new(RefCell::new(ClipboardState::default()));
        let native = EditorPanelImageHost::new(
            Arc::clone(&window),
            WindowId::PRIMARY,
            EditorClipboard::with_access(Clipboard(Rc::clone(&clipboard))),
        );
        let gpu = Some(Graphics::new(&window)?);
        Ok(Self {
            window,
            gpu,
            native,
            application,
            project,
            panels,
            evidence,
            input: InputEvidence::default(),
            clipboard,
            painter: SoftwarePainter::new()?,
            instance,
            document,
            active,
            old_token: None,
            last_image: None,
            last_outcome: None,
            pending: None,
            retired: false,
            retired_absence_checked: false,
            fractional: false,
            stage: 0,
            started: Instant::now(),
        })
    }

    fn snapshot(&mut self) -> (EditorPanelInstanceToken, Arc<Snapshot>) {
        let mut captured = None;
        drop(
            self.panels
                .prepare(&self.application, |panel, context| {
                    if context.model().instance().id() == self.instance {
                        captured = Some((context.instance_token(), Arc::new(panel.compose())))
                    }
                })
                .unwrap(),
        );
        captured.unwrap()
    }

    fn draft(&mut self) -> String {
        self.snapshot().1.draft().to_owned()
    }

    fn placement(&self) -> (DisplayMapping2d, DisplayRect) {
        let size = self
            .window
            .inner_size()
            .to_logical::<f32>(self.window.scale_factor());
        let bounds = if self.fractional {
            DisplayRect {
                min: [8.25, 4.5],
                max: [size.width - 8.25, size.height - 4.5],
            }
        } else {
            DisplayRect {
                min: [0.0, 0.0],
                max: [size.width, size.height],
            }
        };
        let mapping = DisplayMapping2d::fit(
            Extent2d::new(400, 180).unwrap(),
            bounds,
            self.window.scale_factor() as f32,
            DisplayScale::Fit,
        )
        .unwrap();
        let clip = if self.fractional {
            let scale = self.window.scale_factor() as f32;
            DisplayRect {
                min: [30.0, 12.0].map(|value| (value * scale).round() / scale),
                max: [size.width - 30.0, size.height - 12.0]
                    .map(|value| (value * scale).round() / scale),
            }
        } else {
            bounds
        };
        (mapping, clip)
    }

    fn draw(&mut self, retire: bool) -> Result<bool> {
        self.gpu.as_mut().unwrap().begin(&self.window)?;
        let candidate = if let Some(pending) = &self.pending {
            if retire && self.retired {
                None
            } else {
                let next = self.native.stage(
                    &self.application,
                    self.gpu.as_ref().unwrap().frame,
                    pending.images().to_vec(),
                )?;
                self.pending = Some(next.clone());
                Some(next)
            }
        } else {
            let (token, snapshot) = self.snapshot();
            self.native
                .attach(&self.application, token.clone(), CAPABILITIES)?;
            let (mapping, clip) = self.placement();
            let gpu = self.gpu.as_mut().unwrap();
            let display = gpu.render(
                &snapshot,
                &mut self.painter,
                self.stage == 0 || self.stage == 3,
            )?;
            let image = EditorPanelImage::new(token, display, Arc::clone(&snapshot), mapping, clip)
                .with_window_modal(snapshot.popup_open());
            let next = self
                .native
                .stage(&self.application, gpu.frame, vec![image])?;
            self.pending = Some(next.clone());
            Some(next)
        };
        if retire && !self.retired {
            self.retire_pending_panel()?;
            self.retired = true;
        }
        let outcome = if retire && !self.retired_absence_checked {
            self.retired_absence_checked = true;
            WgpuWindowFrameOutcome::default()
        } else {
            self.gpu.as_mut().unwrap().present(
                &self.window,
                self.pending.as_ref().unwrap(),
                true,
            )?
        };
        if let Some(candidate) = candidate {
            let result = self
                .native
                .presented(&self.application, candidate, &outcome);
            if retire {
                assert!(
                    matches!(
                        result,
                        Err(EditorPanelImageError::RetiredInstance
                            | EditorPanelImageError::NotAttached
                            | EditorPanelImageError::SupersededFrame)
                    ),
                    "{result:?}"
                );
            } else {
                assert_eq!(
                    result?,
                    if outcome.window().is_some() {
                        EditorPanelImageStatus::Presented
                    } else {
                        EditorPanelImageStatus::Deferred
                    }
                );
            }
        }
        self.event(None)?;
        if outcome.window().is_none() {
            assert!(outcome.composites().is_empty());
            if retire {
                assert!(
                    !self.pending.as_ref().unwrap().images()[0]
                        .token()
                        .is_current(&self.application)
                );
                assert_eq!(self.evidence.created.get(), 1);
                assert_eq!(*self.evidence.dropped.borrow(), [1]);
            }
            return Ok(false);
        }
        let pending = self.pending.take().unwrap();
        let image = &pending.images()[0];
        assert_eq!(outcome.composites().len(), 1);
        let composite = outcome.composites()[0];
        assert_eq!(composite.window(), outcome.window().unwrap());
        assert_eq!(composite.image(), image.display().receipt());
        assert_eq!(composite.occurrence(), image.display().occurrence());
        if retire {
            assert!(!image.token().is_current(&self.application));
            assert_eq!(self.evidence.created.get(), 1);
        } else {
            self.last_image = Some(image.clone());
        }
        self.last_outcome = Some(outcome);
        Ok(true)
    }

    fn event(&mut self, event: Option<EditorPanelPlatformEvent>) -> Result<bool> {
        let mut reduced = Ok(());
        let input = &mut self.input;
        let document = self.document;
        let outcome = self.native.dispatch(
            &self.application,
            &mut self.panels,
            event,
            |panel, context, event, services| {
                if reduced.is_ok() {
                    reduced = input::reduce(panel, context, event, services, input, document);
                }
            },
        )?;
        reduced.map_err(|error| {
            io::Error::other(format!("conformance stage {}: {error}", self.stage))
        })?;
        if let Some(candidate) = outcome.submission {
            self.panels
                .submit_and_drive(&mut self.application, candidate, Duration::ZERO);
        }
        Ok(outcome.consumed)
    }

    fn focus(&mut self, token: &EditorPanelInstanceToken) -> Result<()> {
        let mut reduced = Ok(());
        let input = &mut self.input;
        let document = self.document;
        let outcome = self.native.focus_panel(
            &self.application,
            &mut self.panels,
            token,
            |panel, context, event, services| {
                if reduced.is_ok() {
                    reduced = input::reduce(panel, context, event, services, input, document);
                }
            },
        )?;
        reduced?;
        assert!(outcome.consumed);
        if let Some(candidate) = outcome.submission {
            self.panels
                .submit_and_drive(&mut self.application, candidate, Duration::ZERO);
        }
        Ok(())
    }

    fn pointer(&mut self, target: [f32; 2]) -> Result<bool> {
        let mapping = self.last_image.as_ref().unwrap().mapping();
        let content = mapping.content_rect();
        let scale = f64::from(mapping.pixels_per_point());
        let window = std::array::from_fn(|axis| {
            f64::from(
                content.min[axis]
                    + target[axis] / [400.0, 180.0][axis] * (content.max[axis] - content.min[axis]),
            ) * scale
        });
        self.event(Some(EditorPanelPlatformEvent::PointerMoved {
            physical_position: window,
        }))
    }

    fn click(&mut self, target: [f32; 2]) -> Result<()> {
        self.pointer(target)?;
        for state in [ElementState::Pressed, ElementState::Released] {
            self.event(Some(EditorPanelPlatformEvent::PointerButton {
                button: MouseButton::Left,
                state,
            }))?;
        }
        Ok(())
    }

    fn key(&mut self, key: Key, modifiers: ModifiersState) -> Result<()> {
        self.event(Some(EditorPanelPlatformEvent::ModifiersChanged(modifiers)))?;
        for state in [ElementState::Pressed, ElementState::Released] {
            self.event(Some(EditorPanelPlatformEvent::logical_key(
                key.clone(),
                state,
                false,
                None,
            )))?;
        }
        self.event(Some(EditorPanelPlatformEvent::ModifiersChanged(
            ModifiersState::empty(),
        )))?;
        Ok(())
    }

    fn command(&mut self, key: &str, shift: bool) -> Result<()> {
        let modifiers = if cfg!(target_os = "macos") {
            ModifiersState::SUPER
        } else {
            ModifiersState::CONTROL
        };
        self.key(
            Key::Character(key.into()),
            if shift {
                modifiers | ModifiersState::SHIFT
            } else {
                modifiers
            },
        )
    }

    fn ime(&mut self, value: Ime) -> Result<()> {
        self.event(Some(EditorPanelPlatformEvent::Ime(value)))?;
        Ok(())
    }

    fn queue(&mut self, action: EditorAction) -> PanelActionDispatch {
        let mut action = Some(action);
        let mut id = None;
        let candidate = self
            .panels
            .prepare(&self.application, |_, context| {
                if context.model().instance().id() == self.instance {
                    id = Some(context.submit(action.take().unwrap()).unwrap())
                }
            })
            .unwrap();
        self.panels
            .submit_and_drive(&mut self.application, candidate, Duration::ZERO);
        let finished = self.evidence.finished.borrow();
        let (_, actual, result) = finished.last().unwrap();
        assert_eq!(Some(*actual), id);
        result.clone()
    }

    fn history_and_persistence(&mut self) -> Result<()> {
        for (undo, text) in [(true, INITIAL), (false, "native draft")] {
            let basis = self.application.document_query_basis(self.document)?;
            let action = if undo {
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
            assert!(
                matches!(self.queue(EditorAction::Mutate(action)), PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)) if report.applied)
            );
            let basis = self.application.document_query_basis(self.document)?;
            assert_eq!(
                self.application
                    .read_document_property(property(&basis))?
                    .value,
                EditorPropertyValue::Present(ComponentValue::String(text.into()))
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
            return Err(io::Error::other(format!("Save failed: {saved:?}")).into());
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
        assert_eq!(
            self.queue(EditorAction::Persistence(
                EditorPersistenceAction::AcknowledgeResult {
                    attempt: save_attempt
                }
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
            return Err(io::Error::other(format!("Reopen failed: {reopened:?}")).into());
        };
        let EditorPersistenceResult::Opened { document, .. } = persistence(&mut self.application)
        else {
            return Err(io::Error::other("Reopen did not complete").into());
        };
        self.document = document;
        assert_ne!(save_attempt, reopen_attempt);
        assert_eq!(
            current(&self.application).persistence().attempt(),
            Some(reopen_attempt)
        );
        assert!(
            self.application
                .read_document_property(property(&basis))
                .is_err()
        );
        let basis = self.application.document_query_basis(document)?;
        assert_eq!(
            self.application
                .read_document_property(property(&basis))?
                .value,
            EditorPropertyValue::Present(ComponentValue::String("native draft".into()))
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
        assert_eq!(
            current(&self.application).workspace().active_document,
            Some(self.active)
        );
        assert_eq!(fs::read(self.project.root().join(SOURCE))?, bytes);
        assert_eq!(
            self.queue(EditorAction::Persistence(
                EditorPersistenceAction::AcknowledgeResult {
                    attempt: reopen_attempt
                }
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

    fn retire_pending_panel(&mut self) -> Result<()> {
        let basis = self.application.document_query_basis(self.document)?;
        let before = current(&self.application);
        let bytes = fs::read(self.project.root().join(SOURCE))?;
        let candidate = self
            .panels
            .prepare(&self.application, |panel, context| {
                assert!(panel.display_text().contains("retiring composition"));
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
        layout.set_visible(self.instance, false);
        layout.reopen(self.instance);
        self.panels
            .submit_and_drive(&mut self.application, candidate, Duration::ZERO);
        assert_eq!(self.evidence.finished.borrow().len(), callbacks);
        assert_eq!(*self.evidence.dropped.borrow(), [1]);
        assert_eq!(self.application.document_query_basis(self.document)?, basis);
        assert_eq!(current(&self.application).workspace(), before.workspace());
        assert_eq!(fs::read(self.project.root().join(SOURCE))?, bytes);
        Ok(())
    }

    fn presentation_controls(&mut self) -> Result<()> {
        let image = self.last_image.as_ref().unwrap().clone();
        let mut foreign = EditorPanelImageHost::new(
            Arc::clone(&self.window),
            WindowId::PRIMARY,
            EditorClipboard::with_access(Clipboard(Rc::clone(&self.clipboard))),
        );
        assert!(matches!(
            foreign.attach(
                &self.application,
                image.token().clone(),
                [EditorPanelCapability::DirectRendering]
            ),
            Err(EditorPanelImageError::UnsupportedCapability(
                EditorPanelCapability::DirectRendering
            ))
        ));
        foreign.attach(&self.application, image.token().clone(), CAPABILITIES)?;
        self.gpu.as_mut().unwrap().begin(&self.window)?;
        let frame = self.gpu.as_ref().unwrap().frame;
        let original = self
            .native
            .stage(&self.application, frame, vec![image.clone()])?;
        let wrong = foreign.stage(&self.application, frame, vec![image.clone()])?;
        let outcome = self
            .gpu
            .as_mut()
            .unwrap()
            .present(&self.window, &original, true)?;
        assert!(outcome.window().is_some());
        assert!(matches!(
            self.native.presented(&self.application, wrong, &outcome),
            Err(EditorPanelImageError::ForeignFrame)
        ));
        assert_eq!(
            self.native
                .presented(&self.application, original, &outcome)?,
            EditorPanelImageStatus::Presented
        );
        foreign.close();
        self.gpu.as_mut().unwrap().begin(&self.window)?;
        let frame = self.gpu.as_ref().unwrap().frame;
        let old = self
            .native
            .stage(&self.application, frame, vec![image.clone()])?;
        let original = self
            .native
            .stage(&self.application, frame, vec![image.clone()])?;
        let outcome = self
            .gpu
            .as_mut()
            .unwrap()
            .present(&self.window, &original, true)?;
        assert!(outcome.window().is_some());
        assert!(matches!(
            self.native.presented(&self.application, old, &outcome),
            Err(EditorPanelImageError::SupersededFrame)
        ));
        assert_eq!(
            self.native
                .presented(&self.application, original, &outcome)?,
            EditorPanelImageStatus::Presented
        );
        self.gpu.as_mut().unwrap().begin(&self.window)?;
        let frame = self.gpu.as_ref().unwrap().frame;
        let missing = self
            .native
            .stage(&self.application, frame, vec![image.clone()])?;
        let outcome = self
            .gpu
            .as_mut()
            .unwrap()
            .present(&self.window, &missing, false)?;
        assert!(outcome.window().is_some() && outcome.composites().is_empty());
        assert!(matches!(
            self.native.presented(&self.application, missing, &outcome),
            Err(EditorPanelImageError::IncompletePresentation)
        ));
        assert!(!self.event(Some(EditorPanelPlatformEvent::Text("unpresented".into())))?);
        self.gpu.as_mut().unwrap().begin(&self.window)?;
        let frame = self.gpu.as_ref().unwrap().frame;
        let restored = self.native.stage(&self.application, frame, vec![image])?;
        let outcome = self
            .gpu
            .as_mut()
            .unwrap()
            .present(&self.window, &restored, true)?;
        assert_eq!(
            self.native
                .presented(&self.application, restored, &outcome)?,
            EditorPanelImageStatus::Presented
        );
        self.last_outcome = Some(outcome);
        Ok(())
    }

    fn two_panel_controls(&mut self) -> Result<()> {
        let other_evidence = Rc::new(Evidence::default());
        let observed = Rc::clone(&other_evidence);
        self.panels.register_panel(
            &mut self.application,
            EditorPanelDescriptor::new(
                "consumer.second-image",
                "Second user frontend",
                EditorPanelMultiplicity::Single,
                EditorPanelPlacement::Bottom,
                true,
            ),
            move || ConformancePanel::new(Rc::clone(&observed)).unwrap(),
        )?;
        let second = self
            .application
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .find(|panel| panel.kind() == "consumer.second-image")
            .unwrap()
            .id();
        self.application
            .panel_workspace_mut()
            .layout_mut()
            .set_target(second, EditorPanelTarget::Workspace);
        let (primary, primary_snapshot) = self.snapshot();
        let mut secondary = None;
        drop(
            self.panels
                .prepare(&self.application, |panel, context| {
                    if context.model().instance().id() == second {
                        secondary = Some((context.instance_token(), Arc::new(panel.compose())));
                    }
                })
                .unwrap(),
        );
        let (secondary, secondary_snapshot) = secondary.unwrap();
        assert_eq!(
            self.painter.paint(&primary_snapshot, 1.0)?,
            self.painter.paint(&secondary_snapshot, 1.0)?
        );
        let size = self
            .window
            .inner_size()
            .to_logical::<f32>(self.window.scale_factor());
        let placements = [
            DisplayRect {
                min: [0.0, 0.0],
                max: [size.width / 2.0, size.height],
            },
            DisplayRect {
                min: [size.width / 2.0, 0.0],
                max: [size.width, size.height],
            },
        ];
        self.native
            .attach(&self.application, secondary.clone(), CAPABILITIES)?;
        let gpu = self.gpu.as_mut().unwrap();
        gpu.begin(&self.window)?;
        let first = gpu.render(&primary_snapshot, &mut self.painter, false)?;
        let second_display = gpu.mirror(&first)?;
        assert_ne!(first.occurrence(), second_display.occurrence());
        let images = [
            (primary.clone(), primary_snapshot, first),
            (secondary.clone(), secondary_snapshot, second_display),
        ]
        .into_iter()
        .zip(placements)
        .map(|((token, snapshot, display), clip)| {
            EditorPanelImage::new(
                token,
                display,
                snapshot,
                DisplayMapping2d::fit(
                    Extent2d::new(400, 180).unwrap(),
                    clip,
                    self.window.scale_factor() as f32,
                    DisplayScale::Fit,
                )
                .unwrap(),
                clip,
            )
        })
        .collect::<Vec<_>>();
        let missing = self
            .native
            .stage(&self.application, gpu.frame, images.clone())?;
        let outcome = gpu.present_count(&self.window, &missing, 1)?;
        assert!(outcome.window().is_some());
        assert_eq!(outcome.composites().len(), 1);
        assert!(matches!(
            self.native.presented(&self.application, missing, &outcome),
            Err(EditorPanelImageError::IncompletePresentation)
        ));
        self.event(Some(EditorPanelPlatformEvent::Focused(true)))?;
        for token in [&primary, &secondary] {
            assert!(matches!(
                self.native.focus_panel(
                    &self.application,
                    &mut self.panels,
                    token,
                    |_, _, _, _| panic!("partially presented panel received focus")
                ),
                Err(EditorPanelImageError::NotPresented)
            ));
        }
        let gpu = self.gpu.as_mut().unwrap();
        gpu.begin(&self.window)?;
        let complete = self.native.stage(&self.application, gpu.frame, images)?;
        let outcome = gpu.present(&self.window, &complete, true)?;
        assert!(outcome.window().is_some());
        assert_eq!(outcome.composites().len(), 2);
        assert_eq!(
            self.native
                .presented(&self.application, complete, &outcome)?,
            EditorPanelImageStatus::Presented
        );
        self.last_outcome = Some(outcome);
        self.event(Some(EditorPanelPlatformEvent::Focused(true)))?;
        self.focus(&primary)?;
        self.ime(Ime::Enabled)?;
        self.ime(Ime::Preedit("first composition".into(), Some((0, 17))))?;
        assert!(
            self.snapshot()
                .1
                .display_text()
                .contains("first composition")
        );
        let old_scope = self.input.scope.as_ref().unwrap().1.clone();
        let mut visits = Vec::new();
        let mut reduced = Ok(());
        let input = &mut self.input;
        let document = self.document;
        let outcome = self.native.focus_panel(
            &self.application,
            &mut self.panels,
            &secondary,
            |panel, context, event, services| {
                visits.push((context.model().instance().id(), event.kind.clone()));
                if reduced.is_ok() {
                    reduced = input::reduce(panel, context, event, services, input, document);
                }
            },
        )?;
        reduced?;
        assert!(outcome.consumed);
        assert!(
            matches!(visits.as_slice(), [(previous, nara_editor::native::EditorPanelInputKind::Cancelled(_)),
            (next, nara_editor::native::EditorPanelInputKind::KeyboardFocused)]
            if *previous == self.instance && *next == second)
        );
        if let Some(candidate) = outcome.submission {
            self.panels
                .submit_and_drive(&mut self.application, candidate, Duration::ZERO);
        }
        assert_eq!(self.snapshot().1.display_text(), INITIAL);
        assert_ne!(self.input.scope.as_ref().unwrap().1, old_scope);
        let stale = self.native.dispatch(
            &self.application,
            &mut self.panels,
            Some(EditorPanelPlatformEvent::Ime(Ime::Commit(
                "stale first".into(),
            ))),
            |_, _, _, _| panic!("retiring IME commit reached the successor field"),
        )?;
        assert!(stale.consumed);
        drop(stale.submission);
        self.ime(Ime::Disabled)?;
        self.ime(Ime::Enabled)?;
        self.ime(Ime::Preedit("second composition".into(), Some((0, 18))))?;
        drop(
            self.panels
                .prepare(&self.application, |panel, context| {
                    if context.model().instance().id() == second {
                        assert!(panel.display_text().contains("second composition"));
                    }
                })
                .unwrap(),
        );
        assert!(self.native.detach(&secondary));
        self.focus(&primary)?;
        self.ime(Ime::Disabled)?;
        drop(
            self.panels
                .prepare(&self.application, |panel, context| {
                    if context.model().instance().id() == second {
                        assert!(panel.input_token().is_none());
                        assert_eq!(panel.display_text(), INITIAL);
                    }
                })
                .unwrap(),
        );
        self.application
            .panel_workspace_mut()
            .layout_mut()
            .set_visible(second, false);
        drop(self.panels.prepare(&self.application, |_, context| {
            assert_ne!(context.model().instance().id(), second);
        }));
        assert!(!secondary.is_current(&self.application));
        assert_eq!(*other_evidence.dropped.borrow(), [1]);
        assert!(other_evidence.finished.borrow().is_empty());
        assert_eq!(self.draft(), INITIAL);
        Ok(())
    }

    fn keyboard_and_semantic_owner_controls(&mut self) -> Result<()> {
        let image = self.last_image.as_ref().unwrap().clone();
        self.native.detach(image.token());
        self.native.attach(
            &self.application,
            image.token().clone(),
            [
                EditorPanelCapability::Keyboard,
                EditorPanelCapability::TextInput,
                EditorPanelCapability::Refresh,
            ],
        )?;
        let gpu = self.gpu.as_mut().unwrap();
        gpu.begin(&self.window)?;
        let candidate = self
            .native
            .stage(&self.application, gpu.frame, vec![image.clone()])?;
        let outcome = gpu.present(&self.window, &candidate, true)?;
        assert_eq!(
            self.native
                .presented(&self.application, candidate, &outcome)?,
            EditorPanelImageStatus::Presented
        );
        self.event(Some(EditorPanelPlatformEvent::Focused(true)))?;
        self.focus(image.token())?;
        self.command("a", false)?;
        self.event(Some(EditorPanelPlatformEvent::Text("keyboard only".into())))?;
        assert_eq!(self.draft(), "keyboard only");
        self.command("z", false)?;
        assert_eq!(self.draft(), INITIAL);

        let other_evidence = Rc::new(Evidence::default());
        let observed = Rc::clone(&other_evidence);
        let mut other = EditorPanelHost::new();
        other.register_panel(
            &mut self.application,
            EditorPanelDescriptor::new(
                "consumer.other-owner",
                "Other semantic owner",
                EditorPanelMultiplicity::Single,
                EditorPanelPlacement::Bottom,
                true,
            ),
            move || ConformancePanel::new(Rc::clone(&observed)).unwrap(),
        )?;
        let mut other_token = None;
        drop(
            other
                .prepare(&self.application, |_, context| {
                    other_token = Some(context.instance_token())
                })
                .unwrap(),
        );
        let other_token = other_token.unwrap();
        assert!(matches!(
            self.native
                .attach(&self.application, other_token.clone(), CAPABILITIES),
            Err(EditorPanelImageError::ForeignSemanticHost)
        ));
        self.native.invalidate();
        let cancelled = self.input.cancellations;
        assert!(matches!(
            self.native
                .dispatch(&self.application, &mut other, None, |_, _, _, _| panic!(
                    "foreign semantic receiver visited"
                )),
            Err(EditorPanelImageError::ForeignSemanticHost)
        ));
        assert!(self.snapshot().1.input_token().is_some());
        assert_eq!(self.input.cancellations, cancelled);
        self.event(None)?;
        assert_eq!(self.input.cancellations, cancelled + 1);
        assert!(self.snapshot().1.input_token().is_none());
        drop(other);
        assert!(!other_token.is_current(&self.application));
        assert_eq!(*other_evidence.dropped.borrow(), [1]);

        self.native.detach(image.token());
        self.native
            .attach(&self.application, image.token().clone(), CAPABILITIES)?;
        let (_, snapshot) = self.snapshot();
        let gpu = self.gpu.as_mut().unwrap();
        gpu.begin(&self.window)?;
        let display = gpu.render(&snapshot, &mut self.painter, false)?;
        let image = EditorPanelImage::new(
            image.token().clone(),
            display,
            Arc::clone(&snapshot),
            image.mapping(),
            image.clip(),
        );
        let candidate = self
            .native
            .stage(&self.application, gpu.frame, vec![image.clone()])?;
        let outcome = gpu.present(&self.window, &candidate, true)?;
        assert_eq!(
            self.native
                .presented(&self.application, candidate, &outcome)?,
            EditorPanelImageStatus::Presented
        );
        self.last_image = Some(image);
        self.last_outcome = Some(outcome);
        Ok(())
    }

    fn step(&mut self) -> Result<bool> {
        if self.started.elapsed() > Duration::from_secs(60) {
            return Err(io::Error::other("conformance native trajectory timed out").into());
        }
        if !self.draw(self.stage == 8)? {
            return Ok(false);
        }
        match self.stage {
            0 => {
                self.presentation_controls()?;
                self.two_panel_controls()?;
                self.keyboard_and_semantic_owner_controls()?;
                self.event(Some(EditorPanelPlatformEvent::Focused(true)))?;
                self.click([40.0, 30.0])?;
                self.ime(Ime::Commit("must wait for Enabled".into()))?;
                assert_eq!(self.draft(), INITIAL);
                self.ime(Ime::Enabled)?;
                self.command("a", false)?;
                self.ime(Ime::Preedit("candidate".into(), Some((0, 9))))?;
                self.ime(Ime::Commit("native draft".into()))?;
                self.old_token = self.snapshot().1.input_token();
                assert_eq!(self.draft(), "native draft");
                assert!(self.evidence.finished.borrow().is_empty());
            }
            1 => {
                self.command("z", false)?;
                assert_eq!(self.draft(), INITIAL);
                self.command("z", true)?;
                assert_eq!(self.draft(), "native draft");
                self.command("a", false)?;
                self.command("c", false)?;
                assert_eq!(self.clipboard.borrow().text, "native draft");
                self.clipboard.borrow_mut().fail_write = true;
                self.command("x", false)?;
                assert_eq!(self.input.clipboard_errors, 1);
                assert_eq!(self.draft(), "native draft");
                self.command("x", false)?;
                assert_eq!(self.draft(), "");
                self.command("v", false)?;
                assert_eq!(self.draft(), "native draft");
            }
            2 => {
                self.pointer([40.0, 30.0])?;
                self.event(Some(EditorPanelPlatformEvent::PointerButton {
                    button: MouseButton::Left,
                    state: ElementState::Pressed,
                }))?;
                assert!(self.pointer([-30.0, -20.0])?);
                assert!(self.input.mapped_point.unwrap()[0] < 0.0);
                self.event(Some(EditorPanelPlatformEvent::PointerButton {
                    button: MouseButton::Right,
                    state: ElementState::Released,
                }))?;
                assert!(self.pointer([-25.0, -15.0])?);
                self.event(Some(EditorPanelPlatformEvent::PointerButton {
                    button: MouseButton::Left,
                    state: ElementState::Released,
                }))?;
                assert!(!self.pointer([-20.0, -10.0])?);
                self.click([40.0, 90.0])?;
                assert!(self.snapshot().1.popup_open());
            }
            3 => {
                assert!(self.pointer([-10.0, -10.0])?);
                self.click([40.0, 90.0])?;
                assert!(self.snapshot().1.popup_open());
                self.click([230.0, 58.0])?;
                let finished = self.evidence.finished.borrow();
                assert_eq!(finished.len(), 1);
                assert!(
                    matches!(&finished[0].2, PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(report)) if report.applied)
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
            }
            4 => {
                self.history_and_persistence()?;
                self.fractional = true;
            }
            5 => {
                self.pointer([40.0, 30.0])?;
                let point = self.input.mapped_point.unwrap();
                assert!((point[0] - 40.0).abs() < 0.001 && (point[1] - 30.0).abs() < 0.001);
                assert!(!self.event(Some(EditorPanelPlatformEvent::PointerMoved {
                    physical_position: [1.0, 1.0]
                }))?);
                self.click([40.0, 30.0])?;
                self.ime(Ime::Enabled)?;
                self.ime(Ime::Preedit("old acknowledgement".into(), Some((0, 19))))?;
                assert!(
                    !self
                        .snapshot()
                        .1
                        .display_text()
                        .contains("old acknowledgement")
                );
                self.ime(Ime::Disabled)?;
                self.ime(Ime::Enabled)?;
                drop(
                    self.panels
                        .prepare(&self.application, |panel, _| {
                            assert!(
                                panel
                                    .edit(
                                        self.old_token.unwrap(),
                                        &TextEdit::Commit("stale".into())
                                    )
                                    .is_err()
                            )
                        })
                        .unwrap(),
                );
            }
            6 => {
                self.key(Key::Named(NamedKey::F7), ModifiersState::empty())?;
                self.key(Key::Named(NamedKey::F6), ModifiersState::empty())?;
                let deadline = self.input.deadline.unwrap();
                assert_eq!(self.native.next_deadline(&self.application), Some(deadline));
                assert!(
                    !self
                        .native
                        .poll_deadlines(&self.application, deadline - Duration::from_nanos(1))
                );
                assert!(self.native.poll_deadlines(&self.application, deadline));
                assert!(!self.native.poll_deadlines(&self.application, deadline));
            }
            7 => {
                assert!(self.native.next_deadline(&self.application).is_none());
                self.ime(Ime::Preedit("retiring composition".into(), Some((0, 20))))?;
                assert!(
                    self.snapshot()
                        .1
                        .display_text()
                        .contains("retiring composition")
                );
            }
            8 => {
                assert!(!self.event(Some(EditorPanelPlatformEvent::Text("late".into())))?);
                self.last_image = None;
            }
            9 => {
                self.click([40.0, 30.0])?;
                self.ime(Ime::Disabled)?;
                self.ime(Ime::Enabled)?;
                drop(
                    self.panels
                        .prepare(&self.application, |panel, _| {
                            assert_eq!(panel.serial(), 2);
                            assert!(
                                panel
                                    .edit(
                                        self.old_token.unwrap(),
                                        &TextEdit::Commit("late old field".into())
                                    )
                                    .is_err()
                            );
                            assert_eq!(panel.draft(), INITIAL);
                        })
                        .unwrap(),
                );
                self.ime(Ime::Preedit("pending at close".into(), Some((0, 16))))?;
                assert!(
                    self.snapshot()
                        .1
                        .display_text()
                        .contains("pending at close")
                );
                assert_eq!(self.evidence.finished.borrow().len(), 7);
                assert_eq!(self.evidence.created.get(), 2);
                assert_eq!(self.gpu.as_ref().unwrap().presentations, 18);
                assert_eq!(self.gpu.as_ref().unwrap().readback_pixels, 2 * 400 * 180);
                assert!(self.input.cancellations > 0);
                assert_eq!(self.input.clipped_ime_refusals, 1);
                assert!(self.retired_absence_checked);
                return Ok(true);
            }
            _ => unreachable!(),
        }
        self.stage += 1;
        Ok(false)
    }

    fn close(&mut self) -> Result<()> {
        self.native.close();
        self.event(None)?;
        drop(
            self.panels
                .prepare(&self.application, |panel, _| {
                    assert!(panel.input_token().is_none());
                    assert_eq!(panel.display_text(), panel.draft());
                })
                .unwrap(),
        );
        let instances = self
            .application
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .filter(|panel| {
                matches!(
                    panel.kind(),
                    "consumer.conformance" | "consumer.second-image"
                )
            })
            .map(|panel| panel.id())
            .collect::<Vec<_>>();
        for instance in instances {
            self.application
                .panel_workspace_mut()
                .layout_mut()
                .set_visible(instance, false);
        }
        drop(self.panels.prepare(&self.application, |_, _| {
            panic!("hidden conformance panel visited")
        }));
        self.last_image = None;
        self.pending = None;
        assert_eq!(
            self.evidence.dropped.borrow().len(),
            self.evidence.created.get()
        );
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
                        .with_title("User frontend conformance")
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
                Err("native conformance window closed early".into()),
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
                .or_else(|| panic.downcast_ref::<&str>().map(|text| (*text).into()))
                .unwrap_or_else(|| "conformance native trajectory panicked".into());
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
        .ok_or_else(|| io::Error::other("conformance event loop has no result"))?
        .map_err(io::Error::other)?;
    println!(
        "EDITOR_FOREIGN_UI_OK native_presentations=18 exact_panel_results=7 readback_pixels=144000 public_image_host=true widgets_required=false source_preserved=true clipboard_cursor_refresh=true capture_modal_retirement=true keyboard_only_focus=true semantic_owner_fenced=true two_panel_atomic=true input_delivery=injected"
    );
    Ok(())
}
