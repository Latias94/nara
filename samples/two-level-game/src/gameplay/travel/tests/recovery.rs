use std::{
    num::NonZeroU32,
    sync::Arc,
    time::{Duration, Instant},
};

use nara::{
    advanced_prelude::{CurrentScene, SceneProductResource, SceneSourceView},
    app::{PluginCategory, PluginDeclaration, PluginId, RealTime},
    asset::Assets,
    ecs::{Entity, World, schedule::IntoScheduleConfigs},
    hierarchy::{
        Children, HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyRoots,
        HierarchyWriter, Parent,
    },
    identity::{
        EntityLookup, EntityReference, SpawnedSceneInstance, WorldEntityToken, WorldIdentityDomain,
        spawn_identity_entity,
    },
    image::ImageAsset,
    material::SamplerDescriptor,
    prelude::{App, Color, CoreStage, Plugin, PluginError, ResMut, Resource, Transform2d, Vec2},
    project_host::{HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome},
    scene::{SceneEntitySource, retire_and_despawn_scene_entity},
    sprite::Sprite,
};

use crate::{
    command::{CommandSchedule, CommandSet, ServerCommand},
    gameplay::{
        ActiveLevel, GamePhase, GameSnapshot, Progress, Session,
        commands::{INSTANCE, MARKER, RETRY, TRAVEL, action},
        travel::{DynamicMarker, MarkerInstance},
    },
    levels::{AUTHORED_LEVEL_MEMBERS, LevelId, LevelKey},
    markers::MarkerLink,
    save::Checkpoint,
};

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
struct MarkerGraph {
    instance: MarkerInstance,
    root: Entity,
    child: Entity,
}

#[derive(Clone)]
struct Baseline {
    checkpoint: Checkpoint,
    progress: (Arc<()>, u64),
    level: (Arc<()>, u64),
    receipt: SpawnedSceneInstance,
    source: SceneSourceView,
    marker: MarkerGraph,
}

#[derive(Clone, Default, Resource)]
struct Recovery {
    retire: bool,
    step: Step,
    command: Option<ServerCommand>,
    retry_sent: bool,
    baseline: Option<Baseline>,
    foreign: Option<WorldEntityToken>,
    history: Vec<MarkerGraph>,
    recovered: Option<MarkerGraph>,
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
        app.add_systems(
            CoreStage::PreUpdate,
            dispatch.in_set(CommandSet::FrameInput),
        )?;
        app.add_systems(
            CoreStage::FixedUpdate,
            dispatch.in_set(CommandSet::FrameInput),
        )?;
        app.add_systems(CoreStage::FixedUpdate, observe.after(CommandSet::Capture))?;
        Ok(())
    }
}

fn dispatch(mut recovery: ResMut<Recovery>, mut commands: ResMut<CommandSchedule>) {
    if let Some(command) = recovery.command.take() {
        recovery.retry_sent |= command == RETRY;
        commands.push_local(action(command)).unwrap();
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
                recovery.history.push(assert_marker(world, 1));
                recovery.command = Some(TRAVEL);
                recovery.step = Step::Cavern;
            }
            Step::Cavern if running && snapshot.level == Some(cavern) => {
                recovery.command = Some(MARKER);
                recovery.step = Step::CavernMarker;
            }
            Step::CavernMarker if running && marker_ids(&snapshot) == [2] => {
                recovery.history.push(assert_marker(world, 2));
                recovery.command = Some(INSTANCE);
                recovery.step = Step::OtherCavern;
            }
            Step::OtherCavern if running && snapshot.level == Some(other_cavern) => {
                recovery.command = Some(MARKER);
                recovery.step = Step::OtherMarker;
            }
            Step::OtherMarker if running && marker_ids(&snapshot) == [3] => {
                recovery.history.push(assert_marker(world, 3));
                recovery.command = Some(INSTANCE);
                recovery.step = Step::Returned;
            }
            Step::Returned if running && snapshot.level == Some(cavern) => {
                assert_eq!(snapshot.travel_count, 3);
                assert_eq!(marker_ids(&snapshot), [2]);
                assert_eq!(recovery.history.len(), 3);
                for marker in &recovery.history {
                    assert_retired(world, marker);
                }
                let marker = assert_marker(world, 2);
                assert_replaced(world, &recovery.history[1], &marker);
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
                recovery.baseline = Some(Baseline {
                    checkpoint: progress.checkpoint().clone(),
                    progress: progress.capture_baseline(),
                    level: world.resource::<ActiveLevel>().capture_baseline(),
                    receipt: current.receipt().clone(),
                    source: current.source_view(),
                    marker: marker.clone(),
                });
                if recovery.retire {
                    retire_and_despawn_scene_entity(world, marker.child).unwrap();
                } else {
                    world.entity_mut(marker.root).remove::<Sprite>();
                    *world.get_mut::<Transform2d>(marker.root).unwrap() = Transform2d {
                        translation: Vec2::new(70.0, 80.0),
                        rotation: 1.0,
                        scale: Vec2::splat(3.0),
                        z: 0.0,
                    };
                }
                recovery.step = Step::Faulted;
            }
            Step::Faulted => {
                assert_eq!(snapshot.phase, GamePhase::Degraded);
                assert!(snapshot.error.is_some());
                let baseline = recovery.baseline.as_ref().unwrap();
                assert_baseline(world, baseline);
                let marker = &baseline.marker;
                let retained = &world.resource::<Session>().markers;
                assert_eq!(retained.len(), 1);
                assert_eq!(retained[0].receipt, marker.instance.receipt);
                assert_eq!(retained[0].occurrence, marker.instance.occurrence);
                assert_eq!(retained[0].receipt.len(), 2);
                assert_eq!(retained[0].root_entity(world).unwrap(), marker.root);
                let active = world
                    .resource::<WorldIdentityDomain>()
                    .active_scene_instance(world, marker.instance.receipt.instance_id())
                    .unwrap();
                if recovery.retire {
                    assert_eq!(active.entity_ids(), std::slice::from_ref(&marker.instance.root));
                    assert!(!world.entities().contains(marker.child));
                    for id in marker.instance.receipt.entity_ids() {
                        if id != &marker.instance.root {
                            assert!(matches!(
                                marker.instance.receipt.resolve(world, id),
                                EntityLookup::Tombstoned(_)
                            ));
                        }
                    }
                } else {
                    assert_eq!(active, marker.instance.receipt);
                    assert!(world.entities().contains(marker.child));
                    assert!(world.get::<Sprite>(marker.root).is_none());
                    assert_eq!(
                        *world.get::<Transform2d>(marker.root).unwrap(),
                        Transform2d {
                            translation: Vec2::new(70.0, 80.0),
                            rotation: 1.0,
                            scale: Vec2::splat(3.0),
                            z: 0.0,
                        }
                    );
                }
                assert_foreign(world, recovery.foreign.unwrap());
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
                let marker = assert_marker(world, 2);
                assert_replaced(world, &baseline.marker, &marker);
                assert_foreign(world, recovery.foreign.unwrap());
                recovery.recovered = Some(marker);
                recovery.stable_since = Some(world.resource::<RealTime>().frame);
                recovery.step = Step::Steady;
            }
            Step::Steady => {
                assert_eq!(snapshot.phase, GamePhase::Running);
                assert!(snapshot.error.is_none());
                let baseline = recovery.baseline.as_ref().unwrap();
                assert_baseline(world, baseline);
                let marker = assert_marker(world, 2);
                let recovered = recovery.recovered.as_ref().unwrap();
                assert_eq!(marker.instance.receipt, recovered.instance.receipt);
                assert_eq!(
                    [marker.root, marker.child],
                    [recovered.root, recovered.child]
                );
                assert_replaced(world, &baseline.marker, &marker);
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
                for marker in [&baseline.marker, recovery.recovered.as_ref().unwrap()] {
                    assert_retired(world, marker);
                }
                for marker in &recovery.history {
                    assert_retired(world, marker);
                }
                let marker = assert_marker(world, 1);
                assert_replaced(world, &recovery.history[0], &marker);
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
    assert_eq!(current.receipt().len(), AUTHORED_LEVEL_MEMBERS);
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

fn assert_marker(world: &mut World, id: u64) -> MarkerGraph {
    let instances = &world.resource::<Session>().markers;
    assert_eq!(instances.len(), 1);
    let instance = instances[0].clone();
    let root = instance.root_entity(world).unwrap();
    let mut query = world.query::<(Entity, &DynamicMarker)>();
    let actual = query
        .iter(world)
        .map(|(entity, marker)| (entity, marker.id.get()))
        .collect::<Vec<_>>();
    assert_eq!(actual, [(root, id)]);
    let record = world
        .resource::<Progress>()
        .checkpoint()
        .markers
        .iter()
        .find(|marker| marker.id.get() == id)
        .unwrap();
    assert_eq!(instance.id, record.id);
    assert_eq!(instance.level, record.level);
    let definition = &world.resource::<ActiveLevel>().content().unwrap().marker;
    assert_eq!(definition.parts.len(), 2);
    assert_eq!(instance.root, definition.root);
    let mut ids = vec![instance.root.clone(), definition.link_target.clone()];
    ids.sort();
    assert_eq!(instance.receipt.entity_ids(), ids);
    assert_ne!(instance.root, definition.link_target);
    let EntityLookup::Resolved(child) = instance.receipt.resolve(world, &definition.link_target)
    else {
        panic!("the complete marker receipt must resolve its authored child");
    };
    assert_ne!(root, child);
    instance
        .validate_geometry(world, definition, Vec2::from_array(record.position))
        .unwrap();
    assert_eq!(
        *world.get::<Transform2d>(root).unwrap(),
        Transform2d::from_translation(Vec2::from_array(record.position))
    );
    assert_eq!(
        *world.get::<Transform2d>(child).unwrap(),
        Transform2d::from_translation(Vec2::splat(3.0))
    );
    assert_eq!(
        world.get::<Sprite>(root),
        Some(&Sprite::from_color(
            Vec2::splat(10.0),
            Color::rgb(1.0, 0.3, 0.8)
        ))
    );
    let child_sprite = world.get::<Sprite>(child).unwrap();
    let handle = child_sprite.material.image.unwrap();
    let image = world.resource::<Assets<ImageAsset>>().get(handle).unwrap();
    assert_eq!(
        image.product(),
        Some(crate::markers::marker_image_product())
    );
    let mut expected = Sprite::from_texture(handle, Vec2::splat(4.0));
    expected.material.sampler = SamplerDescriptor::NEAREST_CLAMP;
    assert_eq!(child_sprite, &expected);
    assert!(world.get::<Parent>(root).is_none());
    assert_eq!(world.get::<Parent>(child).unwrap().parent(), root);
    assert_eq!(&**world.get::<Children>(root).unwrap(), &[child]);
    assert!(
        world
            .get::<Children>(child)
            .is_none_or(|children| children.is_empty())
    );
    let roots = world.resource::<HierarchyRoots>();
    assert!(roots.contains(root));
    assert!(!roots.contains(child));
    let current = CurrentScene::get(world).unwrap();
    assert_eq!(current.receipt().len(), AUTHORED_LEVEL_MEMBERS);
    assert_eq!(instance.receipt.domain_id(), current.receipt().domain_id());
    assert_ne!(
        instance.receipt.instance_id(),
        current.receipt().instance_id()
    );
    let domain = world.resource::<WorldIdentityDomain>();
    assert_eq!(
        domain
            .active_scene_instance(world, instance.receipt.instance_id())
            .unwrap(),
        instance.receipt
    );
    for (id, entity) in [(&instance.root, root), (&definition.link_target, child)] {
        let source = world.get::<SceneEntitySource>(entity).unwrap();
        assert_eq!(source.instance_id, instance.receipt.instance_id());
        assert_eq!(&source.entity_id, id);
        assert_eq!(source.occurrence(), &instance.occurrence);
        let locators = domain.locators_for_entity(world, entity).unwrap().unwrap();
        assert_eq!(locators.scene(), instance.receipt.locator(id).as_ref());
        assert!(locators.persistent().is_none());
    }
    let link = &world.get::<MarkerLink>(root).unwrap().target;
    assert_eq!(
        link,
        &EntityReference::SceneLocal {
            entity: definition.link_target.clone(),
        }
    );
    assert!(world.get::<MarkerLink>(child).is_none());
    assert_eq!(
        domain.resolve_entity_reference(world, link, Some(instance.receipt.instance_id())),
        EntityLookup::Resolved(child)
    );
    assert_eq!(
        domain.resolve_entity_reference(world, link, None),
        EntityLookup::ContextRequired
    );
    assert_ne!(
        domain.resolve_entity_reference(world, link, Some(current.receipt().instance_id())),
        EntityLookup::Resolved(child)
    );
    MarkerGraph {
        instance,
        root,
        child,
    }
}

fn assert_retired(world: &World, marker: &MarkerGraph) {
    assert_eq!(marker.instance.receipt.len(), 2);
    let domain = world.resource::<WorldIdentityDomain>();
    assert!(
        domain
            .active_scene_instance(world, marker.instance.receipt.instance_id())
            .is_err()
    );
    for id in marker.instance.receipt.entity_ids() {
        assert!(matches!(
            marker.instance.receipt.resolve(world, id),
            EntityLookup::Tombstoned(_)
        ));
    }
    for entity in [marker.root, marker.child] {
        assert!(!world.entities().contains(entity));
        assert!(!world.resource::<HierarchyRoots>().contains(entity));
    }
}

fn assert_replaced(world: &World, original: &MarkerGraph, replacement: &MarkerGraph) {
    assert_retired(world, original);
    assert_eq!(replacement.instance.id, original.instance.id);
    assert_eq!(replacement.instance.level, original.instance.level);
    assert_eq!(
        replacement.instance.occurrence,
        original.instance.occurrence
    );
    assert_eq!(replacement.instance.root, original.instance.root);
    assert_eq!(
        replacement.instance.receipt.entity_ids(),
        original.instance.receipt.entity_ids()
    );
    assert_eq!(
        replacement.instance.receipt.domain_id(),
        original.instance.receipt.domain_id()
    );
    assert_ne!(
        replacement.instance.receipt.instance_id(),
        original.instance.receipt.instance_id()
    );
    for id in original.instance.receipt.entity_ids() {
        assert_ne!(
            replacement.instance.receipt.runtime_reference(id),
            original.instance.receipt.runtime_reference(id)
        );
    }
    for entity in [replacement.root, replacement.child] {
        assert!(![original.root, original.child].contains(&entity));
    }
    let link = &world.get::<MarkerLink>(replacement.root).unwrap().target;
    assert!(matches!(
        world
            .resource::<WorldIdentityDomain>()
            .resolve_entity_reference(world, link, Some(original.instance.receipt.instance_id())),
        EntityLookup::Tombstoned(_)
    ));
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
    let mut run = HeadlessRun::new(crate::open_project_root().unwrap(), intent);
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
fn marker_reconstruction_retry_rebuilds_authored_graph_without_replacing_current_scene() {
    run_recovery::<false>();
}

#[test]
fn marker_reconstruction_retry_replaces_legally_retired_child_and_can_travel_again() {
    run_recovery::<true>();
}
