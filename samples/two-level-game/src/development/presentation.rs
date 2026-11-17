//! Bounded CPU witnesses joining canonical game state to the original native presentation.
//!
//! This plugin observes stock Seal/Publish and the existing desktop Host receipt. It owns no
//! transport, native window, device or GPU submission. A witness proves the original submit and
//! present calls, not GPU completion or scanout. Delayed receipts outside eight seals fail closed.

use std::collections::VecDeque;

use nara::{
    app::{
        App, CoreStage, FixedTime, FrameExecutionStart, Plugin, PluginCategory, PluginDeclaration,
        PluginError, PluginId, RuntimeGeneration, RuntimeTimeContinuation, RuntimeTimeSettings,
    },
    ecs::{Resource, World, bevy_ecs::system::SystemState, schedule::IntoScheduleConfigs},
    fs::ContentDigest,
    input::{InputSeatId, InputWindowId, capture_keyboard_holds},
    project_host::DesktopPresentationObservation,
    render::RenderFrameTarget,
    render_wgpu::{
        WGPU_CAPTURE_PLUGIN_ID, WGPU_STOCK_POLICY_PLUGIN_ID, WGPU_STOCK_SPRITE_PLUGIN_ID,
        WgpuBackendState, WgpuPolicyCaptureSet, WgpuPolicyFrameReader, WgpuRenderInput,
    },
};

use crate::{
    gameplay::resume::capture_world,
    resume::{ResumeState, RuntimeTimeState, encode_resume},
};

use super::protocol::DesktopPresentation;

const MAX_RETAINED_SEALS: usize = 8;
const DECLARATION: PluginDeclaration = PluginDeclaration::new(
    PluginId::new("two-level-game.development.presentation"),
    PluginCategory::Runtime,
)
.requires_plugins(&[
    PluginId::new("two-level-game.exploration"),
    WGPU_CAPTURE_PLUGIN_ID,
    WGPU_STOCK_SPRITE_PLUGIN_ID,
    WGPU_STOCK_POLICY_PLUGIN_ID,
]);

/// Adds complete canonical state witnesses to the concrete source-desktop child recipe.
#[derive(Default)]
pub struct DevelopmentPresentationPlugin;

impl Plugin for DevelopmentPresentationPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<SealedStates>()?;
        app.add_systems(
            CoreStage::Render,
            retain_sealed_state
                .after(WgpuPolicyCaptureSet::Seal)
                .before(WgpuPolicyCaptureSet::Publish),
        )?;
        Ok(())
    }
}

struct SealedState {
    input: WgpuRenderInput,
    runtime: RuntimeGeneration,
    app_frame: u64,
    digest: [u8; 32],
}

#[derive(Default, Resource)]
struct SealedStates {
    frames: VecDeque<SealedState>,
    failure: Option<String>,
}

fn retain_sealed_state(
    world: &mut World,
    reader: &mut SystemState<WgpuPolicyFrameReader<'static>>,
) {
    let input = reader
        .get(world)
        .map_err(|error| format!("original stock Seal reader is unavailable: {error}"))
        .and_then(|reader| {
            reader
                .read()
                .map(|input| input.cloned())
                .map_err(|error| format!("original stock Seal failed: {error}"))
        });
    let input = match input {
        Ok(Some(input)) => input,
        Ok(None) => return,
        Err(error) => {
            world.resource_mut::<SealedStates>().failure = Some(error);
            return;
        }
    };
    let runtime = *world.resource::<RuntimeGeneration>();
    let app_frame = world.resource::<FrameExecutionStart>().frame();
    if input.packet().source_generation().runtime_generation() != Some(runtime) {
        world.resource_mut::<SealedStates>().failure =
            Some("stock input does not belong to the original Runtime".into());
        return;
    }
    let RenderFrameTarget::Window(window) = input.packet().target() else {
        world.resource_mut::<SealedStates>().failure =
            Some("source-desktop continuity requires a native window target".into());
        return;
    };
    let Some(time) = world_time(world) else {
        return;
    };
    let Ok(canonical) = capture_world(world, time) else {
        return;
    };
    let bindings = match crate::input::movement_bindings(world) {
        Ok(bindings) => bindings,
        Err(error) => {
            world.resource_mut::<SealedStates>().failure = Some(error);
            return;
        }
    };
    let Ok(holds) = capture_keyboard_holds(
        world,
        InputWindowId::new(window.id.get()),
        InputSeatId::PRIMARY,
        &bindings,
    ) else {
        return;
    };
    if holds.iter().map(|hold| hold.held).ne(canonical.held) {
        return;
    }
    let Ok(digest) = state_digest(&canonical) else {
        return;
    };
    let mut state = world.resource_mut::<SealedStates>();
    if state.frames.len() == MAX_RETAINED_SEALS {
        state.frames.pop_front();
    }
    state.frames.push_back(SealedState {
        input,
        runtime,
        app_frame,
        digest,
    });
}

fn world_time(world: &World) -> Option<RuntimeTimeState> {
    let fixed = world.resource::<FixedTime>();
    let pending = fixed.debt().checked_add(fixed.remainder())?;
    let continuation =
        RuntimeTimeContinuation::new(world.resource::<RuntimeTimeSettings>(), fixed, pending)
            .ok()?;
    RuntimeTimeState::from_continuation(continuation).ok()
}

/// Computes the protocol digest of the entire game-owned canonical encoding.
pub fn state_digest(state: &ResumeState) -> Result<[u8; 32], String> {
    let bytes = encode_resume(state).map_err(|error| error.to_string())?;
    Ok(*ContentDigest::of_bytes(&bytes).as_bytes())
}

/// Joins an actual original receipt to its exact sealed input and canonical state.
///
/// `None` includes startup, reconstruction and frames with unconsumed input. The caller can
/// request another original paused maintenance frame after capture becomes eligible. This
/// function never associates a receipt with the callback's potentially newer game snapshot.
pub fn join_presented(
    observation: DesktopPresentationObservation<'_>,
) -> Result<Option<DesktopPresentation>, String> {
    let Some(world) = observation.world() else {
        return Ok(None);
    };
    let state = world.get_resource::<SealedStates>().ok_or_else(|| {
        "the source-desktop recipe has no development presentation witness".to_owned()
    })?;
    if let Some(error) = &state.failure {
        return Err(error.clone());
    }
    let Some(receipt) = observation.presented_frame() else {
        return Ok(None);
    };
    let Some(sealed) = state.frames.iter().find(|sealed| {
        receipt.runtime_generation() == sealed.runtime
            && receipt.app_frame_index() == sealed.app_frame
            && receipt.render_transaction_frame_index() == sealed.input.packet().frame_index()
    }) else {
        if state.frames.front().is_some_and(|oldest| {
            receipt.runtime_generation() == oldest.runtime
                && receipt.app_frame_index() < oldest.app_frame
        }) {
            return Err("original presentation exceeded the eight-seal witness history".into());
        }
        return Ok(None);
    };
    let RenderFrameTarget::Window(window) = sealed.input.packet().target() else {
        return Err("original sealed input is not a native window frame".into());
    };
    let backend = observation.wgpu();
    let transaction = backend.frame_transaction_stats();
    if observation.runtime_generation() != sealed.runtime
        || sealed
            .input
            .packet()
            .source_generation()
            .runtime_generation()
            != Some(sealed.runtime)
        || receipt.window_id() != window.id
        || receipt.surface_generation() == 0
        || receipt.backend_instance_id() == 0
        || receipt.device_epoch() == 0
        || backend.state() != WgpuBackendState::Ready
        || receipt.backend_instance_id() != backend.instance_id()
        || receipt.device_epoch() != backend.device_epoch()
        || backend.configured_surface_extent(window.id)
            != Some(sealed.input.packet().target().extent())
        || transaction.app_frame_index() != Some(sealed.app_frame)
        || transaction.render_transaction_frame_index() != Some(sealed.input.packet().frame_index())
        || transaction.queue_submissions() == 0
        || transaction.presents() == 0
    {
        return Err("original Runtime/window/surface/backend/device/presentation disagree".into());
    }
    let adapter = observation
        .adapter_summary()
        .ok_or("the original native presentation has no adapter summary")?;
    Ok(Some(DesktopPresentation {
        runtime_generation: sealed.runtime.get(),
        app_frame: sealed.app_frame,
        render_frame: sealed.input.packet().frame_index(),
        window: window.id.get(),
        surface_generation: receipt.surface_generation(),
        backend_instance: receipt.backend_instance_id(),
        device_epoch: receipt.device_epoch(),
        state_digest: sealed.digest,
        adapter_backend: adapter.backend().to_owned(),
        adapter_type: adapter.device_type().to_owned(),
    }))
}
