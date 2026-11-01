use std::sync::{Arc, Weak};

#[cfg(feature = "stock-ui")]
use crate::host::WgpuUiSubmission;
use nara_app::{
    App, CoreStage, FrameExecutionStart, Plugin, PluginError, RuntimeCloseContext,
    RuntimeCloseParticipant, RuntimeCloseParticipantError, RuntimeCloseParticipantId,
    RuntimeCloseProgress, RuntimeFault, RuntimeFaultKind, RuntimeFaultReporter, RuntimeGeneration,
};
use nara_ecs::{
    Query, Res, ResMut, Resource, SystemParam, SystemSet, schedule::IntoScheduleConfigs,
};
use nara_render::{
    ExtractedViews, RenderBackendState, RenderBackendStatus, RenderFrame,
    build_render_frame_packet_with_world_canvas,
};
#[cfg(feature = "stock-ui")]
use nara_ui_render::advanced::UiBackendSubmissionReader;
use nara_window::{PrimaryWindowId, Window, backend::WindowPresentationRequirement};

use crate::{
    WgpuPolicyDefinition, WgpuPolicyInstallation, WgpuRenderObservation, WgpuRenderPolicy,
    WgpuRenderProgramError, WgpuStockCapture, WgpuStockPolicy,
    frame::{WgpuFrameCaptureError, WgpuFramePacketSlot, WgpuFrameSet},
    policy::PolicyCaptureAdmission,
    runtime::{self, ConnectionAuthority, WgpuRuntimeConnection},
};

pub const WGPU_CAPTURE_PLUGIN_ID: nara_app::PluginId =
    nara_app::PluginId::new("nara.render-wgpu.capture");
pub const WGPU_WINDOW_PRESENTATION_PLUGIN_ID: nara_app::PluginId =
    nara_app::PluginId::new("nara.render-wgpu.window-presentation");
pub const WGPU_STOCK_CAPTURE_PLUGIN_ID: nara_app::PluginId =
    nara_app::PluginId::new("nara.render-wgpu.stock-capture");
pub const WGPU_STOCK_POLICY_PLUGIN_ID: nara_app::PluginId =
    nara_app::PluginId::new("nara.render-wgpu.stock-policy");
pub const WGPU_BACKEND_SHUTDOWN_OBLIGATION: nara_app::PluginShutdownObligationId =
    nara_app::PluginShutdownObligationId::new("nara.render-wgpu.backend");
const WGPU_STOCK_CAPTURE_SHUTDOWN_OBLIGATION: nara_app::PluginShutdownObligationId =
    nara_app::PluginShutdownObligationId::new("nara.render-wgpu.stock-capture");

const WGPU_RENDER_PRODUCT_REQUIREMENTS: &[nara_app::PluginProductCapability] =
    &[nara_app::PluginProductCapability::new("render-wgpu")];
const WGPU_CAPTURE_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] = &[nara_render::RENDER_PLUGIN_ID];
const WGPU_WINDOW_PRESENTATION_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] =
    &[WGPU_CAPTURE_PLUGIN_ID];
const WGPU_STOCK_CAPTURE_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] = &[WGPU_CAPTURE_PLUGIN_ID];
const WGPU_STOCK_POLICY_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] =
    &[WGPU_STOCK_CAPTURE_PLUGIN_ID];

pub const WGPU_CAPTURE_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(WGPU_CAPTURE_PLUGIN_ID, nara_app::PluginCategory::Backend)
        .requires_plugins(WGPU_CAPTURE_PLUGIN_REQUIREMENTS)
        .requires_product_capabilities(WGPU_RENDER_PRODUCT_REQUIREMENTS)
        .shutdown_obligations(&[WGPU_BACKEND_SHUTDOWN_OBLIGATION]);
pub const WGPU_WINDOW_PRESENTATION_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(
        WGPU_WINDOW_PRESENTATION_PLUGIN_ID,
        nara_app::PluginCategory::Backend,
    )
    .requires_plugins(WGPU_WINDOW_PRESENTATION_PLUGIN_REQUIREMENTS)
    .requires_product_capabilities(WGPU_RENDER_PRODUCT_REQUIREMENTS);
pub const WGPU_STOCK_CAPTURE_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(
        WGPU_STOCK_CAPTURE_PLUGIN_ID,
        nara_app::PluginCategory::Backend,
    )
    .requires_plugins(WGPU_STOCK_CAPTURE_PLUGIN_REQUIREMENTS)
    .requires_product_capabilities(WGPU_RENDER_PRODUCT_REQUIREMENTS)
    .shutdown_obligations(&[WGPU_STOCK_CAPTURE_SHUTDOWN_OBLIGATION]);
pub const WGPU_STOCK_POLICY_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(
        WGPU_STOCK_POLICY_PLUGIN_ID,
        nara_app::PluginCategory::Backend,
    )
    .requires_plugins(WGPU_STOCK_POLICY_PLUGIN_REQUIREMENTS)
    .requires_product_capabilities(WGPU_RENDER_PRODUCT_REQUIREMENTS);

/// Installs CPU policy capture and its original managed Runtime connection.
/// A consumer takes the sealed input without requiring a native presentation owner.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WgpuCapturePlugin;

impl Plugin for WgpuCapturePlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &WGPU_CAPTURE_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<WgpuRenderObservation>()?;
        app.init_resource::<WgpuFramePacketSlot>()?;
        app.init_resource::<RenderBackendStatus>()?;
        app.world_mut()?
            .resource_mut::<RenderBackendStatus>()
            .mark_state(
                crate::WGPU_RENDER_BACKEND,
                RenderBackendState::Uninitialized,
            );
        app.configure_sets(
            CoreStage::Render,
            (
                WgpuFrameSet::Reset,
                WgpuFrameSet::Capture,
                WgpuFrameSet::Seal,
                WgpuFrameSet::Publish,
            )
                .chain()
                .after(nara_render::begin_render_frame),
        )?;
        app.add_systems(
            CoreStage::Render,
            (
                reset_wgpu_frame_packet.in_set(WgpuFrameSet::Reset),
                seal_wgpu_frame_packet.in_set(WgpuFrameSet::Seal),
            ),
        )?;
        runtime::install(app)?;
        Ok(())
    }
}

/// Selects native-window presentation as the consumer of the sealed CPU capture.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WgpuWindowPresentationPlugin;

impl Plugin for WgpuWindowPresentationPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &WGPU_WINDOW_PRESENTATION_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let world = app.world_mut()?;
        if let Some(requirement) = world.get_resource::<WindowPresentationRequirement>() {
            return Err(PluginError::SetupFailed {
                plugin: WGPU_WINDOW_PRESENTATION_PLUGIN_ID,
                message: format!(
                    "native window presentation is already required by {}",
                    requirement.backend()
                ),
            });
        }
        world.insert_resource(WindowPresentationRequirement::new(
            crate::WGPU_RENDER_BACKEND,
        ));
        app.add_systems(
            CoreStage::Render,
            publish_wgpu_capture.in_set(WgpuFrameSet::Publish),
        )?;
        Ok(())
    }
}

#[derive(Resource)]
struct SelectedPolicy {
    admission: Weak<PolicyCaptureAdmission>,
}

impl SelectedPolicy {
    fn is_open(&self) -> bool {
        self.admission
            .upgrade()
            .is_some_and(|admission| admission.is_open())
    }

    fn accepts<P: WgpuRenderPolicy>(&self, installation: &WgpuPolicyInstallation<P>) -> bool {
        self.admission.upgrade().is_some_and(|admission| {
            admission.id == installation.id()
                && Arc::ptr_eq(&admission, installation.admission())
                && admission.is_open()
        })
    }
}

/// Selects one exact policy installation and installs its typed capture-owner Resource.
/// A second route is refused, including a replacement with the same policy Rust type.
pub fn install_wgpu_policy<P: WgpuRenderPolicy>(
    app: &mut App,
    installation: WgpuPolicyInstallation<P>,
) -> Result<(), PluginError> {
    let world = app.world_mut()?;
    if world.contains_resource::<SelectedPolicy>()
        || world.contains_resource::<WgpuPolicyInstallation<P>>()
    {
        return Err(PluginError::SetupFailed {
            plugin: WGPU_CAPTURE_PLUGIN_ID,
            message: "a wgpu policy capture route is already installed".to_owned(),
        });
    }
    if !installation.is_open() {
        return Err(PluginError::SetupFailed {
            plugin: WGPU_CAPTURE_PLUGIN_ID,
            message: WgpuRenderProgramError::PolicyClosed.to_string(),
        });
    }
    let selected = SelectedPolicy {
        admission: Arc::downgrade(installation.admission()),
    };
    world.insert_resource(installation);
    world.insert_resource(selected);
    Ok(())
}

/// Captures one immutable policy draft during [`crate::WgpuPolicyCaptureSet::Capture`].
/// Backpressure, closed admission and a different installation leave the current input intact.
#[derive(SystemParam)]
pub struct WgpuPolicyFrameWriter<'w> {
    slot: ResMut<'w, WgpuFramePacketSlot>,
    connection: Res<'w, WgpuRuntimeConnection>,
    authority: Res<'w, ConnectionAuthority>,
    selected: Option<Res<'w, SelectedPolicy>>,
}

/// Refused policy capture retaining its original move-only UI completion responsibility.
#[cfg(feature = "stock-ui")]
#[derive(Debug, thiserror::Error)]
#[error("policy UI capture was refused: {error}")]
pub struct WgpuPolicyUiCaptureFailure {
    #[source]
    error: WgpuRenderProgramError,
    ui: WgpuUiSubmission,
}

#[cfg(feature = "stock-ui")]
impl WgpuPolicyUiCaptureFailure {
    #[must_use]
    pub const fn error(&self) -> &WgpuRenderProgramError {
        &self.error
    }

    /// Returns the refusal and the unchanged original UI sidecar for retry or settlement.
    #[must_use]
    pub fn into_parts(self) -> (WgpuRenderProgramError, WgpuUiSubmission) {
        (self.error, self.ui)
    }
}

impl WgpuPolicyFrameWriter<'_> {
    fn can_collect_stock(&self) -> bool {
        self.connection.can_capture_from(&self.authority)
            && self.slot.accepts_capture()
            && self
                .selected
                .as_ref()
                .is_some_and(|selected| selected.is_open())
    }

    #[must_use]
    pub fn can_capture<P: WgpuRenderPolicy>(
        &self,
        installation: &WgpuPolicyInstallation<P>,
    ) -> bool {
        self.connection.can_capture_from(&self.authority)
            && self.slot.accepts_capture()
            && self
                .selected
                .as_ref()
                .is_some_and(|selected| selected.accepts(installation))
    }

    /// Accepts the original definition and draft once; `false` leaves the pending batch intact.
    /// Validation against the neutral packet happens when the generic capture stage seals.
    pub fn capture<P: WgpuRenderPolicy>(
        &mut self,
        installation: &WgpuPolicyInstallation<P>,
        capture: Arc<P::Capture>,
    ) -> Result<bool, WgpuRenderProgramError> {
        if !self.can_capture(installation) {
            return Ok(false);
        }
        Ok(self.slot.collect_policy(installation.accept(capture)?))
    }

    /// Checks admission before a concrete UI collector takes its move-only backend frame.
    /// The subsequent capture still returns the original sidecar if admission is refused.
    /// Call this before [`nara_ui_render::advanced::UiBackendSubmissionReader::take`].
    #[cfg(feature = "stock-ui")]
    #[must_use]
    pub fn can_capture_with_ui<P: WgpuRenderPolicy>(
        &self,
        installation: &WgpuPolicyInstallation<P>,
    ) -> bool {
        self.can_capture(installation) && self.slot.accepts_ui_collection()
    }

    /// Accepts one original policy capture and UI sidecar together, without a partial slot write.
    ///
    /// Capture contains replayable packets only. This sidecar retains the original completion
    /// permissions separately, including when sealing finds no Views or rejects the capture.
    /// A refusal returns that same sidecar through [`WgpuPolicyUiCaptureFailure::into_parts`].
    #[cfg(feature = "stock-ui")]
    pub fn capture_with_ui<P: WgpuRenderPolicy>(
        &mut self,
        installation: &WgpuPolicyInstallation<P>,
        capture: Arc<P::Capture>,
        ui: WgpuUiSubmission,
    ) -> Result<(), WgpuPolicyUiCaptureFailure> {
        let error = if !installation.is_open() {
            Some(WgpuRenderProgramError::PolicyClosed)
        } else if !self
            .selected
            .as_ref()
            .is_some_and(|selected| selected.accepts(installation))
        {
            Some(WgpuRenderProgramError::feature(
                "the UI capture installation is not the selected policy",
            ))
        } else if !self.connection.can_capture_from(&self.authority) {
            Some(WgpuRenderProgramError::feature(
                "the original Runtime capture connection is busy or unavailable",
            ))
        } else if !self.slot.accepts_ui_collection() {
            Some(WgpuRenderProgramError::feature(
                "the policy UI capture slot is busy or outside its capture phase",
            ))
        } else {
            None
        };
        if let Some(error) = error {
            return Err(WgpuPolicyUiCaptureFailure { error, ui });
        }
        let capture = match installation.accept(capture) {
            Ok(capture) => capture,
            Err(error) => return Err(WgpuPolicyUiCaptureFailure { error, ui }),
        };
        self.slot
            .collect_policy_with_ui(capture, ui.into_state())
            .map_err(|(_capture, ui)| WgpuPolicyUiCaptureFailure {
                error: WgpuRenderProgramError::feature(
                    "the policy UI capture slot refused the original capture and sidecar",
                ),
                ui: WgpuUiSubmission::from_state(ui),
            })
    }
}

fn reset_wgpu_frame_packet(
    mut slot: ResMut<WgpuFramePacketSlot>,
    connection: Res<WgpuRuntimeConnection>,
    authority: Res<ConnectionAuthority>,
    faults: Option<Res<RuntimeFaultReporter>>,
) {
    if !authority.is_current(&connection) {
        if let Some(faults) = faults {
            faults.report(RuntimeFault::engine(
                RuntimeFaultKind::RequiredService,
                "nara.render-wgpu.connection",
            ));
        }
        return;
    }
    if connection.can_capture_from(&authority) {
        slot.reset();
    }
}

fn seal_wgpu_frame_packet(
    generation: Option<Res<RuntimeGeneration>>,
    frame_start: Res<FrameExecutionStart>,
    frame: Res<RenderFrame>,
    primary_window_id: Option<Res<PrimaryWindowId>>,
    windows: Query<&Window>,
    views: Res<ExtractedViews>,
    mut slot: ResMut<WgpuFramePacketSlot>,
    connection: Res<WgpuRuntimeConnection>,
    authority: Res<ConnectionAuthority>,
) {
    if !connection.can_capture_from(&authority) || !slot.is_collecting() {
        return;
    }
    if views.as_slice().is_empty() {
        slot.seal(Ok(None), frame_start.frame(), frame_start.started_at());
        return;
    }
    let Some(generation) = generation.map(|generation| *generation) else {
        slot.reject_capture(WgpuFrameCaptureError::MissingRuntimeGeneration);
        return;
    };
    let captured = build_render_frame_packet_with_world_canvas(
        generation,
        frame.index,
        primary_window_id.map(|primary| primary.0),
        windows.iter(),
        &views,
        views.world_canvas(),
    );
    slot.seal(captured, frame_start.frame(), frame_start.started_at());
}

fn publish_wgpu_capture(
    mut slot: ResMut<WgpuFramePacketSlot>,
    mut connection: ResMut<WgpuRuntimeConnection>,
    authority: Res<ConnectionAuthority>,
    frame: Res<RenderFrame>,
    primary_window_id: Option<Res<PrimaryWindowId>>,
    windows: Query<&Window>,
    views: Res<ExtractedViews>,
) {
    if !connection.can_capture_from(&authority) {
        return;
    }
    let Some(batch) = slot.take_batch() else {
        return;
    };
    let primary_window_id = primary_window_id.map(|primary| primary.0);
    let window = primary_window_id
        .and_then(|id| windows.iter().find(|window| window.id == id))
        .map(|window| nara_render::RenderWindowPacket {
            id: window.id,
            resolution: window.resolution,
            present_mode: window.present_mode,
        });
    connection.publish(runtime::PendingFrame {
        captured: batch.captured,
        frame: *frame,
        window,
        canvas: views.world_canvas(),
        #[cfg(feature = "stock-ui")]
        ui: batch.ui,
    });
}

/// Installs policy-independent stock collection and its ordered contribution phase.
///
/// Select this plugin explicitly alongside [`WgpuCapturePlugin`]. It installs neither a policy
/// nor a domain contributor. Custom policies take the collected frame in
/// [`WgpuStockCaptureSet::Compose`] and submit it through [`WgpuPolicyFrameWriter`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WgpuStockCapturePlugin;

#[derive(Default, Resource)]
struct StockCaptureBuilder {
    draft: Option<WgpuStockFrame>,
    closed: bool,
}

impl StockCaptureBuilder {
    fn close(&mut self) {
        self.closed = true;
        // Dropping an original unsubmitted permission returns its pre-admitted Aborted outcome
        // to its original owner, even if that owner's World resource has been replaced.
        self.draft = None;
    }
}

/// Ordered stock collection phases within [`crate::WgpuPolicyCaptureSet::Capture`].
///
/// Domain collectors run in `Contribute`; external composition runs in `Compose`. The frame
/// remains pending until a composer takes it or logical Runtime close returns its permissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub enum WgpuStockCaptureSet {
    Reset,
    Contribute,
    Compose,
}

/// One stock snapshot and its optional original move-only UI completion responsibility.
///
/// The snapshot may be retained for replay. The sidecar must be submitted, retained for retry,
/// or settled exactly once by the caller after handoff; it cannot be reconstructed from packets.
#[derive(Debug)]
#[must_use]
pub struct WgpuStockFrame {
    capture: Arc<WgpuStockCapture>,
    #[cfg(feature = "stock-ui")]
    ui: Option<WgpuUiSubmission>,
}

impl WgpuStockFrame {
    /// Borrows replayable drawing inputs without transferring UI responsibility.
    #[must_use]
    pub fn capture(&self) -> &WgpuStockCapture {
        &self.capture
    }

    /// Moves the immutable snapshot and the unchanged original UI sidecar to the composer.
    #[cfg(feature = "stock-ui")]
    #[must_use]
    pub fn into_parts(self) -> (Arc<WgpuStockCapture>, Option<WgpuUiSubmission>) {
        (self.capture, self.ui)
    }

    /// Moves the immutable snapshot to the composer when UI support is disabled.
    #[cfg(not(feature = "stock-ui"))]
    #[must_use]
    pub fn into_parts(self) -> Arc<WgpuStockCapture> {
        self.capture
    }
}

/// Moves the current stock frame to the exact selected policy during capture.
#[derive(SystemParam)]
pub struct WgpuStockFrameReader<'w> {
    builder: ResMut<'w, StockCaptureBuilder>,
}

impl WgpuStockFrameReader<'_> {
    /// Takes at most once in [`WgpuStockCaptureSet::Compose`].
    ///
    /// A different or closed installation, an unavailable original Runtime connection, or a
    /// busy/out-of-phase slot leaves both the snapshot and original UI sidecar untouched.
    #[must_use]
    pub fn take<P: WgpuRenderPolicy>(
        &mut self,
        installation: &WgpuPolicyInstallation<P>,
        writer: &WgpuPolicyFrameWriter<'_>,
    ) -> Option<WgpuStockFrame> {
        if self.builder.closed || !writer.can_capture(installation) {
            return None;
        }
        #[cfg(feature = "stock-ui")]
        if self.builder.draft.as_ref()?.ui.is_some() && !writer.can_capture_with_ui(installation) {
            return None;
        }
        self.builder.draft.take()
    }
}

struct StockCaptureClose;

impl StockCaptureClose {
    fn drive(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        if let Some(mut builder) =
            context
                .get_resource_mut::<StockCaptureBuilder>()
                .map_err(|_| {
                    RuntimeCloseParticipantError::retryable("stock-capture-builder-unavailable")
                })?
        {
            builder.close();
        }
        Ok(RuntimeCloseProgress::Complete)
    }
}

impl RuntimeCloseParticipant for StockCaptureClose {
    fn begin_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.drive(context)
    }

    fn poll_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.drive(context)
    }
}

impl Plugin for WgpuStockCapturePlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &WGPU_STOCK_CAPTURE_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<StockCaptureBuilder>()?;
        app.register_plugin_runtime_close_participant(
            WGPU_STOCK_CAPTURE_SHUTDOWN_OBLIGATION,
            RuntimeCloseParticipantId::new("nara.render-wgpu.stock-capture"),
            StockCaptureClose,
        )?;
        app.configure_sets(
            CoreStage::Render,
            (
                WgpuStockCaptureSet::Reset,
                WgpuStockCaptureSet::Contribute,
                WgpuStockCaptureSet::Compose,
            )
                .chain()
                .in_set(WgpuFrameSet::Capture),
        )?;
        app.add_systems(
            CoreStage::Render,
            reset_stock_capture.in_set(WgpuStockCaptureSet::Reset),
        )?;
        Ok(())
    }
}

#[cfg_attr(not(feature = "stock-ui"), allow(unused_mut))]
fn reset_stock_capture(
    mut writer: WgpuPolicyFrameWriter,
    mut builder: ResMut<StockCaptureBuilder>,
    #[cfg(feature = "stock-ui")] primary_window_id: Option<Res<PrimaryWindowId>>,
) {
    if builder.closed || !writer.can_collect_stock() {
        return;
    }
    #[cfg(feature = "stock-ui")]
    if builder
        .draft
        .as_ref()
        .is_some_and(|draft| draft.ui.is_some())
    {
        // A missing custom composer must not silently replace original UI permissions on the
        // next frame. Keep the draft for logical close and expose the incomplete composition.
        writer.slot.reject_capture(WgpuFrameCaptureError::Policy(
            WgpuRenderProgramError::feature(
                "the previous stock frame was not handed off during its capture phase",
            ),
        ));
        return;
    }
    let capture = WgpuStockCapture::default();
    #[cfg(feature = "stock-ui")]
    let capture = WgpuStockCapture {
        primary_window_id: primary_window_id.map(|primary| primary.0),
        ..capture
    };
    builder.draft = Some(WgpuStockFrame {
        capture: Arc::new(capture),
        #[cfg(feature = "stock-ui")]
        ui: None,
    });
}

/// Installs the default stock policy and composer without enabling any domain contributor.
/// Requires an explicitly selected [`WgpuStockCapturePlugin`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WgpuStockPolicyPlugin;

impl Plugin for WgpuStockPolicyPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &WGPU_STOCK_POLICY_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let installation = WgpuPolicyDefinition::new(WgpuStockPolicy::default())
            .install()
            .map_err(|error| PluginError::SetupFailed {
                plugin: WGPU_STOCK_POLICY_PLUGIN_ID,
                message: error.to_string(),
            })?;
        install_wgpu_policy(app, installation)?;
        app.add_systems(
            CoreStage::Render,
            emit_stock_capture.in_set(WgpuStockCaptureSet::Compose),
        )?;
        Ok(())
    }
}

fn emit_stock_capture(
    installation: Res<WgpuPolicyInstallation<WgpuStockPolicy>>,
    mut writer: WgpuPolicyFrameWriter,
    mut reader: WgpuStockFrameReader,
) {
    let Some(frame) = reader.take(&installation, &writer) else {
        return;
    };
    let capture = Arc::clone(&frame.capture);
    #[cfg(feature = "stock-ui")]
    let mut frame = frame;
    #[cfg(feature = "stock-ui")]
    let captured = if let Some(ui) = frame.ui.take() {
        writer
            .capture_with_ui(&installation, capture, ui)
            .map(|()| true)
            .map_err(|failure| {
                let (error, ui) = failure.into_parts();
                frame.ui = Some(ui);
                error
            })
    } else {
        writer.capture(&installation, capture)
    };
    #[cfg(not(feature = "stock-ui"))]
    let captured = writer.capture(&installation, capture);
    if matches!(captured, Ok(true)) {
        return;
    }
    reader.builder.draft = Some(frame);
    if let Err(error) = captured {
        writer
            .slot
            .reject_capture(WgpuFrameCaptureError::Policy(error));
    }
}

#[cfg(feature = "stock-sprite")]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WgpuStockSpritePlugin;

#[cfg(feature = "stock-sprite")]
pub const WGPU_STOCK_SPRITE_PLUGIN_ID: nara_app::PluginId =
    nara_app::PluginId::new("nara.render-wgpu.stock-sprite");
#[cfg(feature = "stock-sprite")]
const WGPU_STOCK_SPRITE_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] = &[
    WGPU_STOCK_CAPTURE_PLUGIN_ID,
    nara_sprite_render::SPRITE_RENDER_PLUGIN_ID,
];
#[cfg(feature = "stock-sprite")]
const WGPU_STOCK_SPRITE_PRODUCT_REQUIREMENTS: &[nara_app::PluginProductCapability] = &[
    nara_app::PluginProductCapability::new("presentation-2d"),
    nara_app::PluginProductCapability::new("render-wgpu"),
];
#[cfg(feature = "stock-sprite")]
pub const WGPU_STOCK_SPRITE_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(
        WGPU_STOCK_SPRITE_PLUGIN_ID,
        nara_app::PluginCategory::Backend,
    )
    .requires_plugins(WGPU_STOCK_SPRITE_PLUGIN_REQUIREMENTS)
    .requires_product_capabilities(WGPU_STOCK_SPRITE_PRODUCT_REQUIREMENTS);

#[cfg(feature = "stock-sprite")]
impl Plugin for WgpuStockSpritePlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &WGPU_STOCK_SPRITE_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(
            CoreStage::Render,
            collect_stock_sprites.in_set(WgpuStockCaptureSet::Contribute),
        )?;
        Ok(())
    }
}

#[cfg(feature = "stock-sprite")]
fn collect_stock_sprites(
    sprite_batches: Res<nara_sprite_render::__private::SpriteBatches>,
    writer: WgpuPolicyFrameWriter,
    mut builder: ResMut<StockCaptureBuilder>,
) {
    if builder.closed || !writer.can_collect_stock() {
        return;
    }
    let Some(draft) = builder.draft.as_mut() else {
        return;
    };
    Arc::get_mut(&mut draft.capture)
        .expect("an unhanded stock capture has one snapshot owner")
        .sprite_batches = Some(sprite_batches.clone());
}

#[cfg(feature = "stock-ui")]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WgpuStockUiPlugin;

#[cfg(feature = "stock-ui")]
pub const WGPU_STOCK_UI_PLUGIN_ID: nara_app::PluginId =
    nara_app::PluginId::new("nara.render-wgpu.stock-ui");
#[cfg(feature = "stock-ui")]
const WGPU_STOCK_UI_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] = &[
    WGPU_STOCK_CAPTURE_PLUGIN_ID,
    nara_ui_render::UI_RENDER_PLUGIN_ID,
];
#[cfg(feature = "stock-ui")]
const WGPU_STOCK_UI_PRODUCT_REQUIREMENTS: &[nara_app::PluginProductCapability] = &[
    nara_app::PluginProductCapability::new("presentation-ui"),
    nara_app::PluginProductCapability::new("render-wgpu"),
];
#[cfg(feature = "stock-ui")]
pub const WGPU_STOCK_UI_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(WGPU_STOCK_UI_PLUGIN_ID, nara_app::PluginCategory::Backend)
        .requires_plugins(WGPU_STOCK_UI_PLUGIN_REQUIREMENTS)
        .requires_product_capabilities(WGPU_STOCK_UI_PRODUCT_REQUIREMENTS);

#[cfg(feature = "stock-ui")]
impl Plugin for WgpuStockUiPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &WGPU_STOCK_UI_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(
            CoreStage::Render,
            collect_stock_ui.in_set(WgpuStockCaptureSet::Contribute),
        )?;
        Ok(())
    }
}

#[cfg(feature = "stock-ui")]
fn collect_stock_ui(
    mut submissions: UiBackendSubmissionReader,
    mut writer: WgpuPolicyFrameWriter,
    mut builder: ResMut<StockCaptureBuilder>,
) {
    if builder.closed || !writer.can_collect_stock() || !writer.slot.accepts_ui_collection() {
        return;
    }
    let Some(draft) = builder.draft.as_mut() else {
        return;
    };
    if draft.ui.is_some() {
        return;
    }
    match submissions.take() {
        Ok(frame) => {
            let (packets, ui) = WgpuUiSubmission::capture(frame);
            Arc::get_mut(&mut draft.capture)
                .expect("an unhanded stock capture has one snapshot owner")
                .ui_packets = packets;
            draft.ui = Some(ui);
        }
        Err(error) => {
            writer.slot.reject_ui_collection(error);
            // No original sidecar was acquired. Keep the refusal in the frame slot and let the
            // next capture retry once the UI owner's admission condition has recovered.
            builder.draft = None;
        }
    }
}

#[cfg(test)]
#[path = "plugin/stock_capture_tests.rs"]
mod stock_capture_tests;

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn capture_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            nara_reflect::ComponentRegistryPlugin,
            nara_hierarchy::HierarchyPlugin,
            nara_transform::TransformPlugin,
            nara_render::RenderComponentsPlugin,
            nara_render::RenderPlugin::default(),
            WgpuCapturePlugin,
        ))
        .unwrap();
        app
    }

    fn stock_policy_app() -> App {
        let mut app = capture_app();
        app.add_plugins((WgpuStockCapturePlugin, WgpuStockPolicyPlugin))
            .unwrap();
        app
    }

    #[test]
    fn capture_plugin_declaration_is_feature_invariant() {
        let declaration = WgpuCapturePlugin::declaration();
        assert_eq!(
            declaration.requires_plugins,
            &[nara_render::RENDER_PLUGIN_ID]
        );
        assert_eq!(
            declaration.requires_product_capabilities,
            &[nara_app::PluginProductCapability::new("render-wgpu")]
        );
        assert_eq!(
            declaration.shutdown_obligations,
            &[WGPU_BACKEND_SHUTDOWN_OBLIGATION]
        );
    }

    #[test]
    fn capture_does_not_require_a_native_target_or_select_a_stock_policy() {
        let app = capture_app();

        assert!(app.world().contains_resource::<WgpuFramePacketSlot>());
        assert!(app.world().contains_resource::<WgpuRuntimeConnection>());
        assert!(
            !app.world()
                .contains_resource::<WindowPresentationRequirement>()
        );
        assert!(!app.world().contains_resource::<SelectedPolicy>());
        assert!(!app.has_plugin(WGPU_STOCK_POLICY_PLUGIN_ID));
    }

    #[test]
    fn window_presentation_is_an_explicit_capture_consumer() {
        assert_eq!(
            WgpuWindowPresentationPlugin::declaration().requires_plugins,
            &[WGPU_CAPTURE_PLUGIN_ID]
        );
        let mut app = capture_app();
        app.add_plugins(WgpuWindowPresentationPlugin).unwrap();

        assert_eq!(
            app.world()
                .resource::<WindowPresentationRequirement>()
                .backend(),
            crate::WGPU_RENDER_BACKEND
        );
    }

    #[test]
    fn stock_policy_installs_only_its_explicit_route() {
        let app = stock_policy_app();

        assert!(app.has_plugin(WGPU_STOCK_CAPTURE_PLUGIN_ID));
        assert!(
            app.world()
                .contains_resource::<WgpuPolicyInstallation<WgpuStockPolicy>>()
        );
        assert!(
            !app.world()
                .contains_resource::<WindowPresentationRequirement>()
        );
        #[cfg(feature = "stock-sprite")]
        assert!(!app.has_plugin(WGPU_STOCK_SPRITE_PLUGIN_ID));
        #[cfg(feature = "stock-ui")]
        assert!(!app.has_plugin(WGPU_STOCK_UI_PLUGIN_ID));
    }

    #[test]
    fn a_second_installation_never_replaces_the_selected_route() {
        let mut app = stock_policy_app();
        let original = app
            .world()
            .resource::<WgpuPolicyInstallation<WgpuStockPolicy>>()
            .id();
        let second = WgpuPolicyDefinition::new(WgpuStockPolicy::default())
            .install()
            .unwrap();

        assert!(install_wgpu_policy(&mut app, second).is_err());
        assert_eq!(
            app.world()
                .resource::<WgpuPolicyInstallation<WgpuStockPolicy>>()
                .id(),
            original
        );
    }

    #[test]
    fn writer_accepts_only_the_exact_selected_installation_once() {
        use nara_ecs::bevy_ecs::system::SystemState;

        let mut app = stock_policy_app();
        let other = WgpuPolicyDefinition::new(WgpuStockPolicy::default())
            .install()
            .unwrap();
        let world = app.world_mut().unwrap();
        world.resource_mut::<WgpuFramePacketSlot>().reset();
        let mut state = SystemState::<(
            Res<WgpuPolicyInstallation<WgpuStockPolicy>>,
            WgpuPolicyFrameWriter,
        )>::new(world);
        let (installation, mut writer) = state.get_mut(world).unwrap();

        assert!(!writer.can_capture(&other));
        assert!(
            !writer
                .capture(&other, Arc::new(WgpuStockCapture::default()))
                .unwrap()
        );
        assert!(writer.can_capture(&installation));
        assert!(
            writer
                .capture(&installation, Arc::new(WgpuStockCapture::default()))
                .unwrap()
        );
        assert!(!writer.can_capture(&installation));
        assert!(
            !writer
                .capture(&installation, Arc::new(WgpuStockCapture::default()))
                .unwrap()
        );
    }

    #[test]
    fn replacing_the_typed_resource_cannot_retarget_capture_admission() {
        use nara_ecs::bevy_ecs::system::SystemState;

        let mut app = stock_policy_app();
        let replacement = WgpuPolicyDefinition::new(WgpuStockPolicy::default())
            .install()
            .unwrap();
        let world = app.world_mut().unwrap();
        world.resource_mut::<WgpuFramePacketSlot>().reset();
        world.insert_resource(replacement);
        let mut state = SystemState::<(
            Res<WgpuPolicyInstallation<WgpuStockPolicy>>,
            WgpuPolicyFrameWriter,
        )>::new(world);
        let (installation, mut writer) = state.get_mut(world).unwrap();

        assert!(!writer.can_capture(&installation));
        assert!(
            !writer
                .capture(&installation, Arc::new(WgpuStockCapture::default()))
                .unwrap()
        );
    }

    #[test]
    fn a_busy_connection_does_not_accept_another_policy_draft() {
        use nara_ecs::bevy_ecs::system::SystemState;

        let mut app = stock_policy_app();
        let world = app.world_mut().unwrap();
        world.resource_mut::<WgpuFramePacketSlot>().reset();
        world
            .resource_mut::<WgpuRuntimeConnection>()
            .publish(runtime::PendingFrame {
                captured: Ok(None),
                frame: RenderFrame::default(),
                window: None,
                canvas: None,
                #[cfg(feature = "stock-ui")]
                ui: None,
            });
        let mut state = SystemState::<(
            Res<WgpuPolicyInstallation<WgpuStockPolicy>>,
            WgpuPolicyFrameWriter,
        )>::new(world);
        let (installation, mut writer) = state.get_mut(world).unwrap();

        assert!(!writer.can_capture(&installation));
        assert!(
            !writer
                .capture(&installation, Arc::new(WgpuStockCapture::default()))
                .unwrap()
        );
    }

    #[cfg(feature = "stock-ui")]
    #[test]
    fn ui_capture_returns_original_sidecar_when_connection_is_busy_or_replaced() {
        use crate::frame::WgpuUiSubmissionState;
        use nara_ecs::bevy_ecs::system::SystemState;
        use nara_ui_render::__private::UiPresentationOutcomes;

        for replace_connection in [false, true] {
            let mut app = stock_policy_app();
            let mut outcomes = UiPresentationOutcomes::default();
            let packet = crate::tests::ui_outcome_packet();
            let submissions = vec![crate::tests::admit_ui_attempt(
                &mut outcomes,
                crate::tests::prepared_ui_attempt(packet, 91, 3),
            )];
            let allocation = submissions.as_ptr();
            let ui = WgpuUiSubmission::from_state(WgpuUiSubmissionState::Prepared(submissions));
            let world = app.world_mut().unwrap();
            world.resource_mut::<WgpuFramePacketSlot>().reset();
            if replace_connection {
                world.insert_resource(WgpuRuntimeConnection::default());
            } else {
                world
                    .resource_mut::<WgpuRuntimeConnection>()
                    .publish(runtime::PendingFrame {
                        captured: Ok(None),
                        frame: RenderFrame::default(),
                        window: None,
                        canvas: None,
                        ui: None,
                    });
            }
            {
                let mut state = SystemState::<(
                    Res<WgpuPolicyInstallation<WgpuStockPolicy>>,
                    WgpuPolicyFrameWriter,
                )>::new(world);
                let (installation, mut writer) = state.get_mut(world).unwrap();
                assert!(!writer.can_capture_with_ui(&installation));
                let failure = writer
                    .capture_with_ui(&installation, Arc::new(WgpuStockCapture::default()), ui)
                    .unwrap_err();
                let (error, returned) = failure.into_parts();
                assert!(matches!(error, WgpuRenderProgramError::Feature { .. }));
                assert!(writer.slot.accepts_ui_collection());
                let WgpuUiSubmissionState::Prepared(submissions) = returned.into_state() else {
                    panic!("refusal must preserve the original unsubmitted UI state");
                };
                assert_eq!(submissions.as_ptr(), allocation);
                assert_eq!(submissions.len(), 1);
                for completed in
                    WgpuUiSubmission::from_state(WgpuUiSubmissionState::Prepared(submissions))
                        .content_rejected(
                            "connection refusal returned the original sidecar".to_owned(),
                        )
                {
                    outcomes.commit_backend(completed);
                }
            }
            world.insert_resource(outcomes);
            assert!(matches!(
                nara_ui_render::drain_ui_presentation_outcomes(world).unwrap().as_slice(),
                [nara_ui_render::UiPresentationOutcome::ContentRejected { attempt, .. }]
                    if attempt.admission_id().unwrap().get() == 91,
            ));
            assert!(
                nara_ui_render::drain_ui_presentation_outcomes(world)
                    .unwrap()
                    .is_empty()
            );
        }
    }

    #[cfg(feature = "stock-sprite")]
    #[test]
    fn sprite_contribution_requires_capture_and_the_sprite_owner() {
        let declaration = WgpuStockSpritePlugin::declaration();
        assert_eq!(
            declaration.requires_plugins,
            &[
                WGPU_STOCK_CAPTURE_PLUGIN_ID,
                nara_sprite_render::SPRITE_RENDER_PLUGIN_ID
            ]
        );
        assert_eq!(
            declaration.requires_product_capabilities,
            WGPU_STOCK_SPRITE_PRODUCT_REQUIREMENTS
        );
    }

    #[cfg(feature = "stock-ui")]
    #[test]
    fn ui_contribution_requires_capture_and_the_ui_owner() {
        let declaration = WgpuStockUiPlugin::declaration();
        assert_eq!(
            declaration.requires_plugins,
            &[
                WGPU_STOCK_CAPTURE_PLUGIN_ID,
                nara_ui_render::UI_RENDER_PLUGIN_ID
            ]
        );
        assert_eq!(
            declaration.requires_product_capabilities,
            WGPU_STOCK_UI_PRODUCT_REQUIREMENTS
        );
    }
}
