use std::{collections::HashMap, fmt, time::Duration};

use bevy_ecs::{
    change_detection::DetectChangesMut,
    component::{Component, ComponentMutability},
    entity::Entity,
    prelude::{
        Commands, Local, Message, Messages, Or, Query, QueryState, Res, ResMut, Resource, With,
        World,
    },
    schedule::{IntoScheduleConfigs, SystemSet},
};
use nara_app::{
    App, CoreStage, FixedTime, FixedUpdateSet, FrameCleanup, Plugin, PluginCategory,
    PluginDeclaration, PluginError, PluginId, VirtualTime,
};
use nara_asset::{
    ASSET_PLUGIN_ID, AssetAppExt, AssetLoadSet, AssetServer, AssetStates, Assets, Handle, LoadState,
};
use nara_core::{AssetId, EntityUuid};
use nara_reflect::{
    __macro_support::PersistentFieldCodec, ComponentCodecError, ComponentFieldPath,
    ComponentFieldPathSegment, ComponentTypeId, ComponentValue, Schema,
};

use crate::{
    SceneManager, SceneMember, SceneRegistry, SceneUpdateSet,
    components::AnimationSource,
    spatial::{Transform2d, Visibility},
};

use super::{KeyframeValue, PropertyAnimationClip, PropertyAnimationLoader, PropertyPlaybackMode};

const ID: PluginId = PluginId::new("nara.property-animation");
const DECLARATION: PluginDeclaration =
    PluginDeclaration::new(ID, PluginCategory::Runtime).requires_plugins(&[ASSET_PLUGIN_ID]);
const MARKER_LIMIT_PER_TICK: usize = 1024;

/// Which clock advances an animation. Fixed animation can drive simulation components.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnimationClock {
    #[default]
    Fixed,
    Virtual,
}

/// Property animation and tweening run before step consumers or frame extraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, SystemSet)]
pub enum AnimationSet {
    Fixed,
    Virtual,
}

#[derive(Clone, Debug, PartialEq, Eq, Message)]
pub enum PropertyAnimationEvent {
    Marker {
        entity: Entity,
        clip: Handle<PropertyAnimationClip>,
        name: String,
    },
    Completed {
        entity: Entity,
        clip: Handle<PropertyAnimationClip>,
    },
}

/// Plays a property animation on this entity and other rows in its scene instance.
#[derive(Component)]
pub struct AnimationPlayer {
    clip: Handle<PropertyAnimationClip>,
    pub clock: AnimationClock,
    elapsed: Duration,
    speed: f32,
    playing: bool,
    sampled: bool,
    completed: bool,
    error: Option<PropertyAnimationError>,
}

impl AnimationPlayer {
    #[must_use]
    pub const fn new(clip: Handle<PropertyAnimationClip>) -> Self {
        Self {
            clip,
            clock: AnimationClock::Fixed,
            elapsed: Duration::ZERO,
            speed: 1.0,
            playing: true,
            sampled: false,
            completed: false,
            error: None,
        }
    }

    pub fn play(&mut self, clip: Handle<PropertyAnimationClip>) {
        self.clip = clip;
        self.restart();
    }

    pub fn restart(&mut self) {
        self.elapsed = Duration::ZERO;
        self.playing = true;
        self.sampled = false;
        self.completed = false;
        self.error = None;
    }

    pub fn pause(&mut self) {
        self.playing = false;
    }

    pub fn resume(&mut self) {
        if !self.completed {
            self.playing = true;
            self.error = None;
        }
    }

    pub fn set_speed(&mut self, speed: f32) -> Result<(), PropertyAnimationError> {
        if !speed.is_finite() || speed < 0.0 {
            return Err(PropertyAnimationError::InvalidSpeed);
        }
        self.speed = speed;
        Ok(())
    }

    #[must_use]
    pub const fn clip(&self) -> Handle<PropertyAnimationClip> {
        self.clip
    }

    #[must_use]
    pub const fn elapsed(&self) -> Duration {
        self.elapsed
    }

    #[must_use]
    pub const fn is_playing(&self) -> bool {
        self.playing
    }

    #[must_use]
    pub const fn error(&self) -> Option<&PropertyAnimationError> {
        self.error.as_ref()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PropertyAnimationError {
    #[error("animation speed must be finite and nonnegative")]
    InvalidSpeed,
    #[error("the animation target {0} is absent from this scene instance")]
    MissingTarget(EntityUuid),
    #[error("the animation target does not have component '{0}'")]
    MissingComponent(&'static str),
    #[error("no property applier is registered for '{0}'")]
    MissingApplier(ComponentTypeId),
    #[error("the animation value is incompatible with field '{0}'")]
    WrongValue(ComponentFieldPath),
    #[error("tween endpoints must be finite")]
    InvalidTweenEndpoints,
    #[error(transparent)]
    Schema(#[from] ComponentCodecError),
}

/// Registers the property loader, field writers, and fixed and virtual clocks.
#[derive(Clone, Copy, Debug, Default)]
pub struct AnimationPlugin;

impl Plugin for AnimationPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.register_asset_loader(PropertyAnimationLoader)?
            .init_resource::<PropertyAppliers>()?
            .init_resource::<Messages<PropertyAnimationEvent>>()?
            .add_systems(
                CoreStage::TaskUpdate,
                hydrate_players
                    .after(AssetLoadSet::Publish)
                    .after(SceneUpdateSet),
            )?
            .configure_sets(
                CoreStage::FixedUpdate,
                AnimationSet::Fixed.in_set(FixedUpdateSet::Prepare),
            )?
            .add_systems(
                CoreStage::FixedUpdate,
                hydrate_players
                    .in_set(FixedUpdateSet::Prepare)
                    .before(AnimationSet::Fixed),
            )?
            .add_systems(
                CoreStage::FixedUpdate,
                advance_fixed.in_set(AnimationSet::Fixed),
            )?
            .add_systems(
                CoreStage::Update,
                advance_virtual.in_set(AnimationSet::Virtual),
            )?
            .add_systems(FrameCleanup, |world: &mut World| {
                world
                    .resource_mut::<Messages<PropertyAnimationEvent>>()
                    .update()
            })?;
        Ok(())
    }
}

#[derive(Component)]
struct AnimationRequest {
    id: AssetId,
    generation: u64,
    handle: Option<Handle<PropertyAnimationClip>>,
}

type AuthoredPlayers<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static AnimationSource>,
        Option<&'static AnimationRequest>,
        Option<&'static mut AnimationPlayer>,
    ),
    Or<(With<AnimationSource>, With<AnimationRequest>)>,
>;

fn hydrate_players(
    mut commands: Commands,
    mut server: ResMut<AssetServer>,
    assets: Res<Assets<PropertyAnimationClip>>,
    states: Res<AssetStates>,
    mut sources: AuthoredPlayers,
) {
    let generation = server.runtime_source_generation();
    for (entity, source, request, player) in &mut sources {
        let Some(source) = source else {
            if player.as_ref().is_some_and(|player| {
                request.and_then(|request| request.handle) == Some(player.clip())
            }) {
                commands.entity(entity).try_remove::<AnimationPlayer>();
            }
            commands.entity(entity).try_remove::<AnimationRequest>();
            continue;
        };
        let cached = request.filter(|request| source.clip == request.id);
        let handle = cached.and_then(|request| request.handle);
        let resident = handle.is_some_and(|handle| assets.get(handle).is_some());
        let removed = !resident
            && handle.is_some_and(|handle| {
                states
                    .state(handle.id())
                    .is_some_and(|state| state.load_state() == &LoadState::Removed)
            });
        let handle = if cached.is_none()
            || !resident
                && (removed || cached.is_some_and(|request| request.generation != generation))
        {
            let handle = server.load_id(source.clip).ok();
            commands.entity(entity).try_insert(AnimationRequest {
                id: source.clip,
                generation,
                handle,
            });
            handle
        } else {
            handle
        };
        if let Some(handle) = handle.filter(|&handle| assets.get(handle).is_some()) {
            if let Some(mut player) = player {
                if player.clip() != handle {
                    player.play(handle);
                }
            } else {
                commands
                    .entity(entity)
                    .try_insert(AnimationPlayer::new(handle));
            }
        }
    }
}

/// A typed writer can handle its supported fields or leave the write to the scene Schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertyWrite {
    Applied,
    /// The target is valid but has not been hydrated yet. Playback will retry.
    Waiting,
    Unhandled,
}

/// Typed animation field writer, registered under a stable component ID.
pub type ApplyProperty = fn(
    &mut World,
    Entity,
    &ComponentFieldPath,
    &KeyframeValue,
) -> Result<PropertyWrite, PropertyAnimationError>;

/// Typed field writers registered by runtime plugins. Writers borrow existing track values.
#[derive(Resource)]
pub struct PropertyAppliers(HashMap<&'static str, ApplyProperty>);

impl fmt::Debug for PropertyAppliers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PropertyAppliers")
            .field("count", &self.0.len())
            .finish()
    }
}

impl Default for PropertyAppliers {
    fn default() -> Self {
        let mut writers = Self(HashMap::new());
        writers.register(Transform2d::COMPONENT_ID, transform_property);
        writers.register(Visibility::COMPONENT_ID, visibility_property);
        writers
    }
}

impl PropertyAppliers {
    pub fn register(&mut self, component: &'static str, write: ApplyProperty) {
        self.0.insert(component, write);
    }
}

/// Component ID for runtime sprite fields supplied by the 2D renderer.
pub const SPRITE_COMPONENT_ID: &str = "nara.Sprite";

pub(super) fn apply_property(
    world: &mut World,
    entity: Entity,
    component: &ComponentTypeId,
    path: &ComponentFieldPath,
    value: &KeyframeValue,
) -> Result<PropertyWrite, PropertyAnimationError> {
    let write = world
        .get_resource::<PropertyAppliers>()
        .and_then(|writers| writers.0.get(component.as_str()))
        .copied();
    if let Some(write) = write {
        let result = write(world, entity, path, value)?;
        if result != PropertyWrite::Unhandled {
            return Ok(result);
        }
    }
    let apply = world
        .get_resource::<SceneRegistry>()
        .and_then(|registry| registry.property_applier(component))
        .ok_or_else(|| PropertyAnimationError::MissingApplier(component.clone()))?;
    apply(world, entity, path, value)
}

pub(crate) fn apply_schema_field<T: Schema>(
    world: &mut World,
    entity: Entity,
    path: &ComponentFieldPath,
    value: &KeyframeValue,
) -> Result<PropertyWrite, PropertyAnimationError> {
    let value = match value {
        KeyframeValue::F32(value) => value.encode(),
        KeyframeValue::Vec2(value) => value.encode(),
        KeyframeValue::Color(value) => value.encode(),
        KeyframeValue::Bool(value) => value.encode(),
        KeyframeValue::I64(value) => value.encode(),
        KeyframeValue::String(value) => value.encode(),
    }?;
    let missing = || PropertyAnimationError::MissingComponent(T::COMPONENT_ID);
    let current = world.get::<T>(entity).ok_or_else(missing)?;
    let current = current.get_field(path)?;
    let value = match (&current, value) {
        (ComponentValue::U64(_), ComponentValue::I64(value)) => ComponentValue::U64(
            u64::try_from(value).map_err(|_| PropertyAnimationError::WrongValue(path.clone()))?,
        ),
        (_, value) => value,
    };
    if current == value {
        return Ok(PropertyWrite::Applied);
    }
    if T::Mutability::MUTABLE {
        let id = world.component_id::<T>().ok_or_else(missing)?;
        let component = world.get_mut_by_id(entity, id).ok_or_else(missing)?;
        // SAFETY: `id` was obtained for this exact component type in this world.
        unsafe { component.with_type::<T>() }.set_field(path, value)?;
    } else {
        world
            .modify_component(entity, |component: &mut T| component.set_field(path, value))
            .ok()
            .flatten()
            .ok_or_else(missing)??;
    }
    Ok(PropertyWrite::Applied)
}

fn transform_property(
    world: &mut World,
    entity: Entity,
    path: &ComponentFieldPath,
    value: &KeyframeValue,
) -> Result<PropertyWrite, PropertyAnimationError> {
    let field = match path.segments() {
        [ComponentFieldPathSegment::Field(field), ..] => field.as_str(),
        _ => return Ok(PropertyWrite::Unhandled),
    };
    let mut transform =
        world
            .get_mut::<Transform2d>(entity)
            .ok_or(PropertyAnimationError::MissingComponent(
                Transform2d::COMPONENT_ID,
            ))?;
    let mut next = *transform;
    match (field, &path.segments()[1..], value) {
        ("translation", [], KeyframeValue::Vec2(value)) => next.translation = *value,
        ("rotation", [], KeyframeValue::F32(value)) => next.rotation = *value,
        ("scale", [], KeyframeValue::Vec2(value)) => next.scale = *value,
        ("z", [], KeyframeValue::F32(value)) => next.z = *value,
        (
            "translation" | "scale",
            [ComponentFieldPathSegment::Field(axis)],
            KeyframeValue::F32(value),
        ) => {
            let target = if field == "translation" {
                &mut next.translation
            } else {
                &mut next.scale
            };
            match axis.as_str() {
                "x" => target.x = *value,
                "y" => target.y = *value,
                _ => return Err(PropertyAnimationError::WrongValue(path.clone())),
            }
        }
        ("translation" | "rotation" | "scale" | "z", _, _) => {
            return Err(PropertyAnimationError::WrongValue(path.clone()));
        }
        _ => return Ok(PropertyWrite::Unhandled),
    }
    transform.set_if_neq(next);
    Ok(PropertyWrite::Applied)
}

fn visibility_property(
    world: &mut World,
    entity: Entity,
    path: &ComponentFieldPath,
    value: &KeyframeValue,
) -> Result<PropertyWrite, PropertyAnimationError> {
    if path.top_level_field() != Some("mode") {
        return Ok(PropertyWrite::Unhandled);
    }
    let KeyframeValue::String(mode) = value else {
        return Err(PropertyAnimationError::WrongValue(path.clone()));
    };
    let next = match mode.as_str() {
        "inherited" => Visibility::Inherited,
        "visible" => Visibility::Visible,
        "hidden" => Visibility::Hidden,
        _ => return Err(PropertyAnimationError::WrongValue(path.clone())),
    };
    let mut visibility =
        world
            .get_mut::<Visibility>(entity)
            .ok_or(PropertyAnimationError::MissingComponent(
                Visibility::COMPONENT_ID,
            ))?;
    if *visibility != next {
        *visibility = next;
    }
    Ok(PropertyWrite::Applied)
}

type ActivePlayers = QueryState<Entity, With<AnimationPlayer>>;

pub(super) fn advance_fixed(
    world: &mut World,
    query: &mut ActivePlayers,
    scratch: Local<Vec<Entity>>,
) {
    let delta = world.resource::<FixedTime>().delta();
    advance_players(world, query, scratch, AnimationClock::Fixed, delta);
}

pub(super) fn advance_virtual(
    world: &mut World,
    query: &mut ActivePlayers,
    scratch: Local<Vec<Entity>>,
) {
    let delta = world.resource::<VirtualTime>().delta;
    advance_players(world, query, scratch, AnimationClock::Virtual, delta);
}

fn advance_players(
    world: &mut World,
    query: &mut ActivePlayers,
    mut scratch: Local<Vec<Entity>>,
    clock: AnimationClock,
    delta: Duration,
) {
    scratch.clear();
    scratch.extend(query.iter(world));
    for &entity in scratch.iter() {
        let Some(player) = world.get::<AnimationPlayer>(entity) else {
            continue;
        };
        if !player.playing || player.clock != clock {
            continue;
        }
        let handle = player.clip;
        let Some(clip) = world
            .resource::<Assets<PropertyAnimationClip>>()
            .get(handle)
            .cloned()
        else {
            continue;
        };
        let duration = clip.duration();
        let before = player.elapsed;
        let first = !player.sampled;
        let delta = before.saturating_add(
            Duration::try_from_secs_f64(delta.as_secs_f64() * f64::from(player.speed))
                .unwrap_or(Duration::MAX),
        );
        let after = if clip.mode() == PropertyPlaybackMode::Once {
            delta.min(duration)
        } else {
            delta
        };
        let completed = clip.mode() == PropertyPlaybackMode::Once && after == duration;
        if !first && before == after && !completed {
            continue;
        }
        let phase = sample_time(after, duration, clip.mode());
        let applied = clip.tracks().iter().try_fold(true, |ready, track| {
            let target = track
                .target_child
                .map_or(Ok(entity), |id| scene_target(world, entity, id))?;
            let value = track.sample(phase);
            apply_property(world, target, &track.component, &track.path, value.as_ref())
                .map(|result| ready && result == PropertyWrite::Applied)
        });
        if matches!(applied, Ok(false)) {
            continue;
        }
        let Some(mut player) = world.get_mut::<AnimationPlayer>(entity) else {
            continue;
        };
        if player.clip != handle {
            continue;
        }
        player.elapsed = after;
        player.sampled = true;
        player.completed = completed && applied.is_ok();
        player.playing = !completed && applied.is_ok();
        player.error = applied.err();
        if player.error.is_some() {
            continue;
        }
        let mut events = world.resource_mut::<Messages<PropertyAnimationEvent>>();
        write_markers(&mut events, entity, handle, &clip, first, before, after);
        if completed {
            events.write(PropertyAnimationEvent::Completed {
                entity,
                clip: handle,
            });
        }
    }
}

fn scene_target(
    world: &World,
    entity: Entity,
    target: EntityUuid,
) -> Result<Entity, PropertyAnimationError> {
    world
        .get::<SceneMember>(entity)
        .and_then(|member| world.get_resource::<SceneManager>()?.entities(member.0))
        .and_then(|entities| entities.get(&target).copied())
        .ok_or(PropertyAnimationError::MissingTarget(target))
}

fn sample_time(elapsed: Duration, duration: Duration, mode: PropertyPlaybackMode) -> Duration {
    let length = duration.as_nanos();
    if length == 0 || mode == PropertyPlaybackMode::Once {
        return elapsed.min(duration);
    }
    let cycle = if mode == PropertyPlaybackMode::PingPong {
        length * 2
    } else {
        length
    };
    let phase = elapsed.as_nanos() % cycle;
    let nanos = if phase > length { cycle - phase } else { phase };
    Duration::new(
        (nanos / 1_000_000_000) as u64,
        (nanos % 1_000_000_000) as u32,
    )
}

fn write_markers(
    events: &mut Messages<PropertyAnimationEvent>,
    entity: Entity,
    clip_handle: Handle<PropertyAnimationClip>,
    clip: &PropertyAnimationClip,
    first: bool,
    before: Duration,
    after: Duration,
) {
    if clip.markers().is_empty() {
        return;
    }
    let from = before.as_nanos();
    let to = after.as_nanos();
    let end = clip.duration().as_nanos();
    let mode = clip.mode();
    let cycle = if mode == PropertyPlaybackMode::PingPong {
        end * 2
    } else {
        end
    };
    let (first_cycle, final_cycle) = if cycle != 0 && mode != PropertyPlaybackMode::Once {
        // A marker at a looping clip's end belongs to the preceding cycle.
        (from.saturating_sub(1) / cycle, to / cycle)
    } else {
        (0, 0)
    };
    let mut budget = MARKER_LIMIT_PER_TICK;
    let mut emit = |offset, name: &str| {
        if budget != 0 && offset <= to && (offset > from || first && offset == 0) {
            budget -= 1;
            events.write(PropertyAnimationEvent::Marker {
                entity,
                clip: clip_handle,
                name: name.to_owned(),
            });
        }
        budget != 0
    };
    for index in first_cycle..=final_cycle {
        let start = index * cycle;
        for marker in clip.markers() {
            if !emit(start + marker.time.as_nanos(), &marker.name) {
                return;
            }
        }
        if mode == PropertyPlaybackMode::PingPong {
            for marker in clip
                .markers()
                .iter()
                .rev()
                .filter(|marker| !marker.time.is_zero() && marker.time < clip.duration())
            {
                if !emit(start + cycle - marker.time.as_nanos(), &marker.name) {
                    return;
                }
            }
        }
    }
}
