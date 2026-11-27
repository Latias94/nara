use std::{error::Error, hint::black_box, time::Instant};

use bevy_ecs::{component::Component, world::World};
use nara_core::{DocumentId, EntityUuid};
use nara_reflect::{ComponentSchemaCatalog, PackedU16, Schema};
use nara_scene::{
    authoring::{SceneDocument, SceneEntity},
    components::{SpriteSource, Transform2d},
    cook::cook_flattened_scene,
    cooked::{CookedRegistry, CookedScene},
};
use uuid::Uuid;

const ENTITY_COUNT: u16 = 1_000;
const TILE_ROWS: u16 = 4;
const TILES_PER_ROW: usize = 2_500;
const SAMPLES: usize = 101;

#[derive(Component, Schema)]
#[schema(crate = "nara_reflect", id = "benchmark.TileCells", version = 1)]
struct TileCells {
    cells: PackedU16,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut source = SceneDocument::new(DocumentId::try_from_uuid(Uuid::from_u128(1))?);
    let mut seed = 87_493_u32;
    let mut cell_checksum = 0_u64;
    let entity_id = |index| EntityUuid::try_from_uuid(Uuid::from_u128(u128::from(index)));
    for index in 1..=ENTITY_COUNT {
        let mut row = SceneEntity::new(entity_id(index)?, format!("Scene entity {index}"));
        if index > 10 {
            row.parent = Some(entity_id(index % 10 + 1)?);
        }
        row.set(&Transform2d::from_xy(
            f32::from(index % 80),
            f32::from(index / 80),
        ))?;
        row.set(&SpriteSource {
            layer: i32::from(index % 20),
            ..Default::default()
        })?;
        if index <= TILE_ROWS {
            let cells: Vec<_> = (0..TILES_PER_ROW)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    let cell = (seed >> 3) as u16;
                    cell_checksum += u64::from(cell);
                    cell
                })
                .collect();
            row.set(&TileCells {
                cells: cells.into(),
            })?;
        }
        source.entities.push(row);
    }

    let schemas = ComponentSchemaCatalog::from_components([TileCells::type_schema()?])?;
    let output = cook_flattened_scene(&source, &schemas)?;
    let mut registry = CookedRegistry::new();
    registry.register_builtins()?.register::<TileCells>()?;

    let mut samples = Vec::with_capacity(SAMPLES);
    for index in 0..=SAMPLES {
        let mut world = World::new();
        let start = Instant::now();
        let scene = CookedScene::parse(black_box(&output.bytes))?;
        let entities = scene.instantiate(&mut world, &registry)?;
        let elapsed = start.elapsed();
        assert_eq!(entities.len(), usize::from(ENTITY_COUNT));
        let mut cells = world.query::<&TileCells>();
        let checksum = cells
            .iter(&world)
            .flat_map(|row| row.cells.0.iter().copied())
            .map(u64::from)
            .sum::<u64>();
        assert_eq!(black_box(checksum), cell_checksum);
        if index > 0 {
            samples.push(elapsed);
        }
    }
    samples.sort_unstable();
    println!(
        "NSCN parse + instantiate | {ENTITY_COUNT} entities | {} packed u16 cells | {} bytes | {SAMPLES} samples | p50 {:.3} ms | p95 {:.3} ms",
        usize::from(TILE_ROWS) * TILES_PER_ROW,
        output.bytes.len(),
        samples[SAMPLES / 2].as_secs_f64() * 1_000.0,
        samples[(SAMPLES - 1) * 95 / 100].as_secs_f64() * 1_000.0,
    );
    Ok(())
}
