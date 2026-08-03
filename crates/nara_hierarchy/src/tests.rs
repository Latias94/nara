use nara_ecs::{
    Commands, Entity, Resource, World,
    relationship::{Relationship, RelationshipHookMode, RelationshipTarget},
    schedule::IntoScheduleConfigs,
    world::CommandQueue,
};

use super::*;
use crate::validation::{HierarchyValidationScratch, validate_hierarchy_with_additions};

const DEEP_HIERARCHY_DEPTH: usize = 4_096;

#[test]
fn construction_flushes_an_immediate_reverse_projection() {
    let mut world = World::new();
    let parent = world.spawn_empty().id();
    let child = world.spawn_empty().id();

    HierarchyConstructionWriter::new(&mut world)
        .attach(child, parent)
        .expect("a valid edge should be accepted");
    world.flush();

    assert_eq!(world.get::<Parent>(child).map(Parent::parent), Some(parent));
    assert_eq!(
        world.get::<Children>(parent).map(Children::as_slice),
        Some([child].as_slice())
    );
    validate_hierarchy(&mut world).expect("the forward and reverse projections should agree");
}

#[test]
fn deferred_construction_reuses_validation_and_dirty_tracking() {
    let mut world = World::new();
    let parent = world.spawn_empty().id();
    let child = world.spawn_empty().id();
    let mut queue = CommandQueue::default();

    Commands::new(&mut queue, &world).attach_hierarchy_child(child, parent);
    queue.apply(&mut world);
    world.flush();

    assert_eq!(world.get::<Parent>(child).map(Parent::parent), Some(parent));
    assert!(
        world
            .resource::<HierarchyGenerationState>()
            .needs_validation()
    );
}

#[test]
fn construction_rejects_invalid_edges_before_topology_mutation() {
    let mut world = World::new();
    let parent = world.spawn_empty().id();
    let child = world.spawn_empty().id();
    let missing = world.spawn_empty().id();
    assert!(world.despawn(missing));

    let missing_child = HierarchyConstructionWriter::new(&mut world)
        .attach(missing, parent)
        .unwrap_err();
    assert_eq!(
        missing_child,
        HierarchyError::MissingChild { child: missing }
    );

    let missing_parent = HierarchyConstructionWriter::new(&mut world)
        .attach(child, missing)
        .unwrap_err();
    assert_eq!(
        missing_parent,
        HierarchyError::MissingParent {
            child,
            parent: missing
        }
    );

    let self_parent = HierarchyConstructionWriter::new(&mut world)
        .attach(child, child)
        .unwrap_err();
    assert_eq!(self_parent, HierarchyError::SelfParent { entity: child });
    assert!(world.get::<Parent>(child).is_none());
    assert!(!world.contains_resource::<HierarchyGenerationState>());

    HierarchyConstructionWriter::new(&mut world)
        .attach(child, parent)
        .unwrap();
    world.flush();
    let already_parented = HierarchyConstructionWriter::new(&mut world)
        .attach(child, parent)
        .unwrap_err();
    assert_eq!(
        already_parented,
        HierarchyError::AlreadyParented { child, parent }
    );

    let cycle = HierarchyConstructionWriter::new(&mut world)
        .attach(parent, child)
        .unwrap_err();
    assert!(matches!(cycle, HierarchyError::Cycle { entity } if entity == parent));
    assert!(world.get::<Parent>(parent).is_none());
}

#[test]
fn batch_construction_rejects_the_complete_plan_before_mutation() {
    let mut world = World::new();
    let parent = world.spawn_empty().id();
    let first_child = world.spawn_empty().id();
    let second_child = world.spawn_empty().id();
    let missing_parent = world.spawn_empty().id();
    assert!(world.despawn(missing_parent));

    let error = HierarchyConstructionWriter::new(&mut world)
        .attach_batch(&[
            HierarchyConstructionEdge::new(first_child, parent),
            HierarchyConstructionEdge::new(second_child, missing_parent),
        ])
        .unwrap_err();
    assert_eq!(
        error,
        HierarchyError::MissingParent {
            child: second_child,
            parent: missing_parent,
        }
    );
    assert!(world.get::<Parent>(first_child).is_none());
    assert!(world.get::<Parent>(second_child).is_none());
    assert!(world.get::<Children>(parent).is_none());
    assert!(!world.contains_resource::<HierarchyGenerationState>());

    let first = world.spawn_empty().id();
    let second = world.spawn_empty().id();
    let error = HierarchyConstructionWriter::new(&mut world)
        .attach_batch(&[
            HierarchyConstructionEdge::new(first, second),
            HierarchyConstructionEdge::new(second, first),
        ])
        .unwrap_err();
    assert!(matches!(error, HierarchyError::Cycle { .. }));
    assert!(world.get::<Parent>(first).is_none());
    assert!(world.get::<Parent>(second).is_none());
    assert!(!world.contains_resource::<HierarchyGenerationState>());
}

#[test]
fn deep_and_wide_batches_have_linear_preflight_and_one_dirty_mark() {
    let mut deep_world = World::new();
    let deep_entities = (0..DEEP_HIERARCHY_DEPTH)
        .map(|_| deep_world.spawn_empty().id())
        .collect::<Vec<_>>();
    let deep_edges = (1..deep_entities.len())
        .map(|index| HierarchyConstructionEdge::new(deep_entities[index], deep_entities[index - 1]))
        .collect::<Vec<_>>();
    let deep_stats = HierarchyConstructionWriter::new(&mut deep_world)
        .attach_batch_with_stats(&deep_edges)
        .unwrap();
    assert_eq!(deep_stats.parent_entities_scanned, 0);
    assert_eq!(deep_stats.children_entities_scanned, 0);
    assert_eq!(deep_stats.addition_edges_scanned, deep_edges.len());
    assert_eq!(deep_stats.cycle_starts_scanned, deep_edges.len());
    assert!(deep_stats.cycle_cursor_visits <= deep_edges.len() * 2 + 1);
    assert_eq!(
        deep_world
            .resource::<HierarchyGenerationState>()
            .topology_generation,
        1
    );
    deep_world.flush();
    validate_hierarchy(&mut deep_world).unwrap();

    let mut wide_world = World::new();
    let root = wide_world.spawn_empty().id();
    let children = (0..DEEP_HIERARCHY_DEPTH)
        .map(|_| wide_world.spawn_empty().id())
        .collect::<Vec<_>>();
    let wide_edges = children
        .iter()
        .copied()
        .map(|child| HierarchyConstructionEdge::new(child, root))
        .collect::<Vec<_>>();
    let wide_stats = HierarchyConstructionWriter::new(&mut wide_world)
        .attach_batch_with_stats(&wide_edges)
        .unwrap();
    assert_eq!(wide_stats.parent_entities_scanned, 0);
    assert_eq!(wide_stats.children_entities_scanned, 0);
    assert_eq!(wide_stats.addition_edges_scanned, wide_edges.len());
    assert_eq!(wide_stats.cycle_starts_scanned, wide_edges.len());
    assert!(wide_stats.cycle_cursor_visits <= wide_edges.len() * 2 + 1);
    assert_eq!(
        wide_world
            .resource::<HierarchyGenerationState>()
            .topology_generation,
        1
    );
    wide_world.flush();
    assert_eq!(
        wide_world.get::<Children>(root).map(Children::len),
        Some(children.len())
    );
    validate_hierarchy(&mut wide_world).unwrap();
}

#[test]
fn parent_despawn_marks_the_surviving_topology_dirty_once() {
    const {
        assert!(!<Children as RelationshipTarget>::LINKED_SPAWN);
    }

    let mut app = App::new();
    app.add_plugin(HierarchyPlugin).unwrap();
    let parent = app.world_mut().unwrap().spawn_empty().id();
    let child = app.world_mut().unwrap().spawn_empty().id();
    {
        let world = app.world_mut().unwrap();
        HierarchyConstructionWriter::new(world)
            .attach(child, parent)
            .unwrap();
        world.flush();
        validate_dirty_hierarchy(world).unwrap();
    }
    app.insert_resource(DespawnBeforeHierarchyCompletion(parent))
        .unwrap();
    app.add_systems(
        CoreStage::PostUpdate,
        despawn_parent_before_completion.before(HierarchySet::ValidateAndComplete),
    )
    .unwrap();

    let completed_generation = app
        .world()
        .resource::<HierarchyGenerationState>()
        .topology_generation;
    let completed_scans = app
        .world()
        .resource::<HierarchyGenerationState>()
        .validation_scans;

    app.update().unwrap();

    let world = app.world();
    assert!(world.get_entity(child).is_ok());
    assert!(world.get::<Parent>(child).is_none());
    let completed_state = world.resource::<HierarchyGenerationState>();
    assert_eq!(
        completed_state.completed_generation,
        Some(completed_generation + 1)
    );
    assert_eq!(completed_state.validation_scans, completed_scans + 1);
}

#[derive(Resource)]
struct DespawnBeforeHierarchyCompletion(Entity);

fn despawn_parent_before_completion(world: &mut World) {
    let parent = world.resource::<DespawnBeforeHierarchyCompletion>().0;
    assert!(world.despawn(parent));
}

#[test]
fn unchanged_generation_skips_full_validation_scans() {
    let mut world = World::new();
    world.init_resource::<HierarchyGenerationState>();
    world.init_resource::<RetainedHierarchyValidationScratch>();

    validate_dirty_hierarchy(&mut world).unwrap();
    validate_dirty_hierarchy(&mut world).unwrap();
    assert_eq!(
        world
            .resource::<HierarchyGenerationState>()
            .validation_scans,
        0
    );

    let parent = world.spawn_empty().id();
    let child = world.spawn_empty().id();
    HierarchyConstructionWriter::new(&mut world)
        .attach(child, parent)
        .unwrap();
    world.flush();

    validate_dirty_hierarchy(&mut world).unwrap();
    validate_dirty_hierarchy(&mut world).unwrap();
    let state = world.resource::<HierarchyGenerationState>();
    assert_eq!(state.topology_generation, 1);
    assert_eq!(state.completed_generation, Some(1));
    assert_eq!(state.validation_scans, 1);
}

#[test]
fn dirty_generations_reuse_retained_validation_storage() {
    let mut world = World::new();
    world.init_resource::<HierarchyGenerationState>();
    world.init_resource::<RetainedHierarchyValidationScratch>();

    let parent = world.spawn_empty().id();
    let child = world.spawn_empty().id();
    HierarchyConstructionWriter::new(&mut world)
        .attach(child, parent)
        .unwrap();
    world.flush();
    validate_dirty_hierarchy(&mut world).unwrap();

    let initial_capacities = world
        .resource::<RetainedHierarchyValidationScratch>()
        .0
        .capacities();
    assert!(initial_capacities.into_iter().all(|capacity| capacity > 0));

    mark_topology_dirty(&mut world);
    validate_dirty_hierarchy(&mut world).unwrap();
    assert_eq!(
        world
            .resource::<RetainedHierarchyValidationScratch>()
            .0
            .capacities(),
        initial_capacities
    );
    assert_eq!(
        world
            .resource::<HierarchyGenerationState>()
            .validation_scans,
        2
    );

    validate_dirty_hierarchy(&mut world).unwrap();
    assert_eq!(
        world
            .resource::<RetainedHierarchyValidationScratch>()
            .0
            .capacities(),
        initial_capacities
    );
    assert_eq!(
        world
            .resource::<HierarchyGenerationState>()
            .validation_scans,
        2
    );
}

#[test]
fn deep_cycle_detection_is_iterative() {
    let mut world = World::new();
    let entities = raw_chain(&mut world, DEEP_HIERARCHY_DEPTH);
    world
        .entity_mut(entities[0])
        .insert(<Parent as Relationship>::from(
            *entities.last().expect("the chain is non-empty"),
        ));
    world.flush();

    assert!(matches!(
        validate_hierarchy(&mut world),
        Err(HierarchyError::Cycle { .. })
    ));
}

#[test]
fn deep_missing_parent_detection_is_iterative() {
    let mut world = World::new();
    let entities = raw_chain(&mut world, DEEP_HIERARCHY_DEPTH);
    let missing = world.spawn_empty().id();
    assert!(world.despawn(missing));
    world
        .entity_mut(entities[0])
        .insert_with_relationship_hook_mode(
            <Parent as Relationship>::from(missing),
            RelationshipHookMode::Skip,
        );

    assert_eq!(
        validate_hierarchy(&mut world),
        Err(HierarchyError::MissingParent {
            child: entities[0],
            parent: missing,
        })
    );
}

#[test]
fn deep_reverse_projection_detection_is_iterative() {
    let mut world = World::new();
    let entities = raw_chain(&mut world, DEEP_HIERARCHY_DEPTH);
    let child = entities[DEEP_HIERARCHY_DEPTH / 2];
    let parent = entities[DEEP_HIERARCHY_DEPTH / 2 - 1];
    let sibling = world.spawn(<Parent as Relationship>::from(parent)).id();
    world.flush();
    world
        .get_mut::<Children>(parent)
        .expect("the valid chain should have a reverse projection")
        .collection_mut_risky()
        .retain(|candidate| *candidate != child);

    assert_eq!(
        validate_hierarchy(&mut world),
        Err(HierarchyError::ReverseMissing { parent, child })
    );
    assert_eq!(
        world.get::<Parent>(sibling).map(Parent::parent),
        Some(parent)
    );
}

#[test]
fn sparse_one_sided_relationships_remain_visible_to_complete_validation() {
    let mut parent_only_world = World::new();
    for _ in 0..DEEP_HIERARCHY_DEPTH {
        parent_only_world.spawn_empty();
    }
    let parent = parent_only_world.spawn_empty().id();
    let child = parent_only_world.spawn_empty().id();
    parent_only_world
        .entity_mut(child)
        .insert_with_relationship_hook_mode(
            <Parent as Relationship>::from(parent),
            RelationshipHookMode::Skip,
        );

    assert_eq!(
        validate_hierarchy(&mut parent_only_world),
        Err(HierarchyError::ReverseMissing { parent, child })
    );

    let mut children_only_world = World::new();
    for _ in 0..DEEP_HIERARCHY_DEPTH {
        children_only_world.spawn_empty();
    }
    let parent = children_only_world.spawn_empty().id();
    let child = children_only_world.spawn_empty().id();
    let mut children = Children::default();
    children.collection_mut_risky().push(child);
    children_only_world.entity_mut(parent).insert(children);

    assert_eq!(
        validate_hierarchy(&mut children_only_world),
        Err(HierarchyError::ReverseUnexpected {
            parent,
            child,
            actual_parent: None,
        })
    );
}

#[test]
fn complete_validation_scans_only_hierarchy_archetypes_in_a_sparse_world() {
    let mut world = World::new();
    for _ in 0..DEEP_HIERARCHY_DEPTH {
        world.spawn_empty();
    }
    let parent = world.spawn_empty().id();
    let child = world.spawn(<Parent as Relationship>::from(parent)).id();
    world.flush();
    let mut scratch = HierarchyValidationScratch::default();

    let stats =
        validate_hierarchy_with_additions(&mut world, core::iter::empty(), &mut scratch).unwrap();

    assert_eq!(stats.parent_entities_scanned, 1);
    assert_eq!(stats.children_entities_scanned, 1);
    assert_eq!(world.get::<Parent>(child).map(Parent::parent), Some(parent));
}

#[test]
fn reverse_projection_corruption_reports_exact_errors() {
    let mut empty_world = World::new();
    let empty_parent = empty_world.spawn_empty().id();
    let empty_child = empty_world
        .spawn(<Parent as Relationship>::from(empty_parent))
        .id();
    empty_world.flush();
    empty_world
        .get_mut::<Children>(empty_parent)
        .unwrap()
        .collection_mut_risky()
        .clear();
    assert_eq!(
        validate_hierarchy(&mut empty_world),
        Err(HierarchyError::ReverseEmpty {
            parent: empty_parent,
        })
    );
    assert_eq!(
        empty_world.get::<Parent>(empty_child).map(Parent::parent),
        Some(empty_parent)
    );

    let mut missing_world = World::new();
    let missing_parent = missing_world.spawn_empty().id();
    let missing_child = missing_world
        .spawn(<Parent as Relationship>::from(missing_parent))
        .id();
    let missing = missing_world.spawn_empty().id();
    missing_world.flush();
    assert!(missing_world.despawn(missing));
    missing_world
        .get_mut::<Children>(missing_parent)
        .unwrap()
        .collection_mut_risky()
        .push(missing);
    assert_eq!(
        validate_hierarchy(&mut missing_world),
        Err(HierarchyError::ReverseChildMissing {
            parent: missing_parent,
            child: missing,
        })
    );
    assert_eq!(
        missing_world
            .get::<Parent>(missing_child)
            .map(Parent::parent),
        Some(missing_parent)
    );

    let mut duplicate_world = World::new();
    let duplicate_parent = duplicate_world.spawn_empty().id();
    let duplicate_child = duplicate_world
        .spawn(<Parent as Relationship>::from(duplicate_parent))
        .id();
    duplicate_world.flush();
    duplicate_world
        .get_mut::<Children>(duplicate_parent)
        .unwrap()
        .collection_mut_risky()
        .push(duplicate_child);
    assert_eq!(
        validate_hierarchy(&mut duplicate_world),
        Err(HierarchyError::ReverseDuplicate {
            parent: duplicate_parent,
            child: duplicate_child,
        })
    );

    let mut unexpected_world = World::new();
    let recorded_parent = unexpected_world.spawn_empty().id();
    let actual_parent = unexpected_world.spawn_empty().id();
    let unexpected_child = unexpected_world
        .spawn(<Parent as Relationship>::from(recorded_parent))
        .id();
    unexpected_world.flush();
    unexpected_world
        .entity_mut(unexpected_child)
        .insert_with_relationship_hook_mode(
            <Parent as Relationship>::from(actual_parent),
            RelationshipHookMode::Skip,
        );
    assert_eq!(
        validate_hierarchy(&mut unexpected_world),
        Err(HierarchyError::ReverseUnexpected {
            parent: recorded_parent,
            child: unexpected_child,
            actual_parent: Some(actual_parent),
        })
    );
}

fn raw_chain(world: &mut World, depth: usize) -> Vec<Entity> {
    let entities = (0..depth)
        .map(|_| world.spawn_empty().id())
        .collect::<Vec<_>>();
    for index in 1..entities.len() {
        world
            .entity_mut(entities[index])
            .insert(<Parent as Relationship>::from(entities[index - 1]));
    }
    world.flush();
    entities
}
