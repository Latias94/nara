mod collision;
pub(crate) mod commands;
pub(crate) mod content;
pub mod input;
#[cfg(feature = "desktop")]
pub use input::movement_keyboard_bindings;
mod player_launch;
pub(crate) mod resume;
mod side_room;
mod storage;
mod travel;
mod verification;

use std::{path::PathBuf, sync::Arc};

use nara::{
    ProductConfiguration, ProductRecipe, SchemaContribution,
    advanced_prelude::{CurrentScene, add_startup_scene_activation_systems},
    app::{PluginCategory, PluginDeclaration, PluginId, PluginSchemaProviderId},
    ecs::{World, schedule::IntoScheduleConfigs},
    gameplay::{GameplayCommandSet, add_local_gameplay_command_systems},
    identity::{EntityLookup, SceneEntityId},
    prelude::{App, CoreStage, FixedUpdateSet, Plugin, PluginError, Resource, Transform2d, Vec2},
    reflect::{
        ComponentRegistryBuilder, ComponentRegistryError, ComponentSchemaCatalog,
        ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition, ComponentSchemaProviderSourceError,
        PersistentComponentProvider,
    },
    scene::advanced::SceneProductResource,
};

use crate::{
    levels::{LevelId, LevelKey},
    save::{
        Checkpoint, CheckpointLocation, CollectedPickup, MAX_MARKERS, MAX_SCORE, MarkerId,
        MarkerRecord,
    },
};
pub use collision::CollisionBox;
use content::Content;
pub use content::SoundPalette;
pub use player_launch::{player_launch, player_launch_result};
pub use side_room::SideRoomPhase;

pub(super) const PLUGIN_ID: PluginId = PluginId::new("two-level-game.exploration");
const PROVIDER_ID: PluginSchemaProviderId = PluginSchemaProviderId::new("two-level-game.objects");
const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("two-level-game"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("two-level-game.native", 5),
    catalog,
    register,
);
const DECLARATION: PluginDeclaration = PluginDeclaration::new(PLUGIN_ID, PluginCategory::Runtime)
    .requires_plugins(&[
        nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID,
        nara::transform::TRANSFORM_PLUGIN_ID,
        nara::gameplay::GAMEPLAY_COMMAND_PLUGIN_ID,
        nara::advanced_prelude::STARTUP_SCENE_ACTIVATION_PLUGIN_ID,
    ])
    .provides_schema(&[PROVIDER_ID])
    .shutdown_obligations(&[storage::OBLIGATION]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum Scenario {
    #[default]
    Interactive,
    Travel,
    Save,
    Restore,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RunOptions {
    pub scenario: Scenario,
    pub save_path: Option<PathBuf>,
}
impl ProductConfiguration for RunOptions {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend(serde_json::to_vec(self).expect("finite game configuration"));
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GamePhase {
    #[default]
    Running,
    Preparing,
    Reconstructing,
    Degraded,
}

#[derive(Debug, Clone, Default, Resource)]
pub struct GameSnapshot {
    pub tick: u64,
    pub level: Option<LevelKey>,
    pub player: Vec2,
    pub score: u32,
    pub collected: usize,
    pub visible_pickups: usize,
    pub markers: Vec<MarkerId>,
    pub travel_count: u32,
    pub restarts: u32,
    pub saves: u32,
    pub restores: u32,
    pub phase: GamePhase,
    pub initialized: bool,
    pub verified: bool,
    pub error: Option<String>,
    pub current_instance: Option<nara::identity::SceneInstanceId>,
    pub scene_members: usize,
    pub logical_tick: u64,
    pub logical_elapsed_nanos: u64,
    pub held: [bool; 4],
    pub restoring: bool,
    pub side_room: SideRoomPhase,
}

/// Stable owner identity with a bounded write counter; ordinary mutations never allocate it.
#[derive(Default)]
struct ResourceRevision {
    incarnation: Arc<()>,
    generation: u64,
}

impl ResourceRevision {
    fn advance(&mut self) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("game resource mutation generation exhausted");
    }

    fn capture(&self) -> (Arc<()>, u64) {
        (Arc::clone(&self.incarnation), self.generation)
    }

    fn matches(&self, baseline: &(Arc<()>, u64)) -> bool {
        Arc::ptr_eq(&self.incarnation, &baseline.0) && self.generation == baseline.1
    }
}

#[derive(Resource)]
pub(super) struct Progress {
    checkpoint: Checkpoint,
    revision: ResourceRevision,
}

impl Progress {
    fn new(checkpoint: Checkpoint) -> Self {
        Self {
            checkpoint,
            revision: ResourceRevision::default(),
        }
    }

    fn checkpoint(&self) -> &Checkpoint {
        &self.checkpoint
    }

    fn set_anchor(&mut self, anchor: SceneEntityId) {
        self.revision.advance();
        self.checkpoint.location.anchor = anchor;
    }

    fn collect(&mut self, pickup: CollectedPickup) -> bool {
        if self.checkpoint.collected.contains(&pickup) {
            return false;
        }
        self.revision.advance();
        self.checkpoint.collected.push(pickup);
        self.checkpoint.score = self.checkpoint.score.saturating_add(10).min(MAX_SCORE);
        true
    }

    fn add_marker(&mut self, level: LevelKey, position: Vec2) -> Result<MarkerId, String> {
        if self.checkpoint.markers.len() >= MAX_MARKERS {
            return Err("dynamic marker limit reached".into());
        }
        let id = self.checkpoint.next_marker_id;
        let next = id
            .get()
            .checked_add(1)
            .and_then(MarkerId::new)
            .filter(|id| id.get() != u64::MAX)
            .ok_or("marker identities are exhausted")?;
        self.revision.advance();
        self.checkpoint.markers.push(MarkerRecord {
            id,
            level,
            position: position.to_array(),
        });
        self.checkpoint.next_marker_id = next;
        Ok(id)
    }
}

impl SceneProductResource for Progress {
    type Baseline = (Arc<()>, u64);

    fn capture_baseline(&self) -> Self::Baseline {
        self.revision.capture()
    }

    fn matches_baseline(&self, baseline: &Self::Baseline) -> bool {
        self.revision.matches(baseline)
    }
}

#[derive(Resource)]
struct ActiveLevel {
    key: LevelKey,
    content: Option<Content>,
    revision: ResourceRevision,
}

impl ActiveLevel {
    fn new(key: LevelKey, content: Option<Content>) -> Self {
        Self {
            key,
            content,
            revision: ResourceRevision::default(),
        }
    }

    fn key(&self) -> LevelKey {
        self.key
    }

    fn content(&self) -> Option<&Content> {
        self.content.as_ref()
    }

    fn set_content(&mut self, content: Content) {
        self.revision.advance();
        self.content = Some(content);
    }
}

impl SceneProductResource for ActiveLevel {
    type Baseline = (Arc<()>, u64);

    fn capture_baseline(&self) -> Self::Baseline {
        self.revision.capture()
    }

    fn matches_baseline(&self, baseline: &Self::Baseline) -> bool {
        self.revision.matches(baseline)
    }
}

#[derive(Resource, Default)]
pub(super) struct Session {
    player_launch: Option<Result<(), String>>,
    held: [bool; 4],
    markers: Vec<travel::MarkerInstance>,
    phase: GamePhase,
    travel_count: u32,
    restarts: u32,
    saves: u32,
    restores: u32,
    initialized: bool,
    error: Option<String>,
    clock: crate::resume::GameClock,
}

struct ExplorationPlugin {
    options: RunOptions,
}
impl Plugin for ExplorationPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.world_mut()?
            .register_component::<travel::DynamicMarker>();
        let key = LevelKey::main(LevelId::STARTUP);
        app.insert_resource(Progress::new(Checkpoint::new(CheckpointLocation {
            level: key,
            anchor: SceneEntityId::new("checkpoint").unwrap(),
        })))?
        .insert_resource(ActiveLevel::new(key, None))?
        .insert_resource(Session::default())?
        .insert_resource(collision::CharacterState::default())?
        .insert_resource(GameSnapshot::default())?
        .insert_resource(travel::Travel::default())?
        .insert_resource(side_room::SideRoom::default())?
        .insert_resource(verification::Verification::new(self.options.scenario))?;
        resume::install(app)?;
        storage::install(app, self.options.save_path.as_deref())?;
        add_startup_scene_activation_systems(app, initialize)?;
        add_local_gameplay_command_systems(app, verification::drive)?;
        app.add_systems(
            CoreStage::FixedUpdate,
            (travel::consume, simulate, project_animation, capture)
                .chain()
                .in_set(GameplayCommandSet::Consume)
                .before(GameplayCommandSet::Capture)
                .in_set(FixedUpdateSet::Simulate),
        )?;
        app.add_systems(
            CoreStage::FixedUpdate,
            verification::observe_progress.after(GameplayCommandSet::Capture),
        )?;
        app.add_systems(
            CoreStage::TaskUpdate,
            (
                travel::advance,
                side_room::advance,
                resume::finish,
                project_animation,
            )
                .chain(),
        )?;
        app.add_systems(CoreStage::Last, capture)?;
        Ok(())
    }

    fn finish(&self, app: &mut App) -> Result<(), PluginError> {
        resume::record_configuration(app)
    }
}

#[must_use]
pub fn product_recipe() -> ProductRecipe {
    configured_recipe(RunOptions::default())
}

/// The fixed headless recipe shared by the cooker and the source-free Player.
#[must_use]
pub fn cooked_recipe() -> ProductRecipe {
    with_cooked_image_codec(exploration_recipe(RunOptions::default()))
}

/// The fixed desktop recipe with the game's authored input and semantic commands.
#[cfg(feature = "desktop")]
#[must_use]
pub fn cooked_desktop_recipe() -> ProductRecipe {
    with_cooked_image_codec(product_recipe())
}

fn with_cooked_image_codec(recipe: ProductRecipe) -> ProductRecipe {
    recipe
        .add_runtime_image_codec::<nara::image::ImageRuntimeCodec>(
            nara::runtime_content::RuntimeCodecDefinitionId::new("two-level-game.cooked-image", 1),
        )
        .expect("the game's fixed Runtime Image codec is valid")
}

#[must_use]
pub fn configured_recipe(options: RunOptions) -> ProductRecipe {
    exploration_recipe(options)
}

fn exploration_recipe(options: RunOptions) -> ProductRecipe {
    let contribution = SchemaContribution::configured(
        options,
        |options| ExplorationPlugin {
            options: options.clone(),
        },
        [PROVIDER],
    )
    .expect("static game schema and composition");
    ProductRecipe::new()
        .add_runtime_asset_codec::<nara::input::ActionMapDomain, nara::input::ActionMapRuntimeCodec>(
            nara::runtime_content::RuntimeCodecDefinitionId::new("two-level-game.input", 1),
        )
        .expect("the game's Input Runtime codec is valid")
        .add_asset_domain::<crate::sound_bank::SoundBankDomain>()
        .expect("the game's Sound Bank source importer is valid")
        .add_runtime_asset_codec::<
            crate::sound_bank::SoundBankDomain,
            crate::sound_bank::SoundBankCodec,
        >(nara::runtime_content::RuntimeCodecDefinitionId::new(
            "two-level-game.sound-bank",
            1,
        ))
        .expect("the game's Sound Bank Runtime codec is valid")
        .add_asset_domain::<nara_sprite_animation::SpriteAnimationDomain>()
        .expect("the Sprite animation source importer is valid")
        .add_runtime_asset_codec::<
            nara_sprite_animation::SpriteAnimationDomain,
            nara_sprite_animation::SpriteAnimationCodec,
        >(nara::runtime_content::RuntimeCodecDefinitionId::new(
            "two-level-game.sprite-animation", 1,
        ))
        .expect("the Sprite animation Runtime codec is valid")
        .add_contribution(contribution)
        .expect("static game recipe")
        .add_plugin::<input::GameInputPlugin>()
        .expect("static input plugin")
}

fn catalog() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    Ok(ComponentSchemaCatalog {
        components: vec![
            content::ObjectRole::persistent_component_schema(),
            content::SoundPalette::persistent_component_schema(),
            collision::CollisionBox::persistent_component_schema(),
            crate::animation::ExplorerAnimation::persistent_component_schema(),
            crate::markers::MarkerPalette::persistent_component_schema(),
            crate::markers::MarkerLink::persistent_component_schema(),
            crate::side_room::SideRoomPortal::persistent_component_schema(),
        ],
        ..Default::default()
    })
}
fn register(registry: &mut ComponentRegistryBuilder) -> Result<(), ComponentRegistryError> {
    registry
        .register_persistent_component::<content::ObjectRole>()?
        .register_persistent_component::<content::SoundPalette>()?
        .register_persistent_component::<collision::CollisionBox>()?
        .register_persistent_component::<crate::animation::ExplorerAnimation>()?
        .register_persistent_component::<crate::markers::MarkerPalette>()?
        .register_persistent_component::<crate::markers::MarkerLink>()?
        .register_persistent_component::<crate::side_room::SideRoomPortal>()
        .map(|_| ())
}

fn initialize(world: &mut World) {
    let Ok(timestep) = u64::try_from(
        world
            .resource::<nara::app::FixedTime>()
            .timestep()
            .as_nanos(),
    ) else {
        fail(
            world,
            "the game timestep cannot be represented by its logical clock".into(),
        );
        return;
    };
    world.resource_mut::<Session>().clock.timestep_nanos = timestep;
    resume::remember_current(world);
    let content = Content::read_current(world);
    match content {
        Ok(content) => {
            world
                .resource_mut::<Progress>()
                .set_anchor(content.checkpoint.clone());
            world.resource_mut::<ActiveLevel>().set_content(content);
            if let Err(error) = travel::reconstruct(world) {
                fail(world, error);
            } else {
                world.resource_mut::<Session>().initialized = true;
                project_animation(world);
                println!(
                    "two_level_game: entered Meadow instance=0; using configured game controls"
                );
            }
        }
        Err(error) => fail(world, error),
    }
}

fn fail(world: &mut World, error: String) {
    resume::reject_pending(world);
    eprintln!("two_level_game: reconstruction degraded: {error}");
    let mut session = world.resource_mut::<Session>();
    session.phase = GamePhase::Degraded;
    session.held = [false; 4];
    session.error = Some(error);
}

#[cfg(all(windows, feature = "audio-cpal"))]
pub(crate) fn sound_bank_reference(world: &World) -> Result<nara::asset::AssetRef, String> {
    let session = world.resource::<Session>();
    if !session.initialized || session.phase != GamePhase::Running {
        return Err("audio requires successful initial game reconstruction".into());
    }
    world
        .resource::<ActiveLevel>()
        .content()
        .map(|content| content.bank.clone())
        .ok_or_else(|| "initial game content lacks its Sound Bank".into())
}

fn player(world: &World) -> Option<nara::ecs::Entity> {
    let id = &world.resource::<ActiveLevel>().content()?.player;
    let current = CurrentScene::get(world)?;
    match current.receipt().resolve(world, id) {
        EntityLookup::Resolved(entity) => Some(entity),
        _ => None,
    }
}

fn simulate(world: &mut World) {
    if world.resource::<Session>().phase != GamePhase::Running {
        return;
    }
    if let Err(error) = resume::advance_clock(world) {
        fail(world, error);
        return;
    }
    let Some(player) = player(world) else {
        fail(world, "current explorer identity is unavailable".into());
        return;
    };
    let held = world
        .resource::<Session>()
        .held
        .map(|held| if held { 1.0 } else { 0.0 });
    let direction = Vec2::new(held[1] - held[0], held[2] - held[3]).normalize_or_zero();
    let duration = world.resource::<nara::app::FixedTime>().delta();
    let delta = duration.as_secs_f32();
    let position = match collision::move_player(world, player, direction * 160.0 * delta, duration)
    {
        Ok(position) => position,
        Err(error) => {
            fail(world, error);
            return;
        }
    };
    let level = world.resource::<ActiveLevel>();
    let key = level.key();
    let nearby = level
        .content()
        .unwrap()
        .pickups
        .iter()
        .filter(|(_, point)| point.distance(position) <= 16.0)
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    for entity in nearby {
        let id = entity.clone();
        let pickup = CollectedPickup { level: key, entity };
        let mut progress = world.resource_mut::<Progress>();
        if progress.collect(pickup) {
            println!(
                "two_level_game: collected={id} level={key:?} score={}",
                progress.checkpoint().score
            );
            #[cfg(all(windows, feature = "audio-cpal"))]
            {
                drop(progress);
                crate::audio::pickup_collected(world, key, id);
            }
        }
    }
    travel::hide_collected(world);
    side_room::find_relic(world, position);
}

fn project_animation(world: &mut World) {
    let session = world.resource::<Session>();
    if !session.initialized || session.phase != GamePhase::Running {
        return;
    }
    let Some(content) = world.resource::<ActiveLevel>().content() else {
        fail(world, "the animated explorer content is unavailable".into());
        return;
    };
    let region = content.animation.sample(session.clock, session.held);
    let Some(entity) = player(world) else {
        fail(
            world,
            "the animated explorer identity is unavailable".into(),
        );
        return;
    };
    match world.get::<nara::sprite::Sprite>(entity) {
        Some(sprite) if sprite.texture_region == Some(region) => {}
        Some(_) => {
            world
                .get_mut::<nara::sprite::Sprite>(entity)
                .unwrap()
                .texture_region = Some(region);
        }
        None => fail(world, "the animated explorer has no Sprite".into()),
    }
}

fn capture(world: &mut World) {
    let markers = match travel::observe_markers(world) {
        Ok(markers) => markers,
        Err(error) => {
            fail(world, error);
            Vec::new()
        }
    };
    let player = player(world)
        .and_then(|entity| {
            world
                .get::<Transform2d>(entity)
                .map(|value| value.translation)
        })
        .unwrap_or(Vec2::ZERO);
    let current = CurrentScene::get(world);
    let receipt = current.as_ref().map(|current| current.receipt());
    let progress = world.resource::<Progress>().checkpoint();
    let level = world.resource::<ActiveLevel>();
    let session = world.resource::<Session>();
    let visible = level.content().map_or(0, |content| {
        content
            .pickups
            .keys()
            .filter(|id| {
                receipt
                    .as_ref()
                    .is_some_and(|receipt| match receipt.resolve(world, id) {
                        EntityLookup::Resolved(entity) => world
                            .get::<nara::sprite::Sprite>(entity)
                            .is_some_and(|sprite| sprite.material.tint.a > 0.0),
                        _ => false,
                    })
            })
            .count()
    });
    let snapshot = GameSnapshot {
        tick: world.resource::<nara::app::FixedTime>().tick(),
        level: Some(level.key()),
        player,
        score: progress.score,
        collected: progress.collected.len(),
        visible_pickups: visible,
        markers,
        travel_count: session.travel_count,
        restarts: session.restarts,
        saves: session.saves,
        restores: session.restores,
        phase: session.phase,
        initialized: session.initialized,
        verified: world.resource::<verification::Verification>().complete,
        error: session.error.clone().or_else(|| {
            world
                .resource::<verification::Verification>()
                .failure
                .clone()
        }),
        current_instance: receipt.as_ref().map(|receipt| receipt.instance_id()),
        scene_members: receipt
            .as_ref()
            .map_or(0, |receipt| receipt.entity_ids().len()),
        logical_tick: session.clock.ticks,
        logical_elapsed_nanos: session.clock.elapsed_nanos,
        held: session.held,
        restoring: resume::is_pending(world),
        side_room: world.resource::<side_room::SideRoom>().phase(),
    };
    *world.resource_mut::<GameSnapshot>() = snapshot;
}

#[cfg(test)]
mod resource_tests;
