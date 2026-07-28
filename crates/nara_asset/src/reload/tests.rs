use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::{
    AssetDependencyGraph, AssetEventKind, AssetEventLimits, AssetEventPushOutcome, AssetEvents,
    AssetId, AssetPath, AssetRecord, AssetServer, AssetSourceKind, AssetStates, AssetVersion,
    Assets, ImportArtifactDigest, LoadState, ProjectAssetDatabase, StableAssetId,
};
use nara_core::{ByteLimit, ItemLimit};
use nara_ecs::ResMut;

fn stable_id() -> StableAssetId {
    StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap()
}

fn dependent_stable_id() -> StableAssetId {
    StableAssetId::parse_str("b73f0f16-09e8-4265-b090-b689b41c197e").unwrap()
}

fn transitive_dependent_stable_id() -> StableAssetId {
    StableAssetId::parse_str("d87c8f9d-0dc2-4863-8e9c-d3e6eaa8d41f").unwrap()
}

fn alternate_source_stable_id() -> StableAssetId {
    StableAssetId::parse_str("6df0be49-2e69-4c34-90ae-f4c3174aab02").unwrap()
}

fn image_record(path: &str, stable_id: StableAssetId) -> AssetRecord {
    AssetRecord::new(
        stable_id,
        AssetPath::new(path).unwrap(),
        AssetSourceKind::Image,
    )
}

fn install_asset_plugin(app: &mut App) {
    app.insert_resource(
        nara_tasks::TaskPools::inline_for_tests(nara_tasks::TaskPoolConfig::default()).unwrap(),
    )
    .unwrap();
    app.add_plugins((nara_tasks::TaskPlugin::default(), AssetPlugin))
        .unwrap();
}

#[derive(Debug, Default, Resource)]
struct CapturedReloadRequests(Vec<AssetReloadRequest>);

fn install_test_reload_consumer(app: &mut App) {
    let consumer = register_image_reload_consumer(app).unwrap();
    app.init_resource::<CapturedReloadRequests>().unwrap();
    app.add_systems(
        CoreStage::TaskUpdate,
        (move |mut requests: ResMut<AssetReloadRequests>,
               mut captured: ResMut<CapturedReloadRequests>| {
            captured.0.extend(
                requests
                    .drain_images(&consumer)
                    .expect("test consumer owns requests"),
            );
        })
        .in_set(AssetTaskUpdateSet::SpawnJobs),
    )
    .unwrap();
}

#[path = "tests_requests.rs"]
mod requests;
#[path = "tests_resolution.rs"]
mod resolution;
#[path = "tests_source_changes.rs"]
mod source_changes;
