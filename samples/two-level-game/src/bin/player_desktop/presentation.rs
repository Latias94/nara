use std::{
    any::Any,
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use nara::{
    app::{AppExit, FixedTime, FrameExecutionStart, RuntimeGeneration},
    asset::AssetServer,
    ecs::World,
    image::{ImageAsset, image_resource_key},
    project_host::{DesktopPresentationControl, DesktopPresentationObservation},
    render::RenderFrame,
    render_wgpu::{WgpuBackendState, WgpuRenderObservation},
    sprite_render::__private::SpriteBatches,
    window::{
        WindowId,
        backend::{BackendWindowHandles, WindowTargetPhase},
    },
};
use serde_json::{Value, json};
use two_level_game::{GamePhase, GameSnapshot, Scenario};

use super::{
    desktop_output::{Emitter, Record},
    evidence::Evidence,
    generated, output,
};

pub(super) type Panic = Box<dyn Any + Send + 'static>;
pub(super) type SharedPresentation = Rc<RefCell<Presentation>>;

pub(super) struct Frame {
    pub runtime_generation: u64,
    pub app_frame: u64,
    render_frame: u64,
    window_id: u64,
    surface_generation: u64,
    backend_instance: u64,
    device_epoch: u64,
    queue_submissions: u64,
    presents: u64,
    adapter: Value,
    side_room_sprites: Option<usize>,
}

impl Frame {
    fn facts(&self) -> Value {
        json!({
            "runtime_generation": self.runtime_generation,
            "app_frame": self.app_frame,
            "render_frame": self.render_frame,
            "window_id": self.window_id,
            "surface_generation": self.surface_generation,
            "backend_instance": self.backend_instance,
            "device_epoch": self.device_epoch,
            "queue_submissions": self.queue_submissions,
            "presents": self.presents,
            "side_room_sprites": self.side_room_sprites,
        })
    }
}

pub(super) struct Presentation {
    pub evidence: Evidence,
    pub frame: Option<Frame>,
    pub frames: u64,
    pub launch_admitted: bool,
    pub launch_required: bool,
    pub launch_accepted: bool,
    pub resume_applied: bool,
    pub ready: bool,
    pub failure: Option<String>,
    pub panic: Option<Panic>,
    pub emitter: Emitter,
    observe: bool,
    scenario: Scenario,
    scene_receipts_sampled: bool,
    semantic: Option<Value>,
}

impl Presentation {
    pub(super) fn new(observe: bool, scenario: Scenario) -> SharedPresentation {
        Rc::new(RefCell::new(Self {
            evidence: Evidence::default(),
            frame: None,
            frames: 0,
            launch_admitted: false,
            launch_required: false,
            launch_accepted: false,
            resume_applied: false,
            ready: false,
            failure: None,
            panic: None,
            emitter: Emitter::default(),
            observe,
            scenario,
            scene_receipts_sampled: false,
            semantic: None,
        }))
    }

    pub(super) fn record(
        &self,
        scenario: Scenario,
        message: Option<&str>,
        save: Option<Value>,
    ) -> Value {
        let mut record = self.evidence.result(scenario, message, save);
        record["catalog_hash"] = json!(generated::CATALOG_HASH);
        record["catalog_length"] = json!(generated::CATALOG_LENGTH);
        record["content_target"] = json!(generated::CONTENT_TARGET);
        record["content_profile"] = json!(generated::CONTENT_PROFILE);
        record["presentation"] = self.frame.as_ref().map(Frame::facts).into();
        record["adapter"] = self
            .frame
            .as_ref()
            .map(|frame| frame.adapter.clone())
            .into();
        record["first_present_verified"] = json!(self.frames > 0);
        record["presented_frames"] = json!(self.frames);
        record["launch_accepted"] = json!(self.launch_accepted);
        record["launch_admitted"] = json!(self.launch_admitted);
        record["resume_applied"] = json!(self.resume_applied);
        record["ready"] = json!(self.ready);
        record["original_runtime_retired"] = json!(false);
        record["original_pump_returned"] = json!(false);
        record["cleanup_complete"] = json!(false);
        record["scene_receipts_sampled"] = json!(self.scene_receipts_sampled);
        record["gameplay_error"] = self
            .evidence
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.error.as_deref())
            .map(output::bounded)
            .into();
        record
    }

    fn observe(&mut self, observation: &DesktopPresentationObservation<'_>) -> Result<(), String> {
        if self.failure.is_some() || self.panic.is_some() {
            return Ok(());
        }
        let Some(frame) = join_presented(observation, self.observe)? else {
            return Ok(());
        };
        if let Some(previous) = &self.frame
            && (frame.runtime_generation != previous.runtime_generation
                || frame.window_id != previous.window_id
                || frame.backend_instance != previous.backend_instance
                || frame.app_frame <= previous.app_frame
                || frame.render_frame <= previous.render_frame)
        {
            return Err(
                "desktop presentation left its original Runtime, window, Host or frame order"
                    .into(),
            );
        }
        let world = observation
            .world()
            .ok_or("the presented World is unavailable")?;
        self.frames = self
            .frames
            .checked_add(1)
            .ok_or("desktop presented-frame counter exhausted")?;
        self.frame = Some(frame);
        self.observe_world(world)?;
        if let Some(snapshot) = &self.evidence.snapshot
            && snapshot.tick != world.resource::<FixedTime>().tick()
        {
            return Err(
                "presented gameplay snapshot does not match the original fixed clock".into(),
            );
        }
        let error = self
            .evidence
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.error.clone());
        if self.launch_admitted
            && !self.launch_accepted
            && self
                .evidence
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.tick > 0)
        {
            match two_level_game::player_launch_result(world) {
                Some(Ok(())) => {}
                None if !self.launch_required => {}
                Some(Err(error)) => {
                    return Err(format!(
                        "first-tick desktop launch was rejected: {}",
                        output::bounded(error)
                    ));
                }
                None => {
                    return Err(
                        "the first fixed tick did not publish the admitted Player launch result"
                            .into(),
                    );
                }
            }
            self.launch_accepted = true;
        }
        if self.observe {
            let semantic = self
                .evidence
                .snapshot
                .as_ref()
                .map(semantic_state)
                .unwrap_or(Value::Null);
            if self.semantic.as_ref() != Some(&semantic) {
                let record = self.record(self.scenario, None, None);
                self.emitter.emit(Record::Observation, record)?;
                self.semantic = Some(semantic);
            }
        }
        if let Some(error) = error
            && self.scenario != Scenario::Interactive
        {
            return Err(format!(
                "desktop gameplay failed: {}",
                output::bounded(&error)
            ));
        }
        if !self.ready
            && self.launch_accepted
            && self.resume_applied
            && self.evidence.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.initialized
                    && snapshot.tick > 0
                    && snapshot.phase == GamePhase::Running
                    && !snapshot.restoring
            })
        {
            let mut record = self.record(self.scenario, None, None);
            record["ready"] = json!(true);
            self.emitter.emit(Record::Ready, record)?;
            self.ready = true;
        }
        Ok(())
    }

    fn observe_world(&mut self, world: &World) -> Result<(), String> {
        if self.scenario == Scenario::Interactive {
            self.scene_receipts_sampled |= self.evidence.observe_world_sampled(world)?;
            Ok(())
        } else {
            self.evidence.observe_world(world)
        }
    }
}

pub(super) fn observer(
    shared: &SharedPresentation,
    observation: DesktopPresentationObservation<'_>,
) -> DesktopPresentationControl {
    let result = catch_unwind(AssertUnwindSafe(|| {
        shared.borrow_mut().observe(&observation)
    }));
    let mut state = shared.borrow_mut();
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            state.failure.get_or_insert(error);
        }
        Err(panic) => {
            let detail = panic_detail(panic.as_ref());
            state
                .failure
                .get_or_insert_with(|| format!("desktop presentation observer panicked: {detail}"));
            if state.panic.is_none() {
                state.panic = Some(panic);
            }
        }
    }
    if state.failure.is_some() {
        DesktopPresentationControl::Exit(AppExit::Requested)
    } else {
        DesktopPresentationControl::Continue
    }
}

pub(super) fn panic_detail(panic: &(dyn Any + Send)) -> String {
    output::bounded(
        panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("non-string panic payload"),
    )
}

fn join_presented(
    observation: &DesktopPresentationObservation<'_>,
    observe: bool,
) -> Result<Option<Frame>, String> {
    let Some(receipt) = observation.presented_frame() else {
        return Ok(None);
    };
    let world = observation
        .world()
        .ok_or("the original presented World is unavailable")?;
    let runtime = world
        .get_resource::<RuntimeGeneration>()
        .ok_or("the presented Runtime identity is unavailable")?;
    let start = world
        .get_resource::<FrameExecutionStart>()
        .ok_or("the presented App frame start is unavailable")?;
    let backend = observation.wgpu();
    let transaction = backend.frame_transaction_stats();
    let published = world
        .get_resource::<WgpuRenderObservation>()
        .ok_or("the original published WGPU observation is unavailable")?;
    if receipt.runtime_generation() != observation.runtime_generation()
        || receipt.runtime_generation() != *runtime
        || receipt.app_frame_index() != start.frame()
        || transaction.app_frame_index() != Some(start.frame())
        || transaction.render_transaction_frame_index()
            != Some(receipt.render_transaction_frame_index())
        || receipt.window_id() != WindowId::PRIMARY
        || receipt.surface_generation() == 0
        || receipt.backend_instance_id() == 0
        || receipt.device_epoch() == 0
        || backend.state() != WgpuBackendState::Ready
        || backend.instance_id() != receipt.backend_instance_id()
        || backend.device_epoch() != receipt.device_epoch()
        || backend
            .configured_surface_extent(receipt.window_id())
            .is_none()
        || transaction.queue_submissions() == 0
        || transaction.presents() == 0
        || published.state() != WgpuBackendState::Ready
        || published.instance_id() != backend.instance_id()
        || published.device_epoch() != backend.device_epoch()
        || published.frame_transaction_stats() != transaction
    {
        return Err("original Runtime/App/render transaction/window/WGPU receipt do not match the actual present".into());
    }
    let handles = world
        .get_resource::<BackendWindowHandles>()
        .ok_or("the original native window registry is unavailable")?;
    let target = handles
        .snapshot(receipt.window_id())
        .map_err(|error| format!("presented window: {error}"))?;
    if target.phase != WindowTargetPhase::Active
        || target.fault.is_some()
        || !target.surface_active
        || !target.provider_present
        || target.native_destroyed
        || target.surface_generation != Some(receipt.surface_generation())
    {
        return Err(
            "the presented surface receipt does not match the original live window registry".into(),
        );
    }
    let adapter = observation
        .adapter_summary()
        .ok_or("the original presented adapter summary is unavailable")?;
    let side_room_sprites = observe
        .then(|| side_room_sprites(world, receipt.render_transaction_frame_index()))
        .transpose()?;
    Ok(Some(Frame {
        runtime_generation: runtime.get(),
        app_frame: start.frame(),
        render_frame: receipt.render_transaction_frame_index(),
        window_id: receipt.window_id().get(),
        surface_generation: receipt.surface_generation(),
        backend_instance: receipt.backend_instance_id(),
        device_epoch: receipt.device_epoch(),
        queue_submissions: transaction.queue_submissions(),
        presents: transaction.presents(),
        side_room_sprites,
        adapter: json!({
            "backend": adapter.backend(),
            "name": adapter.name(),
            "device_type": adapter.device_type(),
        }),
    }))
}

fn side_room_sprites(world: &World, frame: u64) -> Result<usize, String> {
    if !world
        .get_resource::<RenderFrame>()
        .is_some_and(|render| render.index == frame)
    {
        return Err("stock Sprite batches are from a different render frame".into());
    }
    let batches = world
        .get_resource::<SpriteBatches>()
        .ok_or("original stock Sprite batches are unavailable")?;
    let Some(relic) = world
        .get_resource::<AssetServer>()
        .and_then(|assets| {
            assets.get_product_handle::<ImageAsset>(two_level_game::side_room::image_product())
        })
        .map(image_resource_key)
    else {
        return Ok(0);
    };
    Ok(batches
        .as_slice()
        .iter()
        .filter(|batch| {
            batch.target.window_id(Some(WindowId::PRIMARY)) == Some(WindowId::PRIMARY)
                && batch
                    .image
                    .as_ref()
                    .is_some_and(|image| image.identity().key() == relic)
        })
        .flat_map(|batch| &batch.instances)
        .filter(|sprite| sprite.color.a > 0.0)
        .count())
}

fn semantic_state(snapshot: &GameSnapshot) -> Value {
    json!({
        "level": snapshot.level,
        "player": snapshot.player.to_array(),
        "score": snapshot.score,
        "collected": snapshot.collected,
        "visible_pickups": snapshot.visible_pickups,
        "markers": snapshot.markers,
        "travel_count": snapshot.travel_count,
        "restarts": snapshot.restarts,
        "saves": snapshot.saves,
        "restores": snapshot.restores,
        "phase": format!("{:?}", snapshot.phase),
        "initialized": snapshot.initialized,
        "verified": snapshot.verified,
        "error": snapshot.error.as_deref().map(output::bounded),
        "current_instance": snapshot.current_instance.map(|id| id.get()),
        "scene_members": snapshot.scene_members,
        "side_room": snapshot.side_room,
        "held": snapshot.held,
        "restoring": snapshot.restoring,
    })
}
