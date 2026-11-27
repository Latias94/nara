//! Deterministic exercises driven through the same plugin and fixed schedule as desktop play.

use std::{error::Error, path::Path, time::Duration};

use nara::prelude::*;

use crate::{
    Failure, GamePlugin, PilotInput, Round, RoundStatus, RunRules,
    arena::{CELL_COUNT, source},
};

type GameResult<T> = Result<T, Box<dyn Error>>;
const STEP: Duration = FixedTime::DEFAULT_TIMESTEP;
pub const MAX_ROUNDS: u32 = 200;
const START: Vec2 = at(2, 2);

const fn at(column: usize, row: usize) -> Vec2 {
    Vec2::new((column as f32 - 10.0) * 32.0, (5.0 - row as f32) * 32.0)
}

#[derive(Debug, PartialEq, Eq)]
pub struct StressReport {
    pub rounds: u32,
    pub fixed_ticks: u64,
    pub wall_contacts_per_round: u32,
    pub escape_time: Duration,
}

pub fn run_stress(rounds: u32) -> GameResult<StressReport> {
    run_stress_in(&source::development_root()?, rounds)
}

fn run_stress_in(project_root: &Path, rounds: u32) -> GameResult<StressReport> {
    if !(1..=MAX_ROUNDS).contains(&rounds) {
        return Err(format!("choose 1–{MAX_ROUNDS} stress rounds").into());
    }
    let mut app = headless_app()?;
    let project = source::SourceProject::new(&mut app, project_root)?;
    let content = project.load(&project.assets()?, None)?;
    app.add_plugins(GamePlugin::from_scene(content.scene))?;
    let baseline = exercise_round(&mut app)?;
    for _ in 1..rounds {
        if exercise_round(&mut app)? != baseline {
            return Err("the fixed-step replay diverged".into());
        }
    }
    let report = StressReport {
        rounds,
        fixed_ticks: app.world().resource::<FixedTime>().tick(),
        wall_contacts_per_round: baseline.0,
        escape_time: baseline.1.elapsed,
    };
    app.finish()
        .map_err(|_| "could not retire the headless game")?;
    Ok(report)
}

fn headless_app() -> GameResult<App> {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins::headless())?;
    Ok(app)
}

fn exercise_round(app: &mut App) -> GameResult<(u32, Round)> {
    restart(app)?;
    for _ in 0..40 {
        step(app, Vec2::NEG_X)?;
    }
    let round = app.world().resource::<Round>();
    let contacts = round.wall_contacts();
    if contacts == 0 || round.position().x < at(1, 2).x - 6.1 {
        return Err("the west wall did not stop the pilot".into());
    }

    restart(app)?;
    follow(app, &[(2, 5), (8, 5), (8, 3), (11, 3)])?;
    for _ in 0..600 {
        if app.world().resource::<Round>().status() != RoundStatus::Running {
            break;
        }
        step(app, Vec2::ZERO)?;
    }
    if app.world().resource::<Round>().status() != RoundStatus::Failed(Failure::ShieldDepleted) {
        return Err("the patrol did not deplete the pilot's shields".into());
    }

    restart(app)?;
    follow(
        app,
        &[
            (2, 5),
            (8, 5),
            (8, 2),
            (9, 2),
            (8, 2),
            (8, 4),
            (17, 4),
            (17, 2),
            (17, 4),
            (19, 4),
            (19, 9),
            (3, 9),
            (3, 8),
            (3, 9),
            (17, 9),
            (17, 8),
        ],
    )?;
    let round = app.world().resource::<Round>();
    if round.status() != RoundStatus::Escaped || round.collected() != CELL_COUNT {
        return Err("the pilot could not power the gate and escape".into());
    }
    Ok((contacts, round.clone()))
}

fn restart(app: &mut App) -> GameResult<()> {
    *app.world_mut()?.resource_mut::<PilotInput>() = PilotInput {
        movement: Vec2::ZERO,
        restart: true,
    };
    app.step_fixed(STEP)?;
    let round = app.world().resource::<Round>();
    if round.status() != RoundStatus::Running
        || round.position() != START
        || round.collected() != 0
        || !round.elapsed.is_zero()
    {
        return Err("restart did not restore the starting round".into());
    }
    Ok(())
}

fn step(app: &mut App, movement: Vec2) -> GameResult<()> {
    app.world_mut()?.resource_mut::<PilotInput>().movement = movement;
    app.step_fixed(STEP)?;
    Ok(())
}

fn follow(app: &mut App, route: &[(usize, usize)]) -> GameResult<()> {
    let stride = RunRules::default().move_speed * STEP.as_secs_f32();
    for &(column, row) in route {
        let target = at(column, row);
        for _ in 0..240 {
            let round = app.world().resource::<Round>();
            if round.status() == RoundStatus::Escaped || round.position().distance(target) < 0.1 {
                break;
            }
            let movement = (target - round.position()) / stride;
            step(app, movement)?;
        }
        let round = app.world().resource::<Round>();
        if round.status() != RoundStatus::Escaped && round.position().distance(target) >= 0.1 {
            return Err(format!("the pilot did not reach arena tile ({column}, {row})").into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::{Pilot, PowerCell, arena, register_game_schema};
    use nara::{
        reflect::{ComponentTypeId, component_registry},
        scene::authoring::{SceneDocument, SpriteSource},
    };

    fn scene_fixture() -> GameResult<SceneDocument> {
        let mut app = headless_app()?;
        let project = source::SourceProject::new(&mut app, source::test_root())?;
        Ok(project.load(&project.assets()?, None)?.scene)
    }

    fn new_app(game: GamePlugin) -> GameResult<App> {
        let mut app = headless_app()?;
        app.add_plugins(game)?;
        Ok(app)
    }

    #[test]
    fn fixed_replay_checks_collision_shield_loss_restart_and_escape() -> GameResult<()> {
        let report = run_stress_in(source::test_root(), 2)?;
        assert_eq!(report.rounds, 2);
        assert_eq!(report.fixed_ticks, 2_330);
        assert_eq!(report.wall_contacts_per_round, 28);
        assert!(report.escape_time < Duration::from_secs(38));
        Ok(())
    }

    #[test]
    fn time_expiry_freezes_the_round_until_restart() -> GameResult<()> {
        let mut app = new_app(
            GamePlugin::from_scene(scene_fixture()?).with_rules(RunRules {
                time_limit_seconds: 1,
                ..Default::default()
            }),
        )?;
        for _ in 0..60 {
            step(&mut app, Vec2::ZERO)?;
        }
        let stopped = app.world().resource::<Round>().clone();
        assert_eq!(stopped.status(), RoundStatus::Failed(Failure::TimeExpired));
        step(&mut app, Vec2::X)?;
        assert_eq!(app.world().resource::<Round>(), &stopped);
        restart(&mut app)?;
        step(&mut app, Vec2::X)?;
        let round = app.world().resource::<Round>();
        assert!(round.position().x > START.x);
        assert_eq!(round.shields(), RunRules::default().shield_charges);
        Ok(())
    }

    #[test]
    fn reaching_an_unpowered_gate_keeps_the_round_running() -> GameResult<()> {
        let mut app = new_app(GamePlugin::from_scene(scene_fixture()?))?;
        follow(
            &mut app,
            &[(2, 5), (8, 5), (8, 4), (19, 4), (19, 9), (17, 9), (17, 8)],
        )?;
        let round = app.world().resource::<Round>();
        assert!(round.position().distance(at(17, 8)) < 0.1);
        assert_eq!(round.collected(), 0);
        assert_eq!(round.status(), RoundStatus::Running);
        Ok(())
    }

    #[test]
    fn authored_rules_and_positions_drive_the_fixed_simulation() -> GameResult<()> {
        let mut scene = scene_fixture()?;
        let spawn = Vec2::new(-256.0, 128.0);
        for entity in &mut scene.entities {
            match entity.name.as_str() {
                "Courier" | "Power cell A" => entity.set(&Transform2d::from_translation(spawn))?,
                "West bulkhead" => entity.set(&Transform2d::from_xy(-288.0, 0.0))?,
                "Round rules" => entity.set(&RunRules {
                    time_limit_seconds: 12,
                    move_speed: 240.0,
                    shield_charges: 4,
                })?,
                _ => {}
            }
        }
        let mut app = new_app(GamePlugin::from_scene(scene))?;
        let round = app.world().resource::<Round>();
        assert_eq!(round.position(), spawn);
        assert_eq!(round.remaining(), Duration::from_secs(12));
        assert_eq!(round.shields(), 4);
        step(&mut app, Vec2::NEG_X)?;
        let round = app.world().resource::<Round>();
        assert!((round.position().x - (spawn.x - 4.0)).abs() < 0.01);
        assert_eq!(round.collected(), 1);
        for _ in 0..30 {
            step(&mut app, Vec2::NEG_X)?;
        }
        let round = app.world().resource::<Round>();
        assert!(round.wall_contacts() > 0);
        assert!((-263.0..-261.0).contains(&round.position().x));
        Ok(())
    }

    #[test]
    fn authored_order_survives_renames_and_reordering() -> GameResult<()> {
        let mut scene = scene_fixture()?;
        scene.entities.reverse();
        for (index, entity) in scene.entities.iter_mut().enumerate() {
            entity.id = format!("a9cf0ab5-1435-4be1-b235-{index:012x}").parse()?;
            entity.name = format!("Renamed {index}");
        }
        let mut app = new_app(GamePlugin::from_scene(scene))?;
        let arena = app.world().resource::<arena::Arena>();
        assert_eq!(arena.pilot.pose.translation, START);
        assert_eq!(
            arena.cells.map(|marker| marker.pose.translation),
            [at(9, 2), at(17, 2), at(3, 8)],
        );
        assert_eq!(
            arena.patrols.map(|patrol| patrol.position(0.0)),
            [
                Vec2::new(-32.0, 64.0),
                Vec2::new(-67.2, -64.0),
                Vec2::new(256.0, -28.8)
            ],
        );
        restart(&mut app)?;
        Ok(())
    }

    #[test]
    fn power_cell_roles_belong_to_the_instances_and_keep_their_ids() -> GameResult<()> {
        let mut app = headless_app()?;
        register_game_schema(&mut app)?;
        let schemas = component_registry(app.world())
            .ok_or("missing game schema")?
            .components();
        let root = source::test_root().join("assets");
        let authored = SceneDocument::read_with(root.join("night-relay.scene.ron"), schemas)?;
        let prefab = SceneDocument::read_with(root.join("power-cell.prefab.ron"), schemas)?;
        assert!(prefab.entities[0].typed::<PowerCell>()?.is_none());
        let role = ComponentTypeId::try_new(PowerCell::COMPONENT_ID)?;
        let saved = authored.to_ron_with(schemas)?;
        assert!(saved.contains("\"night-relay.PowerCell\": (order: 1)"));
        let scene = scene_fixture()?;
        let texture = "01a219be-557a-739e-81a5-2a8c6e88aee5".parse()?;
        for order in 1..=3 {
            let id = format!("4f99b200-2a5e-4872-8d3d-{suffix:012}", suffix = order + 1).parse()?;
            let instance = authored
                .entities
                .iter()
                .find(|entity| entity.id == id)
                .and_then(|entity| entity.prefab.as_ref())
                .ok_or("missing authored power cell instance")?;
            assert_eq!(
                instance
                    .overrides
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                ["nara.Transform2d.translation"]
            );
            assert_eq!(
                instance.added_components[&role].typed::<PowerCell>()?.order,
                order
            );
            let cell = scene
                .entities
                .iter()
                .find(|entity| entity.id == id)
                .ok_or("the prefab root lost its authored ID")?;
            assert!(cell.prefab.is_none());
            assert_eq!(cell.typed::<PowerCell>()?.unwrap().order, order);
            let sprite = cell.typed::<SpriteSource>()?.unwrap();
            assert_eq!(sprite.image, Some(texture));
            assert_eq!(sprite.size, Vec2::splat(15.0));
        }
        Ok(())
    }

    #[test]
    fn moved_scene_prefab_and_texture_keep_replay_and_snapshot_working() -> GameResult<()> {
        let project = tempfile::tempdir()?;
        let files = project.path().join("assets/relocated");
        fs::create_dir_all(&files)?;
        let original = source::test_root().join("assets");
        for (old, new) in [
            ("night-relay.scene.ron", "arena.scene.ron"),
            ("power-cell.prefab.ron", "cell.prefab.ron"),
            ("power-cell.png", "cell.png"),
            ("gate.material.ron", "gate.material.ron"),
            ("gate.wgsl", "gate.wgsl"),
        ] {
            for suffix in ["", ".meta"] {
                fs::copy(
                    original.join(format!("{old}{suffix}")),
                    files.join(format!("{new}{suffix}")),
                )?;
            }
        }
        assert_eq!(
            run_stress_in(project.path(), 2)?,
            run_stress_in(source::test_root(), 2)?
        );

        let snapshots = tempfile::tempdir()?;
        let snapshot = snapshots.path().join("play.scene.ron");
        let entry = files.join("arena.scene.ron");
        let authored = fs::read_to_string(&entry)?;
        fs::write(
            &snapshot,
            authored.replace(
                "\"night-relay.RunRules\": ()",
                "\"night-relay.RunRules\": (shield_charges: 4)",
            ),
        )?;
        let mut app = headless_app()?;
        let source = source::SourceProject::new(&mut app, project.path())?;
        let edited = source.load(&source.assets()?, Some(&snapshot))?;
        #[cfg(feature = "desktop")]
        assert_eq!(edited.images.len(), 1);
        app.add_plugins(GamePlugin::from_scene(edited.scene))?;
        assert_eq!(app.world().resource::<Round>().shields(), 4);
        assert_eq!(fs::read_to_string(entry)?, authored);

        fs::write(files.join("cell.png"), b"not a PNG")?;
        let error = run_stress_in(project.path(), 1)
            .expect_err("headless should load the relocated texture");
        assert!(
            error.to_string().contains("assets/relocated/cell.png"),
            "{error}"
        );
        Ok(())
    }

    #[test]
    fn rejects_ambiguous_or_missing_role_bindings_before_spawning() -> GameResult<()> {
        let mut scene = scene_fixture()?;
        let cell = scene
            .entities
            .iter_mut()
            .find(|entity| entity.name == "Power cell B")
            .ok_or("missing cell B in the fixture")?;
        cell.set(&PowerCell { order: 1 })?;
        let error = reject_scene(scene)?;
        assert!(error.contains("both bind the power cell 1 role"), "{error}");

        let mut scene = scene_fixture()?;
        scene
            .entities
            .retain(|entity| entity.name != "Extraction gate");
        let error = reject_scene(scene.clone())?;
        assert!(error.contains("missing its extraction gate"), "{error}");

        let cell = scene
            .entities
            .iter_mut()
            .find(|entity| entity.name == "Power cell A")
            .ok_or("missing cell A in the fixture")?;
        cell.set(&Pilot)?;
        let error = reject_scene(scene)?;
        assert!(error.contains("multiple arena roles"), "{error}");
        Ok(())
    }

    fn reject_scene(scene: SceneDocument) -> GameResult<String> {
        let mut app = headless_app()?;
        let initial_entities = app.world().entity_count();
        let error = app
            .add_plugins(GamePlugin::from_scene(scene))
            .err()
            .ok_or("invalid scene accepted")?;
        assert_eq!(app.world().entity_count(), initial_entities);
        Ok(error.to_string())
    }
}
