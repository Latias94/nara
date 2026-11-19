use std::{
    any::Any,
    cell::RefCell,
    collections::BTreeSet,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use nara::{
    app::{AppExit, FixedTime, FrameExecutionStart, RuntimeGeneration},
    asset::{AssetServer, AssetStates, Assets},
    core::Vec2,
    ecs::World,
    image::{ImageAsset, PreparedImages, image_resource_key},
    material::Material2dAsset,
    project_host::{DesktopPresentationControl, DesktopPresentationObservation},
    render::{ExtractedViews, RenderFrame},
    render_wgpu::{WgpuBackendState, WgpuRenderObservation},
    scene::SceneEntitySource,
    sprite::Sprite,
    sprite_render::__private::{
        ExtractedSpriteKind, ExtractedSprites, QueuedSpriteItems, SpriteBatchKind, SpriteBatches,
        SpriteInstance,
    },
    tilemap::{TileSet, TileSetAssets, Tilemap},
    transform::presented_spatial_snapshot,
    window::{
        WindowId,
        backend::{BackendWindowHandles, WindowTargetPhase},
    },
};
use serde_json::{Value, json};
use two_level_game::{GamePhase, GameSnapshot, Scenario, region_material, region_tileset};

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
    controllers: ControllerObservation,
    side_rooms: Option<RoomFrame>,
}

#[derive(Clone, Copy, serde::Serialize)]
struct ControllerObservation {
    backend: &'static str,
    connected: usize,
}

#[derive(Default)]
struct RoomFrame {
    sprites: usize,
    image: Option<RoomImage>,
    material: Option<RoomMaterial>,
    tileset: Option<RoomTileSet>,
    tilemaps: usize,
    tile_cells: usize,
}

#[derive(serde::Serialize)]
struct RoomImage {
    asset_id: u64,
    version: u64,
}

#[derive(serde::Serialize)]
struct RoomMaterial {
    product: String,
    asset_id: u64,
    version: u64,
    declaration: String,
    declaration_version: u32,
    generation_sequence: u64,
    shader_digest: [u8; 32],
    interface_digest: [u8; 32],
    source_inputs_available: bool,
}

#[derive(serde::Serialize)]
struct RoomTileSet {
    product: String,
    asset_id: u64,
    version: u64,
    declaration: Value,
    image: Value,
    cpu_tilemaps: usize,
    cpu_tile_cells: usize,
    observation: &'static str,
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
            "side_room_sprites": self.side_rooms.as_ref().map(|rooms| rooms.sprites),
            "side_room_image": self.side_rooms.as_ref().and_then(|rooms| rooms.image.as_ref()),
            "side_room_material": self.side_rooms.as_ref().and_then(|rooms| rooms.material.as_ref()),
            "side_room_tileset": self.side_rooms.as_ref().and_then(|rooms| rooms.tileset.as_ref()),
            "side_room_tilemaps": self.side_rooms.as_ref().map(|rooms| rooms.tilemaps),
            "side_room_tile_cells": self.side_rooms.as_ref().map(|rooms| rooms.tile_cells),
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
        record["controllers"] = self
            .frame
            .as_ref()
            .map(|frame| json!(frame.controllers))
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
    let side_rooms = observe
        .then(|| observe_side_rooms(world, receipt.render_transaction_frame_index()))
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
        side_rooms,
        controllers: observe_controllers(world),
        adapter: json!({
            "backend": adapter.backend(),
            "name": adapter.name(),
            "device_type": adapter.device_type(),
        }),
    }))
}

fn observe_controllers(world: &World) -> ControllerObservation {
    #[cfg(feature = "gamepad")]
    if let Some(devices) = world.get_resource::<nara_gilrs::GilrsDevices>() {
        return ControllerObservation {
            backend: "gilrs-wgi",
            connected: devices.connected().len(),
        };
    }
    #[cfg(not(feature = "gamepad"))]
    let _ = world;
    ControllerObservation {
        backend: "none",
        connected: 0,
    }
}

fn observe_side_rooms(world: &World, frame: u64) -> Result<RoomFrame, String> {
    if !world
        .get_resource::<RenderFrame>()
        .is_some_and(|render| render.index == frame)
    {
        return Err("stock Sprite/Tilemap batches are from a different render frame".into());
    }
    let batches = world
        .get_resource::<SpriteBatches>()
        .ok_or("original stock Sprite batches are unavailable")?;
    let server = world
        .get_resource::<AssetServer>()
        .ok_or("original resident Product bindings are unavailable")?;
    let materials = world
        .get_resource::<Assets<Material2dAsset>>()
        .ok_or("original resident Materials are unavailable")?;
    let states = world
        .get_resource::<AssetStates>()
        .ok_or("original resident Asset versions are unavailable")?;
    let (tileset, tilemaps, tile_cells) = observe_floor_tiles(world, batches)?;
    let material_product = region_material::material_product();
    let material = server
        .get_product_handle::<Material2dAsset>(material_product)
        .and_then(|handle| materials.get(handle).map(|asset| (handle, asset)));
    let material_facts = material
        .map(|(handle, asset)| -> Result<RoomMaterial, String> {
            let generation = asset.generation();
            let version = states
                .state(handle.id())
                .and_then(|state| state.usable_version())
                .ok_or("resident region Material has no AssetVersion")?;
            if asset.product() != material_product
                || generation.declaration() != &region_material::declaration()
                || !generation.inputs().is_empty()
                || version.raw() == 0
                || states.published_slot_revision(handle.id())
                    != Some(&materials.slot_revision(handle))
            {
                return Err(
                    "resident region Material is not the source-free compiled Product".into(),
                );
            }
            Ok(RoomMaterial {
                product: material_product.to_string(),
                asset_id: handle.id().raw(),
                version: version.raw(),
                declaration: generation.declaration().id().to_owned(),
                declaration_version: generation.declaration().version(),
                generation_sequence: generation.sequence(),
                shader_digest: generation.shader().digest(),
                interface_digest: generation.shader().interface_digest(),
                source_inputs_available: !generation.inputs().is_empty(),
            })
        })
        .transpose()?;
    let product = two_level_game::side_room::image_product();
    let Some(handle) = server.get_product_handle::<ImageAsset>(product) else {
        if material.is_some()
            || tileset.is_some()
            || batches.as_slice().iter().any(|batch| {
                batch.material_extension.as_ref().is_some_and(|extension| {
                    extension.binding.generation().declaration().id()
                        == region_material::MATERIAL_ID
                })
            })
        {
            return Err("region Material outlived its resident relic Image".into());
        }
        return Ok(RoomFrame::default());
    };
    let relic = image_resource_key(handle);
    let assets = world
        .get_resource::<Assets<ImageAsset>>()
        .ok_or("original resident Images are unavailable")?;
    let prepared = world
        .get_resource::<PreparedImages>()
        .ok_or("original prepared Images are unavailable")?
        .get(relic)
        .map(|image| image.identity());
    match (assets.get(handle), prepared) {
        (Some(image), Some(capture))
            if image.product() == Some(product)
                && image.source().is_none()
                && capture.slot_revision() == &assets.slot_revision(handle) => {}
        (None, None) => {}
        _ => return Err("Meadow region Image capture disagrees with its resident Product".into()),
    }
    if prepared.is_some() != material.is_some() {
        return Err("Meadow region Image and Material residency disagree".into());
    }
    let mut sprites = 0;
    for batch in batches.as_slice() {
        if batch.kind != SpriteBatchKind::Sprite {
            continue;
        }
        let Some(image) = batch
            .image
            .as_ref()
            .filter(|image| image.identity().key() == relic)
        else {
            continue;
        };
        if Some(image.identity()) != prepared {
            return Err(
                "presented Sprite batch retained a stale Meadow region Image capture".into(),
            );
        }
        let (_, resident) = material.ok_or("presented relic has no resident Material Product")?;
        let extension = batch
            .material_extension
            .as_ref()
            .ok_or("presented relic lost its original Material binding")?;
        if extension.binding.generation() != resident.generation()
            || extension.binding.parameters() != &region_material::parameters()
        {
            return Err(
                "presented relic Material differs from its exact resident generation".into(),
            );
        }
        let palette = server
            .get_product_handle::<ImageAsset>(region_material::palette_product())
            .ok_or("presented region Material has no resident palette Image")?;
        let palette_capture = world
            .resource::<PreparedImages>()
            .get(image_resource_key(palette))
            .ok_or("presented region Material palette has no prepared capture")?;
        if extension
            .binding
            .textures()
            .get(region_material::PALETTE_ID)
            != Some(&palette)
            || extension.textures.len() != 1
            || extension.textures[0].0 != 1
            || extension.textures[0].1.identity() != palette_capture.identity()
        {
            return Err(
                "presented region Material palette differs from its resident Image capture".into(),
            );
        }
        if batch.target.window_id(Some(WindowId::PRIMARY)) == Some(WindowId::PRIMARY) {
            sprites += batch
                .instances
                .iter()
                .filter(|sprite| sprite.color.a > 0.0)
                .count();
        }
    }
    Ok(RoomFrame {
        sprites,
        image: prepared.map(|capture| RoomImage {
            asset_id: relic.asset_id().raw(),
            version: capture.asset_version().raw(),
        }),
        material: material_facts,
        tileset,
        tilemaps,
        tile_cells,
    })
}

fn observe_floor_tiles(
    world: &World,
    batches: &SpriteBatches,
) -> Result<(Option<RoomTileSet>, usize, usize), String> {
    let server = world.resource::<AssetServer>();
    let states = world.resource::<AssetStates>();
    let assets = world
        .get_resource::<Assets<TileSet>>()
        .ok_or("original resident TileSets are unavailable")?;
    let owner = world
        .get_resource::<TileSetAssets>()
        .ok_or("original TileSet declaration owner is unavailable")?;
    let resident = server
        .get_product_handle::<TileSet>(region_tileset::tileset_product())
        .and_then(|handle| assets.get(handle).map(|asset| (handle, asset)));
    let Some((handle, asset)) = resident else {
        if batches
            .as_slice()
            .iter()
            .any(|batch| batch.kind == SpriteBatchKind::Tilemap)
            || world
                .iter_entities()
                .any(|entity| entity.contains::<Tilemap>())
            || !owner.is_empty()
        {
            return Err("region Tilemap content outlived its TileSet declaration".into());
        }
        return Ok((None, 0, 0));
    };
    let state = states
        .state(handle.id())
        .ok_or("resident TileSet has no Asset state")?;
    let version = state
        .usable_version()
        .ok_or("resident TileSet has no usable version")?;
    let declaration = region_tileset::descriptor();
    let material = asset.material();
    let atlas = asset.atlas().ok_or("resident TileSet has no atlas")?;
    if version.raw() == 0
        || states.published_slot_revision(handle.id()) != Some(&assets.slot_revision(handle))
        || owner.content_receipt(handle, assets, states).is_none()
        || owner.desired(handle).is_some()
        || owner.error(handle).is_some()
        || state.source_hash().is_some()
        || state.import_recipe_digest().is_some()
        || server.path(handle.id()).is_some()
        || atlas != declaration.atlas()
        || asset.tile_size() != atlas.tile_size
        || material.sampler != declaration.sampler()
        || material.alpha_mode != declaration.alpha_mode()
        || material.tint != declaration.tint()
    {
        return Err("resident TileSet is not the original source-free Product declaration".into());
    }
    let image = asset
        .image_content()
        .ok_or("resident TileSet lost its original Image snapshot")?;
    let images = world.resource::<Assets<ImageAsset>>();
    let prepared = image.prepared_snapshot();
    if material.image != Some(image.handle())
        || image.product() != declaration.image_product()
        || image.source().is_some()
        || !image.is_current(server, states, images)
        || world
            .resource::<PreparedImages>()
            .get(image_resource_key(image.handle()))
            != Some(&prepared)
        || image.pixels() != region_material::SOURCE_PIXEL.repeat(4)
    {
        return Err("resident TileSet atlas differs from its current captured Image".into());
    }
    let extracted = world.resource::<ExtractedSprites>();
    let queued = world.resource::<QueuedSpriteItems>();
    let views = world.resource::<ExtractedViews>();
    let spatial = presented_spatial_snapshot(world).map_err(|error| error.to_string())?;
    let mut floors = BTreeSet::new();
    let mut cpu_tile_cells = 0;
    for entity in world.iter_entities() {
        let Some(map) = entity.get::<Tilemap>() else {
            continue;
        };
        let source = entity
            .get::<SceneEntitySource>()
            .ok_or("region Tilemap lost its Scene identity")?;
        let (origin, cells) = match source.entity_id.as_str() {
            region_tileset::SIDE_ROOM_FLOOR => (
                region_tileset::SIDE_ROOM_ORIGIN,
                region_tileset::side_room_cells(),
            ),
            region_tileset::LOOKOUT_FLOOR => (
                region_tileset::LOOKOUT_ORIGIN,
                region_tileset::lookout_cells(),
            ),
            _ => return Err("unexpected Tilemap in the region consumer".into()),
        };
        let local = spatial
            .local(entity.id())
            .map_err(|error| error.to_string())?;
        if map.tileset != Some(handle)
            || map.tile_size != region_tileset::TILE_SIZE
            || map.layer.index != region_tileset::FLOOR_LAYER
            || map.sort_key != 0
            || map.y_sort_anchor.is_some()
            || entity.contains::<Sprite>()
            || local.translation != origin
            || local.rotation != 0.0
            || local.scale != Vec2::ONE
            || spatial
                .parent(entity.id())
                .map_err(|error| error.to_string())?
                .is_some()
            || map
                .cells()
                .map(|(coord, cell)| (coord, *cell))
                .collect::<Vec<_>>()
                != cells
        {
            return Err("presented region floor differs from its authored Tilemap".into());
        }
        floors.insert(entity.id());
        let matrix = spatial
            .global(entity.id())
            .map_err(|error| error.to_string())?
            .matrix();
        let mut extracted_cells = 0;
        for sprite in extracted
            .as_slice()
            .iter()
            .filter(|sprite| sprite.entity == entity.id())
        {
            let ExtractedSpriteKind::TilemapCell { coord, tile } = sprite.kind else {
                return Err("region floor was not extracted as a real Tilemap cell".into());
            };
            let cell = map
                .get_cell(coord)
                .ok_or("extracted region cell has no live Tilemap cell")?;
            let region = asset
                .normalized_region(tile)
                .ok_or("extracted region cell has an invalid atlas index")?;
            let content = sprite
                .image_content
                .as_ref()
                .ok_or("extracted region cell lost its atlas capture")?;
            let center =
                (Vec2::new(coord.x as f32, coord.y as f32) + Vec2::splat(0.5)) * map.tile_size;
            if tile != cell.tile
                || sprite.world_center != matrix.transform_point2(center)
                || sprite.world_x_axis
                    != matrix.transform_vector2(Vec2::new(map.tile_size.x * 0.5, 0.0))
                || sprite.world_y_axis
                    != matrix.transform_vector2(Vec2::new(0.0, map.tile_size.y * 0.5))
                || sprite.color != cell.color
                || sprite.texture_region.min != region.min
                || sprite.texture_region.size != region.size
                || sprite.material.image != Some(image.handle())
                || sprite.material.sampler != material.sampler
                || sprite.material.alpha_mode != material.alpha_mode
                || sprite.material.tint != material.tint
                || sprite.layer != map.layer.index
                || sprite.sort_key != map.sort_key
                || sprite.material_extension.is_some()
                || content.prepared_snapshot() != prepared
                || !content.same_allocation(image)
            {
                return Err(
                    "original extracted Tilemap cell lost its geometry, color or atlas identity"
                        .into(),
                );
            }
            extracted_cells += 1;
        }
        if extracted_cells != map.cell_count() {
            return Err("the original floor extraction omitted or duplicated Tilemap cells".into());
        }
        cpu_tile_cells += extracted_cells;
    }
    let mut submitted_floors = BTreeSet::new();
    let mut submitted_cells = BTreeSet::new();
    let mut tile_cells = 0;
    for batch in batches
        .as_slice()
        .iter()
        .filter(|batch| batch.kind == SpriteBatchKind::Tilemap)
    {
        if batch.image.as_ref() != Some(&prepared) || batch.material_extension.is_some() {
            return Err("original Tilemap batch lost the shared TileSet atlas capture".into());
        }
        if batch.target.window_id(Some(WindowId::PRIMARY)) != Some(WindowId::PRIMARY) {
            continue;
        }
        let view = views
            .as_slice()
            .get(batch.view_index)
            .ok_or("Tilemap batch has no original View")?;
        if view.target != batch.target || view.order != batch.view_order {
            return Err("original Tilemap batch disagrees with its presented View".into());
        }
        for instance in batch
            .instances
            .iter()
            .filter(|instance| instance.color.a > 0.0)
        {
            let mut matches = queued.as_slice().iter().filter(|item| {
                item.kind == batch.kind
                    && item.view_index == batch.view_index
                    && item.view_order == batch.view_order
                    && item.target == batch.target
                    && item.layer == batch.layer
                    && item.sort_key == batch.sort_key
                    && item.material == batch.material
                    && item.image == batch.image
                    && item.instance == *instance
                    && item.material_extension == batch.material_extension
            });
            let item = matches
                .next()
                .ok_or("presented Tilemap instance has no original queued cell")?;
            if matches.next().is_some()
                || !floors.contains(&item.entity)
                || !submitted_cells.insert((item.view_index, item.source_order))
            {
                return Err("presented Tilemap instance has an ambiguous or retired source".into());
            }
            let sprite = extracted
                .as_slice()
                .iter()
                .find(|sprite| {
                    sprite.entity == item.entity && sprite.source_order == item.source_order
                })
                .ok_or("presented Tilemap instance lost its original extracted source")?;
            let projected = SpriteInstance {
                center: view.mapping.world_to_clip(sprite.world_center),
                x_axis: view.mapping.world_vector_to_clip(sprite.world_x_axis),
                y_axis: view.mapping.world_vector_to_clip(sprite.world_y_axis),
                color: sprite.color,
                uv: sprite.texture_region,
            };
            if *instance != projected
                || item.layer != sprite.layer
                || item.sort_key != sprite.sort_key
                || item.world_sort_y != sprite.world_sort_y
            {
                return Err(
                    "presented Tilemap instance differs from its exact captured source projection"
                        .into(),
                );
            }
            submitted_floors.insert(item.entity);
            tile_cells += 1;
        }
    }
    Ok((
        Some(RoomTileSet {
            product: server
                .product(handle.id())
                .ok_or("resident TileSet lost its Product binding")?
                .to_string(),
            asset_id: handle.id().raw(),
            version: version.raw(),
            declaration: json!({
                "image_product": image.product().to_string(),
                "atlas": {"tile_size": atlas.tile_size.to_array(), "columns": atlas.columns,
                    "rows": atlas.rows, "margin": atlas.margin.to_array(), "spacing": atlas.spacing.to_array()},
                "sampler": material.sampler, "alpha_mode": material.alpha_mode, "tint": material.tint,
            }),
            image: json!({
                "product": image.product().to_string(), "asset_id": image.handle().id().raw(),
                "version": image.version().raw(), "extent": image.extent(), "color_space": image.color_space(),
                "content_hash": image.content_hash().as_bytes(), "pixel_bytes": image.pixels().len(),
                "source_free": image.source().is_none(),
            }),
            cpu_tilemaps: floors.len(),
            cpu_tile_cells,
            observation: "resident_cpu_declaration",
        }),
        submitted_floors.len(),
        tile_cells,
    ))
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
        "lookout": snapshot.lookout,
        "held": snapshot.held,
        "restoring": snapshot.restoring,
    })
}
