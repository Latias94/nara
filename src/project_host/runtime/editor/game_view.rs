use nara_app::{
    __RuntimeDriverPort, App, CoreStage, Plugin, PluginCategory, PluginDeclaration,
    PluginDefinition, PluginDefinitionId, PluginError, PluginId, PluginPlanFingerprint,
    PluginProductCapability, RuntimeGeneration, RuntimeScopeError, RuntimeState,
};
use nara_ecs::{Mut, Resource, World};
use nara_input::{
    ButtonDriverInput, ButtonInputDriverError, InputWindowEvent, KeyCode, MouseButton,
    PointerDriverInput, apply_keyboard_driver_input_at, apply_mouse_driver_input_at,
    apply_pointer_driver_input_at, apply_window_input_driver_event,
};
use nara_project::{ProductCapability, ProductCapabilitySet};
use nara_reflect::SchemaCompositionFingerprint;
use nara_render::{
    Extent2d, ExtractedViews, RenderFrame, RenderFramePacket, RenderFramePacketError,
    RenderFrameTarget, RenderWindowPacket, build_render_frame_packet,
};
use nara_scene::SceneAuthoringRevision;
#[cfg(feature = "presentation-2d")]
use nara_sprite_render::SpriteBatches;
#[cfg(feature = "presentation-ui")]
use nara_ui_render::__private::{
    UiBackendPresentationFrame, UiCompletedBackendOutcome, commit_ui_backend_outcomes,
    take_ui_backend_presentation_frame,
};
#[cfg(feature = "presentation-ui")]
use nara_ui_render::UiPresentationIntegrationError;
use nara_window::{PresentMode, PrimaryWindowId, Window, WindowEvent, push_window_event};
use thiserror::Error;

use crate::project_host::{ProjectContentRevision, ProjectSettingsLineage};
use nara_tooling::EditorDocumentId;

const EDITOR_GAME_VIEW_BRIDGE_PLUGIN_ID: PluginId = PluginId::new("nara.editor-game-view-bridge");
const EDITOR_GAME_VIEW_BRIDGE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.editor-game-view-bridge", 1);
const EDITOR_GAME_VIEW_BRIDGE_REQUIREMENTS: &[PluginId] =
    &[nara_render::RENDER_PLUGIN_ID, nara_window::WINDOW_PLUGIN_ID];
const EDITOR_GAME_VIEW_BRIDGE_PRODUCT_REQUIREMENTS: &[PluginProductCapability] = &[
    PluginProductCapability::new("desktop-winit"),
    PluginProductCapability::new("render-wgpu"),
];
const EDITOR_GAME_VIEW_BRIDGE_PRODUCT_ALTERNATIVES: &[PluginProductCapability] = &[
    PluginProductCapability::new("presentation-2d"),
    PluginProductCapability::new("presentation-ui"),
];
const EDITOR_GAME_VIEW_BRIDGE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(EDITOR_GAME_VIEW_BRIDGE_PLUGIN_ID, PluginCategory::Tooling)
        .requires_plugins(EDITOR_GAME_VIEW_BRIDGE_REQUIREMENTS)
        .requires_product_capabilities(EDITOR_GAME_VIEW_BRIDGE_PRODUCT_REQUIREMENTS)
        .requires_any_product_capabilities(EDITOR_GAME_VIEW_BRIDGE_PRODUCT_ALTERNATIVES);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct EditorGameViewCaptureConfig {
    logical_window: RenderWindowPacket,
    resizable: bool,
    sprites: bool,
    ui: bool,
}

impl EditorGameViewCaptureConfig {
    fn from_profile(capabilities: ProductCapabilitySet, window: Option<Window>) -> Option<Self> {
        let available = capabilities.contains(ProductCapability::DesktopWinit)
            && capabilities.contains(ProductCapability::RenderWgpu)
            && (capabilities.contains(ProductCapability::Presentation2d)
                || capabilities.contains(ProductCapability::PresentationUi));
        let window = available.then_some(window).flatten()?;
        Some(Self {
            logical_window: RenderWindowPacket {
                id: window.id,
                resolution: window.resolution,
                present_mode: window.present_mode,
            },
            resizable: window.resizable,
            sprites: capabilities.contains(ProductCapability::Presentation2d),
            ui: capabilities.contains(ProductCapability::PresentationUi),
        })
    }

    fn canonical_configuration(self) -> Vec<u8> {
        let mut encoded = Vec::with_capacity(48);
        encoded.extend_from_slice(&self.logical_window.id.get().to_le_bytes());
        encoded.extend_from_slice(&self.logical_window.resolution.physical_width.to_le_bytes());
        encoded.extend_from_slice(&self.logical_window.resolution.physical_height.to_le_bytes());
        encoded.extend_from_slice(
            &self
                .logical_window
                .resolution
                .scale_factor
                .to_bits()
                .to_le_bytes(),
        );
        let safe_area = self.logical_window.resolution.safe_area_insets;
        for inset in [
            safe_area.left,
            safe_area.top,
            safe_area.right,
            safe_area.bottom,
        ] {
            encoded.extend_from_slice(&inset.to_bits().to_le_bytes());
        }
        encoded.push(match self.logical_window.present_mode {
            PresentMode::AutoVsync => 0,
            PresentMode::AutoNoVsync => 1,
            PresentMode::Fifo => 2,
            PresentMode::Immediate => 3,
            PresentMode::Mailbox => 4,
        });
        encoded.extend([
            u8::from(self.resizable),
            u8::from(self.sprites),
            u8::from(self.ui),
        ]);
        encoded
    }
}

#[derive(Debug, Clone, Copy)]
struct EditorGameViewBridgePlugin {
    config: EditorGameViewCaptureConfig,
}

impl Plugin for EditorGameViewBridgePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &EDITOR_GAME_VIEW_BRIDGE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(EditorGameViewBridge::new(self.config))?
            .add_systems(CoreStage::Last, capture_editor_game_view_candidate)?;
        Ok(())
    }
}

pub(super) fn editor_game_view_bridge_definition(
    capabilities: ProductCapabilitySet,
    window: Option<Window>,
) -> Option<(EditorGameViewCaptureConfig, PluginDefinition)> {
    let config = EditorGameViewCaptureConfig::from_profile(capabilities, window)?;
    let canonical_configuration = config.canonical_configuration();
    let definition = PluginDefinition::infallible::<EditorGameViewBridgePlugin, _>(
        EDITOR_GAME_VIEW_BRIDGE_DEFINITION_ID,
        canonical_configuration,
        move || EditorGameViewBridgePlugin { config },
    );
    Some((config, definition))
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorGameStartRequest {
    document: EditorDocumentId,
    source_revision: SceneAuthoringRevision,
    project_lineage: ProjectSettingsLineage,
    content_revision: ProjectContentRevision,
    schema_fingerprint: SchemaCompositionFingerprint,
    plugin_plan_fingerprint: PluginPlanFingerprint,
    runtime_generation: RuntimeGeneration,
    capture: EditorGameViewCaptureConfig,
    geometry_generation: u64,
}

impl EditorGameStartRequest {
    #[must_use]
    pub const fn document(self) -> EditorDocumentId {
        self.document
    }

    #[must_use]
    pub const fn source_revision(self) -> SceneAuthoringRevision {
        self.source_revision
    }

    #[must_use]
    pub const fn content_revision(self) -> ProjectContentRevision {
        self.content_revision
    }

    #[must_use]
    pub const fn runtime_generation(self) -> RuntimeGeneration {
        self.runtime_generation
    }

    #[must_use]
    pub const fn logical_window(self) -> RenderWindowPacket {
        self.capture.logical_window
    }

    #[must_use]
    pub const fn geometry_generation(self) -> u64 {
        self.geometry_generation
    }

    #[must_use]
    pub const fn resizable(self) -> bool {
        self.capture.resizable
    }

    #[must_use]
    pub const fn captures_sprites(self) -> bool {
        self.capture.sprites
    }

    #[must_use]
    pub const fn captures_runtime_ui(self) -> bool {
        self.capture.ui
    }

    pub(super) fn resized(self, extent: Extent2d) -> Result<Self, EditorGameViewBridgeFailure> {
        if !self.resizable() {
            return Ok(self);
        }
        let geometry_generation = self
            .geometry_generation
            .checked_add(1)
            .ok_or(EditorGameViewBridgeFailure::GeometryGenerationExhausted)?;
        let mut capture = self.capture;
        capture.logical_window.resolution.physical_width = extent.width;
        capture.logical_window.resolution.physical_height = extent.height;
        Ok(Self {
            capture,
            geometry_generation,
            ..self
        })
    }

    pub(super) const fn capture_config(self) -> EditorGameViewCaptureConfig {
        self.capture
    }
}

/// Move-only authorization for the coordinated native-target and Runtime start commit.
#[doc(hidden)]
#[derive(Debug)]
pub struct EditorGameStartActivation {
    request: EditorGameStartRequest,
}

impl EditorGameStartActivation {
    pub(super) const fn new(request: EditorGameStartRequest) -> Self {
        Self { request }
    }

    #[must_use]
    pub const fn request(&self) -> EditorGameStartRequest {
        self.request
    }

    pub(super) const fn into_request(self) -> EditorGameStartRequest {
        self.request
    }
}

pub(super) fn start_request(
    config: EditorGameViewCaptureConfig,
    document: EditorDocumentId,
    source_revision: SceneAuthoringRevision,
    prepared: &super::super::PreparedRuntimeStart,
) -> EditorGameStartRequest {
    EditorGameStartRequest {
        document,
        source_revision,
        project_lineage: prepared.plan.lineage(),
        content_revision: prepared.snapshot.revision(),
        schema_fingerprint: prepared.snapshot.schema_fingerprint(),
        plugin_plan_fingerprint: prepared.plan.plugin_plan().fingerprint(),
        runtime_generation: prepared.generation(),
        capture: config,
        geometry_generation: 1,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct EditorGameViewRuntimeCandidateIdentity {
    runtime_generation: RuntimeGeneration,
    frame_index: u64,
    sequence: u64,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorGameViewCandidateIdentity {
    start: EditorGameStartRequest,
    runtime: EditorGameViewRuntimeCandidateIdentity,
}

impl EditorGameViewCandidateIdentity {
    #[must_use]
    pub const fn runtime_generation(self) -> RuntimeGeneration {
        self.runtime.runtime_generation
    }

    #[must_use]
    pub const fn start(self) -> EditorGameStartRequest {
        self.start
    }

    #[must_use]
    pub const fn frame_index(self) -> u64 {
        self.runtime.frame_index
    }
}

/// One physical Game View event retained by the private Editor Host until the next Runtime drive.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EditorGameInputEvent {
    Focused(bool),
    SurfaceRetired,
    Pointer(PointerDriverInput),
    Mouse(ButtonDriverInput<MouseButton>),
    Keyboard(ButtonDriverInput<KeyCode>),
}

/// Exact-generation-bound Game View input batch.
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct EditorGameInputBatch {
    events: Vec<EditorGameInputEnvelope>,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorGameInputEnvelope {
    identity: EditorGameViewCandidateIdentity,
    route_generation: u64,
    event: EditorGameInputEvent,
}

impl EditorGameInputEnvelope {
    #[must_use]
    pub const fn new(
        identity: EditorGameViewCandidateIdentity,
        route_generation: u64,
        event: EditorGameInputEvent,
    ) -> Self {
        Self {
            identity,
            route_generation,
            event,
        }
    }

    #[must_use]
    pub const fn identity(self) -> EditorGameViewCandidateIdentity {
        self.identity
    }

    #[must_use]
    pub const fn route_generation(self) -> u64 {
        self.route_generation
    }

    #[must_use]
    pub const fn event(self) -> EditorGameInputEvent {
        self.event
    }
}

impl EditorGameInputBatch {
    /// Maximum number of native events admitted by one Editor redraw transaction.
    pub const MAX_EVENTS: usize = 256;

    #[must_use]
    pub fn new(events: impl IntoIterator<Item = EditorGameInputEnvelope>) -> Self {
        Self {
            events: events.into_iter().collect(),
        }
    }

    #[must_use]
    pub fn into_events(self) -> Vec<EditorGameInputEnvelope> {
        self.events
    }
}

#[derive(Debug)]
pub(super) struct PendingEditorGameViewCandidate {
    identity: EditorGameViewRuntimeCandidateIdentity,
    packet: RenderFramePacket,
    #[cfg(feature = "presentation-2d")]
    sprite_batches: Option<SpriteBatches>,
    #[cfg(feature = "presentation-ui")]
    ui_frame: Option<UiBackendPresentationFrame>,
}

impl PendingEditorGameViewCandidate {
    pub(super) fn bind_start(self, start: EditorGameStartRequest) -> EditorGameViewCandidate {
        debug_assert_eq!(self.identity.runtime_generation, start.runtime_generation);
        debug_assert_eq!(
            self.packet.target(),
            RenderFrameTarget::Window(start.logical_window())
        );
        EditorGameViewCandidate {
            identity: EditorGameViewCandidateIdentity {
                start,
                runtime: self.identity,
            },
            packet: self.packet,
            #[cfg(feature = "presentation-2d")]
            sprite_batches: self.sprite_batches,
            #[cfg(feature = "presentation-ui")]
            ui_frame: self.ui_frame,
        }
    }
}

#[doc(hidden)]
#[derive(Debug)]
pub struct EditorGameViewCandidate {
    identity: EditorGameViewCandidateIdentity,
    packet: RenderFramePacket,
    #[cfg(feature = "presentation-2d")]
    sprite_batches: Option<SpriteBatches>,
    #[cfg(feature = "presentation-ui")]
    ui_frame: Option<UiBackendPresentationFrame>,
}

impl EditorGameViewCandidate {
    #[must_use]
    pub const fn identity(&self) -> EditorGameViewCandidateIdentity {
        self.identity
    }

    #[must_use]
    pub const fn packet(&self) -> &RenderFramePacket {
        &self.packet
    }

    #[must_use]
    pub fn into_parts(self) -> EditorGameViewCandidateParts {
        EditorGameViewCandidateParts {
            identity: self.identity,
            packet: self.packet,
            #[cfg(feature = "presentation-2d")]
            sprite_batches: self.sprite_batches,
            #[cfg(feature = "presentation-ui")]
            ui_frame: self.ui_frame,
        }
    }

    #[must_use]
    pub fn content_rejected(self, message: impl Into<String>) -> EditorGameViewCompletion {
        let message = message.into();
        #[cfg(not(feature = "presentation-ui"))]
        let _ = &message;
        EditorGameViewCompletion {
            identity: self.identity,
            disposition: EditorGameViewCompletionDisposition::ContentRejected,
            #[cfg(feature = "presentation-ui")]
            outcomes: self
                .ui_frame
                .map(|frame| {
                    frame
                        .into_parts()
                        .1
                        .into_iter()
                        .map(|submission| submission.content_rejected(message.clone()))
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

#[doc(hidden)]
#[derive(Debug)]
pub struct EditorGameViewCandidateParts {
    identity: EditorGameViewCandidateIdentity,
    packet: RenderFramePacket,
    #[cfg(feature = "presentation-2d")]
    sprite_batches: Option<SpriteBatches>,
    #[cfg(feature = "presentation-ui")]
    ui_frame: Option<UiBackendPresentationFrame>,
}

impl EditorGameViewCandidateParts {
    #[must_use]
    pub const fn identity(&self) -> EditorGameViewCandidateIdentity {
        self.identity
    }

    #[must_use]
    pub const fn packet(&self) -> &RenderFramePacket {
        &self.packet
    }

    #[cfg(feature = "presentation-2d")]
    #[must_use]
    pub fn take_sprite_batches(&mut self) -> Option<SpriteBatches> {
        self.sprite_batches.take()
    }

    #[cfg(feature = "presentation-ui")]
    #[must_use]
    pub fn take_ui_frame(&mut self) -> Option<UiBackendPresentationFrame> {
        self.ui_frame.take()
    }

    #[cfg(all(feature = "presentation-2d", feature = "presentation-ui"))]
    #[must_use]
    pub fn into_submission_parts(
        self,
    ) -> (
        EditorGameViewCandidateIdentity,
        RenderFramePacket,
        Option<SpriteBatches>,
        Option<UiBackendPresentationFrame>,
    ) {
        (
            self.identity,
            self.packet,
            self.sprite_batches,
            self.ui_frame,
        )
    }
}

#[doc(hidden)]
#[derive(Debug)]
pub struct EditorGameViewCompletion {
    identity: EditorGameViewCandidateIdentity,
    disposition: EditorGameViewCompletionDisposition,
    #[cfg(feature = "presentation-ui")]
    outcomes: Vec<UiCompletedBackendOutcome>,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorGameViewCompletionDisposition {
    Presented,
    Deferred,
    ContentRejected,
    Failed,
}

impl EditorGameViewCompletion {
    #[must_use]
    pub const fn identity(&self) -> EditorGameViewCandidateIdentity {
        self.identity
    }

    #[must_use]
    pub const fn disposition(&self) -> EditorGameViewCompletionDisposition {
        self.disposition
    }

    #[must_use]
    pub const fn without_ui(identity: EditorGameViewCandidateIdentity) -> Self {
        Self::without_ui_with_disposition(identity, EditorGameViewCompletionDisposition::Presented)
    }

    #[must_use]
    pub const fn without_ui_deferred(identity: EditorGameViewCandidateIdentity) -> Self {
        Self::without_ui_with_disposition(identity, EditorGameViewCompletionDisposition::Deferred)
    }

    #[must_use]
    pub const fn without_ui_content_rejected(identity: EditorGameViewCandidateIdentity) -> Self {
        Self::without_ui_with_disposition(
            identity,
            EditorGameViewCompletionDisposition::ContentRejected,
        )
    }

    #[must_use]
    pub const fn without_ui_failed(identity: EditorGameViewCandidateIdentity) -> Self {
        Self::without_ui_with_disposition(identity, EditorGameViewCompletionDisposition::Failed)
    }

    const fn without_ui_with_disposition(
        identity: EditorGameViewCandidateIdentity,
        disposition: EditorGameViewCompletionDisposition,
    ) -> Self {
        Self {
            identity,
            disposition,
            #[cfg(feature = "presentation-ui")]
            outcomes: Vec::new(),
        }
    }

    #[cfg(feature = "presentation-ui")]
    #[must_use]
    pub fn with_ui(
        identity: EditorGameViewCandidateIdentity,
        outcomes: Vec<UiCompletedBackendOutcome>,
    ) -> Self {
        Self::with_ui_disposition(
            identity,
            EditorGameViewCompletionDisposition::Presented,
            outcomes,
        )
    }

    #[cfg(feature = "presentation-ui")]
    #[must_use]
    pub fn with_ui_deferred(
        identity: EditorGameViewCandidateIdentity,
        outcomes: Vec<UiCompletedBackendOutcome>,
    ) -> Self {
        Self::with_ui_disposition(
            identity,
            EditorGameViewCompletionDisposition::Deferred,
            outcomes,
        )
    }

    #[cfg(feature = "presentation-ui")]
    #[must_use]
    pub fn with_ui_content_rejected(
        identity: EditorGameViewCandidateIdentity,
        outcomes: Vec<UiCompletedBackendOutcome>,
    ) -> Self {
        Self::with_ui_disposition(
            identity,
            EditorGameViewCompletionDisposition::ContentRejected,
            outcomes,
        )
    }

    #[cfg(feature = "presentation-ui")]
    #[must_use]
    pub fn with_ui_failed(
        identity: EditorGameViewCandidateIdentity,
        outcomes: Vec<UiCompletedBackendOutcome>,
    ) -> Self {
        Self::with_ui_disposition(
            identity,
            EditorGameViewCompletionDisposition::Failed,
            outcomes,
        )
    }

    #[cfg(feature = "presentation-ui")]
    fn with_ui_disposition(
        identity: EditorGameViewCandidateIdentity,
        disposition: EditorGameViewCompletionDisposition,
        outcomes: Vec<UiCompletedBackendOutcome>,
    ) -> Self {
        Self {
            identity,
            disposition,
            outcomes,
        }
    }
}

#[doc(hidden)]
#[derive(Clone, Debug, Error, PartialEq)]
pub enum EditorGameViewBridgeFailure {
    #[error("a Game View candidate is already owned by the Host")]
    CandidateBusy,
    #[error("the Game View candidate sequence was exhausted")]
    CandidateSequenceExhausted,
    #[error("the Game View geometry generation was exhausted")]
    GeometryGenerationExhausted,
    #[error("the Runtime candidate does not match the configured logical Game window")]
    CandidateSourceMismatch,
    #[error("the Game View completion does not match the in-flight candidate")]
    CompletionIdentityMismatch,
    #[error("the Runtime rejected the returned Game View completion authority")]
    CompletionAuthorityMismatch,
    #[error("no Game View start is waiting for native target preparation")]
    GameStartNotPending,
    #[error("the native Game View start does not match the prepared Runtime")]
    GameStartRequestMismatch,
    #[error("the Runtime did not publish a render frame")]
    MissingRenderFrame,
    #[error("the Runtime generation is unavailable")]
    MissingRuntimeGeneration,
    #[error("the logical primary window is unavailable")]
    MissingPrimaryWindow,
    #[error("the Runtime did not publish sprite batches")]
    MissingSpriteBatches,
    #[error("the Game View bridge accepts only logical window targets")]
    NonWindowTarget,
    #[error("the managed Play Runtime is unavailable")]
    RuntimeUnavailable,
    #[error("the Game View input batch mixes incompatible capture routes")]
    InputRouteMismatch,
    #[error("the Game View input batch exceeded its bounded event budget")]
    InputBudgetExceeded,
    #[error("the managed Play Runtime rejected Game View input event {event_index}: {source}")]
    InputEventRejected {
        event_index: usize,
        #[source]
        source: ButtonInputDriverError,
    },
    #[error("the managed Play Runtime rejected the Game View input scope: {source}")]
    InputScopeRejected {
        #[source]
        source: RuntimeScopeError,
    },
    #[error("the Runtime render frame was rejected: {0}")]
    Packet(RenderFramePacketError),
    #[cfg(feature = "presentation-ui")]
    #[error("the Runtime UI presentation frame was rejected: {0}")]
    Ui(UiPresentationIntegrationError),
}

#[derive(Debug)]
enum EditorGameViewBridgeSlot {
    Empty,
    Buffered(PendingEditorGameViewCandidate),
    InFlight(EditorGameViewRuntimeCandidateIdentity),
}

#[derive(Debug, Resource)]
struct EditorGameViewBridge {
    config: EditorGameViewCaptureConfig,
    capture_enabled: bool,
    next_sequence: u64,
    slot: EditorGameViewBridgeSlot,
    failure: Option<EditorGameViewBridgeFailure>,
}

impl EditorGameViewBridge {
    const fn new(config: EditorGameViewCaptureConfig) -> Self {
        Self {
            config,
            capture_enabled: true,
            next_sequence: 1,
            slot: EditorGameViewBridgeSlot::Empty,
            failure: None,
        }
    }

    fn can_capture(&self) -> bool {
        self.capture_enabled && matches!(self.slot, EditorGameViewBridgeSlot::Empty)
    }

    fn is_idle(&self) -> bool {
        matches!(self.slot, EditorGameViewBridgeSlot::Empty)
    }

    fn can_issue_sequence(&self) -> bool {
        self.next_sequence.checked_add(1).is_some()
    }

    fn publish(
        &mut self,
        captured: CapturedEditorGameViewCandidate,
    ) -> Result<(), EditorGameViewBridgeFailure> {
        if !self.can_capture() {
            return Err(EditorGameViewBridgeFailure::CandidateBusy);
        }
        let sequence = self.next_sequence;
        self.next_sequence = sequence
            .checked_add(1)
            .expect("capture rejects before exhausting the candidate sequence");
        let identity = EditorGameViewRuntimeCandidateIdentity {
            runtime_generation: captured.runtime_generation,
            frame_index: captured.packet.frame_index(),
            sequence,
        };
        self.slot = EditorGameViewBridgeSlot::Buffered(PendingEditorGameViewCandidate {
            identity,
            packet: captured.packet,
            #[cfg(feature = "presentation-2d")]
            sprite_batches: captured.sprite_batches,
            #[cfg(feature = "presentation-ui")]
            ui_frame: captured.ui_frame,
        });
        Ok(())
    }

    fn take_candidate(&mut self) -> EditorGameViewBridgeOutput {
        if matches!(self.slot, EditorGameViewBridgeSlot::InFlight(_)) {
            return EditorGameViewBridgeOutput::Busy;
        }
        if let Some(failure) = self.failure.take() {
            return EditorGameViewBridgeOutput::Failed(failure);
        }
        let slot = std::mem::replace(&mut self.slot, EditorGameViewBridgeSlot::Empty);
        let EditorGameViewBridgeSlot::Buffered(candidate) = slot else {
            debug_assert!(matches!(slot, EditorGameViewBridgeSlot::Empty));
            return EditorGameViewBridgeOutput::Pending;
        };
        self.slot = EditorGameViewBridgeSlot::InFlight(candidate.identity);
        EditorGameViewBridgeOutput::Candidate(candidate)
    }

    fn accepts_completion(&mut self, identity: EditorGameViewRuntimeCandidateIdentity) -> bool {
        if !matches!(self.slot, EditorGameViewBridgeSlot::InFlight(actual) if actual == identity) {
            self.failure = Some(EditorGameViewBridgeFailure::CompletionIdentityMismatch);
            return false;
        }
        true
    }

    fn finish_completion(&mut self, identity: EditorGameViewRuntimeCandidateIdentity) {
        debug_assert!(matches!(
            self.slot,
            EditorGameViewBridgeSlot::InFlight(actual) if actual == identity
        ));
        self.slot = EditorGameViewBridgeSlot::Empty;
    }

    fn set_capture_enabled(&mut self, enabled: bool) {
        self.capture_enabled = enabled;
    }

    fn reconfigure(
        &mut self,
        config: EditorGameViewCaptureConfig,
    ) -> Result<(), EditorGameViewBridgeFailure> {
        if !self.is_idle() {
            return Err(EditorGameViewBridgeFailure::CandidateBusy);
        }
        self.config = config;
        Ok(())
    }
}

enum EditorGameViewBridgeInput {
    TakeCandidate,
    RetirementBlocked,
    SetCaptureEnabled(bool),
    Reconfigure(EditorGameViewCaptureConfig),
    Complete(EditorGameViewCompletion),
}

enum EditorGameViewBridgeOutput {
    Candidate(PendingEditorGameViewCandidate),
    Pending,
    Busy,
    RetirementBlocked(bool),
    Updated(Result<(), EditorGameViewBridgeFailure>),
    Completed(Result<(), EditorGameViewCompletion>),
    Failed(EditorGameViewBridgeFailure),
}

impl __RuntimeDriverPort for EditorGameViewBridge {
    type Input = EditorGameViewBridgeInput;
    type Output = EditorGameViewBridgeOutput;

    fn accepts_driver_input(state: RuntimeState, input: &Self::Input) -> bool {
        match input {
            EditorGameViewBridgeInput::TakeCandidate => {
                matches!(
                    state,
                    RuntimeState::Running
                        | RuntimeState::Paused
                        | RuntimeState::Faulted
                        | RuntimeState::Stopping
                        | RuntimeState::CloseIncomplete
                )
            }
            EditorGameViewBridgeInput::RetirementBlocked => !matches!(state, RuntimeState::Stopped),
            EditorGameViewBridgeInput::SetCaptureEnabled(_) => {
                !matches!(state, RuntimeState::Stopped)
            }
            EditorGameViewBridgeInput::Reconfigure(_) => {
                matches!(state, RuntimeState::Running | RuntimeState::Paused)
            }
            EditorGameViewBridgeInput::Complete(_) => matches!(
                state,
                RuntimeState::Running
                    | RuntimeState::Paused
                    | RuntimeState::Faulted
                    | RuntimeState::Stopping
                    | RuntimeState::CloseIncomplete
            ),
        }
    }

    fn apply_driver_input(&mut self, input: Self::Input) -> Self::Output {
        match input {
            EditorGameViewBridgeInput::TakeCandidate => self.take_candidate(),
            EditorGameViewBridgeInput::RetirementBlocked => {
                EditorGameViewBridgeOutput::RetirementBlocked(!self.is_idle())
            }
            EditorGameViewBridgeInput::SetCaptureEnabled(enabled) => {
                self.set_capture_enabled(enabled);
                EditorGameViewBridgeOutput::Updated(Ok(()))
            }
            EditorGameViewBridgeInput::Reconfigure(config) => {
                EditorGameViewBridgeOutput::Updated(self.reconfigure(config))
            }
            EditorGameViewBridgeInput::Complete(completion) => {
                EditorGameViewBridgeOutput::Completed(Err(completion))
            }
        }
    }

    fn apply_driver_input_in_world(
        world: &mut World,
        input: Self::Input,
    ) -> Result<Self::Output, (nara_app::RuntimeWorldAccessError, Self::Input)> {
        if !world.contains_resource::<Self>() {
            return Err((
                nara_app::RuntimeWorldAccessError::MissingResource {
                    type_name: std::any::type_name::<Self>(),
                },
                input,
            ));
        }
        Ok(
            world.resource_scope(|world, mut bridge: Mut<'_, EditorGameViewBridge>| {
                #[cfg(not(feature = "presentation-ui"))]
                let _ = &world;
                match input {
                    EditorGameViewBridgeInput::Complete(completion) => {
                        let runtime_identity = completion.identity.runtime;
                        if !bridge.accepts_completion(runtime_identity) {
                            return EditorGameViewBridgeOutput::Completed(Err(completion));
                        }

                        #[cfg(feature = "presentation-ui")]
                        {
                            let EditorGameViewCompletion {
                                identity: candidate_identity,
                                disposition,
                                outcomes,
                            } = completion;
                            if !outcomes.is_empty() {
                                if let Err(outcomes) = commit_ui_backend_outcomes(world, outcomes) {
                                    bridge.failure = Some(
                                        EditorGameViewBridgeFailure::CompletionAuthorityMismatch,
                                    );
                                    return EditorGameViewBridgeOutput::Completed(Err(
                                        EditorGameViewCompletion {
                                            identity: candidate_identity,
                                            disposition,
                                            outcomes,
                                        },
                                    ));
                                }
                            }
                            if world
                                .contains_resource::<nara_ui_render::__private::UiPresentationOutcomes>()
                            {
                                crate::ui_presentation::commit_runtime_ui_presentations(world);
                            }
                        }
                        #[cfg(not(feature = "presentation-ui"))]
                        let _ = completion;

                        bridge.finish_completion(runtime_identity);
                        EditorGameViewBridgeOutput::Completed(Ok(()))
                    }
                    EditorGameViewBridgeInput::Reconfigure(config) => {
                        let result = bridge.reconfigure(config);
                        if result.is_ok() {
                            push_window_event(
                                world,
                                WindowEvent::Resized {
                                    window_id: config.logical_window.id,
                                    resolution: config.logical_window.resolution,
                                },
                            );
                        }
                        EditorGameViewBridgeOutput::Updated(result)
                    }
                    input => bridge.apply_driver_input(input),
                }
            }),
        )
    }
}

struct CapturedEditorGameViewCandidate {
    runtime_generation: RuntimeGeneration,
    packet: RenderFramePacket,
    #[cfg(feature = "presentation-2d")]
    sprite_batches: Option<SpriteBatches>,
    #[cfg(feature = "presentation-ui")]
    ui_frame: Option<UiBackendPresentationFrame>,
}

fn capture_editor_game_view_candidate(world: &mut World) {
    let Some(bridge) = world.get_resource::<EditorGameViewBridge>() else {
        return;
    };
    if !bridge.can_capture() {
        return;
    }
    if !bridge.can_issue_sequence() {
        world.resource_mut::<EditorGameViewBridge>().failure =
            Some(EditorGameViewBridgeFailure::CandidateSequenceExhausted);
        return;
    }
    let config = bridge.config;
    let captured = capture_candidate(world, config);
    let mut bridge = world.resource_mut::<EditorGameViewBridge>();
    match captured {
        Ok(Some(candidate)) => {
            if let Err(failure) = bridge.publish(candidate) {
                bridge.failure = Some(failure);
            }
        }
        Ok(None) => {}
        Err(failure) => bridge.failure = Some(failure),
    }
}

fn capture_candidate(
    world: &mut World,
    config: EditorGameViewCaptureConfig,
) -> Result<Option<CapturedEditorGameViewCandidate>, EditorGameViewBridgeFailure> {
    let runtime_generation = world
        .get_resource::<RuntimeGeneration>()
        .copied()
        .ok_or(EditorGameViewBridgeFailure::MissingRuntimeGeneration)?;
    let frame_index = world
        .get_resource::<RenderFrame>()
        .map(|frame| frame.index)
        .ok_or(EditorGameViewBridgeFailure::MissingRenderFrame)?;
    let primary_window_id = world
        .get_resource::<PrimaryWindowId>()
        .map(|primary| primary.0)
        .ok_or(EditorGameViewBridgeFailure::MissingPrimaryWindow)?;
    let packet = {
        let mut query = world.query::<&Window>();
        build_render_frame_packet(
            runtime_generation,
            frame_index,
            Some(primary_window_id),
            query.iter(world),
            world.resource::<ExtractedViews>(),
        )
        .map_err(EditorGameViewBridgeFailure::Packet)?
    };
    let Some(packet) = packet else {
        return Ok(None);
    };
    if !matches!(packet.target(), RenderFrameTarget::Window(_)) {
        return Err(EditorGameViewBridgeFailure::NonWindowTarget);
    }
    if packet.target() != RenderFrameTarget::Window(config.logical_window) {
        return Err(EditorGameViewBridgeFailure::CandidateSourceMismatch);
    }

    #[cfg(feature = "presentation-2d")]
    let sprite_batches = if config.sprites {
        Some(
            world
                .get_resource::<SpriteBatches>()
                .cloned()
                .ok_or(EditorGameViewBridgeFailure::MissingSpriteBatches)?,
        )
    } else {
        None
    };
    #[cfg(not(feature = "presentation-2d"))]
    let _ = config.sprites;

    #[cfg(feature = "presentation-ui")]
    let ui_frame = if config.ui {
        Some(take_ui_backend_presentation_frame(world).map_err(EditorGameViewBridgeFailure::Ui)?)
    } else {
        None
    };
    #[cfg(not(feature = "presentation-ui"))]
    let _ = config.ui;

    Ok(Some(CapturedEditorGameViewCandidate {
        runtime_generation,
        packet,
        #[cfg(feature = "presentation-2d")]
        sprite_batches,
        #[cfg(feature = "presentation-ui")]
        ui_frame,
    }))
}

pub(super) fn take_candidate(
    runtime: &mut nara_app::RuntimeInstance,
) -> Result<Option<PendingEditorGameViewCandidate>, EditorGameViewBridgeFailure> {
    let output = runtime
        .with_driver_scope(|scope| {
            scope.__apply_port::<EditorGameViewBridge>(EditorGameViewBridgeInput::TakeCandidate)
        })
        .map_err(|_| EditorGameViewBridgeFailure::CandidateBusy)?
        .map_err(|_| EditorGameViewBridgeFailure::CandidateBusy)?;
    match output {
        EditorGameViewBridgeOutput::Candidate(candidate) => Ok(Some(candidate)),
        EditorGameViewBridgeOutput::Pending => Ok(None),
        EditorGameViewBridgeOutput::Busy => Err(EditorGameViewBridgeFailure::CandidateBusy),
        EditorGameViewBridgeOutput::Failed(failure) => Err(failure),
        EditorGameViewBridgeOutput::Completed(_)
        | EditorGameViewBridgeOutput::Updated(_)
        | EditorGameViewBridgeOutput::RetirementBlocked(_) => {
            unreachable!("taking a Game View candidate cannot complete one")
        }
    }
}

pub(super) fn set_capture_enabled(
    runtime: &mut nara_app::RuntimeInstance,
    enabled: bool,
) -> Result<(), EditorGameViewBridgeFailure> {
    let output = runtime
        .with_driver_scope(|scope| {
            scope.__apply_port::<EditorGameViewBridge>(
                EditorGameViewBridgeInput::SetCaptureEnabled(enabled),
            )
        })
        .map_err(|_| EditorGameViewBridgeFailure::RuntimeUnavailable)?
        .map_err(|_| EditorGameViewBridgeFailure::RuntimeUnavailable)?;
    match output {
        EditorGameViewBridgeOutput::Updated(result) => result,
        EditorGameViewBridgeOutput::Candidate(_)
        | EditorGameViewBridgeOutput::Pending
        | EditorGameViewBridgeOutput::Busy
        | EditorGameViewBridgeOutput::RetirementBlocked(_)
        | EditorGameViewBridgeOutput::Completed(_)
        | EditorGameViewBridgeOutput::Failed(_) => {
            unreachable!("the typed capture-state port returns an update outcome")
        }
    }
}

pub(super) fn reconfigure(
    runtime: &mut nara_app::RuntimeInstance,
    config: EditorGameViewCaptureConfig,
) -> Result<(), EditorGameViewBridgeFailure> {
    let output = runtime
        .with_driver_scope(|scope| {
            scope.__apply_port::<EditorGameViewBridge>(EditorGameViewBridgeInput::Reconfigure(
                config,
            ))
        })
        .map_err(|_| EditorGameViewBridgeFailure::RuntimeUnavailable)?
        .map_err(|_| EditorGameViewBridgeFailure::RuntimeUnavailable)?;
    match output {
        EditorGameViewBridgeOutput::Updated(result) => result,
        EditorGameViewBridgeOutput::Candidate(_)
        | EditorGameViewBridgeOutput::Pending
        | EditorGameViewBridgeOutput::Busy
        | EditorGameViewBridgeOutput::RetirementBlocked(_)
        | EditorGameViewBridgeOutput::Completed(_)
        | EditorGameViewBridgeOutput::Failed(_) => {
            unreachable!("the typed Game View reconfiguration port returns an update outcome")
        }
    }
}

pub(super) fn retirement_blocked(runtime: &mut nara_app::RuntimeInstance) -> bool {
    matches!(
        runtime.with_driver_scope(|scope| {
            scope.__apply_port::<EditorGameViewBridge>(EditorGameViewBridgeInput::RetirementBlocked)
        }),
        Ok(Ok(EditorGameViewBridgeOutput::RetirementBlocked(true)))
    )
}

pub(super) fn complete_candidate(
    runtime: &mut nara_app::RuntimeInstance,
    completion: EditorGameViewCompletion,
) -> Result<(), EditorGameViewCompletion> {
    let mut owned = Some(completion);
    let output = runtime.with_driver_scope(|scope| {
        let input = EditorGameViewBridgeInput::Complete(
            owned
                .take()
                .expect("the completion enters one typed Runtime scope"),
        );
        match scope.__apply_owned_port::<EditorGameViewBridge>(input) {
            Ok(output) => Ok(output),
            Err((_, EditorGameViewBridgeInput::Complete(completion))) => Err(completion),
            Err((_, EditorGameViewBridgeInput::TakeCandidate)) => {
                unreachable!("the owned completion path cannot issue a take request")
            }
            Err((_, EditorGameViewBridgeInput::RetirementBlocked)) => {
                unreachable!("the owned completion path cannot issue a status request")
            }
            Err((_, EditorGameViewBridgeInput::SetCaptureEnabled(_))) => {
                unreachable!("the owned completion path cannot change capture state")
            }
            Err((_, EditorGameViewBridgeInput::Reconfigure(_))) => {
                unreachable!("the owned completion path cannot reconfigure Game View")
            }
        }
    });
    match output {
        Ok(Ok(EditorGameViewBridgeOutput::Completed(result))) => result,
        Ok(Err(completion)) => Err(completion),
        Err(_) => Err(owned.expect("a rejected Runtime scope retains the completion")),
        Ok(Ok(
            EditorGameViewBridgeOutput::Candidate(_)
            | EditorGameViewBridgeOutput::Pending
            | EditorGameViewBridgeOutput::Busy
            | EditorGameViewBridgeOutput::RetirementBlocked(_)
            | EditorGameViewBridgeOutput::Updated(_)
            | EditorGameViewBridgeOutput::Failed(_),
        )) => unreachable!("the typed completion port returns a completion outcome"),
    }
}

pub(super) fn apply_input_batch(
    runtime: &mut nara_app::RuntimeInstance,
    batch: EditorGameInputBatch,
) -> Result<(), EditorGameViewBridgeFailure> {
    let events = batch.into_events();
    let Some(first) = events.first().copied() else {
        return Ok(());
    };
    let window = nara_input::InputWindowId::new(first.identity.start().logical_window().id.get());
    let result = runtime.with_driver_scope(|scope| {
        for (event_index, envelope) in events.into_iter().enumerate() {
            let event = envelope.event();
            let result = match event {
                EditorGameInputEvent::Focused(focused) => apply_window_input_driver_event(
                    scope,
                    window,
                    nara_input::InputSeatId::PRIMARY,
                    InputWindowEvent::Focused(focused),
                )
                .map(|_| ()),
                EditorGameInputEvent::SurfaceRetired => apply_window_input_driver_event(
                    scope,
                    window,
                    nara_input::InputSeatId::PRIMARY,
                    InputWindowEvent::SurfaceRetired,
                )
                .map(|_| ()),
                EditorGameInputEvent::Pointer(input) => apply_pointer_driver_input_at(
                    scope,
                    window,
                    nara_input::InputSeatId::PRIMARY,
                    nara_input::InputPointerId::PRIMARY,
                    input,
                )
                .map(|_| ()),
                EditorGameInputEvent::Mouse(input) => apply_mouse_driver_input_at(
                    scope,
                    window,
                    nara_input::InputSeatId::PRIMARY,
                    nara_input::InputPointerId::PRIMARY,
                    input,
                )
                .map(|_| ()),
                EditorGameInputEvent::Keyboard(input) => apply_keyboard_driver_input_at(
                    scope,
                    window,
                    nara_input::InputSeatId::PRIMARY,
                    input,
                )
                .map(|_| ()),
            };
            result.map_err(|source| (event_index, source))?;
        }
        Ok(())
    });
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err((event_index, source))) => Err(EditorGameViewBridgeFailure::InputEventRejected {
            event_index,
            source,
        }),
        Err(source) => Err(EditorGameViewBridgeFailure::InputScopeRejected { source }),
    }
}
