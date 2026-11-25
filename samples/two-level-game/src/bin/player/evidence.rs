use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

use nara::{
    advanced_prelude::CurrentScene,
    asset::{AssetProductRef, AssetRef, Assets},
    ecs::{Entity, World},
    fs::ContentDigest,
    hierarchy::Parent,
    identity::{EntityLookup, SpawnedSceneInstance},
    image::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat},
    input::{ActionMapBindings, ActionMapDefinition, ActionMapDomain, ActionMaps, InputSeatId},
    material::SamplerDescriptor,
    prelude::{Color, Transform2d, Vec2},
    project_host::{HeadlessSession, ProjectDomainAssets},
    runtime_content::RuntimeInputContent,
    scene::{SourceDocumentKind, SourceDocumentOrigin},
    sprite::{Sprite, SpriteAnchor},
};
use nara_render::animation::{SpriteAnimationDomain, SpriteAnimationSet};
use serde_json::{Value, json};
use two_level_game::{
    CollisionBox, GamePhase, GameSnapshot, Scenario,
    animation::{CLIPS_PATH, ExplorerAnimation, atlas_product, clips_product},
    input::{INPUT_PATH, input_product},
    levels::{LevelId, WALL_ENTITY_IDS},
    markers::marker_image_product,
};

use super::output;

const SOURCE_UNIVERSE: &str = "be3dea0c-93ef-5607-ab84-af715a1f98b3";
const MAX_SCENE_RECEIPTS: usize = 16;
const MAX_REPORTED_ENTITY_IDS: usize = 32;
const MAX_REPORTED_ENTITY_ID_BYTES: usize = 256;
/// BLAKE3 of the complete RGBA8 pixels independently decoded from each source PNG.
const RGBA_HASH: [u8; 32] = [
    61, 120, 191, 178, 229, 162, 52, 78, 30, 139, 104, 91, 1, 216, 130, 90, 82, 239, 180, 84, 73,
    118, 182, 59, 143, 200, 59, 95, 88, 7, 54, 75,
];
const ATLAS_RGBA_HASH: [u8; 32] = [
    181, 15, 191, 222, 208, 248, 60, 15, 165, 121, 233, 83, 238, 249, 97, 147, 186, 153, 15, 101,
    119, 235, 173, 161, 179, 46, 29, 61, 98, 242, 249, 66,
];
const ATLAS_FRAME_PIXELS: [[u8; 4]; 4] = [
    [64, 160, 240, 255],
    [32, 216, 232, 255],
    [240, 152, 48, 255],
    [112, 224, 80, 255],
];

#[derive(Default)]
pub(super) struct Evidence {
    pub snapshot: Option<GameSnapshot>,
    pub startup: Option<SpawnedSceneInstance>,
    pub current: Option<SpawnedSceneInstance>,
    pub visited: BTreeSet<LevelId>,
    images: BTreeMap<AssetProductRef, Value>,
    resident: BTreeSet<AssetProductRef>,
    animation_frames: BTreeSet<u8>,
    input: Option<Value>,
    input_bindings: Option<ActionMapBindings>,
    receipts: Vec<Value>,
}

impl Evidence {
    pub(super) fn expect_input(&mut self, input: &RuntimeInputContent) -> Result<(), String> {
        if input.product() != input_product()
            || input.base().plan().settings().input().action_map != Some(input.product())
        {
            return Err("cooked global Input has the wrong complete selected Product".into());
        }
        let proof = definition_proof(input.product(), input.definition());
        if self
            .input
            .as_ref()
            .is_some_and(|previous| previous != &proof)
        {
            return Err("cooked global Input definition changed".into());
        }
        self.input = Some(proof);
        Ok(())
    }

    pub(super) fn observe_world_sampled(&mut self, world: &World) -> Result<bool, String> {
        let changed =
            CurrentScene::get(world).is_some_and(|current| self.scene_changed(current.receipt()));
        let sampled = changed && self.receipts.len() >= MAX_SCENE_RECEIPTS;
        if sampled {
            self.receipts.remove(1);
        }
        self.observe_world(world)?;
        Ok(sampled)
    }

    fn scene_changed(&self, instance: &SpawnedSceneInstance) -> bool {
        self.current.as_ref().is_none_or(|previous| {
            previous.domain_id() != instance.domain_id()
                || previous.instance_id() != instance.instance_id()
        })
    }

    pub fn observe(&mut self, session: &HeadlessSession) -> Result<(), String> {
        #[cfg(any(feature = "development", feature = "cook"))]
        if session.startup_content_closure().is_some() {
            return Err("processed Player unexpectedly retained a source startup closure".into());
        }
        session
            .observe(|world| self.observe_world(world))
            .map_err(|error| error.to_string())?
    }

    pub(super) fn observe_world(&mut self, world: &World) -> Result<(), String> {
        let map = world
            .get_resource::<ActionMaps>()
            .and_then(|maps| maps.get(InputSeatId::PRIMARY))
            .ok_or("cooked Player has no installed PRIMARY Input map")?;
        let bindings = map.bindings_handle();
        if let Some(original) = &self.input_bindings {
            if !original.shares_owner_with(&bindings) {
                return Err("cooked Player replaced its original global Input owner".into());
            }
        } else {
            let proof = input_proof(world)?;
            if self
                .input
                .as_ref()
                .is_some_and(|previous| previous != &proof)
            {
                return Err("installed PRIMARY Input differs from its cooked definition".into());
            }
            self.input = Some(proof);
            self.input_bindings = Some(bindings);
        }
        self.resident = world
            .get_resource::<Assets<ImageAsset>>()
            .into_iter()
            .flat_map(|assets| assets.iter())
            .filter_map(|(_, image)| image.product())
            .collect();
        let had_error = self
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.error.is_some());
        self.snapshot = world.get_resource::<GameSnapshot>().cloned();
        let current = CurrentScene::get(world).ok_or("current Scene is unavailable")?;
        let instance = current.receipt();
        let changed = self.scene_changed(instance);
        if changed {
            self.current = Some(instance.clone());
        }
        let startup = self.startup.get_or_insert_with(|| instance.clone());
        if startup.domain_id() != instance.domain_id() {
            return Err("current Scene changed the original World identity domain".into());
        }
        if let Some(snapshot) = &self.snapshot
            && snapshot.initialized
            && (snapshot.current_instance != Some(instance.instance_id())
                || snapshot.scene_members != instance.len())
        {
            return Err("gameplay snapshot disagrees with the actual current Scene receipt".into());
        }
        if let Some(snapshot) = &self.snapshot
            && snapshot.initialized
            && snapshot.phase == GamePhase::Running
            && !snapshot.restoring
            && snapshot.error.is_none()
        {
            let entity = resolve_scene_member(world, instance, "explorer")?;
            let sprite = world
                .get::<Sprite>(entity)
                .ok_or("current receipt-resolved explorer has no Sprite")?;
            let animation = world
                .get::<ExplorerAnimation>(entity)
                .ok_or("current receipt-resolved explorer has no authored animation reference")?;
            let clips = selected_clips(world, &animation.clips)?;
            let name =
                if snapshot.held[0] == snapshot.held[1] && snapshot.held[2] == snapshot.held[3] {
                    "idle"
                } else {
                    "walk"
                };
            let sample = clips
                .get(name)
                .ok_or_else(|| format!("admitted explorer animation has no {name} clip"))?
                .sample(Duration::from_nanos(snapshot.logical_elapsed_nanos));
            if sprite.texture_region != Some(sample.region)
                || sprite.material.sampler != SamplerDescriptor::NEAREST_CLAMP
                || sprite.material.tint != Color::WHITE
            {
                return Err(format!(
                    "current explorer Sprite disagrees with its admitted {name} clip sample, nearest sampling or white tint"
                ));
            }
            let image = sprite
                .material
                .image
                .and_then(|handle| world.get_resource::<Assets<ImageAsset>>()?.get(handle))
                .ok_or("current explorer Sprite has no resident bound Image")?;
            if image.product() != Some(atlas_product()) || image.source().is_some() {
                return Err(
                    "current explorer Sprite is not bound to the source-free atlas Image Product"
                        .into(),
                );
            }
            self.animation_frames
                .insert((sample.region.min.x * 4.0) as u8);
        }
        let new_error = !had_error
            && self
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.error.is_some());
        if !changed && !new_error {
            return Ok(());
        }
        if changed && self.receipts.len() >= MAX_SCENE_RECEIPTS {
            return Err("Player exceeded its Scene receipt history limit".into());
        }
        let entities = current.source().entities();
        if instance.len() != entities.len()
            || entities.iter().any(|entity| {
                !matches!(
                    instance.resolve(world, entity.id()),
                    EntityLookup::Resolved(_)
                )
            })
        {
            return Err(
                "current Scene receipt does not resolve the admitted level's members".into(),
            );
        }
        if !changed {
            return Ok(());
        }
        let view = current.source_view();
        #[cfg(any(feature = "development", feature = "cook"))]
        if current.source_content_stamp().is_some() || view.with_compiled_scene(|_, _| ()).is_some()
        {
            return Err("Player current Scene unexpectedly retained authoring source".into());
        }
        let (level, source_key) = view
            .with_runtime_scene(|runtime| {
                if runtime.source_key().kind() != SourceDocumentKind::Scene {
                    return Err("Runtime Scene has the wrong containing document kind".to_owned());
                }
                let SourceDocumentOrigin::ProjectLocation { universe, path } =
                    runtime.source_key().origin()
                else {
                    return Err("Runtime Scene lost its ProjectLocation identity".into());
                };
                if universe.to_string() != SOURCE_UNIVERSE {
                    return Err("Runtime Scene changed the authored source universe".into());
                }
                let level = LevelId::ALL
                    .into_iter()
                    .find(|level| path.as_str() == format!("scenes/{}", level.scene_file_name()))
                    .ok_or("Runtime Scene has an unknown ProjectLocation path")?;
                Ok((level, json!(runtime.source_key())))
            })
            .ok_or("current Scene does not have a source-free RuntimeScene representation")??;
        if let Some(snapshot) = &self.snapshot
            && snapshot.initialized
            && snapshot.level.map(|key| key.level()) != Some(level)
        {
            return Err("gameplay level disagrees with the actual Runtime Scene identity".into());
        }
        if self.receipts.is_empty() && level != LevelId::STARTUP {
            return Err("Player did not activate the actual Meadow startup Scene".into());
        }
        let walls = wall_proofs(world, instance, level)?;
        let animation = animation_proof(world, instance)?;
        let product = level.image_product();
        let assets = world
            .get_resource::<Assets<ImageAsset>>()
            .ok_or("current Scene has no resident CPU Image resources")?;
        let mut matches = assets
            .iter()
            .filter(|(_, image)| image.product() == Some(product));
        let (_, image) = matches
            .next()
            .ok_or("current level Image Product is not resident")?;
        if matches.next().is_some()
            || image.source().is_some()
            || image.extent() != ImageExtent::new(16, 16)
            || image.format() != ImageFormat::Rgba8
            || image.color_space() != ImageColorSpace::Srgb
            || ContentDigest::of_bytes(image.pixels()) != ContentDigest::from_parts(1024, RGBA_HASH)
        {
            return Err(
                "resident Runtime Image disagrees with the complete source PNG pixels".into(),
            );
        }
        self.images.insert(
            product,
            json!({
                "level": level,
                "product": product.to_string(),
                "format": image.format(),
                "color_space": image.color_space(),
                "extent": image.extent(),
                "pixel_bytes": image.pixels().len(),
                "pixel_blake3": ContentDigest::of_bytes(image.pixels()).as_bytes(),
                "content_hash": image.content_hash().as_bytes(),
                "source_free": image.source().is_none(),
            }),
        );
        let product = atlas_product();
        let mut matches = assets
            .iter()
            .filter(|(_, image)| image.product() == Some(product));
        let (_, image) = matches
            .next()
            .ok_or("shared explorer atlas Image Product is not resident")?;
        let pixel_digest = ContentDigest::of_bytes(image.pixels());
        if matches.next().is_some()
            || image.source().is_some()
            || image.extent() != ImageExtent::new(64, 16)
            || image.format() != ImageFormat::Rgba8
            || image.color_space() != ImageColorSpace::Srgb
            || pixel_digest != ContentDigest::from_parts(4096, ATLAS_RGBA_HASH)
            || ATLAS_FRAME_PIXELS
                .iter()
                .enumerate()
                .any(|(frame, expected)| {
                    let offset = (8 * 64 + frame * 16 + 8) * 4;
                    image.pixels().get(offset..offset + 4) != Some(expected.as_slice())
                })
        {
            return Err(
                "resident explorer atlas disagrees with its exact RGBA8 sRGB descriptor and four-frame pixels"
                    .into(),
            );
        }
        self.images.insert(
            product,
            json!({
                "role": "explorer_atlas",
                "product": product.to_string(),
                "format": image.format(),
                "color_space": image.color_space(),
                "extent": image.extent(),
                "columns": 4,
                "rows": 1,
                "cell_extent": ImageExtent::new(16, 16),
                "frame_center_rgba": ATLAS_FRAME_PIXELS,
                "pixel_bytes": image.pixels().len(),
                "pixel_blake3": pixel_digest.as_bytes(),
                "content_hash": image.content_hash().as_bytes(),
                "source_free": image.source().is_none(),
            }),
        );
        let product = marker_image_product();
        let (_, image) = assets
            .iter()
            .find(|(_, image)| image.product() == Some(product))
            .ok_or("marker Scene's Image Product is not resident")?;
        self.images.insert(
            product,
            json!({
                "role": "marker_badge",
                "product": product.to_string(),
                "format": image.format(),
                "color_space": image.color_space(),
                "extent": image.extent(),
                "pixel_bytes": image.pixels().len(),
                "pixel_blake3": ContentDigest::of_bytes(image.pixels()).as_bytes(),
                "content_hash": image.content_hash().as_bytes(),
                "source_free": image.source().is_none(),
            }),
        );
        self.visited.insert(level);
        self.receipts.push(json!({
            "level": level,
            "scene_product": level.scene_product().to_string(),
            "source_key": source_key,
            "representation": "runtime_scene",
            "receipt": receipt(instance),
            "wall_proofs": walls,
            "animation_proof": animation,
        }));
        Ok(())
    }

    pub fn result(&self, scenario: Scenario, message: Option<&str>, save: Option<Value>) -> Value {
        let mut result = output::failure(Some(scenario), message.unwrap_or_default());
        result["success"] = json!(message.is_none());
        result["message"] = message.map(output::bounded).into();
        result["error"] = result["message"].clone();
        result["processed_runtime"] = json!(!self.receipts.is_empty());
        if let Some(snapshot) = &self.snapshot {
            result["level"] = json!(snapshot.level);
            result["score"] = json!(snapshot.score);
            result["collected"] = json!(snapshot.collected);
            result["markers"] = json!(snapshot.markers);
            result["state"] = json!({
                "tick": snapshot.tick,
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
                "logical_tick": snapshot.logical_tick,
                "side_room": snapshot.side_room,
                "lookout": snapshot.lookout,
                "logical_elapsed_nanos": snapshot.logical_elapsed_nanos,
                "held": snapshot.held,
                "restoring": snapshot.restoring,
            });
        }
        if let Some(save) = save {
            result["next_marker_id"] = save["next_marker_id"].clone();
            result["save"] = save;
        }
        result["visited_levels"] = json!(self.visited);
        result["observed_image_products"] = json!(
            self.images
                .keys()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        );
        result["resident_images"] = json!(
            self.resident
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        );
        result["image_proofs"] = json!(self.images.values().collect::<Vec<_>>());
        result["animation_frames"] = json!(self.animation_frames);
        result["input_proof"] = json!(self.input);
        result["startup_receipt"] = self.startup.as_ref().map(receipt).into();
        result["current_receipt"] = self.current.as_ref().map(receipt).into();
        result["scene_receipts"] = json!(self.receipts);
        result["preserved_startup_instance"] = json!(
            self.startup
                .as_ref()
                .zip(self.current.as_ref())
                .map(|(startup, current)| startup == current)
        );
        result
    }
}

pub(super) fn input_proof(world: &World) -> Result<Value, String> {
    let map = world
        .get_resource::<ActionMaps>()
        .and_then(|maps| maps.get(InputSeatId::PRIMARY))
        .ok_or("cooked Player has no installed PRIMARY Input map")?;
    let original = nara::input::startup_action_map_bindings(world)
        .ok_or("cooked Player has no finalized original Input selection")?;
    let installed = map.bindings_handle();
    let admitted = match world.get_resource::<two_level_game::flow::FlowBindings>() {
        Some(flow) if flow.installed().shares_owner_with(&installed) => flow.admitted(),
        Some(_) => return Err("cooked round Player replaced its admitted menu derivation".into()),
        None => &installed,
    };
    if !original.shares_owner_with(admitted) {
        return Err("cooked Player changed its selected Input during Startup".into());
    }
    if let Some(collection) = world
        .get_resource::<ProjectDomainAssets>()
        .and_then(|assets| assets.collection::<ActionMapDomain>())
        && (collection.source_records().len() != 0
            || collection
                .get(&AssetRef::path(INPUT_PATH).map_err(|error| error.to_string())?)
                .is_some())
    {
        return Err("cooked Input retained source records or an authored path alias".into());
    }
    Ok(definition_proof(input_product(), admitted.definition()))
}

fn definition_proof(product: AssetProductRef, definition: &ActionMapDefinition) -> Value {
    json!({
        "product": product.to_string(),
        "source_free": true,
        "definition": {
            "product": product,
            "contexts": definition.contexts().iter().map(|entry| json!({
                "context": entry.context.as_str(),
                "enabled": entry.enabled,
            })).collect::<Vec<_>>(),
            "bindings": definition.bindings(),
        },
    })
}

fn selected_clips<'a>(
    world: &'a World,
    reference: &AssetRef,
) -> Result<&'a SpriteAnimationSet, String> {
    let clips = world
        .get_resource::<ProjectDomainAssets>()
        .and_then(|assets| assets.collection::<SpriteAnimationDomain>())
        .and_then(|collection| collection.get(reference))
        .ok_or("current explorer animation has no admitted Sprite animation Product")?;
    if clips.product() != clips_product() {
        return Err("current explorer animation has the wrong complete Product identity".into());
    }
    Ok(clips)
}

fn animation_proof(world: &World, receipt: &SpawnedSceneInstance) -> Result<Value, String> {
    let entity = resolve_scene_member(world, receipt, "explorer")?;
    let animation = world
        .get::<ExplorerAnimation>(entity)
        .ok_or("current receipt-resolved explorer has no authored animation reference")?;
    let clips = selected_clips(world, &animation.clips)?;
    let collection = world
        .get_resource::<ProjectDomainAssets>()
        .and_then(|assets| assets.collection::<SpriteAnimationDomain>())
        .ok_or("current World has no Sprite animation collection")?;
    if collection.iter().len() != 1
        || collection.source_records().len() != 0
        || collection
            .get(&AssetRef::path(CLIPS_PATH).map_err(|error| error.to_string())?)
            .is_some()
    {
        return Err(
            "cooked Sprite animation retained source aliases, records or extra Products".into(),
        );
    }
    Ok(json!({
        "product": clips.product().to_string(),
        "source_free": true,
        "clips": clips.clips().map(|(name, clip)| json!({
            "name": name,
            "mode": clip.mode(),
            "frames": clip.frames().iter().map(|frame| {
                let region = frame.region();
                json!({
                    "min": region.min.to_array(),
                    "size": region.size.to_array(),
                    "duration_nanos": frame.duration().as_nanos() as u64,
                })
            }).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    }))
}

fn receipt(receipt: &SpawnedSceneInstance) -> Value {
    let reported = &receipt.entity_ids()[..receipt.len().min(MAX_REPORTED_ENTITY_IDS)];
    let truncated = receipt.len() > reported.len()
        || reported
            .iter()
            .any(|id| id.as_str().len() > MAX_REPORTED_ENTITY_ID_BYTES);
    let entity_ids = reported
        .iter()
        .map(|id| {
            let id = id.as_str();
            &id[..id.floor_char_boundary(MAX_REPORTED_ENTITY_ID_BYTES)]
        })
        .collect::<Vec<_>>();
    json!({
        "domain_id": receipt.domain_id().get(),
        "instance_id": receipt.instance_id().get(),
        "entity_count": receipt.len(),
        "entity_ids": entity_ids,
        "truncated": truncated,
    })
}

fn resolve_scene_member(
    world: &World,
    receipt: &SpawnedSceneInstance,
    name: &str,
) -> Result<Entity, String> {
    let ids = receipt.entity_ids();
    let index = ids
        .binary_search_by(|id| id.as_str().cmp(name))
        .map_err(|_| format!("current Scene receipt has no {name} identity"))?;
    match receipt.resolve(world, &ids[index]) {
        EntityLookup::Resolved(entity) => Ok(entity),
        _ => Err(format!(
            "current Scene receipt does not resolve its live {name}"
        )),
    }
}

fn wall_proofs(
    world: &World,
    receipt: &SpawnedSceneInstance,
    level: LevelId,
) -> Result<Vec<Value>, String> {
    let explorer = resolve_scene_member(world, receipt, "explorer")?;
    if world.get::<CollisionBox>(explorer)
        != Some(&CollisionBox {
            center: Vec2::ZERO,
            half_width: 9.0,
            half_height: 9.0,
        })
        || world.get::<Parent>(explorer).is_some()
    {
        return Err("receipt-resolved explorer lost its authored root collision box".into());
    }
    let inner_x = match level {
        LevelId::Meadow => 0.0,
        LevelId::Cavern => 20.0,
    };
    WALL_ENTITY_IDS
        .into_iter()
        .zip([
            ([-294.0, 0.0], [5.0, 164.0]),
            ([294.0, 0.0], [5.0, 164.0]),
            ([0.0, 164.0], [299.0, 5.0]),
            ([0.0, -164.0], [299.0, 5.0]),
            ([inner_x, -12.0], [6.0, 36.0]),
        ])
        .map(|(id, (translation, half))| {
            let entity = resolve_scene_member(world, receipt, id)?;
            let transform = world
                .get::<Transform2d>(entity)
                .ok_or_else(|| format!("receipt-resolved {id} has no Transform2d"))?;
            let definition = world
                .get::<CollisionBox>(entity)
                .ok_or_else(|| format!("receipt-resolved {id} has no CollisionBox"))?;
            let sprite = world
                .get::<Sprite>(entity)
                .ok_or_else(|| format!("receipt-resolved {id} has no Sprite"))?;
            if world.get::<Parent>(entity).is_some()
                || *transform != Transform2d::from_translation(Vec2::from_array(translation))
                || *definition
                    != (CollisionBox {
                        center: Vec2::ZERO,
                        half_width: half[0],
                        half_height: half[1],
                    })
                || sprite.size != Vec2::from_array(half) * 2.0
                || sprite.anchor != SpriteAnchor::CENTER
                || sprite.material.tint != Color::rgba(0.18, 0.22, 0.28, 1.0)
                || sprite.material.image.is_some()
                || sprite.texture_region.is_some()
            {
                return Err(format!("receipt-resolved {id} disagrees with its authored wall"));
            }
            Ok(json!({
                "id": id,
                "root": true,
                "translation": transform.translation.to_array(),
                "rotation": transform.rotation,
                "scale": transform.scale.to_array(),
                "box": {
                    "center": definition.center.to_array(),
                    "half_width": definition.half_width,
                    "half_height": definition.half_height,
                },
                "sprite_size": sprite.size.to_array(),
                "sprite_anchor": sprite.anchor.normalized.to_array(),
                "tint": [sprite.material.tint.r, sprite.material.tint.g, sprite.material.tint.b, sprite.material.tint.a],
                "textured": false,
            }))
        })
        .collect()
}
