use std::collections::{BTreeMap, BTreeSet};

use bevy_ecs::prelude::{Component, Entity, Message, Messages, Resource, World};
use nara_asset::{
    AssetError, AssetId as RuntimeId, AssetLoadError, AssetServer, AssetStates, Assets, Handle,
    LoadState, load_asset_sync, load_dependency_sync,
};
use nara_core::{AssetId, EntityUuid};
use thiserror::Error;

use crate::cooked::{CookedScene, CookedSceneError};

use super::{SceneAsset, SceneAssetError, SceneRegistry};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SceneInstanceId(u64);

/// Identifies an entity owned by a loaded scene, including its true ECS roots.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneMember(pub SceneInstanceId);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneLoadState {
    LoadingScene,
    LoadingDependencies { loaded: usize, total: usize },
    Ready,
    Failed(String),
}

#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub enum SceneEvent {
    Ready {
        instance: SceneInstanceId,
        scene_id: AssetId,
    },
    Unloaded {
        instance: SceneInstanceId,
        scene_id: AssetId,
    },
    Failed {
        instance: SceneInstanceId,
        scene_id: AssetId,
        message: String,
    },
}

#[derive(Debug, Error)]
pub enum SceneError {
    #[error("install ScenePlugin before loading scenes")]
    MissingPlugin,
    #[error(transparent)]
    Asset(#[from] AssetLoadError),
    #[error(transparent)]
    Request(#[from] AssetError),
    #[error(transparent)]
    Source(#[from] SceneAssetError),
    #[error(transparent)]
    Cooked(#[from] CookedSceneError),
    #[error(transparent)]
    App(#[from] nara_app::PluginError),
    #[error(transparent)]
    Registration(#[from] nara_reflect::SchemaRegistrationError),
    #[error("scene asset or dependency '{id}' failed: {message}")]
    LoadFailed { id: AssetId, message: String },
}

#[derive(Default)]
struct PendingScene {
    scene: Option<Handle<SceneAsset>>,
    dependencies: Option<Vec<(AssetId, RuntimeId)>>,
}

struct Instance {
    scene_id: AssetId,
    state: SceneLoadState,
    pending: Option<PendingScene>,
    entities: BTreeMap<EntityUuid, Entity>,
}

/// Loads independent instances and manages one main scene. A switch commits after the new scene's
/// dependencies and native component codecs have passed validation.
#[derive(Default, Resource)]
pub struct SceneManager {
    next: u64,
    instances: BTreeMap<SceneInstanceId, Instance>,
    unload: BTreeSet<SceneInstanceId>,
    main: Option<SceneInstanceId>,
    switching: Option<SceneInstanceId>,
}

impl SceneManager {
    pub fn load_scene(&mut self, scene_id: AssetId) -> SceneInstanceId {
        self.next = self.next.wrapping_add(1);
        let instance = SceneInstanceId(self.next);
        self.instances.insert(
            instance,
            Instance {
                scene_id,
                state: SceneLoadState::LoadingScene,
                pending: Some(PendingScene::default()),
                entities: BTreeMap::new(),
            },
        );
        instance
    }

    /// Replaces the main scene after the new scene is ready. A later request supersedes an earlier
    /// switch that is still pending.
    pub fn switch_scene(&mut self, scene_id: AssetId) -> SceneInstanceId {
        let instance = self.load_scene(scene_id);
        if let Some(previous) = self.switching.replace(instance) {
            self.despawn_scene(previous);
        }
        instance
    }

    pub fn despawn_scene(&mut self, instance: SceneInstanceId) {
        if self.instances.contains_key(&instance) {
            self.unload.insert(instance);
        }
    }

    #[must_use]
    pub fn state(&self, instance: SceneInstanceId) -> Option<&SceneLoadState> {
        self.instances.get(&instance).map(|entry| &entry.state)
    }

    #[must_use]
    pub const fn main_instance(&self) -> Option<SceneInstanceId> {
        self.main
    }

    /// Returns the scene's initial entity mapping after it reaches `Ready`.
    #[must_use]
    pub fn entities(&self, instance: SceneInstanceId) -> Option<&BTreeMap<EntityUuid, Entity>> {
        let entry = self.instances.get(&instance)?;
        (entry.state == SceneLoadState::Ready).then_some(&entry.entities)
    }
}

/// Immediately loads and activates an entry scene during app setup.
pub fn switch_scene_sync(
    world: &mut World,
    scene_id: AssetId,
) -> Result<SceneInstanceId, SceneError> {
    require_plugin(world)?;
    let handle = load_asset_sync::<SceneAsset>(world, scene_id)?;
    let asset = resident_scene(world, scene_id, handle)?;
    switch_scene_asset_sync(world, scene_id, &asset)
}

/// Immediately activates a prepared scene, such as a supervised editor snapshot. Dependencies
/// still use the configured asset source and the same typed loaders as an asynchronous switch.
pub fn switch_scene_asset_sync(
    world: &mut World,
    scene_id: AssetId,
    asset: &SceneAsset,
) -> Result<SceneInstanceId, SceneError> {
    require_plugin(world)?;
    for &dependency in asset.asset_dependencies() {
        load_dependency_sync(world, dependency)?;
    }
    let instance = world.resource_mut::<SceneManager>().switch_scene(scene_id);
    let result = instantiate(world, instance, scene_id, asset);
    if let Err(error) = &result {
        fail(world, instance, scene_id, error);
    }
    result.map(|()| instance)
}

fn require_plugin(world: &World) -> Result<(), SceneError> {
    if world.contains_resource::<SceneManager>()
        && world.contains_resource::<SceneRegistry>()
        && world.contains_resource::<Messages<SceneEvent>>()
    {
        Ok(())
    } else {
        Err(SceneError::MissingPlugin)
    }
}

pub(super) fn update_scenes(world: &mut World) {
    let retiring = std::mem::take(&mut world.resource_mut::<SceneManager>().unload);
    for instance in retiring {
        unload(world, instance);
    }
    let pending = world
        .resource::<SceneManager>()
        .instances
        .iter()
        .filter_map(|(&instance, entry)| entry.pending.as_ref().map(|_| instance))
        .collect::<Vec<_>>();
    for instance in pending {
        let mut manager = world.resource_mut::<SceneManager>();
        let Some(entry) = manager.instances.get_mut(&instance) else {
            continue;
        };
        let Some(mut request) = entry.pending.take() else {
            continue;
        };
        let scene_id = entry.scene_id;
        match request.advance(world, scene_id) {
            Ok(Progress::Loading(state)) => {
                if let Some(entry) = world
                    .resource_mut::<SceneManager>()
                    .instances
                    .get_mut(&instance)
                {
                    entry.pending = Some(request);
                    entry.state = state;
                }
            }
            Ok(Progress::Ready(asset)) => {
                if let Err(error) = instantiate(world, instance, scene_id, &asset) {
                    fail(world, instance, scene_id, &error);
                }
            }
            Err(error) => fail(world, instance, scene_id, &error),
        }
    }
}

enum Progress {
    Loading(SceneLoadState),
    Ready(SceneAsset),
}

impl PendingScene {
    fn advance(&mut self, world: &mut World, scene_id: AssetId) -> Result<Progress, SceneError> {
        let handle = match self.scene {
            Some(handle) => handle,
            None => *self
                .scene
                .insert(world.resource_mut::<AssetServer>().load_id(scene_id)?),
        };
        if !is_loaded(world, scene_id, handle.id())? {
            return Ok(Progress::Loading(SceneLoadState::LoadingScene));
        }
        let asset = resident_scene(world, scene_id, handle)?;
        let dependencies = match &self.dependencies {
            Some(dependencies) => dependencies,
            None => {
                let mut server = world.resource_mut::<AssetServer>();
                let handles = asset
                    .asset_dependencies()
                    .iter()
                    .map(|&id| Ok((id, server.load_dependency(id)?)))
                    .collect::<Result<Vec<_>, AssetError>>()?;
                self.dependencies.insert(handles)
            }
        };
        let total = dependencies.len();
        let mut loaded = 0;
        for &(id, handle) in dependencies {
            loaded += usize::from(is_loaded(world, id, handle)?);
        }
        if loaded == total {
            Ok(Progress::Ready(asset))
        } else {
            Ok(Progress::Loading(SceneLoadState::LoadingDependencies {
                loaded,
                total,
            }))
        }
    }
}

fn is_loaded(world: &World, id: AssetId, handle: RuntimeId) -> Result<bool, SceneError> {
    match world
        .resource::<AssetStates>()
        .state(handle)
        .map(|state| state.load_state())
    {
        Some(LoadState::Loaded) => Ok(true),
        Some(LoadState::Failed { message }) => Err(SceneError::LoadFailed {
            id,
            message: message.clone(),
        }),
        _ => Ok(false),
    }
}

fn resident_scene(
    world: &World,
    id: AssetId,
    handle: Handle<SceneAsset>,
) -> Result<SceneAsset, SceneError> {
    world
        .resource::<Assets<SceneAsset>>()
        .get(handle)
        .cloned()
        .ok_or_else(|| SceneError::LoadFailed {
            id,
            message: "the published scene is no longer resident".into(),
        })
}

fn instantiate(
    world: &mut World,
    instance: SceneInstanceId,
    scene_id: AssetId,
    asset: &SceneAsset,
) -> Result<(), SceneError> {
    let scene = CookedScene::parse(asset.bytes())?;
    let prepared = scene.prepare(&world.resource::<SceneRegistry>().native)?;
    let manager = world.resource::<SceneManager>();
    let promote = manager.switching == Some(instance);
    if promote && let Some(previous) = manager.main {
        unload(world, previous);
    }
    let entities = prepared.spawn(world, || SceneMember(instance));
    let mut manager = world.resource_mut::<SceneManager>();
    if let Some(entry) = manager.instances.get_mut(&instance) {
        entry.entities = entities;
        entry.pending = None;
        entry.state = SceneLoadState::Ready;
    }
    if promote {
        manager.main = Some(instance);
        manager.switching = None;
    }
    emit(world, SceneEvent::Ready { instance, scene_id });
    Ok(())
}

fn fail(world: &mut World, instance: SceneInstanceId, scene_id: AssetId, error: &SceneError) {
    let message = error.to_string();
    let mut manager = world.resource_mut::<SceneManager>();
    if let Some(entry) = manager.instances.get_mut(&instance) {
        entry.pending = None;
        entry.state = SceneLoadState::Failed(message.clone());
    }
    if manager.switching == Some(instance) {
        manager.switching = None;
    }
    emit(
        world,
        SceneEvent::Failed {
            instance,
            scene_id,
            message,
        },
    );
}

fn unload(world: &mut World, instance: SceneInstanceId) {
    let mut manager = world.resource_mut::<SceneManager>();
    let Some(entry) = manager.instances.remove(&instance) else {
        return;
    };
    if manager.main == Some(instance) {
        manager.main = None;
    }
    if manager.switching == Some(instance) {
        manager.switching = None;
    }
    if entry.state == SceneLoadState::Ready {
        let entities = world
            .query::<(Entity, &SceneMember)>()
            .iter(world)
            .filter_map(|(entity, member)| (member.0 == instance).then_some(entity))
            .collect::<Vec<_>>();
        for entity in entities {
            if let Ok(entity) = world.get_entity_mut(entity) {
                entity.despawn();
            }
        }
    }
    emit(
        world,
        SceneEvent::Unloaded {
            instance,
            scene_id: entry.scene_id,
        },
    );
}

fn emit(world: &mut World, event: SceneEvent) {
    world.resource_mut::<Messages<SceneEvent>>().write(event);
}
