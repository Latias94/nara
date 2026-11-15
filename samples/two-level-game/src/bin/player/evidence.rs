use std::collections::{BTreeMap, BTreeSet};

use nara::{
    advanced_prelude::CurrentScene,
    asset::{AssetProductRef, Assets},
    ecs::World,
    fs::ContentDigest,
    identity::{EntityLookup, SpawnedSceneInstance},
    image::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat},
    project_host::HeadlessSession,
    scene::{SourceDocumentKind, SourceDocumentOrigin},
};
use serde_json::{Value, json};
use two_level_game::{GameSnapshot, Scenario, levels::LevelId};

use super::output;

const SOURCE_UNIVERSE: &str = "be3dea0c-93ef-5607-ab84-af715a1f98b3";
/// BLAKE3 of the complete RGBA8 pixels independently decoded from each source PNG.
const RGBA_HASH: [u8; 32] = [
    61, 120, 191, 178, 229, 162, 52, 78, 30, 139, 104, 91, 1, 216, 130, 90, 82, 239, 180, 84, 73,
    118, 182, 59, 143, 200, 59, 95, 88, 7, 54, 75,
];

#[derive(Default)]
pub(super) struct Evidence {
    pub snapshot: Option<GameSnapshot>,
    pub startup: Option<SpawnedSceneInstance>,
    pub current: Option<SpawnedSceneInstance>,
    pub visited: BTreeSet<LevelId>,
    images: BTreeMap<AssetProductRef, Value>,
    resident: BTreeSet<AssetProductRef>,
    receipts: Vec<Value>,
}

impl Evidence {
    pub fn observe(&mut self, session: &HeadlessSession) -> Result<(), String> {
        if session.startup_content_closure().is_some() {
            return Err("processed Player unexpectedly retained a source startup closure".into());
        }
        session
            .observe(|world| self.observe_world(world))
            .map_err(|error| error.to_string())?
    }

    fn observe_world(&mut self, world: &World) -> Result<(), String> {
        self.resident = world
            .get_resource::<Assets<ImageAsset>>()
            .into_iter()
            .flat_map(|assets| assets.iter())
            .filter_map(|(_, image)| image.product())
            .collect();
        self.snapshot = world.get_resource::<GameSnapshot>().cloned();
        let current = CurrentScene::get(world).ok_or("current Scene is unavailable")?;
        let changed = self.current.as_ref() != Some(current.receipt());
        self.current = Some(current.receipt().clone());
        let startup = self
            .startup
            .get_or_insert_with(|| current.receipt().clone());
        if startup.domain_id() != current.receipt().domain_id() {
            return Err("current Scene changed the original World identity domain".into());
        }
        if let Some(snapshot) = &self.snapshot
            && snapshot.initialized
            && (snapshot.current_instance != Some(current.receipt().instance_id())
                || snapshot.scene_members != current.receipt().len())
        {
            return Err("gameplay snapshot disagrees with the actual current Scene receipt".into());
        }
        if !changed
            && self
                .snapshot
                .as_ref()
                .is_none_or(|snapshot| snapshot.error.is_none())
        {
            return Ok(());
        }
        if (changed && self.receipts.len() >= 16)
            || current.receipt().len() != 6
            || current.receipt().entity_ids().iter().any(|id| {
                id.as_str().len() > 256
                    || !matches!(
                        current.receipt().resolve(world, id),
                        EntityLookup::Resolved(_)
                    )
            })
        {
            return Err(
                "current Scene receipt is not the bounded live six-entity game level".into(),
            );
        }
        if !changed {
            return Ok(());
        }
        let view = current.source_view();
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
                    .find(|level| path.as_str() == format!("scenes/{}", level.scene_path()))
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
        self.visited.insert(level);
        self.receipts.push(json!({
            "level": level,
            "scene_product": level.scene_product().to_string(),
            "source_key": source_key,
            "representation": "runtime_scene",
            "receipt": receipt(current.receipt()),
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

fn receipt(receipt: &SpawnedSceneInstance) -> Value {
    json!({
        "domain_id": receipt.domain_id().get(),
        "instance_id": receipt.instance_id().get(),
        "entity_count": receipt.len(),
        "entity_ids": receipt.entity_ids().iter().take(6)
            .map(|id| id.as_str().chars().take(256).collect::<String>()).collect::<Vec<_>>(),
        "truncated": receipt.len() > 6
            || receipt.entity_ids().iter().take(6).any(|id| id.as_str().len() > 256),
    })
}
