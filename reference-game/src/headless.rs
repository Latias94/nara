//! Deterministic exercises driven through the same plugin and fixed schedule as desktop play.

use std::{error::Error, path::Path, time::Duration};

use nara::prelude::*;

use crate::{
    Failure, GamePlugin, PilotInput, Round, RoundStatus, RunRules,
    arena::CELL_COUNT,
    campaign::Sector,
    development_root,
    route::{ESCAPE_ROUTE, at},
    scene_roles::register_scene_roles,
};

type GameResult<T> = Result<T, Box<dyn Error>>;
const STEP: Duration = FixedTime::DEFAULT_TIMESTEP;
pub const MAX_ROUNDS: u32 = 200;
const START: Vec2 = at(2, 2);

#[derive(Debug, PartialEq, Eq)]
pub struct StressReport {
    pub rounds: u32,
    pub fixed_ticks: u64,
    pub wall_contacts_per_round: u32,
    pub escape_time: Duration,
}

pub fn run_stress(rounds: u32) -> GameResult<StressReport> {
    run_stress_in(&development_root()?, rounds)
}

fn run_stress_in(project_root: &Path, rounds: u32) -> GameResult<StressReport> {
    if !(1..=MAX_ROUNDS).contains(&rounds) {
        return Err(format!("choose 1–{MAX_ROUNDS} stress rounds").into());
    }
    let mut app = headless_app()?;
    install_project(&mut app, project_root, Sector::One)?;
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
    register_scene_roles(&mut app)?;
    Ok(app)
}

fn install_project(app: &mut App, root: &Path, sector: Sector) -> GameResult<()> {
    app.set_asset_source(RuntimeAssetSource::from_project_dir(root, "assets")?)?;
    switch_scene_sync(app.world_mut()?, sector.asset()?)?;
    app.add_plugins(GamePlugin)?;
    Ok(())
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
    follow(app, &ESCAPE_ROUTE)?;
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
    use std::{fs, thread, time::Instant};

    use super::*;
    use crate::{
        arena::{self, source},
        audio::ARRIVAL_BUS,
        campaign::{ARRIVAL_AUDIO, Campaign, SaveSnapshot},
        scene_roles::{Pilot, PowerCell},
    };
    use nara::{
        audio::{AudioOutput, PlaybackState},
        reflect::{ComponentTypeId, component_registry},
        save::SaveStore,
        scene::authoring::{SceneDocument, SpriteSource},
    };

    fn scene_fixture() -> GameResult<SceneDocument> {
        let app = headless_app()?;
        let project = source::SourceProject::new(&app)?;
        let assets = RuntimeAssetSource::from_project_dir(source::test_root(), "assets")?;
        project.read_scene(&assets, Sector::One.asset()?)
    }

    fn new_app(scene: SceneDocument) -> GameResult<App> {
        let mut app = headless_app()?;
        app.set_asset_source(RuntimeAssetSource::from_project_dir(
            source::test_root(),
            "assets",
        )?)?;
        install_document(&mut app, &scene)?;
        Ok(app)
    }

    fn install_document(app: &mut App, scene: &SceneDocument) -> GameResult<()> {
        let cooked = source::SourceProject::new(app)?.cook(scene)?;
        switch_scene_asset_sync(app.world_mut()?, Sector::One.asset()?, &cooked)?;
        app.add_plugins(GamePlugin)?;
        Ok(())
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
    fn escape_switches_sectors_plays_arrival_and_saves_progress() -> GameResult<()> {
        let temporary = tempfile::tempdir()?;
        let mut app = headless_app()?;
        app.insert_resource(Campaign::with_save(
            SaveStore::in_dir(temporary.path()),
            "progress",
        ))?;
        install_project(&mut app, source::test_root(), Sector::One)?;
        let first = app
            .world()
            .resource::<SceneManager>()
            .main_instance()
            .unwrap();
        let old = app
            .world()
            .resource::<SceneManager>()
            .entities(first)
            .unwrap()
            .values()
            .copied()
            .collect::<Vec<_>>();

        restart(&mut app)?;
        follow(&mut app, &ESCAPE_ROUTE)?;
        let round = app.world().resource::<Round>();
        assert_eq!(round.status(), RoundStatus::Escaped);
        let expected = SaveSnapshot {
            sector_one_best_ms: Some(round.elapsed.as_millis() as u64),
            sector_two_best_ms: None,
        };
        let next = app
            .world()
            .resource::<Campaign>()
            .pending
            .ok_or("switch not requested")?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.world().resource::<SceneManager>().main_instance() != Some(next) {
            assert!(Instant::now() < deadline, "Sector 2 did not become ready");
            app.update_with_delta(Duration::ZERO)?;
            thread::sleep(Duration::from_millis(1));
        }
        let world = app.world_mut()?;
        let manager = world.resource::<SceneManager>();
        assert_eq!(manager.state(next), Some(&SceneLoadState::Ready));
        assert_eq!(manager.state(first), None);
        assert!(old.iter().all(|&entity| world.get_entity(entity).is_err()));
        assert!(
            world
                .query::<&SceneMember>()
                .iter(world)
                .all(|member| member.0 == next)
        );
        assert_eq!(world.resource::<Campaign>().sector, Sector::Two);
        let round = world.resource::<Round>();
        assert_eq!(round.status(), RoundStatus::Running);
        assert_eq!(round.collected(), 0);
        assert_eq!(round.position(), Vec2::new(-256.0, -96.0));
        assert_eq!(round.remaining(), Duration::from_secs(38));

        step(&mut app, Vec2::ZERO)?;
        let world = app.world_mut()?;
        let arrival: AssetId = ARRIVAL_AUDIO.parse()?;
        assert_eq!(world.resource::<AudioMixer>().output(), AudioOutput::Mock);
        assert!(
            world
                .query::<(&SceneMember, &AudioClipSource, &AudioPlayback)>()
                .iter(world)
                .any(|(member, source, playback)| member.0 == next
                    && source.clip == arrival
                    && source.bus == ARRIVAL_BUS
                    && playback.state() == PlaybackState::Playing)
        );

        let store = SaveStore::in_dir(temporary.path());
        assert_eq!(store.load::<SaveSnapshot>("progress")?, expected);
        let campaign = Campaign::load(store, "progress")?;
        assert_eq!(campaign.progress, expected);
        let sector = campaign.sector;
        let mut resumed = headless_app()?;
        resumed.insert_resource(campaign)?;
        install_project(&mut resumed, source::test_root(), sector)?;
        assert_eq!(
            resumed.world().resource::<Round>().position(),
            Vec2::new(-256.0, -96.0)
        );
        follow(
            &mut resumed,
            &[(7, 8), (7, 7), (18, 7), (19, 7), (19, 2), (15, 2), (2, 2)],
        )?;
        let round = resumed.world().resource::<Round>();
        assert_eq!(round.status(), RoundStatus::Escaped);
        assert_eq!(round.collected(), CELL_COUNT);
        assert_eq!(
            SaveStore::in_dir(temporary.path()).load::<SaveSnapshot>("progress")?,
            SaveSnapshot {
                sector_two_best_ms: Some(round.elapsed.as_millis() as u64),
                ..expected
            }
        );
        Ok(())
    }

    #[test]
    fn an_invalid_sector_keeps_the_current_arena_and_round() -> GameResult<()> {
        use nara::{
            asset::{AssetServer, PreparedAsset},
            scene::cook::cook_flattened_scene,
        };

        let mut app = headless_app()?;
        app.insert_resource(Campaign::default())?;
        install_project(&mut app, source::test_root(), Sector::One)?;
        let id = Sector::Two.asset()?;
        let assets = app.world().resource::<RuntimeAssetSource>();
        let project = source::SourceProject::new(&app)?;
        let mut scene = project.read_scene(assets, id)?;
        scene
            .entities
            .retain(|entity| entity.name != "Extraction gate");
        let schemas = component_registry(app.world()).ok_or("missing game schema")?;
        let broken = SceneAsset::from_cooked(cook_flattened_scene(&scene, schemas)?.bytes)?;
        let world = app.world_mut()?;
        let original = world
            .resource::<SceneManager>()
            .main_instance()
            .ok_or("missing entry scene")?;
        let handle = world
            .resource_mut::<AssetServer>()
            .load_id::<SceneAsset>(id)?;
        PreparedAsset::from_value(broken).publish(world, handle)?;

        follow(&mut app, &ESCAPE_ROUTE)?;
        let round = app.world().resource::<Round>().clone();
        assert_eq!(round.status(), RoundStatus::Escaped);
        assert!(app.world().resource::<Campaign>().pending.is_some());
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.world().resource::<Campaign>().pending.is_some() {
            assert!(Instant::now() < deadline, "Sector 2 did not settle");
            app.update_with_delta(Duration::ZERO)?;
            thread::sleep(Duration::from_millis(1));
        }
        let world = app.world_mut()?;
        assert_eq!(
            world.resource::<SceneManager>().main_instance(),
            Some(original)
        );
        assert!(
            world
                .query::<&SceneMember>()
                .iter(world)
                .all(|member| member.0 == original)
        );
        assert_eq!(world.resource::<Campaign>().sector, Sector::One);
        assert_eq!(world.resource::<Round>(), &round);

        restart(&mut app)?;
        follow(&mut app, &ESCAPE_ROUTE)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.world().resource::<Campaign>().pending.is_some() {
            assert!(
                Instant::now() < deadline,
                "Sector 2 did not settle after retry"
            );
            app.update_with_delta(Duration::ZERO)?;
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(app.world().resource::<Campaign>().sector, Sector::Two);
        Ok(())
    }

    #[test]
    fn time_expiry_freezes_the_round_until_restart() -> GameResult<()> {
        let mut scene = scene_fixture()?;
        scene
            .entities
            .iter_mut()
            .find(|entity| entity.name == "Round rules")
            .ok_or("missing round rules")?
            .set(&RunRules {
                time_limit_seconds: 1,
                ..Default::default()
            })?;
        let mut app = new_app(scene)?;
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
        let mut app = new_app(scene_fixture()?)?;
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
        let mut app = new_app(scene)?;
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
        let mut app = new_app(scene)?;
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
        let app = headless_app()?;
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
        let source = source::SourceProject::new(&app)?;
        let assets = RuntimeAssetSource::from_project_dir(project.path(), "assets")?;
        let edited = source.read_snapshot(&assets, &snapshot)?;
        app.set_asset_source(assets)?;
        install_document(&mut app, &edited)?;
        assert_eq!(app.world().resource::<Round>().shields(), 4);
        assert_eq!(fs::read_to_string(entry)?, authored);

        fs::write(files.join("cell.png"), b"not a PNG")?;
        let error = run_stress_in(project.path(), 1)
            .expect_err("headless should load the relocated texture");
        assert!(
            error
                .to_string()
                .contains("01a219be-557a-739e-81a5-2a8c6e88aee5"),
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
        let error = install_document(&mut app, &scene).expect_err("invalid scene accepted");
        assert_eq!(app.world().entity_count(), initial_entities);
        Ok(error.to_string())
    }
}
