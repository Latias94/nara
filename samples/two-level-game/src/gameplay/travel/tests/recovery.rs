use std::{
    num::NonZeroU32,
    sync::Arc,
    time::{Duration, Instant},
};

use nara::{
    advanced_prelude::{CurrentScene, SceneSourceView},
    app::{PluginCategory, PluginDeclaration, PluginId, RealTime},
    ecs::{Entity, World, schedule::IntoScheduleConfigs},
    gameplay::{GameplayCommandSet, LocalGameplayCommands, add_local_gameplay_command_systems},
    hierarchy::{
        HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyRoots,
        HierarchyWriter, Parent,
    },
    identity::{
        SpawnedSceneInstance, WorldEntityToken, WorldIdentityDomain, spawn_identity_entity,
    },
    prelude::{App, Color, CoreStage, Plugin, PluginError, ResMut, Resource, Transform2d, Vec2},
    project_host::{HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome},
    scene::advanced::SceneProductResource,
    sprite::Sprite,
};

use crate::{
    gameplay::{ActiveLevel, GamePhase, GameSnapshot, Progress, Session, travel::DynamicMarker},
    levels::{LevelId, LevelKey},
    save::Checkpoint,
};

use super::super::{INSTANCE, MARKER, RETRY, TRAVEL, action};

#[derive(Clone, Copy, Default)]
enum Step {
    #[default]
    Start,
    MeadowMarker,
    Cavern,
    CavernMarker,
    OtherCavern,
    OtherMarker,
    Returned,
    Faulted,
    Retrying,
    Steady,
    Meadow,
    Done,
}

#[derive(Clone)]
struct Baseline {
    checkpoint: Checkpoint,
    progress: (Arc<()>, u64),
    level: (Arc<()>, u64),
    receipt: SpawnedSceneInstance,
    source: SceneSourceView,
    marker: WorldEntityToken,
}

#[derive(Clone, Default, Resource)]
struct Recovery {
    retire: bool,
    step: Step,
    command: Option<&'static str>,
    retry_sent: bool,
    baseline: Option<Baseline>,
    foreign: Option<WorldEntityToken>,
    recovered: Option<WorldEntityToken>,
    stable_since: Option<u64>,
    complete: bool,
}

#[derive(Default)]
struct RecoveryProbe<const RETIRE: bool>;

impl<const RETIRE: bool> Plugin for RecoveryProbe<RETIRE> {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("two-level-game.test-marker-recovery"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[crate::gameplay::PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(Recovery {
            retire: RETIRE,
            ..Recovery::default()
        })?;
        add_local_gameplay_command_systems(app, dispatch)?;
        app.add_systems(
            CoreStage::FixedUpdate,
            observe.after(GameplayCommandSet::Capture),
        )?;
        Ok(())
    }
}

fn dispatch(mut recovery: ResMut<Recovery>, mut commands: ResMut<LocalGameplayCommands>) {
    if let Some(command) = recovery.command.take() {
        commands.push(action(command)).unwrap();
        recovery.retry_sent |= command == RETRY;
    }
    std::thread::yield_now();
}

fn observe(world: &mut World) {
    world.resource_scope(|world, mut recovery: nara::ecs::Mut<Recovery>| {
        let snapshot = world.resource::<GameSnapshot>().clone();
        let running = snapshot.phase == GamePhase::Running;
        let cavern = LevelKey::main(LevelId::Cavern);
        let other_cavern = LevelKey::new(LevelId::Cavern, 1).unwrap();
        let meadow = LevelKey::main(LevelId::Meadow);
        match recovery.step {
            Step::Start if snapshot.initialized && running => {
                let foreign = spawn_identity_entity(world).unwrap();
                world.entity_mut(foreign.entity()).insert((
                    Transform2d::from_translation(Vec2::new(240.0, 100.0)),
                    Sprite::from_color(Vec2::splat(7.0), Color::WHITE),
                ));
                HierarchyWriter::new(world)
                    .apply(HierarchyOperation::Place {
                        entity: foreign.entity(),
                        placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
                    })
                    .unwrap();
                recovery.foreign = Some(foreign);
                recovery.command = Some(MARKER);
                recovery.step = Step::MeadowMarker;
            }
            Step::MeadowMarker if running && marker_ids(&snapshot) == [1] => {
                recovery.command = Some(TRAVEL);
                recovery.step = Step::Cavern;
            }
            Step::Cavern if running && snapshot.level == Some(cavern) => {
                recovery.command = Some(MARKER);
                recovery.step = Step::CavernMarker;
            }
            Step::CavernMarker if running && marker_ids(&snapshot) == [2] => {
                recovery.command = Some(INSTANCE);
                recovery.step = Step::OtherCavern;
            }
            Step::OtherCavern if running && snapshot.level == Some(other_cavern) => {
                recovery.command = Some(MARKER);
                recovery.step = Step::OtherMarker;
            }
            Step::OtherMarker if running && marker_ids(&snapshot) == [3] => {
                recovery.command = Some(INSTANCE);
                recovery.step = Step::Returned;
            }
            Step::Returned if running && snapshot.level == Some(cavern) => {
                assert_eq!(snapshot.travel_count, 3);
                assert_eq!(marker_ids(&snapshot), [2]);
                let progress = world.resource::<Progress>();
                assert_eq!(
                    progress
                        .checkpoint()
                        .markers
                        .iter()
                        .map(|marker| (marker.id.get(), marker.level))
                        .collect::<Vec<_>>(),
                    [(1, meadow), (2, cavern), (3, other_cavern)]
                );
                let current = CurrentScene::get(world).unwrap();
                let marker = world.resource::<Session>().markers[0];
                recovery.baseline = Some(Baseline {
                    checkpoint: progress.checkpoint().clone(),
                    progress: progress.capture_baseline(),
                    level: world.resource::<ActiveLevel>().capture_baseline(),
                    receipt: current.receipt().clone(),
                    source: current.source_view(),
                    marker,
                });
                let from = HierarchyPlacement::current(world, marker.entity()).unwrap();
                HierarchyWriter::new(world)
                    .apply(HierarchyOperation::Remove {
                        entity: marker.entity(),
                        from,
                    })
                    .unwrap();
                if recovery.retire {
                    assert!(matches!(
                        world
                            .resource::<WorldIdentityDomain>()
                            .locators_for_token(world, marker),
                        Ok(None)
                    ));
                    assert!(world.despawn(marker.entity()));
                } else {
                    world.entity_mut(marker.entity()).remove::<Sprite>();
                    *world.get_mut::<Transform2d>(marker.entity()).unwrap() = Transform2d {
                        translation: Vec2::new(70.0, 80.0),
                        rotation: 1.0,
                        scale: Vec2::splat(3.0),
                    };
                }
                recovery.step = Step::Faulted;
            }
            Step::Faulted => {
                assert_eq!(snapshot.phase, GamePhase::Degraded);
                assert!(snapshot.error.is_some());
                assert_baseline(world, recovery.baseline.as_ref().unwrap());
                recovery.command = Some(RETRY);
                recovery.step = Step::Retrying;
            }
            Step::Retrying if recovery.retry_sent => {
                assert_eq!(
                    snapshot.phase,
                    GamePhase::Running,
                    "Retry must reconstruct the committed marker after natural degradation: {snapshot:?}"
                );
                assert!(snapshot.error.is_none());
                let baseline = recovery.baseline.as_ref().unwrap();
                assert_baseline(world, baseline);
                let token = assert_marker(world, baseline);
                if recovery.retire {
                    assert_ne!(token, baseline.marker);
                } else {
                    assert_eq!(token, baseline.marker);
                }
                recovery.recovered = Some(token);
                recovery.stable_since = Some(world.resource::<RealTime>().frame);
                recovery.step = Step::Steady;
            }
            Step::Steady => {
                assert_eq!(snapshot.phase, GamePhase::Running);
                assert!(snapshot.error.is_none());
                let baseline = recovery.baseline.as_ref().unwrap();
                assert_baseline(world, baseline);
                assert_eq!(assert_marker(world, baseline), recovery.recovered.unwrap());
                assert_foreign(world, recovery.foreign.unwrap());
                if world.resource::<RealTime>().frame >= recovery.stable_since.unwrap() + 4 {
                    recovery.command = Some(TRAVEL);
                    recovery.step = Step::Meadow;
                }
            }
            Step::Meadow if running && snapshot.level == Some(meadow) => {
                assert_eq!(snapshot.travel_count, 4);
                assert_eq!(snapshot.restarts, 0);
                assert_eq!(marker_ids(&snapshot), [1]);
                let baseline = recovery.baseline.as_ref().unwrap();
                assert_ne!(snapshot.current_instance, Some(baseline.receipt.instance_id()));
                let progress = world.resource::<Progress>().checkpoint();
                assert_eq!(progress.markers, baseline.checkpoint.markers);
                assert_eq!(progress.next_marker_id, baseline.checkpoint.next_marker_id);
                assert_eq!(progress.collected, baseline.checkpoint.collected);
                assert_eq!(progress.score, baseline.checkpoint.score);
                for token in [baseline.marker, recovery.recovered.unwrap()] {
                    assert!(!world.entities().contains(token.entity()));
                    assert!(
                        world
                            .resource::<WorldIdentityDomain>()
                            .locators_for_token(world, token)
                            .is_err()
                    );
                }
                assert_foreign(world, recovery.foreign.unwrap());
                recovery.complete = true;
                recovery.step = Step::Done;
            }
            _ => {}
        }
    });
}

fn marker_ids(snapshot: &GameSnapshot) -> Vec<u64> {
    snapshot.markers.iter().map(|marker| marker.get()).collect()
}

fn assert_baseline(world: &World, baseline: &Baseline) {
    let current = CurrentScene::get(world).unwrap();
    assert_eq!(current.receipt(), &baseline.receipt);
    assert_eq!(
        baseline
            .source
            .with_document(|source| std::ptr::eq(source, current.source())),
        Some(true)
    );
    let progress = world.resource::<Progress>();
    assert_eq!(progress.checkpoint(), &baseline.checkpoint);
    assert!(progress.matches_baseline(&baseline.progress));
    assert!(
        world
            .resource::<ActiveLevel>()
            .matches_baseline(&baseline.level)
    );
}

fn assert_marker(world: &mut World, baseline: &Baseline) -> WorldEntityToken {
    let tokens = &world.resource::<Session>().markers;
    assert_eq!(tokens.len(), 1);
    let token = tokens[0];
    let mut query = world.query::<(Entity, &DynamicMarker)>();
    let actual = query
        .iter(world)
        .map(|(entity, marker)| (entity, marker.id.get()))
        .collect::<Vec<_>>();
    assert_eq!(actual, [(token.entity(), 2)]);
    let record = baseline
        .checkpoint
        .markers
        .iter()
        .find(|marker| marker.id.get() == 2)
        .unwrap();
    assert_eq!(
        *world.get::<Transform2d>(token.entity()).unwrap(),
        Transform2d::from_translation(Vec2::from_array(record.position))
    );
    let sprite = world.get::<Sprite>(token.entity()).unwrap();
    assert_eq!(sprite.size, Vec2::splat(10.0));
    assert_eq!(sprite.material.tint, Color::rgb(1.0, 0.3, 0.8));
    assert!(sprite.material.image.is_none());
    assert!(world.get::<Parent>(token.entity()).is_none());
    assert!(
        world
            .resource::<HierarchyRoots>()
            .as_slice()
            .contains(&token.entity())
    );
    assert!(matches!(
        world
            .resource::<WorldIdentityDomain>()
            .locators_for_token(world, token),
        Ok(None)
    ));
    token
}

fn assert_foreign(world: &World, token: WorldEntityToken) {
    assert!(matches!(
        world
            .resource::<WorldIdentityDomain>()
            .locators_for_token(world, token),
        Ok(None)
    ));
    assert_eq!(
        world
            .get::<Transform2d>(token.entity())
            .unwrap()
            .translation,
        Vec2::new(240.0, 100.0)
    );
    assert_eq!(
        world.get::<Sprite>(token.entity()).unwrap().size,
        Vec2::splat(7.0)
    );
    assert!(
        world
            .resource::<HierarchyRoots>()
            .as_slice()
            .contains(&token.entity())
    );
}

fn run_recovery<const RETIRE: bool>() {
    let recipe = crate::product_recipe()
        .add_plugin::<RecoveryProbe<RETIRE>>()
        .unwrap();
    let intent =
        HeadlessRunIntent::frames(NonZeroU32::new(200_000).unwrap(), Duration::from_millis(20))
            .with_recipe(recipe)
            .stop_when(|recovery: &Recovery| recovery.complete);
    let mut run = HeadlessRun::new(crate::open_project_root().unwrap(), intent, Vec::new());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(recovery) => {
                assert!(recovery.complete);
                break;
            }
            HeadlessRunOutcome::CleanupIncomplete if Instant::now() < deadline => {
                std::thread::yield_now();
            }
            _ => panic!("marker recovery Runtime failed: {:?}", report.diagnostics()),
        }
    }
}

#[test]
fn marker_reconstruction_retry_repairs_components_without_replacing_current_scene() {
    run_recovery::<false>();
}

#[test]
fn marker_reconstruction_retry_replaces_legally_retired_token_and_can_travel_again() {
    run_recovery::<true>();
}
