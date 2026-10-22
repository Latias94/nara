use super::*;
use nara::{
    hierarchy::{HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter},
    transform::{
        SpatialSnapshotError, SpatialSnapshotPhase, fixed_spatial_snapshot,
        historical_spatial_snapshot,
    },
};

#[test]
fn pursuit_stops_at_a_nonspatial_direct_parent_and_capture_reads_the_same_tick() {
    let (project, plan, root) = headless_wave_candidate_plan_and_root();
    let content = ProjectContentLoader::new(root)
        .unwrap()
        .load(&project, &plan)
        .unwrap();
    let mut runtime = admit_adapter_candidate_with_edit(plan, content.scene_source(), |world| {
        let enemy = first_owned_enemy(world);
        let grouping = world.get::<Parent>(enemy).unwrap().parent();
        world.entity_mut(grouping).remove::<Transform2d>();
        let ancestor = world
            .spawn(Transform2d {
                translation: Vec2::new(100.0, -30.0),
                rotation: std::f32::consts::FRAC_PI_2,
                scale: Vec2::splat(3.0),
            })
            .id();
        let from = HierarchyPlacement::current(world, grouping).unwrap();
        HierarchyWriter::new(world)
            .apply(HierarchyOperation::Place {
                entity: ancestor,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            })
            .unwrap();
        HierarchyWriter::new(world)
            .apply(HierarchyOperation::Move {
                entity: grouping,
                from,
                to: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent: ancestor }),
            })
            .unwrap();
        world.get_mut::<Transform2d>(enemy).unwrap().translation = Vec2::new(6.0, 0.0);
        world.get_mut::<WaveSpawn>(enemy).unwrap().tick = 1;
    })
    .complete_startup()
    .unwrap()
    .promote();
    let enemy = first_owned_enemy(runtime.world());
    let source_id = runtime
        .world()
        .get::<SceneEntitySource>(enemy)
        .unwrap()
        .entity_id
        .clone();
    let previous = historical_spatial_snapshot(runtime.world()).unwrap();
    assert_eq!(previous.effective_parent(enemy).unwrap(), None);
    assert_eq!(
        previous.global(enemy).unwrap().translation(),
        Vec2::new(6.0, 0.0)
    );

    drive_adapter_fixed(&mut runtime);

    assert_eq!(
        runtime
            .world()
            .get::<Transform2d>(enemy)
            .unwrap()
            .translation,
        Vec2::new(5.5, 0.0)
    );
    assert!(
        runtime
            .world()
            .resource::<RuntimeFaultReporter>()
            .fault()
            .is_none()
    );
    let captured = runtime.world().resource::<WaveSnapshot>();
    assert_eq!(captured.tick, 1);
    let enemy_snapshot = captured
        .enemies
        .iter()
        .find(|value| value.id == source_id.as_str())
        .unwrap();
    assert_eq!(enemy_snapshot.position, Vec2::new(5.5, 0.0));
    stop_adapter_retry_runtime(runtime);
}

const CUT_PLUGIN_ID: PluginId = PluginId::new("reference-game.test.selected-spatial-cut");
const CUT_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(CUT_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(TEST_PLUGIN_REQUIREMENTS);

#[derive(Default)]
struct SelectedCutPlugin;

#[derive(Resource)]
struct SelectedCut {
    retired: Entity,
    spawned: Option<Entity>,
    captured: bool,
}

impl Plugin for SelectedCutPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &CUT_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(
            CoreStage::FixedUpdate,
            consume_then_change_membership.in_set(GameplayCommandSet::Consume),
        )?
        .add_systems(
            CoreStage::FixedUpdate,
            capture_changed_membership.in_set(GameplayCommandSet::Capture),
        )?;
        Ok(())
    }
}

fn consume_then_change_membership(world: &mut World) {
    if world.resource::<FixedTime>().tick() != 1 {
        return;
    }
    let retired = world.resource::<SelectedCut>().retired;
    let (previous_global, previous_stamp) = {
        let previous = historical_spatial_snapshot(world).unwrap();
        (previous.global(retired).unwrap(), previous.stamp().unwrap())
    };
    let from = HierarchyPlacement::current(world, retired).unwrap();
    HierarchyWriter::new(world).remove(retired, from).unwrap();
    assert!(world.despawn(retired));
    let spawned = world
        .spawn(Transform2d::from_translation(Vec2::new(9.0, 7.0)))
        .id();
    HierarchyWriter::new(world)
        .apply(HierarchyOperation::Place {
            entity: spawned,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        })
        .unwrap();

    let selected = historical_spatial_snapshot(world).unwrap();
    assert_eq!(selected.stamp().unwrap(), previous_stamp);
    assert_eq!(selected.global(retired).unwrap(), previous_global);
    assert!(selected.order().unwrap().contains(&retired));
    assert!(
        matches!(selected.global(spawned), Err(SpatialSnapshotError::MissingEntity { entity }) if entity == spawned)
    );
    assert!(!selected.order().unwrap().contains(&spawned));
    world.resource_mut::<SelectedCut>().spawned = Some(spawned);
}

fn capture_changed_membership(world: &mut World) {
    if world.resource::<FixedTime>().tick() != 1 {
        return;
    }
    let state = world.resource::<SelectedCut>();
    let spawned = state.spawned.unwrap();
    let current = fixed_spatial_snapshot(world).unwrap();
    let stamp = current.stamp().unwrap();
    assert_eq!(stamp.phase, SpatialSnapshotPhase::Fixed);
    assert_eq!(stamp.tick, world.resource::<FixedTime>().tick());
    assert_eq!(
        current.global(spawned).unwrap().translation(),
        Vec2::new(9.0, 7.0)
    );
    assert!(
        matches!(current.global(state.retired), Err(SpatialSnapshotError::MissingEntity { entity }) if entity == state.retired)
    );
    assert!(current.order().unwrap().contains(&spawned));
    assert!(!current.order().unwrap().contains(&state.retired));
    world.resource_mut::<SelectedCut>().captured = true;
}

#[test]
fn consuming_a_cut_then_spawning_and_retiring_only_changes_the_next_publication() {
    let (project, plan, root) =
        headless_wave_candidate_plan_and_root_with(PluginDefinition::for_default::<
            SelectedCutPlugin,
        >());
    let content = ProjectContentLoader::new(root)
        .unwrap()
        .load(&project, &plan)
        .unwrap();
    let mut runtime = admit_adapter_candidate_with_edit(plan, content.scene_source(), |world| {
        let retired = world
            .spawn(Transform2d::from_translation(Vec2::new(2.0, 4.0)))
            .id();
        HierarchyWriter::new(world)
            .apply(HierarchyOperation::Place {
                entity: retired,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            })
            .unwrap();
        world.insert_resource(SelectedCut {
            retired,
            spawned: None,
            captured: false,
        });
    })
    .complete_startup()
    .unwrap()
    .promote();

    drive_adapter_fixed(&mut runtime);

    assert!(runtime.world().resource::<SelectedCut>().captured);
    assert_eq!(runtime.world().resource::<WaveSnapshot>().tick, 1);
    assert!(
        runtime
            .world()
            .resource::<RuntimeFaultReporter>()
            .fault()
            .is_none()
    );
    stop_adapter_retry_runtime(runtime);
}

fn first_owned_enemy(world: &World) -> Entity {
    world
        .iter_entities()
        .find_map(|entity| {
            (entity.contains::<EnemyRole>()
                && registered_scene_entity(world, entity.id())
                && entity
                    .get::<SceneEntitySource>()
                    .is_some_and(is_first_enemy_source))
            .then_some(entity.id())
        })
        .unwrap()
}
