#![cfg(feature = "sprites")]

use std::{f32::consts::FRAC_PI_2, time::Duration};

use nara_app::{App, CoreStage, FixedTime, FixedUpdateSet};
use nara_asset::allocate_anonymous_handle;
use nara_core::{Color, Vec2};
use nara_ecs::schedule::IntoScheduleConfigs;
use nara_ecs::{Entity, Query, Res};
use nara_render::material::AlphaMode2d;
use nara_render::particles::{
    ParticleEmitter2d, ParticleEmitterConfig, ParticleEmitterError, ParticlePlugin,
    ParticleRenderInstance,
};
use nara_scene::spatial::{ChildOf, GlobalTransform2d, SpatialPlugin, Transform2d};

fn particle_app() -> App {
    let mut app = App::new();
    app.add_plugins((SpatialPlugin, ParticlePlugin)).unwrap();
    app
}

fn spawn_emitter(app: &mut App, config: ParticleEmitterConfig) -> Entity {
    app.world_mut()
        .unwrap()
        .spawn(ParticleEmitter2d::new(config).unwrap())
        .id()
}

fn instances(app: &App, entity: Entity) -> impl ExactSizeIterator<Item = ParticleRenderInstance> {
    let world = app.world();
    let emitter = world.get::<ParticleEmitter2d>(entity).unwrap();
    let global = world.get::<GlobalTransform2d>(entity).unwrap();
    emitter.render_instances(global)
}

#[test]
fn equal_seeds_replay_identically_across_expiry_and_pool_reuse() {
    let mut app = particle_app();
    let config = ParticleEmitterConfig {
        max_particles: 8,
        rate: 14.0,
        burst: 3,
        lifetime_range: 0.2..=0.8,
        speed_range: 5.0..=20.0,
        angle_range: -1.3..=2.0,
        gravity: Vec2::new(0.0, -3.0),
        size_over_life: [4.0, 1.0],
        seed: 0,
        ..Default::default()
    };
    let a = spawn_emitter(&mut app, config.clone());
    let b = spawn_emitter(&mut app, config.clone());
    let different = spawn_emitter(&mut app, ParticleEmitterConfig { seed: 1, ..config });

    for tick in 0..24 {
        app.step_fixed(Duration::from_millis(50)).unwrap();
        let a = instances(&app, a).collect::<Vec<_>>();
        assert_eq!(a, instances(&app, b).collect::<Vec<_>>());
        if tick == 1 {
            assert_ne!(a, instances(&app, different).collect::<Vec<_>>());
        }
    }
}

#[test]
fn full_pool_drops_excess_emissions_and_reuses_expired_slots() {
    let mut app = particle_app();
    let entity = spawn_emitter(
        &mut app,
        ParticleEmitterConfig {
            max_particles: 3,
            rate: f32::MAX,
            burst: usize::MAX,
            lifetime_range: 0.5..=0.5,
            ..Default::default()
        },
    );
    let tick = Duration::from_millis(250);
    assert_eq!(instances(&app, entity).len(), 0);

    app.step_fixed(tick).unwrap();
    assert_eq!(instances(&app, entity).len(), 3);
    {
        let world = app.world_mut().unwrap();
        let mut emitter = world.get_mut::<ParticleEmitter2d>(entity).unwrap();
        emitter.set_rate(0.0).unwrap();
        emitter.emit(usize::MAX);
        emitter.emit(usize::MAX);
    }

    app.step_fixed(tick).unwrap();
    assert_eq!(instances(&app, entity).len(), 3);
    assert!(instances(&app, entity).all(|particle| particle.color.a == 0.5));
    app.step_fixed(tick).unwrap();
    assert_eq!(instances(&app, entity).len(), 0);

    {
        let world = app.world_mut().unwrap();
        let mut emitter = world.get_mut::<ParticleEmitter2d>(entity).unwrap();
        emitter.emit(1);
        emitter.emit(2);
    }
    app.step_fixed(tick).unwrap();
    assert_eq!(instances(&app, entity).len(), 3);
    assert!(instances(&app, entity).all(|particle| particle.color.a == 1.0));
}

#[test]
fn rate_changes_preserve_fractional_fixed_step_credit() {
    let mut app = particle_app();
    let entity = spawn_emitter(
        &mut app,
        ParticleEmitterConfig {
            rate: 1.0,
            ..Default::default()
        },
    );
    let tick = Duration::from_millis(100);
    for _ in 0..4 {
        app.step_fixed(tick).unwrap();
    }
    assert_eq!(instances(&app, entity).len(), 0);

    app.world_mut()
        .unwrap()
        .get_mut::<ParticleEmitter2d>(entity)
        .unwrap()
        .set_rate(0.0)
        .unwrap();
    for _ in 0..8 {
        app.step_fixed(tick).unwrap();
    }
    assert_eq!(instances(&app, entity).len(), 0);

    {
        let world = app.world_mut().unwrap();
        let mut emitter = world.get_mut::<ParticleEmitter2d>(entity).unwrap();
        emitter.set_rate(2.0).unwrap();
        assert!(matches!(
            emitter.set_rate(f32::NAN),
            Err(ParticleEmitterError::InvalidRate)
        ));
        assert_eq!(emitter.rate(), 2.0);
    }
    for _ in 0..2 {
        app.step_fixed(tick).unwrap();
        assert_eq!(instances(&app, entity).len(), 0);
    }
    app.step_fixed(tick).unwrap();
    assert_eq!(instances(&app, entity).len(), 1);
}

#[test]
fn local_quads_follow_current_hierarchy_and_world_quads_keep_their_birth_transform() {
    let mut app = particle_app();
    let texture = allocate_anonymous_handle().unwrap();
    let config = ParticleEmitterConfig {
        max_particles: 2,
        burst: 1,
        lifetime_range: 1.0..=1.0,
        speed_range: 2.0..=2.0,
        angle_range: 0.0..=0.0,
        gravity: Vec2::new(0.0, -4.0),
        size_over_life: [4.0, 2.0],
        color_over_life: [
            Color::rgba(0.25, 0.5, 1.0, 1.0),
            Color::rgba(0.75, 1.5, 0.5, 0.0),
        ],
        texture: Some(texture),
        alpha_mode: AlphaMode2d::Additive,
        layer: -2,
        sort_key: 6,
        ..Default::default()
    };
    let world = app.world_mut().unwrap();
    let parent = world
        .spawn(Transform2d {
            translation: Vec2::new(10.0, 20.0),
            scale: Vec2::new(2.0, 3.0),
            z: 4.0,
            ..Default::default()
        })
        .id();
    let child_transform = Transform2d {
        translation: Vec2::new(1.0, 2.0),
        z: 3.0,
        ..Default::default()
    };
    let detached_space = world
        .spawn((
            ChildOf(parent),
            child_transform,
            ParticleEmitter2d::new(config.clone()).unwrap(),
        ))
        .id();
    let local_space = world
        .spawn((
            ChildOf(parent),
            child_transform,
            ParticleEmitter2d::new(ParticleEmitterConfig {
                local_space: true,
                ..config
            })
            .unwrap(),
        ))
        .id();
    app.add_systems(
        CoreStage::FixedUpdate,
        (move |time: Res<FixedTime>, mut transforms: Query<&mut Transform2d>| {
            if time.tick() == 2 {
                *transforms.get_mut(parent).unwrap() = Transform2d {
                    translation: Vec2::new(80.0, 40.0),
                    rotation: FRAC_PI_2,
                    scale: Vec2::new(4.0, 2.0),
                    z: -3.0,
                };
            }
        })
        .in_set(FixedUpdateSet::Simulate),
    )
    .unwrap();

    let tick = Duration::from_millis(500);
    app.step_fixed(tick).unwrap();
    for entity in [detached_space, local_space] {
        let born = instances(&app, entity).next().unwrap();
        assert_eq!(born.world_center, Vec2::new(12.0, 26.0));
        assert_eq!(born.world_z, 7.0);
        app.world_mut()
            .unwrap()
            .get_mut::<ParticleEmitter2d>(entity)
            .unwrap()
            .emit(1);
    }

    app.step_fixed(tick).unwrap();
    let world = instances(&app, detached_space).collect::<Vec<_>>();
    let local = instances(&app, local_space).collect::<Vec<_>>();
    assert_eq!(world.len(), 2);
    assert_eq!(local.len(), 2);
    assert_eq!(world[0].world_center, Vec2::new(14.0, 25.5));
    assert_eq!(world[0].world_x_axis, Vec2::new(3.0, 0.0));
    assert_eq!(world[0].world_y_axis, Vec2::new(0.0, 4.5));
    assert_eq!(world[0].world_z, 7.0);
    assert!(
        local[0]
            .world_center
            .abs_diff_eq(Vec2::new(77.0, 48.0), 1e-5)
    );
    assert!(local[0].world_x_axis.abs_diff_eq(Vec2::new(0.0, 6.0), 1e-5));
    assert!(
        local[0]
            .world_y_axis
            .abs_diff_eq(Vec2::new(-3.0, 0.0), 1e-5)
    );
    assert_eq!(local[0].world_z, 0.0);

    for particle in [world[0], local[0]] {
        assert_eq!(particle.color, Color::rgba(0.5, 1.0, 0.75, 0.5));
        assert_eq!(particle.texture, Some(texture));
        assert_eq!(particle.alpha_mode, AlphaMode2d::Additive);
        assert_eq!((particle.layer, particle.sort_key), (-2, 6));
    }
    for particle in [world[1], local[1]] {
        assert!(
            particle
                .world_center
                .abs_diff_eq(Vec2::new(76.0, 44.0), 1e-5)
        );
        assert_eq!(particle.world_z, 0.0);
    }
}

#[test]
fn despawning_the_hierarchy_removes_the_live_pool() {
    let mut app = particle_app();
    let world = app.world_mut().unwrap();
    let parent = world.spawn(Transform2d::IDENTITY).id();
    let child = world
        .spawn((
            ChildOf(parent),
            ParticleEmitter2d::new(ParticleEmitterConfig {
                burst: 1,
                ..Default::default()
            })
            .unwrap(),
        ))
        .id();
    let tick = Duration::from_millis(16);
    app.step_fixed(tick).unwrap();
    assert_eq!(instances(&app, child).len(), 1);

    assert!(app.world_mut().unwrap().despawn(parent));
    app.step_fixed(tick).unwrap();
    let world = app.world_mut().unwrap();
    assert!(world.get_entity(child).is_err());
    assert_eq!(world.query::<&ParticleEmitter2d>().iter(world).count(), 0);
}

#[test]
fn construction_rejects_invalid_settings_and_impossible_capacity() {
    let config = ParticleEmitterConfig::default();
    assert!(matches!(
        ParticleEmitter2d::new(ParticleEmitterConfig {
            max_particles: 0,
            ..config.clone()
        }),
        Err(ParticleEmitterError::ZeroCapacity)
    ));
    assert!(matches!(
        ParticleEmitter2d::new(ParticleEmitterConfig {
            lifetime_range: 0.0..=1.0,
            ..config.clone()
        }),
        Err(ParticleEmitterError::InvalidLifetimeRange)
    ));
    assert!(matches!(
        ParticleEmitter2d::new(ParticleEmitterConfig {
            max_particles: usize::MAX,
            ..config
        }),
        Err(ParticleEmitterError::PoolAllocation { .. })
    ));
}
