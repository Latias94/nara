//! World-space movement and pickup contact for the collection game.

use nara::{
    ecs::{Entity, With, bevy_ecs::error::BevyError},
    prelude::{FixedTime, Query, Res, ResMut, Transform2d, Vec2},
    render::Camera2d,
    scene::fixed_spatial::{
        CapturedTransform2d, FixedInputSpatialSnapshot2d, FixedSpatialSnapshot2d,
    },
    sprite::Sprite,
};

use super::{Collectible, GameState, PlayerConfig, VICTORY};

const WORLD_POSITION_TOLERANCE: f32 = 0.001;

#[derive(Clone, Copy)]
struct Walk {
    player: Entity,
    world_start: Vec2,
    local_end: Vec2,
    parent: Option<CapturedTransform2d>,
}

#[derive(Clone, Copy)]
struct MovementParent {
    player: Entity,
    frame: MovementFrame,
}

#[derive(Clone, Copy)]
enum MovementFrame {
    World,
    Parent(CapturedTransform2d),
    Singular,
}

#[derive(Default)]
pub(super) struct WalkingState {
    parents: Vec<MovementParent>,
    walk: Option<Walk>,
}

pub(super) fn capture_movement_parents(
    spatial: FixedInputSpatialSnapshot2d<'_>,
    players: Query<Entity, (With<PlayerConfig>, With<Transform2d>)>,
    mut state: ResMut<GameState>,
) -> Result<(), BevyError> {
    state.walking.walk = None;
    state.walking.parents.clear();
    for player in &players {
        let mut effective_parent = spatial.effective_parent(player).map_err(BevyError::error)?;
        let mut frame = effective_parent.map_or(MovementFrame::World, MovementFrame::Parent);
        let mut child = player;
        while effective_parent.is_some()
            && let Some(parent) = spatial.parent(child).map_err(BevyError::error)?
        {
            let scale = spatial.local(parent).map_err(BevyError::error)?.scale;
            if scale.x == 0.0 || scale.y == 0.0 {
                frame = MovementFrame::Singular;
                break;
            }
            child = parent;
            effective_parent = spatial.effective_parent(child).map_err(BevyError::error)?;
        }
        state.walking.parents.push(MovementParent { player, frame });
    }
    Ok(())
}

pub(super) fn move_player(
    time: Res<FixedTime>,
    mut state: ResMut<GameState>,
    mut players: Query<(&PlayerConfig, &mut Transform2d)>,
) {
    if state.won {
        return;
    }
    let [left, right, up, down] = state.held.map(|held| if held { 1.0 } else { 0.0 });
    let direction = Vec2::new(right - left, up - down).normalize_or_zero();
    if direction == Vec2::ZERO {
        return;
    }

    let state = &mut *state;
    for &MovementParent { player, frame } in &state.walking.parents {
        let parent = match frame {
            MovementFrame::World => None,
            MovementFrame::Parent(parent) => Some(parent),
            MovementFrame::Singular => continue,
        };
        let Ok((config, mut transform)) = players.get_mut(player) else {
            continue;
        };
        let step = direction * (config.speed * time.delta().as_secs_f32());
        if step == Vec2::ZERO || !step.is_finite() {
            continue;
        }
        let local_start = transform.translation;
        let world_from_local = parent.map(|parent| parent.matrix());
        let world_start = world_from_local.map_or(local_start, |matrix| {
            matrix.mul_vec3(local_start.extend(1.0)).truncate()
        });
        if !world_start.is_finite() {
            continue;
        }
        let min = Vec2::new(-280.0, -160.0).min(world_start);
        let max = Vec2::new(280.0, 160.0).max(world_start);
        let world_end = (world_start + step).clamp(min, max);
        if world_start == world_end {
            continue;
        }
        let local_end = if let Some(matrix) = world_from_local {
            let x = matrix.x_axis.truncate().as_dvec2();
            let y = matrix.y_axis.truncate().as_dvec2();
            let determinant = x.perp_dot(y);
            if determinant == 0.0 {
                continue;
            }
            let displacement = world_end.as_dvec2() - world_start.as_dvec2();
            let local = local_start.as_dvec2();
            let local_end = Vec2::new(
                (local.x + displacement.perp_dot(y) / determinant) as f32,
                (local.y + x.perp_dot(displacement) / determinant) as f32,
            );
            let projected = matrix.mul_vec3(local_end.extend(1.0)).truncate();
            if projected == world_start
                || !projected.abs_diff_eq(world_end, WORLD_POSITION_TOLERANCE)
            {
                continue;
            }
            local_end
        } else {
            world_end
        };
        if local_end.is_finite() && local_end != local_start {
            transform.translation = local_end;
            state.walking.walk = Some(Walk {
                player,
                world_start,
                local_end,
                parent,
            });
        }
    }
}

pub(super) fn collect_targets(
    mut state: ResMut<GameState>,
    spatial: FixedSpatialSnapshot2d<'_>,
    players: Query<Entity, With<PlayerConfig>>,
    mut targets: Query<(Entity, &Collectible, &mut Sprite)>,
    mut cameras: Query<&mut Camera2d>,
) -> Result<(), BevyError> {
    let Ok(player) = players.single() else {
        return Ok(());
    };
    let end = spatial
        .global(player)
        .map_err(BevyError::error)?
        .translation();
    let start = if let Some(walk) = state.walking.walk
        && walk.player == player
        && spatial.local(player).map_err(BevyError::error)?.translation == walk.local_end
        && spatial.effective_parent(player).map_err(BevyError::error)? == walk.parent
    {
        walk.world_start
    } else {
        end
    };
    let start = start.as_dvec2();
    let end = end.as_dvec2();
    let travel = end - start;
    let travel_squared = travel.length_squared();

    let total = targets.iter().count();
    for (entity, config, mut sprite) in &mut targets {
        if state.collected.contains(&entity) || config.radius < 0.0 {
            continue;
        }
        let position = spatial
            .global(entity)
            .map_err(BevyError::error)?
            .translation();
        let distance_squared =
            if travel_squared == 0.0 || inherits_player_walk(&spatial, entity, player)? {
                position.as_dvec2().distance_squared(end)
            } else {
                let offset = position.as_dvec2() - start;
                let fraction = (offset.dot(travel) / travel_squared).clamp(0.0, 1.0);
                offset.distance_squared(travel * fraction)
            };
        if distance_squared <= f64::from(config.radius).powi(2) {
            state.collected.insert(entity);
            sprite.material.tint.a = 0.0;
            println!("collect_game: collected={}/{total}", state.collected.len());
        }
    }
    if !state.won && total > 0 && state.collected.len() == total {
        state.won = true;
        for mut camera in &mut cameras {
            camera.clear_color = Some(VICTORY);
        }
        println!("collect_game: victory! Press R to reset.");
    }
    Ok(())
}

fn inherits_player_walk(
    spatial: &FixedSpatialSnapshot2d<'_>,
    mut entity: Entity,
    player: Entity,
) -> Result<bool, BevyError> {
    while entity != player {
        if spatial
            .effective_parent(entity)
            .map_err(BevyError::error)?
            .is_none()
        {
            return Ok(false);
        }
        let Some(parent) = spatial.parent(entity).map_err(BevyError::error)? else {
            return Ok(false);
        };
        entity = parent;
    }
    Ok(true)
}
