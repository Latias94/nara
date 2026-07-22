#![cfg(feature = "desktop")]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{
    fs::{File, remove_file},
    io::Read,
    path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara::{
    app::{
        RuntimeAdmissionReservation, RuntimeClosePolicy, RuntimeInstance, RuntimeObligationLedger,
    },
    input::{ButtonDriverInput, KeyCode, apply_keyboard_driver_input},
    material::SamplerDescriptor,
    prelude::{FixedTime, Vec2},
    project_host::ProjectContentLoader,
    scene::spawn_scene,
    sprite_render::{ExtractedSprites, TextureUvRect},
    ui_render::UiBatches,
};
use nara_reference_game::{Enemy, Player, ReferenceHudProjection, WaveOutcome};
use project_content_fixture::{desktop_candidate_plan_and_root, stop_runtime};

#[test]
fn desktop_projection_emits_sprites_clipped_hud_and_distinct_terminal_geometry() {
    let completed = render_terminal(WaveOutcome::Completed, 20, 3);
    let defeated = render_terminal(WaveOutcome::Defeated, 0, 1);

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
    let mut runtime = render_runtime(None, 0);

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
        .find(|entity| entity.contains::<Player>())
        .expect("the desktop fixture must retain one player")
        .id();
    let enemy_entity = world
        .iter_entities()
        .find(|entity| entity.contains::<Enemy>())
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
    let atlas_tile_size = Vec2::new(1.0 / 12.0, 1.0 / 11.0);

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
fn desktop_product_host_prepares_and_submits_the_committed_texture() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_desktop_render_probe"));
    command.current_dir(std::env::temp_dir());
    let output = run_child_with_timeout(command, Duration::from_secs(45));

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

struct ChildOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn run_child_with_timeout(mut command: Command, timeout: Duration) -> ChildOutput {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let prefix = format!("nara-desktop-render-{}-{nonce}", std::process::id());
    let stdout_path = temporary_output_path(&prefix, "stdout");
    let stderr_path = temporary_output_path(&prefix, "stderr");
    let stdout_file = File::create(&stdout_path).expect("probe stdout capture should open");
    let stderr_file = File::create(&stderr_path).expect("probe stderr capture should open");
    command.stdout(Stdio::from(stdout_file));
    command.stderr(Stdio::from(stderr_file));
    let mut child = command
        .spawn()
        .expect("the desktop render probe process should start");
    let deadline = Instant::now()
        .checked_add(timeout)
        .expect("the desktop render probe timeout is bounded");
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(poll_error) => {
                let reap = terminate_and_reap(&mut child);
                let stdout = read_and_remove(&stdout_path);
                let stderr = read_and_remove(&stderr_path);
                panic!(
                    "desktop render probe polling failed: {poll_error}; reap={reap:?}; stdout={}; stderr={}",
                    String::from_utf8_lossy(&stdout),
                    String::from_utf8_lossy(&stderr)
                );
            }
        }
        if Instant::now() >= deadline {
            let status = terminate_and_reap(&mut child)
                .expect("the timed-out desktop render probe should be reaped");
            let stdout = read_and_remove(&stdout_path);
            let stderr = read_and_remove(&stderr_path);
            panic!(
                "desktop render probe exceeded {timeout:?} ({status}); stdout={}; stderr={}",
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr)
            );
        }
        std::thread::park_timeout(Duration::from_millis(10));
    };
    ChildOutput {
        status,
        stdout: read_and_remove(&stdout_path),
        stderr: read_and_remove(&stderr_path),
    }
}

fn terminate_and_reap(child: &mut Child) -> Result<ExitStatus, String> {
    let kill_error = child.kill().err();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() < deadline => {
                std::thread::park_timeout(Duration::from_millis(10));
            }
            Ok(None) => {
                return Err(match kill_error {
                    Some(kill_error) => {
                        format!("kill={kill_error}; child remained live past the reap deadline")
                    }
                    None => "child remained live past the reap deadline".to_owned(),
                });
            }
            Err(poll_error) => {
                return Err(match kill_error {
                    Some(kill_error) => format!("kill={kill_error}; poll={poll_error}"),
                    None => format!("poll={poll_error}"),
                });
            }
        }
    }
}

fn temporary_output_path(prefix: &str, stream: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{prefix}.{stream}"))
}

fn read_and_remove(path: &PathBuf) -> Vec<u8> {
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .expect("probe output capture should remain readable");
    remove_file(path).expect("probe output capture should be removable");
    bytes
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

fn render_terminal(
    outcome: WaveOutcome,
    hit_points: i64,
    defeated_enemies: u64,
) -> RenderObservation {
    let mut runtime = render_runtime(Some(hit_points), defeated_enemies);
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
        .find(|entity| entity.contains::<Player>())
        .expect("the desktop fixture must retain one player")
        .id();
    let enemy_entity = world
        .iter_entities()
        .find(|entity| entity.contains::<Enemy>())
        .map(|entity| entity.id());
    let player_sprite = extracted
        .as_slice()
        .iter()
        .find(|sprite| sprite.entity == player_entity)
        .expect("the player must be extracted as a sprite");
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

fn render_runtime(hit_points: Option<i64>, defeated_enemies: u64) -> RuntimeInstance {
    let (project, plan, root) = desktop_candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let content = loader.load(&project, &plan).unwrap();
    let scene = content.expanded_startup_scene().clone();
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
                let report = spawn_scene(world, plan.schema_validation().registry(), &scene);
                assert!(
                    !report.diagnostics.has_errors(),
                    "{:#?}",
                    report.diagnostics
                );
                assert!(report.instance.is_some());

                let mut players = world.query::<&mut Player>();
                let mut player_count = 0;
                for mut player in players.iter_mut(world) {
                    if let Some(hit_points) = hit_points {
                        player.hit_points = hit_points;
                    }
                    player_count += 1;
                }
                assert_eq!(player_count, 1);

                let mut enemies = world.query::<&mut Enemy>();
                let mut enemy_count = 0_u64;
                for mut enemy in enemies.iter_mut(world) {
                    if enemy_count < defeated_enemies {
                        enemy.hit_points = 0;
                    }
                    enemy_count += 1;
                }
                assert_eq!(enemy_count, 3);
            });
        })
        .unwrap();
    candidate.complete_startup().unwrap().promote()
}
