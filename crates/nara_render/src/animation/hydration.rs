use nara_app::{App, CoreStage, PluginError};
use nara_asset::{AssetError, AssetLoadSet, AssetServer, AssetStates, Assets, Handle, LoadState};
use nara_core::AssetId;
use nara_ecs::{
    Commands, Component, Entity, EntityWorldMut, IntoScheduleConfigs, Or, Query, Res, ResMut, With,
    World,
    bevy_ecs::error::{BevyError, warn},
};
use nara_scene::{SceneUpdateSet, components::SpriteAnimatorSource};

use super::{SpriteAnimationSet, SpriteAnimator};

pub(super) fn install(app: &mut App) -> Result<(), PluginError> {
    app.add_systems(
        CoreStage::TaskUpdate,
        hydrate_animators
            .after(AssetLoadSet::Publish)
            .after(SceneUpdateSet),
    )?;
    Ok(())
}

#[derive(Component)]
struct AnimatorRequest {
    id: AssetId,
    source_generation: u64,
    handle: Option<Handle<SpriteAnimationSet>>,
    applied_clip: Option<String>,
}

type AnimatorSources<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static SpriteAnimatorSource>,
        Option<&'static mut SpriteAnimator>,
        Option<&'static mut AnimatorRequest>,
    ),
    Or<(With<SpriteAnimatorSource>, With<AnimatorRequest>)>,
>;

fn hydrate_animators(
    mut commands: Commands,
    mut sources: AnimatorSources,
    mut server: ResMut<AssetServer>,
    states: Res<AssetStates>,
    assets: Res<Assets<SpriteAnimationSet>>,
) {
    let source_generation = server.runtime_source_generation();
    for (entity, source, animator, mut cached) in &mut sources {
        let Some(source) = source else {
            commands
                .entity(entity)
                .queue_silenced(|mut entity: EntityWorldMut| {
                    if entity.contains::<SpriteAnimatorSource>() {
                        return;
                    }
                    if entity
                        .get::<SpriteAnimator>()
                        .is_some_and(|animator| animator.generated_by_source)
                    {
                        entity.remove::<SpriteAnimator>();
                    }
                    entity.remove::<AnimatorRequest>();
                });
            continue;
        };

        let refresh = cached.as_deref().is_none_or(|request| {
            request.id != source.set
                || request.source_generation != source_generation
                || request.handle.is_some_and(|handle| {
                    states
                        .state(handle.id())
                        .is_some_and(|state| state.load_state() == &LoadState::Removed)
                })
        });
        let mut next = None;
        let request = cached.as_deref_mut().unwrap_or_else(|| {
            next.insert(AnimatorRequest {
                id: source.set,
                source_generation,
                handle: None,
                applied_clip: None,
            })
        });
        if refresh {
            let handle = match server.load_id(source.set) {
                Ok(handle) => Some(handle),
                Err(AssetError::MissingRuntimeSource) => None,
                Err(error) => {
                    commands.queue_handled(
                        move |_: &mut World| -> Result<(), BevyError> {
                            Err(BevyError::warning(format!(
                                "sprite animator source on {entity} could not load its set: {error}"
                            )))
                        },
                        warn,
                    );
                    None
                }
            };
            if request.id != source.set || request.handle != handle {
                request.applied_clip = None;
            }
            request.id = source.set;
            request.source_generation = source_generation;
            request.handle = handle;
        }

        if let Some(handle) = request.handle
            && (request.applied_clip.as_deref() != Some(source.clip.as_str()) || animator.is_none())
            && assets
                .get(handle)
                .is_some_and(|set| set.get(&source.clip).is_some())
        {
            if let Some(mut animator) = animator {
                animator.apply_source(handle, &source.clip);
            } else {
                let mut animator = SpriteAnimator::new(handle, source.clip.as_str());
                animator.generated_by_source = true;
                commands.entity(entity).try_insert_if_new(animator);
            }
            request.applied_clip = Some(source.clip.clone());
        }
        if let Some(next) = next {
            commands.entity(entity).try_insert(next);
        }
    }
}
