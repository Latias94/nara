//! Game-specific desktop evidence, shared by a main-thread executable and its ignored test.
//!
//! Compile the consumer with `desktop` and a direct `wgpu` dependency at the workspace's exact
//! revision. This module installs no GPU owner, takes no transport, and changes no game state.
//! It retains bounded original stock inputs between Seal and Publish, then accepts one only when the
//! original desktop Host issues the matching presentation receipt. READY is not consulted.
//!
//! Typical main-thread setup (the caller owns reconstruction, driving, and native retirement):
//!
//! ```ignore
//! #[path = "../tests/support/desktop_capture.rs"]
//! mod desktop_capture;
//! let recipe = two_level_game::product_recipe()
//!     .add_plugin::<desktop_capture::DesktopCapturePlugin>()?;
//! // Inside the original desktop presentation observer:
//! let frame = desktop_capture::join_presented(observation)?;
//! // Retain the selected frame and finish all original native cleanup before replay.
//! let frame = frame.ok_or("no matching presentation")?;
//! let replay = desktop_capture::replay_and_check(&frame, &expected_resume)?;
//! // replay.rgba8 is an independent image replay, never a swapchain or display capture.
//! ```
//!
//! `join_presented` returns the sealed snapshot for any original presentation, even if the World
//! has since advanced through an exact step without rendering. It does not request exit.
//! A parent ignored test should launch a main-thread child with its own wall-clock deadline;
//! callback deadlines cannot interrupt a stalled driver, device acquisition, or synchronous API.
//!
//! Scope: the shipped source Scenes, one 960x540 physical window, and a 320x180 world canvas.
//! Scale factor is observed, never changed. Resize/mixed-DPI qualification belongs elsewhere.
//! GameSnapshot and rendered objects expose active-level semantics, not private inactive-level
//! progress, next-marker allocation, or the App configuration fingerprint. The caller must also
//! compare complete authoritative ResumeStates; this module cannot manufacture that capture or
//! make an unsupported desktop service eligible for the game's resume API.
//! Choose a non-anchor player position with an unoccluded marker and pickup interiors for pixel
//! evidence. Completely hidden required objects fail the pixel oracle instead of counting as seen.

#![cfg(feature = "desktop")]

#[path = "desktop_capture/oracle.rs"]
mod oracle;
#[path = "desktop_capture/replay.rs"]
mod replay;

pub use oracle::PixelChecks;
pub use replay::replay_and_check;

use std::{collections::VecDeque, error::Error, io};

use nara::{
    advanced_prelude::{CurrentScene, SceneSourceView},
    app::{
        App, CoreStage, FixedTime, FrameExecutionStart, Plugin, PluginCategory, PluginDeclaration,
        PluginError, PluginId, RuntimeGeneration,
    },
    ecs::{Query, Res, ResMut, Resource, schedule::IntoScheduleConfigs},
    project_host::{DesktopPresentationObservation, ProjectSceneContentStamp},
    render::{Camera2d, Extent2d, RenderFrameTarget, RenderWindowPacket},
    render_wgpu::{
        WGPU_CAPTURE_PLUGIN_ID, WGPU_STOCK_POLICY_PLUGIN_ID, WGPU_STOCK_SPRITE_PLUGIN_ID,
        WgpuAdapterSummary, WgpuBackendState, WgpuPolicyCaptureSet, WgpuPolicyFrameReader,
        WgpuRenderInput,
    },
    scene::{SourceDocumentKey, SpawnedSceneInstance},
};
use two_level_game::{GamePhase, GameSnapshot, resume::ResumeState};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub const WINDOW_EXTENT: Extent2d = Extent2d {
    width: 960,
    height: 540,
};
pub const CANVAS_EXTENT: Extent2d = Extent2d {
    width: 320,
    height: 180,
};
/// A delayed receipt may lag newer seals. Evidence outside this finite history fails closed.
const MAX_RETAINED_FRAMES: usize = 8;
const DECLARATION: PluginDeclaration = PluginDeclaration::new(
    PluginId::new("two-level-game.test.desktop-capture"),
    PluginCategory::Runtime,
)
.requires_plugins(&[
    PluginId::new("two-level-game.exploration"),
    WGPU_CAPTURE_PLUGIN_ID,
    WGPU_STOCK_SPRITE_PLUGIN_ID,
    WGPU_STOCK_POLICY_PLUGIN_ID,
]);

/// CPU observation only. Retains at most eight sealed inputs in the production World.
#[derive(Default)]
pub struct DesktopCapturePlugin;

impl Plugin for DesktopCapturePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> std::result::Result<(), PluginError> {
        app.init_resource::<CaptureState>()?;
        app.add_systems(
            CoreStage::Render,
            retain_stock_input
                .after(WgpuPolicyCaptureSet::Seal)
                .before(WgpuPolicyCaptureSet::Publish),
        )?;
        Ok(())
    }
}

#[derive(Clone)]
struct SealedGameFrame {
    input: WgpuRenderInput,
    runtime: RuntimeGeneration,
    app_frame: u64,
    timestep_nanos: u64,
    snapshot: GameSnapshot,
    source: SourceDocumentKey,
    stamp: ProjectSceneContentStamp,
    instance: SpawnedSceneInstance,
    source_view: SceneSourceView,
}

#[derive(Default, Resource)]
struct CaptureState {
    frames: VecDeque<SealedGameFrame>,
    failure: Option<String>,
}

fn retain_stock_input(
    reader: WgpuPolicyFrameReader,
    runtime: Res<RuntimeGeneration>,
    frame: Res<FrameExecutionStart>,
    time: Res<FixedTime>,
    current: CurrentScene,
    snapshot: Res<GameSnapshot>,
    cameras: Query<&Camera2d>,
    mut state: ResMut<CaptureState>,
) {
    let input = match reader.read() {
        Ok(Some(input)) => input,
        Ok(None) => return,
        Err(error) => {
            state.failure = Some(format!("original stock Seal failed: {error}"));
            return;
        }
    };
    if !snapshot.initialized || snapshot.phase != GamePhase::Running || snapshot.restoring {
        return;
    }
    if snapshot.tick != time.tick()
        || snapshot.current_instance != Some(current.receipt().instance_id())
    {
        return;
    }
    let Some(stamp) = current.source_content_stamp() else {
        state.failure = Some("desktop evidence requires an admitted source Scene".into());
        return;
    };
    let Ok(timestep_nanos) = u64::try_from(time.timestep().as_nanos()) else {
        state.failure = Some("original game fixed timestep cannot be represented".into());
        return;
    };
    let [view] = input.packet().views() else {
        state.failure = Some("the shipped game requires one original camera View".into());
        return;
    };
    if cameras.iter().count() != 1
        || cameras.get(view.camera_entity).is_err()
        || input.packet().source_generation().runtime_generation() != Some(*runtime)
    {
        state.failure =
            Some("sealed View/camera/source does not belong to the original game Runtime".into());
        return;
    }
    if state.frames.len() == MAX_RETAINED_FRAMES {
        state.frames.pop_front();
    }
    let source_view = current.source_view();
    let Some(source) = source_view.with_compiled_scene(|scene, _| scene.source_key().clone())
    else {
        state.failure =
            Some("desktop evidence requires the original admitted source document".into());
        return;
    };
    state.frames.push_back(SealedGameFrame {
        input: input.clone(),
        runtime: *runtime,
        app_frame: frame.frame(),
        timestep_nanos,
        snapshot: snapshot.clone(),
        source,
        stamp,
        instance: current.receipt().clone(),
        source_view,
    });
}

/// Copied fields of the original unforgeable Host receipt; no GPU handles or present authority.
#[derive(Clone, Copy, Debug)]
pub struct NativePresentation {
    pub runtime: RuntimeGeneration,
    pub app_frame: u64,
    pub render_frame: u64,
    pub window: RenderWindowPacket,
    pub surface_generation: u64,
    pub backend_instance: u64,
    pub device_epoch: u64,
}

/// Constructed only after exact original stock-input/presentation correlation.
#[derive(Clone)]
pub struct PresentedGameFrame {
    sealed: SealedGameFrame,
    pub presentation: NativePresentation,
    pub adapter: WgpuAdapterSummary,
}

impl PresentedGameFrame {
    pub fn input(&self) -> &WgpuRenderInput {
        &self.sealed.input
    }

    pub fn snapshot(&self) -> &GameSnapshot {
        &self.sealed.snapshot
    }

    pub fn source(&self) -> &SourceDocumentKey {
        &self.sealed.source
    }

    pub fn source_stamp(&self) -> ProjectSceneContentStamp {
        self.sealed.stamp
    }

    pub fn scene_instance(&self) -> &SpawnedSceneInstance {
        &self.sealed.instance
    }

    /// Checks the sealed game semantics and actual captured stock objects against this state.
    /// This does not replace the caller's complete authoritative capture comparison.
    pub fn assert_expected(&self, expected: &ResumeState) -> Result<()> {
        oracle::assert_frame(&self.sealed, expected)
    }
}

/// Joins the original receipt to its Seal record, never to the callback's newer game projection.
/// Suitable for an existing main-thread native-control observer that records every presentation.
/// None means no receipt, or a seal skipped while startup/reconstruction was incomplete.
pub fn join_presented(
    observation: DesktopPresentationObservation<'_>,
) -> Result<Option<PresentedGameFrame>> {
    let Some(world) = observation.world() else {
        return Ok(None);
    };
    let state = world
        .get_resource::<CaptureState>()
        .ok_or_else(|| io::Error::other("install DesktopCapturePlugin in the original recipe"))?;
    if let Some(failure) = &state.failure {
        return Err(io::Error::other(failure.clone()).into());
    }
    let Some(receipt) = observation.presented_frame() else {
        return Ok(None);
    };
    let Some(frame) = state.frames.iter().find(|frame| {
        receipt.runtime_generation() == frame.runtime
            && receipt.app_frame_index() == frame.app_frame
            && receipt.render_transaction_frame_index() == frame.input.packet().frame_index()
    }) else {
        if state.frames.front().is_some_and(|oldest| {
            receipt.runtime_generation() == oldest.runtime
                && receipt.app_frame_index() < oldest.app_frame
        }) {
            return Err(io::Error::other(
                "original presentation exceeded the eight-seal capture history",
            )
            .into());
        }
        return Ok(None);
    };
    require(
        observation.runtime_generation() == frame.runtime
            && frame
                .input
                .packet()
                .source_generation()
                .runtime_generation()
                == Some(frame.runtime),
        "presentation no longer names the original Runtime/source generation",
    )?;
    let RenderFrameTarget::Window(window) = frame.input.packet().target() else {
        return Err(io::Error::other("original game capture must target a native window").into());
    };
    let backend = observation.wgpu();
    let transaction = backend.frame_transaction_stats();
    require(
        receipt.window_id() == window.id
            && receipt.surface_generation() > 0
            && receipt.backend_instance_id() != 0
            && receipt.device_epoch() > 0
            && backend.state() == WgpuBackendState::Ready
            && receipt.backend_instance_id() == backend.instance_id()
            && receipt.device_epoch() == backend.device_epoch()
            && backend.configured_surface_extent(window.id)
                == Some(frame.input.packet().target().extent())
            && transaction.app_frame_index() == Some(frame.app_frame)
            && transaction.render_transaction_frame_index()
                == Some(frame.input.packet().frame_index())
            && transaction.queue_submissions() > 0
            && transaction.presents() > 0,
        "original window/surface/backend/device/transaction presentation facts disagree",
    )?;
    let adapter = observation
        .adapter_summary()
        .ok_or_else(|| io::Error::other("native presentation has no adapter observation"))?;
    require(
        adapter.device_type() != "cpu",
        "software adapters do not qualify native evidence",
    )?;
    require(
        matches!(adapter.backend(), "dx12" | "vulkan" | "metal"),
        "desktop evidence requires a native graphics backend",
    )?;
    Ok(Some(PresentedGameFrame {
        sealed: frame.clone(),
        presentation: NativePresentation {
            runtime: receipt.runtime_generation(),
            app_frame: receipt.app_frame_index(),
            render_frame: receipt.render_transaction_frame_index(),
            window,
            surface_generation: receipt.surface_generation(),
            backend_instance: receipt.backend_instance_id(),
            device_epoch: receipt.device_epoch(),
        },
        adapter: adapter.clone(),
    }))
}

fn require(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(io::Error::other(message.into()).into())
    }
}
