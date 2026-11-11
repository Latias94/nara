use std::{
    cell::RefCell,
    fs,
    path::Path,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail, ensure};
use nara_core::{Color, Mat3};
use nara_ecs::Entity;
use nara_engine::{
    ProductRecipe,
    app::RuntimeCandidateRetirementState,
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode},
    project_host::{
        EditorAcceptedPreview, EditorPresentationImage, EditorPresentedToken,
        EditorPreviewContentStamp, EditorPreviewPresentation, EditorProjectCloseProgress,
        EditorProjectSession, TrustedCompiledEditorProjectIntent,
        advanced::built_in_schema_providers,
    },
    reflect::{
        AuthoringSchemaBundle, ComponentFieldId, ComponentPropertyPath, ComponentSchemaVersion,
        ComponentTypeId, ComponentValue,
    },
    scene::{
        AuthoringPropertyEdit, SceneAuthoringRevision, SceneDocument, SceneEntityId,
        ScenePlacement, ScenePlacementParent,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentId,
        EditorDocumentMutation, EditorNavigationAction, EditorPropertyRead,
        EditorPropertyReadLimits, EditorPropertyTarget, EditorPropertyValue,
    },
};
use nara_render::{
    DisplayMapping2d, Extent2d, ExtractedView, ExtractedViews, RenderFrameSourceGeneration,
    RenderFrameTarget, RenderImage2d, RenderImagePacket, RenderTarget, RenderViewId,
    RenderWindowPacket, ViewLoadOp, ViewMapping2d, ViewportRect, build_image_render_frame_packet,
};
use nara_render_wgpu::{
    WgpuDeviceRequirements, WgpuImageCompositeReceipt, WgpuImageDisplay, WgpuImageOutput,
    WgpuPolicyDefinition, WgpuPolicyInstallation, WgpuRenderHost, WgpuRenderProducer,
    WgpuStockCapture, WgpuStockPolicy, WgpuSubmissionPrefix,
};
use nara_window::{PresentMode, WindowId, WindowResolution, backend::WindowHandleProvider};
use tempfile::TempDir;
use winit::window::Window;

const TIMEOUT: Duration = Duration::from_secs(10);
const TRANSFORM: &str = "nara.transform.Transform2d";
const SPRITE: &str = "nara.sprite.Sprite";

struct ContentImage {
    _input: nara_render_wgpu::WgpuRenderInput,
    producer: WgpuRenderProducer,
    display: WgpuImageDisplay,
}

pub(super) struct Preview {
    pub accepted: RefCell<Option<EditorAcceptedPreview>>,
    pub display: WgpuImageDisplay,
    pub stamp: EditorPreviewContentStamp,
    pub document: EditorDocumentId,
    pub revision: SceneAuthoringRevision,
    presented: RefCell<Option<PresentedPreview>>,
}

#[derive(Clone, Copy)]
struct PresentedPreview {
    token: EditorPresentedToken,
    composite: WgpuImageCompositeReceipt,
    mapping: DisplayMapping2d,
}

pub(super) struct Engine {
    pub host: WgpuRenderHost,
    pub window: Arc<Window>,
    pub captures: u64,
    pub completions: u64,
    pub picks: u64,
    pub moves: u64,
    pub undos: u64,
    pub redos: u64,
    pub other_frames: u64,
    session: EditorProjectSession,
    presentation: EditorPreviewPresentation,
    scene_producer: WgpuRenderProducer,
    other_producer: WgpuRenderProducer,
    other_source: RenderFrameSourceGeneration,
    other_view: RenderViewId,
    other_target: RenderImagePacket,
    other_policy: WgpuPolicyInstallation<WgpuStockPolicy>,
    document: EditorDocumentId,
    sprite: SceneEntityId,
    host_frame: u64,
    last_presentation: u64,
    retiring: Vec<WgpuSubmissionPrefix>,
    closing: bool,
    closed: bool,
    cleanup_failed: bool,
    content: Vec<ContentImage>,
    _project: TempDir,
}

impl Engine {
    pub fn new(window: Arc<Window>) -> Result<Self> {
        let mut host = WgpuRenderHost::new(
            WindowId::PRIMARY,
            WindowHandleProvider::new(Arc::clone(&window)),
        )?;
        let mut limits = wgpu::Limits::default();
        limits.max_buffers_and_acceleration_structures_per_shader_stage = limits
            .max_buffers_and_acceleration_structures_per_shader_stage
            .max(4);
        host.request_device_requirements(&WgpuDeviceRequirements::new(
            wgpu::Features::empty(),
            limits,
        ))?;
        let scene_producer = host.create_producer()?;
        let other_producer = host.create_producer()?;
        let other_source = RenderFrameSourceGeneration::try_acquire()
            .context("independent render source identities are exhausted")?;
        let other_view = RenderViewId::try_acquire()
            .context("independent render view identities are exhausted")?;
        let other_target = RenderImagePacket::new(
            RenderImage2d::allocate_handle()?,
            Extent2d::new(32, 32).context("the control extent must be nonempty")?,
        )?;
        let other_policy = WgpuPolicyDefinition::new(WgpuStockPolicy::new()).install()?;
        let project = create_project()?;
        let capability = open_directory(project.path())?;
        let mut session = match EditorProjectSession::open_trusted_compiled(
            capability,
            TrustedCompiledEditorProjectIntent::new().with_recipe(ProductRecipe::new()),
        ) {
            Ok(session) => session,
            Err(mut error) => {
                let deadline = Instant::now() + TIMEOUT;
                while error.advance_retirement() != RuntimeCandidateRetirementState::Retired {
                    ensure!(
                        Instant::now() < deadline,
                        "failed Editor Open did not retire: {:?}",
                        error.diagnostics()
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
                bail!(
                    "open the compiled Editor project: {:?}",
                    error.diagnostics()
                );
            }
        };
        let initialized = initialize_sprite(&mut session);
        let (document, sprite) = match initialized {
            Ok(initialized) => initialized,
            Err(error) => {
                close_session(&mut session)
                    .context("retire the Editor after failed sprite initialization")?;
                return Err(error);
            }
        };
        session.configure_editor_preview(Some(
            Extent2d::new(640, 360).context("the Scene extent must be nonempty")?,
        ));
        let mut engine = Self {
            host,
            window,
            captures: 0,
            completions: 0,
            picks: 0,
            moves: 0,
            undos: 0,
            redos: 0,
            other_frames: 0,
            session,
            presentation: EditorPreviewPresentation::new(),
            scene_producer,
            other_producer,
            other_source,
            other_view,
            other_target,
            other_policy,
            document,
            sprite,
            host_frame: 0,
            last_presentation: 0,
            retiring: Vec::new(),
            closing: false,
            closed: false,
            cleanup_failed: false,
            content: Vec::new(),
            _project: project,
        };
        if let Err(error) = engine.load_content() {
            engine
                .close()
                .context("retire after content capture failure")?;
            return Err(error);
        }
        Ok(engine)
    }

    fn load_content(&mut self) -> Result<()> {
        let captures = super::content::capture()?;
        for input in [captures.source, captures.processed] {
            let target = RenderImagePacket::new(
                RenderImage2d::allocate_handle()?,
                Extent2d::new(super::content::TARGET_WIDTH, super::content::TARGET_HEIGHT)
                    .context("content render extent must be nonempty")?,
            )?;
            let input = input.with_image_target(target)?;
            let producer = self.host.create_producer()?;
            self.host_frame = self
                .host_frame
                .checked_add(1)
                .context("Host transaction identities are exhausted")?;
            self.host.begin_frame(self.host_frame)?;
            let reservation = self.host.reserve_image_target(
                &producer,
                input.packet().source_generation(),
                target,
            )?;
            self.host.activate_image_target(&producer, reservation)?;
            let output = self.host.submit(&producer, &input)?;
            self.content.push(ContentImage {
                _input: input,
                producer,
                display: self.host.display_image(output)?,
            });
        }
        Ok(())
    }

    pub fn content_displays(&self) -> Vec<WgpuImageDisplay> {
        self.content
            .iter()
            .map(|content| content.display.clone())
            .collect()
    }

    pub fn begin_window(&mut self) -> Result<(RenderWindowPacket, u64)> {
        ensure!(!self.closing, "the engine is closing");
        self.host_frame = self
            .host_frame
            .checked_add(1)
            .context("Host transaction identities are exhausted")?;
        let size = self.window.inner_size();
        let packet = RenderWindowPacket {
            id: WindowId::PRIMARY,
            resolution: WindowResolution::new(size.width, size.height)
                .with_scale_factor(self.window.scale_factor()),
            present_mode: PresentMode::AutoNoVsync,
        };
        self.host.begin_window_frame(packet, self.host_frame)?;
        Ok((packet, self.host_frame))
    }

    pub fn capture(&mut self) -> Result<Rc<Preview>> {
        ensure!(!self.closing, "the engine is closing");
        if self.host_frame == 0 {
            self.begin_window()?;
        }
        self.session.drive_editor_frame(Duration::ZERO);
        let candidate = self
            .session
            .take_editor_preview_candidate()
            .context("the Editor did not produce a Scene preview candidate")?;
        let stamp = candidate.content_stamp();
        let (accepted, input) = self.presentation.accept(candidate)?;
        let RenderFrameTarget::Image(target) = input.packet().target() else {
            bail!("the Scene candidate did not retain an image target");
        };
        let source = input.packet().source_generation();
        if !self
            .host
            .image_target_matches(&self.scene_producer, source, target)
        {
            let reservation =
                self.host
                    .reserve_image_target(&self.scene_producer, source, target)?;
            self.host
                .activate_image_target(&self.scene_producer, reservation)?;
        }
        let output = self.host.submit(&self.scene_producer, &input)?;
        let display = self.host.display_image(output)?;
        let preview = Rc::new(Preview {
            document: accepted.document(),
            revision: accepted.revision(),
            accepted: RefCell::new(Some(accepted)),
            display,
            stamp,
            presented: RefCell::new(None),
        });
        self.captures += 1;
        Ok(preview)
    }

    pub fn complete(
        &mut self,
        preview: &Preview,
        composite: WgpuImageCompositeReceipt,
        mapping: DisplayMapping2d,
    ) -> Result<()> {
        self.require_current(preview)?;
        let image = preview.display.receipt();
        let window = composite.window();
        ensure!(
            composite.image() == image
                && composite.occurrence() == preview.display.occurrence()
                && window.window_id() == WindowId::PRIMARY
                && window.backend_instance_id() == self.host.backend_instance_id()
                && window.device_epoch() == self.host.device_epoch()
                && window.display_frame_index() > 0
                && window.display_frame_index() <= self.host_frame
                && composite.presentation_generation() > self.last_presentation
                && mapping.target() == image.target().extent(),
            "the accepted GPUI frame did not display this exact current Scene occurrence"
        );
        let facts = presentation_image(composite);
        let token = if let Some(accepted) = preview.accepted.borrow_mut().take() {
            self.presentation.finish(accepted, facts, mapping)?
        } else {
            let presented = preview.presented.borrow();
            let presented = presented
                .as_ref()
                .context("the Scene has no previously completed presentation")?;
            self.presentation
                .redisplay(presented.token, facts, mapping)?
        };
        *preview.presented.borrow_mut() = Some(PresentedPreview {
            token,
            composite,
            mapping,
        });
        self.last_presentation = composite.presentation_generation();
        self.completions += 1;
        Ok(())
    }

    pub fn pick(
        &mut self,
        preview: &Preview,
        mapping: DisplayMapping2d,
        point: [f32; 2],
    ) -> Result<Option<SceneEntityId>> {
        self.require_current(preview)?;
        let presented =
            (*preview.presented.borrow()).context("the Scene has no accepted GPUI presentation")?;
        ensure!(
            preview.accepted.borrow().is_none()
                && presented.mapping == mapping
                && presented.composite.image() == preview.display.receipt()
                && presented.composite.occurrence() == preview.display.occurrence()
                && presented.composite.presentation_generation() == self.last_presentation,
            "the Scene mapping or displayed image is stale"
        );
        let token = self.presentation.token(
            preview.document,
            preview.revision,
            presentation_image(presented.composite),
            mapping,
        );
        ensure!(
            token == Some(presented.token),
            "the Scene presentation token is stale"
        );
        let entity = self.presentation.pick(presented.token, point)?;
        dispatch(
            &mut self.session,
            EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                document: preview.document,
                entity: entity.clone(),
            }),
        )?;
        self.picks += 1;
        Ok(entity)
    }

    pub fn move_sprite(&mut self) -> Result<()> {
        ensure!(!self.closing, "the engine is closing");
        let x = if self.sprite_x()? == 0.0 { 80.0 } else { 0.0 };
        set_field(
            &mut self.session,
            self.document,
            &self.sprite,
            TRANSFORM,
            "translation.x",
            x,
        )?;
        self.invalidate();
        self.moves += 1;
        Ok(())
    }

    pub fn undo(&mut self) -> Result<()> {
        ensure!(!self.closing, "the engine is closing");
        let base_revision = self.revision()?;
        dispatch(
            &mut self.session,
            EditorAction::Mutate(EditorDocumentMutation::Undo {
                document: self.document,
                base_revision,
            }),
        )?;
        self.invalidate();
        self.undos += 1;
        Ok(())
    }

    pub fn redo(&mut self) -> Result<()> {
        ensure!(!self.closing, "the engine is closing");
        let base_revision = self.revision()?;
        dispatch(
            &mut self.session,
            EditorAction::Mutate(EditorDocumentMutation::Redo {
                document: self.document,
                base_revision,
            }),
        )?;
        self.invalidate();
        self.redos += 1;
        Ok(())
    }

    pub fn revision(&self) -> Result<SceneAuthoringRevision> {
        Ok(self.session.document_query_basis(self.document)?.revision)
    }

    pub fn selected_sprite(&self) -> bool {
        self.session
            .current_frame()
            .workspace()
            .active_scene
            .as_ref()
            .is_some_and(|scene| {
                scene.document == self.document
                    && scene.selection.top_entity() == Some(&self.sprite)
            })
    }

    pub fn sprite_x(&self) -> Result<f64> {
        let observation = self.session.read_document_property(EditorPropertyRead {
            target: EditorPropertyTarget {
                basis: self.session.document_query_basis(self.document)?,
                entity: self.sprite.clone(),
                component: ComponentTypeId::new(TRANSFORM),
                property: ComponentPropertyPath::new(ComponentFieldId::new("translation.x")),
            },
            limits: EditorPropertyReadLimits::new(4096, 16, 256)?,
        })?;
        match observation.value {
            EditorPropertyValue::Present(value) | EditorPropertyValue::Defaulted(value) => value
                .as_f64()
                .context("Sprite translation.x is not a scalar"),
            EditorPropertyValue::Missing => bail!("Sprite translation.x is missing"),
        }
    }

    pub fn pulse_other(&mut self) -> Result<WgpuImageOutput> {
        ensure!(!self.closing, "the engine is closing");
        self.host_frame = self
            .host_frame
            .checked_add(1)
            .context("Host transaction identities are exhausted")?;
        self.host.begin_frame(self.host_frame)?;
        let frame = self
            .other_frames
            .checked_add(1)
            .context("control producer frame identities are exhausted")?;
        let target = self.other_target;
        if !self
            .host
            .image_target_matches(&self.other_producer, self.other_source, target)
        {
            let reservation =
                self.host
                    .reserve_image_target(&self.other_producer, self.other_source, target)?;
            self.host
                .activate_image_target(&self.other_producer, reservation)?;
        }
        let viewport = ViewportRect::from_extent(target.extent())
            .context("the control viewport must be nonempty")?;
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            id: self.other_view,
            camera_entity: Entity::PLACEHOLDER,
            target: RenderTarget::Image(target.image()),
            viewport,
            mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, 32.0, viewport)
                .context("the control camera must have a finite mapping")?,
            order: 0,
            load: ViewLoadOp::Clear(if frame % 2 == 0 {
                Color::rgba(0.0, 1.0, 0.0, 1.0)
            } else {
                Color::rgba(0.0, 0.0, 1.0, 1.0)
            }),
        });
        let packet = build_image_render_frame_packet(self.other_source, frame, &views, target)?
            .context("the control view did not produce an image packet")?;
        let input = self
            .other_policy
            .capture(packet, Arc::new(WgpuStockCapture::new()))?;
        let output = self.host.submit(&self.other_producer, &input)?;
        self.other_frames = frame;
        Ok(output)
    }

    pub fn close(&mut self) -> Result<()> {
        if !self.closing {
            self.closing = true;
            self.invalidate();
            for producer in [&self.scene_producer, &self.other_producer] {
                if let Some(prefix) = self.host.retire_producer(producer) {
                    self.retiring.push(prefix);
                }
            }
            for content in &self.content {
                if let Some(prefix) = self.host.retire_producer(&content.producer) {
                    self.retiring.push(prefix);
                }
            }
        }
        let deadline = Instant::now() + TIMEOUT;
        while !self.closed {
            let session_closed = match self.session.advance_close() {
                EditorProjectCloseProgress::Complete(receipt) => {
                    self.cleanup_failed |= receipt.has_cleanup_failure();
                    true
                }
                EditorProjectCloseProgress::Incomplete { .. } => false,
            };
            let mut producers_closed = true;
            for prefix in &self.retiring {
                producers_closed &= prefix.poll()?;
                self.cleanup_failed |= prefix.cleanup_fault_count() != 0;
            }
            let host_closed = self.host.poll_retirement()?;
            self.closed = session_closed && producers_closed && host_closed;
            if self.closed {
                self.cleanup_failed |= self.host.cleanup_fault_count() != 0;
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "engine retirement exceeded ten seconds"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        ensure!(
            !self.cleanup_failed,
            "engine retirement recorded a cleanup failure"
        );
        Ok(())
    }

    pub fn accepts_preview(&self, preview: &Preview) -> bool {
        self.require_current(preview).is_ok()
    }

    fn require_current(&self, preview: &Preview) -> Result<()> {
        ensure!(!self.closing, "the engine is closing");
        let frame = self.session.current_frame();
        ensure!(
            preview.document == self.document
                && frame.workspace().active_document == Some(preview.document)
                && frame.scene_preview_document() == Some(preview.document)
                && self.revision()? == preview.revision
                && self.session.current_editor_preview_content_stamp().as_ref()
                    == Some(&preview.stamp)
                && preview.display.output().is_current()
                && preview.display.receipt().backend_instance_id()
                    == self.host.backend_instance_id()
                && preview.display.receipt().device_epoch() == self.host.device_epoch(),
            "the Scene document, content stamp, output or device is stale"
        );
        Ok(())
    }

    pub fn invalidate(&mut self) {
        self.presentation.retire();
    }
}

fn dispatch(session: &mut EditorProjectSession, action: EditorAction) -> Result<()> {
    let frame = session.current_frame();
    let report = session
        .dispatch_editor_actions(EditorActionBatch::new(frame.revision(), [action]))
        .map_err(|error| anyhow!("Editor action admission failed: {error:?}"))?;
    session.drive_editor_frame(Duration::ZERO);
    match report.outcomes() {
        [EditorActionOutcome::Workspace(report)] if report.applied => Ok(()),
        outcomes => bail!("Editor command did not apply: {outcomes:?}"),
    }
}

fn set_field(
    session: &mut EditorProjectSession,
    document: EditorDocumentId,
    entity: &SceneEntityId,
    component: &str,
    field: &str,
    value: f64,
) -> Result<()> {
    let basis = session.document_query_basis(document)?;
    let prepared = session.prepare_authoring_property_edit(
        &basis,
        &[AuthoringPropertyEdit {
            entity: entity.clone(),
            component: ComponentTypeId::new(component),
            component_version: ComponentSchemaVersion::ONE,
            property: ComponentPropertyPath::new(ComponentFieldId::new(field)),
            value: ComponentValue::f64(value)?,
        }],
    )?;
    dispatch(session, EditorAction::CommitAuthoringEdit(prepared))
}

fn initialize_sprite(
    session: &mut EditorProjectSession,
) -> Result<(EditorDocumentId, SceneEntityId)> {
    let frame = session.current_frame();
    let document = frame
        .workspace()
        .active_document
        .context("the Editor did not open a Scene document")?;
    let basis = session.document_query_basis(document)?;
    let mut draft = session.begin_authoring_construction(&basis)?;
    let entity = draft
        .create_entity(ScenePlacement::last(ScenePlacementParent::Root))
        .map_err(|error| anyhow!("source construction refused placement: {error:?}"))?;
    for component in [TRANSFORM, SPRITE] {
        draft
            .add_component(&entity, ComponentTypeId::new(component))
            .map_err(|error| anyhow!("source construction refused template: {error:?}"))?;
    }
    for (field, value) in [
        ("size.x", 96.0),
        ("size.y", 64.0),
        ("material.tint.r", 1.0),
        ("material.tint.g", 0.0),
        ("material.tint.b", 0.0),
        ("material.tint.a", 1.0),
    ] {
        draft
            .set_property(
                &entity,
                ComponentTypeId::new(SPRITE),
                ComponentPropertyPath::new(ComponentFieldId::new(field)),
                ComponentValue::f64(value)?,
            )
            .map_err(|error| anyhow!("source construction refused a property: {error:?}"))?;
    }
    let prepared = session.prepare_authoring_construction(&basis, &draft)?;
    let committed = session
        .commit_authoring_construction(prepared)
        .map_err(|outcome| anyhow!("source construction did not commit: {outcome:?}"))?;
    let sprite = committed
        .entity(&entity)
        .cloned()
        .context("source owner did not return the original local entity result")?;
    session.drive_editor_frame(Duration::ZERO);
    Ok((document, sprite))
}

fn create_project() -> Result<TempDir> {
    let project = tempfile::Builder::new()
        .prefix("nara-gpui-scene-")
        .tempdir()?;
    for directory in ["assets", "scenes", "prefabs", "schema"] {
        fs::create_dir(project.path().join(directory))?;
    }
    fs::write(
        project.path().join("nara.toml"),
        r#"schema_version = 1
[project]
name = "GPUI Scene consumer"
source_universe = "9c1c168c-a0f5-56b0-bd78-a38a7f3dc0e6"
native_document_identity = "project_location"
[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"
[startup]
default_scene = "startup.scene.json"
[authoring]
state = "ready"
schema_bundle = "schema/authoring.bundle.json"
[runtime]
preset = "local-headless"
[capabilities]
requested = ["runtime-2d"]
"#,
    )?;
    let bundle =
        AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())?;
    fs::write(
        project.path().join("schema/authoring.bundle.json"),
        bundle.to_json_string()?,
    )?;
    fs::write(
        project.path().join("scenes/startup.scene.json"),
        SceneDocument::default().to_json_string()?,
    )?;
    Ok(project)
}

fn open_directory(path: &Path) -> Result<DirectoryCapability> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options
            .share_mode(7)
            .custom_flags(0x0200_0000 | 0x0020_0000);
    }
    Ok(DirectoryCapability::from_host_handle(
        options.open(path)?,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )?)
}

fn close_session(session: &mut EditorProjectSession) -> Result<()> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let EditorProjectCloseProgress::Complete(receipt) = session.advance_close() {
            ensure!(
                !receipt.has_cleanup_failure(),
                "Editor retirement recorded a cleanup failure"
            );
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "Editor retirement exceeded ten seconds"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn presentation_image(composite: WgpuImageCompositeReceipt) -> EditorPresentationImage {
    let image = composite.image();
    EditorPresentationImage {
        output_version: image.output_version(),
        display_occurrence: composite.occurrence().get(),
        target: image.target(),
        target_generation: image.target_generation(),
        source_generation: image.source_generation(),
        source_handoff_generation: image.source_handoff_generation(),
        backend_instance_id: image.backend_instance_id(),
        device_epoch: image.device_epoch(),
        frame_index: image.render_transaction_frame_index(),
    }
}
