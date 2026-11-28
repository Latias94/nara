use std::collections::BTreeMap;

use bevy_ecs::{
    change_detection::{DetectChanges, Ref},
    entity_disabling::Disabled,
    error::{BevyError, warn},
    prelude::{Commands, Component, Entity, Has, Or, Query, Res, With, World},
    query::Allow,
};
use nara_asset::{AssetSlotRevision, Assets, Handle, tilemap::TileSetAsset};
use nara_core::Vec2;
use nara_scene::{
    TilemapAsset,
    components::TilemapSource,
    spatial::{ChildOf, GlobalTransform2d, Transform2d},
};

use crate::{
    CollisionGroups2d, CompoundBox2d, CompoundCollider2d, PhysicsError, merge_solid_grid_rects,
};

const CHUNK_SIDE: i32 = 32;

/// Default membership for solid tiles whose definition omits a collision group.
pub const TILEMAP_SOLID_GROUP: u32 = 1;
/// Default membership for one-way tiles. Pass this bit to the platformer controller.
pub const TILEMAP_ONE_WAY_GROUP: u32 = 1 << 31;

/// Generated static collider entities for a hydrated tilemap. Collision layers come from the
/// tileset. One-way tiles share a compound only with tiles at the same local height.
#[derive(Component)]
pub struct TilemapColliders2d {
    entities: Vec<Entity>,
    source: (Handle<TileSetAsset>, AssetSlotRevision),
    scale: Vec2,
    grid_changed: bool,
}

impl TilemapColliders2d {
    #[must_use]
    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }
}

type Sources<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Has<Disabled>,
        Option<Ref<'static, TilemapSource>>,
        Option<&'static TilemapAsset>,
        Option<&'static GlobalTransform2d>,
        Option<&'static mut TilemapColliders2d>,
    ),
    (
        Or<(With<TilemapSource>, With<TilemapColliders2d>)>,
        Allow<Disabled>,
    ),
>;

pub(super) fn hydrate_tilemap_colliders(
    mut commands: Commands,
    assets: Res<Assets<TileSetAsset>>,
    mut maps: Sources,
) {
    for (entity, disabled, source, asset, global, mut built) in &mut maps {
        let Some(source) = source.filter(|_| !disabled) else {
            if let Some(built) = built {
                despawn_colliders(&mut commands, &built.entities);
                commands.entity(entity).try_remove::<TilemapColliders2d>();
            }
            continue;
        };
        if let Some(built) = &mut built {
            built.grid_changed |= source.is_changed();
        }
        let Some((handle, revision)) = asset.and_then(TilemapAsset::ready) else {
            continue;
        };
        let Some(tileset) = assets.get(handle) else {
            continue;
        };
        let scale = global.and_then(tile_scale).unwrap_or(Vec2::ZERO);
        if built.as_ref().is_some_and(|built| {
            !built.grid_changed
                && built.source.0 == handle
                && &built.source.1 == revision
                && built.scale == scale
        }) {
            continue;
        }
        let entities = match build_colliders(&source, tileset, scale) {
            Ok(colliders) => colliders
                .into_iter()
                .map(|(position, memberships, collider)| {
                    commands
                        .spawn((
                            ChildOf(entity),
                            Transform2d::from_translation(position),
                            CollisionGroups2d {
                                memberships,
                                filters: u32::MAX,
                            },
                            collider,
                        ))
                        .id()
                })
                .collect(),
            Err(error) => {
                commands.queue_handled(
                    move |_: &mut World| -> Result<(), BevyError> {
                        Err(BevyError::warning(format!(
                            "tilemap collider on {entity}: {error}"
                        )))
                    },
                    warn,
                );
                Vec::new()
            }
        };
        if let Some(built) = built {
            despawn_colliders(&mut commands, &built.entities);
        }
        commands.entity(entity).try_insert(TilemapColliders2d {
            entities,
            source: (handle, revision.clone()),
            scale,
            grid_changed: false,
        });
    }
}

fn despawn_colliders(commands: &mut Commands, entities: &[Entity]) {
    for &entity in entities {
        commands.entity(entity).try_despawn();
    }
}

fn tile_scale(global: &GlobalTransform2d) -> Option<Vec2> {
    let axes = global.affine().matrix2;
    let right = axes.x_axis.try_normalize()?;
    let up = axes.y_axis.try_normalize()?;
    // Compound boxes require orthogonal map axes. The sign on Y preserves reflection.
    let scale = Vec2::new(
        axes.x_axis.length(),
        right.perp_dot(up).signum() * axes.y_axis.length(),
    );
    (right.dot(up).abs() <= 1e-5 && scale.is_finite()).then_some(scale)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ChunkLayer {
    x: i32,
    y: i32,
    memberships: u32,
    one_way_row: Option<i32>,
}

fn build_colliders(
    source: &TilemapSource,
    tileset: &TileSetAsset,
    scale: Vec2,
) -> Result<Vec<(Vec2, u32, CompoundCollider2d)>, PhysicsError> {
    let mut chunks = BTreeMap::<ChunkLayer, Vec<(u32, u32)>>::new();
    for (index, &cell) in source.cells.0.iter().enumerate() {
        let Some(tile) = cell
            .checked_sub(1)
            .and_then(|tile| tileset.tiles.get(usize::from(tile)))
        else {
            continue;
        };
        if !tile.solid && !tile.one_way {
            continue;
        }
        let Some((x, y)) = source.cell_coordinates(index) else {
            continue;
        };
        let local_y = y.rem_euclid(CHUNK_SIDE);
        let memberships = match (tile.collision_group, tile.one_way) {
            (0, true) => TILEMAP_ONE_WAY_GROUP,
            (0, false) => TILEMAP_SOLID_GROUP,
            (group, _) => group,
        };
        chunks
            .entry(ChunkLayer {
                x: x.div_euclid(CHUNK_SIDE),
                y: y.div_euclid(CHUNK_SIDE),
                memberships,
                one_way_row: tile.one_way.then_some(local_y),
            })
            .or_default()
            .push((x.rem_euclid(CHUNK_SIDE) as u32, local_y as u32));
    }

    chunks
        .into_iter()
        .map(|(chunk, cells)| {
            let mut occupied = [[false; CHUNK_SIDE as usize]; CHUNK_SIDE as usize];
            for (x, y) in cells {
                occupied[y as usize][x as usize] = true;
            }
            let parts: Vec<_> =
                merge_solid_grid_rects(CHUNK_SIDE as u32, CHUNK_SIDE as u32, |x, y| {
                    occupied[y as usize][x as usize]
                })
                .into_iter()
                .map(|rect| {
                    let half = Vec2::new(rect.width as f32, rect.height as f32) * 0.5;
                    CompoundBox2d {
                        center: (Vec2::new(rect.x as f32, rect.y as f32) + half)
                            * source.tile_size
                            * scale,
                        half_extents: half * source.tile_size * scale.abs(),
                    }
                })
                .collect();
            let position = Vec2::new((chunk.x * CHUNK_SIDE) as f32, (chunk.y * CHUNK_SIDE) as f32)
                * source.tile_size;
            Ok((position, chunk.memberships, CompoundCollider2d::new(parts)?))
        })
        .collect()
}
