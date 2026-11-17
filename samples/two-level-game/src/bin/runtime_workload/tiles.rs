//! Bounded CPU capture of the original TilemapTravel presentations.
//!
//! This plugin owns no GPU state. The four-entry ledger retains original Arc-backed stock
//! inputs between Seal and Publish; a caller can retain at most three selected presentations
//! for independent image replay after the original World and Scene owner have retired.

#[path = "tiles/oracle.rs"]
mod oracle;
#[path = "tiles/replay.rs"]
mod replay;

pub use replay::replay;

use std::{
    collections::VecDeque,
    sync::{Arc, Weak},
};

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
    tilemap::Tilemap,
    window::{
        WindowId,
        backend::{BackendWindowHandles, WindowTargetPhase},
    },
};
use two_level_game::{GamePhase, GameSnapshot, levels::AUTHORED_LEVEL_MEMBERS};

const WINDOW_EXTENT: Extent2d = Extent2d {
    width: 960,
    height: 540,
};
const CANVAS_EXTENT: Extent2d = Extent2d {
    width: 320,
    height: 180,
};
const MAX_SEALS: usize = 4;
const MAX_REPLAYS: usize = 3;
const DECLARATION: PluginDeclaration = PluginDeclaration::new(
    PluginId::new("two-level-game.runtime-workload.tile-capture"),
    PluginCategory::Runtime,
)
.requires_plugins(&[
    PluginId::new("two-level-game.exploration"),
    WGPU_CAPTURE_PLUGIN_ID,
    WGPU_STOCK_SPRITE_PLUGIN_ID,
    WGPU_STOCK_POLICY_PLUGIN_ID,
]);

#[derive(Default)]
pub struct TileCapturePlugin;

impl Plugin for TileCapturePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<CaptureLedger>()?;
        app.add_systems(
            CoreStage::Render,
            capture
                .after(WgpuPolicyCaptureSet::Seal)
                .before(WgpuPolicyCaptureSet::Publish),
        )?;
        Ok(())
    }
}

#[derive(Default, Resource)]
struct CaptureLedger {
    seals: VecDeque<Seal>,
    failure: Option<String>,
    world_lifetime: Arc<()>,
}

#[derive(Clone)]
struct Seal {
    input: WgpuRenderInput,
    runtime: RuntimeGeneration,
    app_frame: u64,
    game: Option<CapturedGame>,
}

#[derive(Clone)]
struct CapturedGame {
    snapshot: GameSnapshot,
    source: SourceDocumentKey,
    stamp: ProjectSceneContentStamp,
    instance: SpawnedSceneInstance,
    source_view: SceneSourceView,
    world_lifetime: Weak<()>,
    population: usize,
    maps: usize,
}

fn capture(
    reader: WgpuPolicyFrameReader,
    runtime: Res<RuntimeGeneration>,
    frame: Res<FrameExecutionStart>,
    time: Res<FixedTime>,
    current: CurrentScene,
    snapshot: Res<GameSnapshot>,
    cameras: Query<&Camera2d>,
    maps: Query<&Tilemap>,
    mut ledger: ResMut<CaptureLedger>,
) {
    if ledger.failure.is_some() {
        return;
    }
    let input = match reader.read() {
        Ok(Some(input)) => input,
        Ok(None) => return,
        Err(error) => {
            ledger.failure = Some(format!("original TilemapTravel Seal failed: {error}"));
            return;
        }
    };
    let game = (|| -> Result<Option<CapturedGame>, String> {
        if let Some(error) = &snapshot.error {
            return Err(format!("original TilemapTravel failed: {error}"));
        }
        if !snapshot.initialized || snapshot.phase != GamePhase::Running || snapshot.restoring {
            return Ok(None);
        }
        require(
            snapshot.tick == time.tick()
                && snapshot.current_instance == Some(current.receipt().instance_id())
                && snapshot.scene_members == current.receipt().entity_ids().len()
                && snapshot.scene_members == AUTHORED_LEVEL_MEMBERS,
            "initialized original snapshot disagrees with its fixed clock or CurrentScene receipt",
        )?;
        let [view] = input.packet().views() else {
            return Err("TilemapTravel requires exactly one original camera View".into());
        };
        require(
            cameras.iter().count() == 1
                && cameras.get(view.camera_entity).is_ok()
                && input.packet().source_generation().runtime_generation() == Some(*runtime),
            "original sealed View/camera/source does not belong to this Runtime",
        )?;
        let source_view = current.source_view();
        let source = source_view
            .with_compiled_scene(|scene, _| scene.source_key().clone())
            .ok_or("TilemapTravel requires its original admitted source Scene")?;
        let stamp = current
            .source_content_stamp()
            .ok_or("TilemapTravel requires the original Project Scene content stamp")?;
        let mut population = 0_usize;
        let mut map_count = 0;
        for map in &maps {
            population = population
                .checked_add(map.cell_count())
                .ok_or("original Tilemap cell count overflowed")?;
            map_count += 1;
        }
        Ok(Some(CapturedGame {
            snapshot: snapshot.clone(),
            source,
            stamp,
            instance: current.receipt().clone(),
            source_view,
            world_lifetime: Arc::downgrade(&ledger.world_lifetime),
            population,
            maps: map_count,
        }))
    })();
    let game = match game {
        Ok(game) => game,
        Err(error) => {
            ledger.failure = Some(error);
            return;
        }
    };
    if ledger.seals.len() == MAX_SEALS {
        ledger.seals.pop_front();
    }
    ledger.seals.push_back(Seal {
        input: input.clone(),
        runtime: *runtime,
        app_frame: frame.frame(),
        game,
    });
}

/// Copied facts from the original unforgeable desktop receipt, with no presentation authority.
#[derive(Clone, Copy, Debug)]
struct NativePresentation {
    runtime: RuntimeGeneration,
    app_frame: u64,
    render_frame: u64,
    window: RenderWindowPacket,
    surface_generation: u64,
    backend_instance: u64,
    device_epoch: u64,
}

/// Constructible only by joining the original receipt to its exact private Seal record.
#[derive(Clone)]
pub struct PresentedTileFrame {
    input: WgpuRenderInput,
    game: CapturedGame,
    presentation: NativePresentation,
    adapter: WgpuAdapterSummary,
}

impl PresentedTileFrame {
    pub fn snapshot(&self) -> &GameSnapshot {
        &self.game.snapshot
    }

    /// Both the private World resource and original Scene owner must have disappeared.
    pub fn original_source_retired(&self) -> bool {
        self.game.world_lifetime.upgrade().is_none()
            && self.game.source_view.with_document(|_| ()).is_none()
    }
}

/// None means no actual receipt or an exactly identified, not-yet-initialized Seal.
/// A missing, evicted or mismatched actual receipt fails closed instead of using newer state.
pub(super) fn join(
    observation: &DesktopPresentationObservation<'_>,
    population: usize,
) -> Result<Option<PresentedTileFrame>, String> {
    let Some(receipt) = observation.presented_frame() else {
        return Ok(None);
    };
    oracle::tile_side(population)?;
    let world = observation
        .world()
        .ok_or("presented original World is unavailable")?;
    let ledger = world
        .get_resource::<CaptureLedger>()
        .ok_or("install TileCapturePlugin in the original TilemapTravel recipe")?;
    if let Some(error) = &ledger.failure {
        return Err(error.clone());
    }
    let seal = ledger
        .seals
        .iter()
        .find(|seal| {
            seal.runtime == receipt.runtime_generation()
                && seal.app_frame == receipt.app_frame_index()
                && seal.input.packet().frame_index() == receipt.render_transaction_frame_index()
        })
        .ok_or(
            "actual TilemapTravel receipt has no exact original input in the four-Seal ledger",
        )?;
    let backend = observation.wgpu();
    let transaction = backend.frame_transaction_stats();
    let RenderFrameTarget::Window(window) = seal.input.packet().target() else {
        return Err("original TilemapTravel input must target its native window".into());
    };
    require(
        observation.runtime_generation() == seal.runtime
            && world.get_resource::<RuntimeGeneration>() == Some(&seal.runtime)
            && seal.input.packet().source_generation().runtime_generation() == Some(seal.runtime)
            && receipt.window_id() == window.id
            && window.id == WindowId::PRIMARY
            && receipt.surface_generation() > 0
            && receipt.backend_instance_id() != 0
            && receipt.device_epoch() > 0
            && backend.state() == WgpuBackendState::Ready
            && receipt.backend_instance_id() == backend.instance_id()
            && receipt.device_epoch() == backend.device_epoch()
            && backend.configured_surface_extent(window.id) == Some(WINDOW_EXTENT)
            && seal.input.packet().target().extent() == WINDOW_EXTENT
            && transaction.app_frame_index() == Some(seal.app_frame)
            && transaction.render_transaction_frame_index()
                == Some(seal.input.packet().frame_index())
            && transaction.queue_submissions() > 0
            && transaction.presents() > 0,
        "original Runtime/App/render/window/backend/device/transaction receipt facts disagree",
    )?;
    let native = world
        .get_resource::<BackendWindowHandles>()
        .ok_or("original native window registry is unavailable")?
        .snapshot(window.id)
        .map_err(|error| error.to_string())?;
    require(
        native.phase == WindowTargetPhase::Active
            && native.fault.is_none()
            && native.surface_active
            && native.provider_present
            && !native.native_destroyed
            && native.surface_generation == Some(receipt.surface_generation()),
        "original TilemapTravel receipt does not match its live native surface",
    )?;
    let Some(game) = &seal.game else {
        return Ok(None);
    };
    require(
        game.population == population && game.maps == if population == 16_384 { 2 } else { 1 },
        "original initialized World has a different Tilemap population or map count",
    )?;
    let adapter = observation
        .adapter_summary()
        .ok_or("original presentation has no adapter summary")?;
    require(
        matches!(adapter.backend(), "dx12" | "vulkan" | "metal")
            && matches!(adapter.device_type(), "discrete-gpu" | "integrated-gpu"),
        "TilemapTravel evidence requires an actual native hardware adapter",
    )?;
    let frame = PresentedTileFrame {
        input: seal.input.clone(),
        game: game.clone(),
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
    };
    oracle::assert_packet(&frame)?;
    Ok(Some(frame))
}

fn require(condition: bool, message: impl Into<String>) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
