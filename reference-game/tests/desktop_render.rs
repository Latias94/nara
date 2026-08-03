#![cfg(feature = "desktop")]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

#[path = "support/child_process.rs"]
mod child_process;

use std::{process::Command, sync::Arc, time::Duration};

use child_process::{ChildOutputLimits, run_child_with_timeout};
use nara::{
    advanced_prelude::{StartupSceneSource, materialize_startup_scene},
    app::{
        RuntimeAdmissionReservation, RuntimeClosePolicy, RuntimeInstance, RuntimeObligationLedger,
    },
    core::{ByteLimit, Color},
    ecs::Entity,
    hierarchy::Parent,
    input::{ButtonDriverInput, KeyCode, apply_keyboard_driver_input},
    material::SamplerDescriptor,
    prelude::{FixedTime, Sprite, Vec2, World},
    project_host::ProjectContentLoader,
    scene::SceneEntitySource,
    sprite_render::{ExtractedSprites, TextureUvRect},
    transform::{GlobalTransform2d, Transform2d},
    ui_render::UiBatches,
};
use nara_reference_game::{
    EnemyRole, PlayerRole, ReferenceHudProjection, WaveOutcome, WaveSnapshot, Weapon,
};
use project_content_fixture::{desktop_candidate_plan_and_root, stop_runtime};

const DESKTOP_RENDER_PROBE_OUTPUT_LIMITS: ChildOutputLimits = ChildOutputLimits::new(1024, 4096);

#[test]
fn desktop_projection_emits_sprites_clipped_hud_and_distinct_terminal_geometry() {
    let completed = render_terminal(WaveOutcome::Completed);
    let defeated = render_terminal(WaveOutcome::Defeated);

    assert_eq!(completed.hud.health_current, 20);
    assert_eq!(completed.hud.health_maximum, 20);
    assert_eq!(completed.hud.health_width, 240.0);
    assert_eq!(completed.hud.progress_width, 240.0);
    assert!(completed.extracted_sprites >= 1);
    assert!(completed.clipped_batches >= 1);
    assert!(completed.terminal_half_width > defeated.terminal_half_width);
    assert_eq!(completed.arena_floor_sprites, 15 * 8);
    assert!(defeated.player_and_enemy_share_atlas);
    assert!(defeated.player_and_enemy_regions_are_distinct);
    assert!(defeated.atlas_sprites_use_nearest_sampling);

    assert_eq!(defeated.hud.health_current, 0);
    assert_eq!(defeated.hud.health_width, 0.0);
    assert_eq!(defeated.hud.outcome, WaveOutcome::Defeated);
    assert!(defeated.terminal_half_height > completed.terminal_half_height);
}

#[test]
fn desktop_first_frame_projects_exact_atlas_regions_before_a_fixed_tick() {
    let mut runtime = render_runtime();

    runtime.drive(Duration::ZERO).unwrap();

    let world = runtime.world();
    assert_eq!(world.resource::<FixedTime>().tick(), 0);
    let extracted = world.resource::<ExtractedSprites>();
    assert_eq!(
        extracted
            .as_slice()
            .iter()
            .filter(|sprite| sprite.layer == -20)
            .count(),
        15 * 8,
    );
    let player_entity = world
        .iter_entities()
        .find(|entity| entity.contains::<PlayerRole>())
        .expect("the desktop fixture must retain one player")
        .id();
    let enemy_entity = world
        .iter_entities()
        .find(|entity| entity.contains::<EnemyRole>())
        .expect("the desktop fixture must retain at least one enemy")
        .id();
    let player = extracted
        .as_slice()
        .iter()
        .find(|sprite| sprite.entity == player_entity)
        .expect("the first frame must extract the player");
    let enemy = extracted
        .as_slice()
        .iter()
        .find(|sprite| sprite.entity == enemy_entity)
        .expect("the first frame must extract an enemy");
    let player_position = world
        .get::<Transform2d>(player_entity)
        .expect("the player must retain its authored local transform")
        .translation;
    let enemy_position = world
        .get::<Transform2d>(enemy_entity)
        .expect("the enemy must retain its authored local transform")
        .translation;
    let enemy_parent = world
        .get::<Parent>(enemy_entity)
        .expect("the expanded prefab enemy should retain its runtime anchor")
        .parent();
    let atlas_tile_size = Vec2::new(1.0 / 12.0, 1.0 / 11.0);

    assert_eq!(
        world
            .get::<GlobalTransform2d>(player_entity)
            .expect("startup must complete the player transform")
            .translation(),
        player_position,
    );
    assert_eq!(
        world
            .get::<GlobalTransform2d>(enemy_entity)
            .expect("startup must complete the expanded enemy transform")
            .translation(),
        enemy_position,
    );
    assert_eq!(
        world.get::<Transform2d>(enemy_parent),
        Some(&Transform2d::IDENTITY),
        "the prefab anchor must explicitly participate in the continuous transform chain",
    );
    assert_eq!(player.world_center, player_position);
    assert_eq!(enemy.world_center, enemy_position);
    assert_eq!(
        player.texture_region,
        TextureUvRect::new(Vec2::new(0.0, 8.0 / 11.0), atlas_tile_size),
    );
    assert_eq!(
        enemy.texture_region,
        TextureUvRect::new(Vec2::new(1.0 / 6.0, 9.0 / 11.0), atlas_tile_size),
    );
    assert_eq!(player.material.image, enemy.material.image);
    assert!(player.material.image.is_some());
    assert_eq!(player.material.sampler, SamplerDescriptor::NEAREST_CLAMP);
    assert_eq!(enemy.material.sampler, SamplerDescriptor::NEAREST_CLAMP);

    stop_runtime(runtime);
}

#[test]
fn desktop_projection_preserves_authored_actor_sprite_fields() {
    let player_size = Vec2::new(2.4, 1.7);
    let enemy_size = Vec2::new(1.9, 2.2);
    let player_tint = Color::rgba(0.12, 0.34, 0.56, 0.78);
    let enemy_tint = Color::rgba(0.81, 0.63, 0.27, 0.91);
    let mut runtime = render_runtime_with_admission_edit(move |world| {
        let player = scene_entity(world, "player");
        let enemy = scene_entity(world, "enemy-anchor/enemy");
        let mut player_sprite = world
            .get_mut::<Sprite>(player)
            .expect("the player should retain its authored Sprite");
        player_sprite.texture_region = None;
        player_sprite.size = player_size;
        player_sprite.material.tint = player_tint;
        player_sprite.layer = 7;
        player_sprite.sort_key = 71;

        let mut enemy_sprite = world
            .get_mut::<Sprite>(enemy)
            .expect("the enemy should retain its authored Sprite");
        enemy_sprite.texture_region = None;
        enemy_sprite.size = enemy_size;
        enemy_sprite.material.tint = enemy_tint;
        enemy_sprite.layer = 8;
        enemy_sprite.sort_key = 82;
    });
    let timestep = runtime.world().resource::<FixedTime>().timestep();
    runtime.drive(timestep).unwrap();

    let player = scene_entity(runtime.world(), "player");
    let enemy = scene_entity(runtime.world(), "enemy-anchor/enemy");

    let player_sprite = runtime
        .world()
        .get::<Sprite>(player)
        .expect("desktop projection must retain the player Sprite");
    assert_eq!(player_sprite.texture_region, None);
    assert_eq!(player_sprite.size, player_size);
    assert_eq!(player_sprite.material.tint, player_tint);
    assert_eq!(player_sprite.layer, 7);
    assert_eq!(player_sprite.sort_key, 71);

    let enemy_sprite = runtime
        .world()
        .get::<Sprite>(enemy)
        .expect("desktop projection must retain the enemy Sprite");
    assert_eq!(enemy_sprite.texture_region, None);
    assert_eq!(enemy_sprite.size, enemy_size);
    assert_eq!(enemy_sprite.material.tint, enemy_tint);
    assert_eq!(enemy_sprite.layer, 8);
    assert_eq!(enemy_sprite.sort_key, 82);

    stop_runtime(runtime);
}

#[test]
fn parented_enemy_gameplay_and_snapshot_use_completed_world_space() {
    let mut runtime = render_runtime_with_admission_edit(|world| {
        let anchor = scene_entity(world, "enemy-anchor");
        world
            .get_mut::<Transform2d>(anchor)
            .expect("the enemy anchor should participate in the transform chain")
            .translation = Vec2::new(-10.0, 0.0);
    });
    let enemy = scene_entity(runtime.world(), "enemy-anchor/enemy");
    let enemy_local_before = runtime
        .world()
        .get::<Transform2d>(enemy)
        .expect("the enemy should retain its authored local transform")
        .translation;

    let world_before = runtime
        .world()
        .get::<GlobalTransform2d>(enemy)
        .expect("startup should complete the edited anchor transform")
        .translation();
    assert_vec2_close(world_before, enemy_local_before + Vec2::new(-10.0, 0.0));
    assert!(world_before.x < 0.0);

    let timestep = runtime.world().resource::<FixedTime>().timestep();
    runtime.drive(timestep).unwrap();

    let enemy_local_after = runtime
        .world()
        .get::<Transform2d>(enemy)
        .expect("the enemy should retain a local transform after pursuit")
        .translation;
    let world_after = runtime
        .world()
        .get::<GlobalTransform2d>(enemy)
        .expect("the fixed tick should publish the pursued enemy global")
        .translation();
    assert_vec2_close(enemy_local_after, enemy_local_before + Vec2::new(0.5, 0.0));
    assert_vec2_close(world_after, world_before + Vec2::new(0.5, 0.0));
    let snapshot_enemy = runtime
        .world()
        .resource::<WaveSnapshot>()
        .enemies
        .iter()
        .find(|enemy| enemy.id == "enemy-anchor/enemy")
        .expect("the current receipt enemy should be present in the snapshot");
    assert_vec2_close(snapshot_enemy.position, world_after);

    stop_runtime(runtime);
}

#[test]
fn desktop_weapon_keeps_its_local_offset_and_follows_the_player_in_same_tick_extraction() {
    let mut runtime = render_runtime();
    runtime.drive(Duration::ZERO).unwrap();

    let (
        player_entity,
        weapon_entity,
        player_before,
        weapon_local_before,
        weapon_global_before,
        weapon_extracted_before,
    ) = {
        let world = runtime.world();
        let player_entity = scene_entity(world, "player");
        let weapon_entity = scene_entity(world, "player-weapon");
        assert!(world.get::<PlayerRole>(player_entity).is_some());
        assert!(world.get::<Weapon>(weapon_entity).is_some());
        assert_eq!(
            world
                .get::<Parent>(weapon_entity)
                .expect("the authored weapon must retain its runtime parent")
                .parent(),
            player_entity,
        );
        let player_before = world
            .get::<Transform2d>(player_entity)
            .expect("the player must retain its authored local transform")
            .translation;
        let weapon_local_before = *world
            .get::<Transform2d>(weapon_entity)
            .expect("the weapon must retain its authored local transform");
        assert_eq!(
            weapon_local_before,
            Transform2d::from_translation(Vec2::new(1.2, 0.0)),
        );
        let weapon_global_before = world
            .get::<GlobalTransform2d>(weapon_entity)
            .expect("Startup must project the weapon hierarchy")
            .translation();
        let weapon_extracted_before = assert_extracted_sprite_matches_global(world, weapon_entity);
        (
            player_entity,
            weapon_entity,
            player_before,
            weapon_local_before,
            weapon_global_before,
            weapon_extracted_before,
        )
    };

    runtime
        .with_driver_scope(|scope| {
            apply_keyboard_driver_input(scope, ButtonDriverInput::Press(KeyCode::Character('a')))
        })
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let timestep = runtime.world().resource::<FixedTime>().timestep();
    runtime.drive(timestep).unwrap();

    let world = runtime.world();
    assert_eq!(world.resource::<FixedTime>().tick(), 1);
    let player_after = world
        .get::<Transform2d>(player_entity)
        .expect("the player must retain its authoritative transform")
        .translation;
    let weapon_local_after = *world
        .get::<Transform2d>(weapon_entity)
        .expect("the weapon must retain its authored local transform");
    assert_eq!(weapon_local_after, weapon_local_before);
    let player_delta = player_after - player_before;
    assert!(player_delta.x < 0.0 && player_delta.y == 0.0);
    let weapon_global_after = world
        .get::<GlobalTransform2d>(weapon_entity)
        .expect("the fixed-step completion must refresh the weapon global")
        .translation();
    assert_vec2_close(weapon_global_after - weapon_global_before, player_delta);
    assert_vec2_close(
        weapon_global_after,
        player_after + weapon_local_after.translation,
    );
    let extracted_after = assert_extracted_sprite_matches_global(world, weapon_entity);
    assert_vec2_close(extracted_after - weapon_extracted_before, player_delta);

    stop_runtime(runtime);
}

#[test]
fn desktop_product_host_prepares_and_submits_the_committed_texture() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_desktop_render_probe"));
    command.current_dir(std::env::temp_dir());
    let output = run_child_with_timeout(
        command,
        Duration::from_secs(45),
        DESKTOP_RENDER_PROBE_OUTPUT_LIMITS,
        "desktop render probe",
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "desktop_render_probe: ok\n"
    );
    assert!(output.stderr.is_empty());
}

struct RenderObservation {
    hud: ReferenceHudProjection,
    extracted_sprites: usize,
    arena_floor_sprites: usize,
    player_and_enemy_share_atlas: bool,
    player_and_enemy_regions_are_distinct: bool,
    atlas_sprites_use_nearest_sampling: bool,
    clipped_batches: usize,
    terminal_half_width: f32,
    terminal_half_height: f32,
}

fn render_terminal(outcome: WaveOutcome) -> RenderObservation {
    let mut runtime = render_runtime();
    let movement_key = match outcome {
        WaveOutcome::Completed => 'a',
        WaveOutcome::Defeated => 'd',
        WaveOutcome::Running => panic!("render_terminal requires a terminal outcome"),
    };
    runtime
        .with_driver_scope(|scope| {
            apply_keyboard_driver_input(
                scope,
                ButtonDriverInput::Press(KeyCode::Character(movement_key)),
            )
        })
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let timestep = runtime.world().resource::<FixedTime>().timestep();
    for _ in 0..96 {
        runtime.drive(timestep).unwrap();
        if runtime.world().resource::<ReferenceHudProjection>().outcome != WaveOutcome::Running {
            break;
        }
    }

    let world = runtime.world();
    let hud = *world.resource::<ReferenceHudProjection>();
    assert_eq!(hud.outcome, outcome);
    let extracted = world.resource::<ExtractedSprites>();
    let extracted_sprites = extracted.len();
    let arena_floor_sprites = extracted
        .as_slice()
        .iter()
        .filter(|sprite| sprite.layer == -20)
        .count();
    let player_entity = world
        .iter_entities()
        .find(|entity| entity.contains::<PlayerRole>())
        .expect("the desktop fixture must retain one player")
        .id();
    let enemy_entity = world
        .iter_entities()
        .find(|entity| entity.contains::<EnemyRole>())
        .map(|entity| entity.id());
    let player_sprite = extracted
        .as_slice()
        .iter()
        .find(|sprite| sprite.entity == player_entity)
        .expect("the player must be extracted as a sprite");
    assert_eq!(
        player_sprite.world_center,
        world
            .get::<GlobalTransform2d>(player_entity)
            .expect("same-frame extraction requires the completed player global")
            .translation(),
        "same-frame extraction must consume the transform projected after gameplay mutation",
    );
    let enemy_sprite = enemy_entity.and_then(|enemy_entity| {
        extracted
            .as_slice()
            .iter()
            .find(|sprite| sprite.entity == enemy_entity)
    });
    let player_and_enemy_share_atlas = enemy_sprite.is_some_and(|enemy_sprite| {
        player_sprite.material.image.is_some()
            && player_sprite.material.image == enemy_sprite.material.image
    });
    let player_and_enemy_regions_are_distinct = enemy_sprite
        .is_some_and(|enemy_sprite| player_sprite.texture_region != enemy_sprite.texture_region);
    let atlas_sprites_use_nearest_sampling = extracted
        .as_slice()
        .iter()
        .filter(|sprite| sprite.material.image == player_sprite.material.image)
        .all(|sprite| sprite.material.sampler == SamplerDescriptor::NEAREST_CLAMP);
    let batches = world.resource::<UiBatches>();
    let clipped_batches = batches
        .as_slice()
        .iter()
        .filter(|batch| batch.clip_rect.is_some())
        .count();
    let terminal = batches
        .as_slice()
        .iter()
        .find(|batch| batch.order == 10)
        .expect("terminal geometry should be queued");
    let terminal_instance = terminal.instances.first().unwrap();
    let observation = RenderObservation {
        hud,
        extracted_sprites,
        arena_floor_sprites,
        player_and_enemy_share_atlas,
        player_and_enemy_regions_are_distinct,
        atlas_sprites_use_nearest_sampling,
        clipped_batches,
        terminal_half_width: terminal_instance.x_axis.x.abs(),
        terminal_half_height: terminal_instance.y_axis.y.abs(),
    };

    stop_runtime(runtime);
    observation
}

fn render_runtime() -> RuntimeInstance {
    render_runtime_with_admission_edit(|_| {})
}

fn render_runtime_with_admission_edit(
    edit: impl FnOnce(&mut World) + Send + Sync + 'static,
) -> RuntimeInstance {
    let (project, plan, root) = desktop_candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let content = loader.load(&project, &plan).unwrap();
    let scene = content.expanded_startup_scene().clone();
    let runtime_time = plan.settings().runtime.runtime_time_settings();
    let fixed_time = plan.settings().runtime.fixed_time();
    let source = StartupSceneSource::direct(
        Arc::new(scene),
        ByteLimit::new(16 * 1024 * 1024).expect("the direct retained-scene limit is non-zero"),
    )
    .expect("the render scene should fit the bounded direct retention limit");
    let sealed = plan.plugin_plan().instantiate().unwrap();
    let mut candidate = RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            sealed,
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap();
    candidate
        .with_admission_scope(move |scope| {
            scope.apply_command(move |world: &mut nara::prelude::World| {
                world.insert_resource(runtime_time);
                world.insert_resource(fixed_time);
                let report = materialize_startup_scene(world, source)
                    .expect("the retained render scene should materialize");
                assert!(!report.has_errors(), "{report:#?}");
                edit(world);
            });
        })
        .unwrap();
    candidate.complete_startup().unwrap().promote()
}

fn scene_entity(world: &World, scene_id: &str) -> Entity {
    world
        .iter_entities()
        .find_map(|entity| {
            entity
                .get::<SceneEntitySource>()
                .is_some_and(|source| source.entity_id.as_str() == scene_id)
                .then_some(entity.id())
        })
        .unwrap_or_else(|| panic!("the render fixture must contain scene entity {scene_id}"))
}

fn assert_extracted_sprite_matches_global(world: &World, entity: Entity) -> Vec2 {
    let sprite = world
        .get::<Sprite>(entity)
        .expect("the observed scene entity must be a sprite");
    let global = world
        .get::<GlobalTransform2d>(entity)
        .expect("sprite extraction requires a completed global transform");
    let extracted = world
        .resource::<ExtractedSprites>()
        .as_slice()
        .iter()
        .find(|sprite| sprite.entity == entity)
        .expect("the observed sprite must be extracted in the current frame");
    let matrix = global.matrix();
    assert_vec2_close(
        extracted.world_center,
        matrix.transform_point2(-sprite.anchor.normalized * sprite.size),
    );
    assert_vec2_close(
        extracted.world_x_axis,
        matrix.transform_vector2(Vec2::new(sprite.size.x * 0.5, 0.0)),
    );
    assert_vec2_close(
        extracted.world_y_axis,
        matrix.transform_vector2(Vec2::new(0.0, sprite.size.y * 0.5)),
    );
    extracted.world_center
}

fn assert_vec2_close(actual: Vec2, expected: Vec2) {
    assert!(
        (actual - expected).length() <= 1.0e-6,
        "expected {expected:?}, observed {actual:?}",
    );
}
